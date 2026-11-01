#!/usr/bin/env python3
"""Agent self-test: can a model really work on files through the bundled opencode?

Runs ON THE PHONE (python3 is part of Sailfish OS; stdlib only). For each model
it starts a private `opencode serve` in a scratch work directory, so the user's
own projects and the app's server are never touched, and runs four tasks:

  write   create a file with exact content            -> checked on disk
  edit    change that file in place                   -> checked on disk
  debug   run a failing script, find and fix the bug  -> script re-run by us
  read    report a secret value stored in a file      -> checked in the reply

Permission requests from the agent (edit/bash) are approved automatically for
the duration of the test. Results are verified by this script, never by the
model's own claims. The test sessions are deleted afterwards (unless --keep).

Usage (as defaultuser on the phone):
  python3 agent-selftest.py                       # free OpenCode model
  python3 agent-selftest.py anthropic/claude-haiku-5-5 opencode/big-pickle
  python3 agent-selftest.py --variant low anthropic/claude-haiku-5-5

Exit code 0 only when every task of every model passed. Credentials come from
the user's normal opencode data directory (keys saved in the app work here).
"""

import argparse
import base64
import json
import os
import random
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request

OPENCODE_BIN = os.environ.get("OPENCODE_BIN", "/usr/libexec/harbour-opencode/opencode")
DEFAULT_MODELS = ["opencode/big-pickle"]
TASK_TIMEOUT_S = 240
POLL_S = 1.0


class Server:
    """A private `opencode serve` on a free port, killed on exit."""

    def __init__(self, workdir):
        self.workdir = workdir
        self.proc = None
        self.base = None
        self.auth = None

    def __enter__(self):
        self.proc = subprocess.Popen(
            [OPENCODE_BIN, "serve", "--port", "0"],
            cwd=self.workdir,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
        )
        password, url = None, None
        deadline = time.time() + 60
        while time.time() < deadline and (password is None or url is None):
            line = self.proc.stdout.readline()
            if not line:
                if self.proc.poll() is not None:
                    raise RuntimeError("opencode serve exited during start-up")
                continue
            line = line.strip()
            if line.startswith("server password "):
                password = line.split(" ", 2)[2]
            if line.startswith("server listening on "):
                url = line.split(" ")[-1]
        if password is None or url is None:
            raise RuntimeError("opencode serve did not report its URL/password in time")
        self.base = url.rstrip("/")
        self.auth = "Basic " + base64.b64encode(("opencode:" + password).encode()).decode()
        # Integrations load right after start; wait until the catalog answers.
        for _ in range(40):
            try:
                if self.get("/api/model").get("data"):
                    break
            except (OSError, ValueError, urllib.error.URLError):
                pass
            time.sleep(0.5)
        return self

    def __exit__(self, *exc):
        if self.proc and self.proc.poll() is None:
            self.proc.terminate()
            try:
                self.proc.wait(timeout=10)
            except subprocess.TimeoutExpired:
                self.proc.kill()

    def _req(self, method, path, body=None):
        data = None if body is None else json.dumps(body).encode()
        req = urllib.request.Request(self.base + path, data=data, method=method)
        req.add_header("Authorization", self.auth)
        if data is not None:
            req.add_header("Content-Type", "application/json")
        with urllib.request.urlopen(req, timeout=30) as resp:
            raw = resp.read()
        return json.loads(raw) if raw else {}

    def get(self, path):
        return self._req("GET", path)

    def post(self, path, body=None):
        return self._req("POST", path, body if body is not None else {})


def available_models(srv):
    return {m["providerID"] + "/" + m["id"] for m in srv.get("/api/model").get("data", [])}


def run_turn(srv, sid, text):
    """Send one prompt, approve permissions, wait for the turn to finish.

    Returns (reply_text, error, seconds)."""
    t0 = time.time()
    srv.post("/api/session/%s/prompt" % sid, {"text": text})
    seen_busy = False
    idle_polls = 0
    while time.time() - t0 < TASK_TIMEOUT_S:
        time.sleep(POLL_S)
        try:
            perms = srv.get("/api/session/%s/permission" % sid).get("data", [])
        except urllib.error.HTTPError:
            perms = []
        for p in perms:
            srv.post("/api/session/%s/permission/%s/reply" % (sid, p["id"]), {"decision": "once"})
        active = srv.get("/api/session/active").get("data", {}) or {}
        if sid in active:
            seen_busy = True
            idle_polls = 0
            continue
        idle_polls += 1
        # Done once the session left the active list (or never entered it and
        # stays idle a few polls: a very fast turn).
        if (seen_busy and idle_polls >= 1) or idle_polls >= 4:
            break
    else:
        return "", "timeout after %ds" % TASK_TIMEOUT_S, time.time() - t0
    msgs = srv.get("/api/session/%s/message?order=desc&limit=20" % sid).get("data", [])
    msgs = [m for m in msgs if m.get("type") == "assistant"]
    msgs.sort(key=lambda m: (m.get("time") or {}).get("created", 0))
    reply, error = "", None
    if msgs:
        last = msgs[-1]
        reply = "\n".join(c.get("text", "") for c in last.get("content", []) if c.get("type") == "text")
        if last.get("error"):
            error = last["error"].get("message") or last["error"].get("type")
    return reply, error, time.time() - t0


def check_write(work, nonce):
    path = os.path.join(work, "selftest", "hello.txt")
    if not os.path.exists(path):
        return False, "selftest/hello.txt was not created"
    content = open(path).read().strip()
    return content == "HELLO-%s" % nonce, "content: %r" % content[:80]


def check_edit(work, nonce):
    path = os.path.join(work, "selftest", "hello.txt")
    content = open(path).read().strip() if os.path.exists(path) else ""
    return content == "BYE-%s" % nonce, "content: %r" % content[:80]


BUGGY_CALC = '''def average(values):
    total = 0
    for v in values:
        total += v
    return total / (len(values) + 1)


if __name__ == "__main__":
    result = average([2, 4, 6])
    assert result == 4, "average is wrong: %r" % result
    print("OK", result)
'''


def check_debug(work):
    path = os.path.join(work, "selftest", "calc.py")
    r = subprocess.run([sys.executable, path], capture_output=True, text=True, timeout=30)
    return r.returncode == 0 and "OK" in r.stdout, (r.stdout + r.stderr).strip()[-120:]


def check_read(reply, secret):
    return secret in reply, "reply: %r" % reply.strip()[:80]


def test_model(model, variant, keep):
    provider, _, model_id = model.partition("/")
    work = tempfile.mkdtemp(prefix="oc-selftest-", dir=os.path.expanduser("~"))
    os.makedirs(os.path.join(work, "selftest"))
    nonce = "%06d" % random.randint(0, 999999)
    secret = "%08d" % random.randint(0, 99999999)
    with open(os.path.join(work, "selftest", "calc.py"), "w") as f:
        f.write(BUGGY_CALC)
    with open(os.path.join(work, "selftest", "secret.txt"), "w") as f:
        f.write("The secret number is %s.\n" % secret)
    results = []
    try:
        with Server(work) as srv:
            if model not in available_models(srv):
                return [("setup", False, "model %s is not available (missing API key?)" % model, 0)]
            sid = srv.post("/api/session", {"title": "agent self-test"})["data"]["id"]
            ref = {"providerID": provider, "id": model_id}
            if variant:
                ref["variant"] = variant
            srv.post("/api/session/%s/model" % sid, {"model": ref})
            tasks = [
                ("write", "Create the file selftest/hello.txt containing exactly the text "
                          "HELLO-%s (no other text, no quotes). Use your file tools." % nonce,
                 lambda reply: check_write(work, nonce)),
                ("edit", "In selftest/hello.txt replace HELLO with BYE, keeping the rest of "
                         "the line unchanged.",
                 lambda reply: check_edit(work, nonce)),
                ("debug", "Run `python3 selftest/calc.py`. It fails. Find the bug in "
                          "selftest/calc.py, fix it, and run the script again to confirm it "
                          "prints OK. Do not change the assert.",
                 lambda reply: check_debug(work)),
                ("read", "What is the secret number written in selftest/secret.txt? "
                         "Reply with just the number.",
                 lambda reply: check_read(reply, secret)),
            ]
            try:
                for name, prompt, check in tasks:
                    try:
                        reply, error, secs = run_turn(srv, sid, prompt)
                        ok, detail = check(reply)
                        if error and not ok:
                            detail = "error: %s; %s" % (error, detail)
                    except Exception as e:  # report and go on with the next task
                        ok, detail, secs = False, "exception: %s" % e, 0
                    results.append((name, ok, detail, secs))
            finally:
                # The test session lives in the user's real opencode database
                # (that is where the keys are). Remove it, or the app may later
                # resume a session whose scratch directory no longer exists.
                if not keep:
                    try:
                        srv._req("DELETE", "/api/session/%s" % sid)
                    except Exception as e:
                        print("  warning: could not delete test session %s: %s" % (sid, e))
    except Exception as e:
        results.append(("setup", False, "exception: %s" % e, 0))
    finally:
        if not keep:
            shutil.rmtree(work, ignore_errors=True)
        else:
            print("  work dir kept:", work)
    return results


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("models", nargs="*", default=DEFAULT_MODELS, help="provider/model ids")
    ap.add_argument("--variant", default="", help="reasoning variant (low, high, …)")
    ap.add_argument("--keep", action="store_true", help="keep the scratch work directories")
    args = ap.parse_args()
    if not os.path.exists(OPENCODE_BIN):
        print("opencode binary not found:", OPENCODE_BIN)
        return 2
    all_ok = True
    for model in args.models:
        print("== %s%s" % (model, " (variant %s)" % args.variant if args.variant else ""))
        for name, ok, detail, secs in test_model(model, args.variant, args.keep):
            all_ok = all_ok and ok
            print("  %-6s %s  %5.1fs  %s" % (name, "PASS" if ok else "FAIL", secs, detail))
    print("RESULT:", "ALL PASSED" if all_ok else "FAILURES")
    return 0 if all_ok else 1


if __name__ == "__main__":
    sys.exit(main())
