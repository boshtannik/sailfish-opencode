# OpenRepos publishing kit — harbour-opencode

Everything needed to publish **mason / harbour-opencode** on
[openrepos.net](https://openrepos.net). Copy the fields below into the
"Create application" form, then attach the RPM and the screenshots.

---

## Title

```
OpenCode
```

## Package name

```
harbour-opencode
```

## Summary (one line)

```
Run the opencode AI coding agent natively on your Sailfish phone.
```

## Category

`Development` (fallback: `Utility`)

## License

`MIT`

## Source / homepage

```
https://github.com/boshtannik/sailfish-opencode
```

## Description

```html
<p><strong>OpenCode</strong> brings the <a href="https://opencode.ai">opencode</a>
AI coding agent to Sailfish OS — as a native app, in your pocket, not in a browser tab.</p>

<p>Your phone becomes a real coding workspace. Create sessions, chat with the agent,
watch its answers stream in live, review and approve every tool call, and let it read
and edit files, run shell commands and use git — all directly on the device.</p>

<h3>Why you'll want it</h3>
<ul>
  <li><strong>Native Sailfish UI.</strong> Rust backend + Silica QML. Fast, smooth,
      built for the platform — not a wrapped website.</li>
  <li><strong>The agent lives on your phone.</strong> The thinking is in the cloud,
      but the work happens locally: files, shell and git are executed on the device,
      against your own filesystem.</li>
  <li><strong>Nothing runs silently.</strong> Every tool call is surfaced as a
      permission request, so you stay the one who decides what actually runs.</li>
  <li><strong>Talk to it.</strong> Built-in voice input (local whisper.cpp) and
      voice output (local piper) let you dictate prompts and have answers read back —
      hands-free.</li>
  <li><strong>Your data, your phone.</strong> Works with your own opencode account/API
      key; no third-party middleman.</li>
  <li><strong>Speaks your language.</strong> Fully localized: English, Русский, Suomi,
      Deutsch, Українська.</li>
  <li><strong>Zero setup.</strong> The opencode engine is bundled right into the app —
      just install and go.</li>
</ul>

<h3>Good for</h3>
<ul>
  <li>Quick edits, review and small fixes when you're away from your laptop.</li>
  <li>Brainstorming and planning with an AI that can actually touch your files.</li>
  <li>Learning what an agentic coding tool feels like, right in your pocket.</li>
</ul>

<p>Independent open-source project (MIT). Not affiliated with Jolla Ltd. or with the
opencode project — it is a community client that speaks the opencode server protocol.</p>

<p><strong>Compatibility (for now):</strong> aarch64 devices only, built and tested on
<b>Sailfish OS 5.1</b> (Sony Xperia 10 IV). Other architectures are not built yet.</p>
```

## Changelog (0.2.77)

```
Second release: switched to the opencode v2 protocol (bundled engine 2.1.4).
- Account sign-in: "Sign in with OpenCode" device flow over the v2 integration API
  (browser + one-time code), no CLI dependency.
- Composer state now driven by server events (generation / retry / reconnect),
  no local timeouts.
- Engine updated to opencode 2.1.4 (v2 HTTP + SSE protocol, archive events,
  slimmer compaction payloads).
- Fixes: rename session, forms/permissions over v2, i18n contexts.
```

## Changelog (0.1.0)

```
First public release.
- Native opencode session flow over HTTP + SSE: create sessions, chat, streaming answers.
- Permission flow — every tool call approved by the user.
- Markdown rendering of assistant replies.
- Voice: local speech-to-text (whisper.cpp) and text-to-speech (piper).
- Localizations: English, Russian, Finnish, German, Ukrainian.
- Bundled arm64 opencode engine — no separate installation required.
```

## Attachments to upload

- `harbour-opencode-0.2.77-1.aarch64.rpm` (release build — from `mbuild.sh --release`)
- `screenshots/photo_*_2026-10-18_01-46-12.jpg` (4 screenshots, English UI)
- `harbour-opencode.png` (86×86 icon)

> OpenRepos account nick: **bosthannik**.
> Publisher repo (once the app page exists):
> `zypper ar -f https://sailfish.openrepos.net/bosthannik/personal-main.repo`

---

## Release checklist

1. [ ] Fix spec `URL:` → `https://github.com/boshtannik/sailfish-opencode` (done)
2. [ ] Bump `Version` → `0.1.0` in `rpm/harbour-opencode.spec` (done)
2a. [x] Bump `Version` → **`0.2.77`** and rebuild `--release` (2026-10-27)
3. [ ] **Rename the GitHub repo** `boshtannik/mason` → `boshtannik/sailfish-opencode`
4. [ ] Build the release RPM: `sg docker -c "./mbuild.sh --release"`
5. [ ] Create the OpenRepos account and the app page (this file)
6. [ ] Attach RPM + screenshots + icon, publish

> **Security note:** the phone password `091772` still exists in this repo's
> **git history** (`25a986e`, `dea769f`, `261b0c4`, `8c97d21`). It is not in the
> working tree, but anyone can recover it with `git log -S`. Consider rotating
> the phone password and/or scrubbing history before wide publication.
