use std::io::BufRead;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::sync::Mutex as StdMutex;
use std::time::Duration;

use log::{debug, error, info, warn};
use tokio::time::sleep;

use crate::api::{OpenCodeApi, V2Adapter};

/// HTTP Basic username opencode 2.0.32 requires on its v2 API (paired with the
/// startup-printed password; see `parse_server_password`).
const BASIC_AUTH_USER: &str = "opencode";

/// RAII owner of the `opencode serve` process.
///
/// - `start()` — starts the server in the constructor
/// - `Drop` — stops the server (kill) when the owner dies
///
/// Port conflict solved: `port = 0` (default) — the server picks
/// a free port itself, we read it from the log. Another opencode
/// on the phone doesn't collide.
pub struct ServerConfig {
    /// Port. 0 = auto-select (default).
    pub port: u16,
    /// opencode binary ($OPENCODE_BIN → which).
    pub bin: Option<std::path::PathBuf>,
    /// Extra arguments for `opencode serve`.
    pub args: Vec<String>,
    /// Working directory for opencode serve (cwd). Defaults to $HOME/opencode-home
    pub cwd: Option<PathBuf>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            port: 0,
            bin: None,
            // No `--pure`: opencode 2.0.32 dropped that flag (`serve` rejects it
            // with "Unrecognized flag: --pure" and exits 1 — the server never
            // came up). 1.19.0 accepted it to run without plugins; 2.0.32 has no
            // equivalent and does not need one to start.
            args: Vec::new(),
            cwd: None,
        }
    }
}

pub struct ServerGuard {
    pub child: StdMutex<Option<Child>>,
    pub base: String,
    pub port: u16,
    pub auth: Option<(String, String)>,
}

impl ServerGuard {
    /// Starts `opencode serve`, reads the assigned port from the log,
    /// waits for a health response and returns a ready guard (RAII).
    pub async fn start(cfg: ServerConfig) -> Result<Self, String> {
        // First clean up our orphans from previous crash runs.
        reap_orphans();

        let bin = resolve_bin(cfg.bin);
        info!("PATH={:?}, opencode bin={:?}", std::env::var("PATH"), bin);

        let mut cmd = Command::new(&bin);
        let home = std::env::var("HOME").unwrap_or_else(|_| "/home/defaultuser".into());
        // An empty cwd would make the child's chdir fail with ENOENT
        // (Command::spawn reports it as os error 2) — fall back to the default.
        let cwd = cfg
            .cwd
            .clone()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| PathBuf::from(&home).join("opencode-home"));
        if let Err(e) = std::fs::create_dir_all(&cwd) {
            warn!("failed to create cwd {cwd:?}: {e}");
        }
        cmd.current_dir(&cwd)
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", format!("{home}/.config"))
            .env("XDG_DATA_HOME", format!("{home}/.local/share"))
            .env("XDG_CACHE_HOME", format!("{home}/.cache"))
            .env("XDG_STATE_HOME", format!("{home}/.local/state"))
            .env(
                "PATH",
                std::env::var("PATH").unwrap_or_else(|_| "/usr/local/bin:/usr/bin:/bin".into()),
            );
        cmd.arg("serve")
            .arg("--port")
            .arg(cfg.port.to_string())
            .args(&cfg.args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        info!("запускаю: {:?}", cmd);
        let mut child = cmd.spawn().map_err(|e| {
            format!("не удалось запустить `{}`: {e}", bin.display())
        })?;
        // Remember our PID: only these processes we are allowed to stop.
        let pid = child.id();
        register_spawn(pid);

        // std::sync::mpsc — inter-thread, without a tokio runtime.
        let (tx, rx) = mpsc::channel::<String>();

        if let Some(out) = child.stdout.take() {
            let tx = tx.clone();
            std::thread::spawn(move || read_lines(out, tx));
        }
        if let Some(err) = child.stderr.take() {
            let tx = tx.clone();
            std::thread::spawn(move || read_lines(err, tx));
        }
        drop(tx);

        // Wait for the line "listening on http://127.0.0.1:PORT".
        // Cold start of `opencode serve` (a Bun app) on a busy phone can take
        // well over the old 5 s — a too-short wait killed it and left the UI on
        // a red "error" badge. Give it up to 30 s.
        let mut port: Option<u16> = None;
        let mut password: Option<String> = None;
        const MAX_ATTEMPTS: u32 = 300;
        for _i in 0..MAX_ATTEMPTS {
            match child.try_wait() {
                Ok(Some(status)) => {
                    unregister_spawn(pid);
                    return Err(format!("opencode serve умер: {status}"));
                }
                Ok(None) => {}
                Err(e) => return Err(format!("try_wait err: {e}")),
            }
            while let Ok(line) = rx.try_recv() {
                debug!("[opencode] {line}");
                if port.is_none() {
                    port = parse_listening_port(&line);
                }
                if password.is_none() {
                    password = parse_server_password(&line);
                }
            }
            if port.is_some() {
                break;
            }
            sleep(Duration::from_millis(100)).await;
        }

        let port = match port {
            Some(p) => p,
            None => {
                let _ = child.kill();
                unregister_spawn(pid);
                return Err("opencode serve не сообщил порт за 30s".into());
            }
        };
        let base = format!("http://127.0.0.1:{port}");
        // opencode 2.0.32 guards the v2 API with HTTP Basic auth. The username
        // is the fixed literal `opencode`; the password is the one the server
        // printed on startup. Absent (older builds that print none) → no auth
        // header, same as before.
        let auth = password.map(|pw| (BASIC_AUTH_USER.to_string(), pw));

        // Background log collector.
        tokio::spawn(async move {
            while let Ok(line) = rx.recv() {
                debug!("[opencode-bg] {line}");
            }
        });

        // Wait for health.
        let client = V2Adapter::new(base.clone(), auth.clone());
        let mut ready = false;
        for _ in 0..200 {
            if client.health().await.unwrap_or(false) {
                ready = true;
                break;
            }
            sleep(Duration::from_millis(100)).await;
        }
        if !ready {
            let _ = child.kill();
            unregister_spawn(pid);
            return Err(format!("opencode serve не ответил на health: {base}"));
        }

        info!("opencode serve готов: port={port}, pid={}", child.id());
        Ok(Self {
            child: StdMutex::new(Some(child)),
            base,
            port,
            auth,
        })
    }

    pub fn base_url(&self) -> &str {
        &self.base
    }

    pub fn pid(&self) -> u32 {
        self.child
            .lock()
            .unwrap()
            .as_ref()
            .map(|c| c.id())
            .unwrap_or(0)
    }

    pub async fn health(&self) -> Result<bool, String> {
        let client = V2Adapter::new(self.base.clone(), self.auth.clone());
        client.health().await
    }
}

impl Drop for ServerGuard {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.lock().unwrap().take() {
            let pid = child.id();
            info!("останавливаю opencode serve (pid={pid})");
            match child.kill() {
                Ok(_) => {
                    let _ = child.wait();
                }
                Err(e) => error!("не смог остановить opencode serve: {e}"),
            }
            unregister_spawn(pid);
        }
    }
}

// --- Tracking of `opencode serve` servers we spawned ----------------------
//
// mapplauncherd-booster sometimes kills the app in a way that `Drop` doesn't
// run, and `opencode serve` remains hanging as an orphan. To clean up
// EXACTLY OUR processes on the next start (and never touch
// `opencode` launched by the user from a terminal), we keep a registry:
//   pid + start time from /proc/<pid>/stat (protection against PID reuse).
// File: $XDG_DATA_HOME/harbour-opencode/spawned-servers.tsv

extern "C" {
    fn kill(pid: i32, sig: i32) -> i32;
}

const SIGTERM: i32 = 15;
const SIGKILL: i32 = 9;

fn registry_path() -> std::path::PathBuf {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .map(|h| std::path::PathBuf::from(h).join(".local/share"))
        })
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"));
    base.join("harbour-opencode").join("spawned-servers.tsv")
}

/// Field 22 `starttime` from /proc/<pid>/stat (in ticks). `None` if the process doesn't exist.
fn proc_start_time(pid: u32) -> Option<u64> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // comm is wrapped in parentheses and may contain ')' — split at the LAST ')'.
    let after = stat.rsplit_once(')')?.1;
    // After comm, the state field is the 3rd field, so starttime (22) is the 20th here.
    after.split_whitespace().nth(19)?.parse().ok()
}

fn proc_alive(pid: u32) -> bool {
    std::path::Path::new(&format!("/proc/{pid}")).exists()
}

/// Whether the PID currently is really `opencode ... serve` (extra safety).
fn is_opencode_serve(pid: u32) -> bool {
    match std::fs::read(format!("/proc/{pid}/cmdline")) {
        Ok(bytes) => {
            let s = String::from_utf8_lossy(&bytes).replace('\0', " ");
            s.contains("opencode") && s.contains("serve")
        }
        Err(_) => false,
    }
}

fn kill_pid(pid: u32) {
    for sig in [SIGTERM, SIGKILL] {
        unsafe { kill(pid as i32, sig) };
        for _ in 0..10 {
            if !proc_alive(pid) {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

fn read_registry() -> Vec<(u32, u64)> {
    std::fs::read_to_string(registry_path())
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            Some((it.next()?.parse().ok()?, it.next()?.parse().ok()?))
        })
        .collect()
}

fn write_registry(records: &[(u32, u64)]) {
    let path = registry_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let body: String = records.iter().map(|(p, t)| format!("{p} {t}\n")).collect();
    let _ = std::fs::write(path, body);
}

fn register_spawn(pid: u32) {
    if let Some(start) = proc_start_time(pid) {
        let mut rec = read_registry();
        rec.retain(|(p, _)| *p != pid);
        rec.push((pid, start));
        write_registry(&rec);
    }
}

fn unregister_spawn(pid: u32) {
    let mut rec = read_registry();
    let before = rec.len();
    rec.retain(|(p, _)| *p != pid);
    if rec.len() != before {
        write_registry(&rec);
    }
}

/// Stops only OUR orphaned servers from the registry.
fn reap_orphans() {
    let records = read_registry();
    if records.is_empty() {
        return;
    }
    let mut alive = Vec::new();
    for (pid, start) in records {
        // Dead or reused PID — just forget it.
        if proc_start_time(pid) != Some(start) {
            continue;
        }
        if is_opencode_serve(pid) {
            info!("подчищаю осиротевший opencode serve (pid={pid})");
            kill_pid(pid);
        } else {
            debug!("pid={pid} из реестра уже не opencode serve — не трогаю");
        }
        if proc_alive(pid) {
            alive.push((pid, start));
        }
    }
    write_registry(&alive);
}

/// Finds the opencode binary. Priority: explicit path → OPENCODE_BIN → bundled in the RPM
/// (ours, isolated) → usual system locations → PATH.
pub fn resolve_bin(explicit: Option<std::path::PathBuf>) -> std::path::PathBuf {
    if let Some(b) = explicit {
        return b;
    }
    if let Ok(p) = std::env::var("OPENCODE_BIN") {
        return p.into();
    }
    for cand in [
        "/usr/libexec/harbour-opencode/opencode",
        "/usr/local/bin/opencode",
        "/usr/bin/opencode",
        "/opt/opencode/bin/opencode",
    ] {
        if std::path::Path::new(cand).exists() {
            return cand.into();
        }
    }
    let home = std::env::var("HOME").unwrap_or_default();
    if !home.is_empty() {
        let p = format!("{home}/.opencode/bin/opencode");
        if std::path::Path::new(&p).exists() {
            return p.into();
        }
    }
    "opencode".into()
}

fn read_lines<R: std::io::Read>(reader: R, tx: mpsc::Sender<String>) {
    for line in std::io::BufReader::new(reader).lines().flatten() {
        if tx.send(line).is_err() {
            return;
        }
    }
}

/// "[out] opencode server listening on http://127.0.0.1:45067" → Some(45067)
fn parse_listening_port(line: &str) -> Option<u16> {
    let needle = "listening on http://127.0.0.1:";
    let pos = line.find(needle)?;
    let rest = &line[pos + needle.len()..];
    rest.chars()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>()
        .parse()
        .ok()
}

/// "server password <pw>" → Some("<pw>"). opencode 2.0.32 protects the v2 API
/// with HTTP Basic auth (user `opencode`) and prints the password once on
/// startup. We feed it into `ServerGuard::auth` so every request/SSE carries
/// `Authorization: Basic`.
fn parse_server_password(line: &str) -> Option<String> {
    let needle = "server password ";
    let pos = line.find(needle)?;
    let rest = line[pos + needle.len()..].trim();
    if rest.is_empty() {
        None
    } else {
        Some(rest.to_string())
    }
}