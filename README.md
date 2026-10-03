# Substrate Census

A one-window desktop app for the [Substrate AI Software Census](https://gosubstrate.com/census/). Press one button,
watch one progress bar, get your score and a link to your dashboard. No terminal.

It is for people who would rather not paste `curl ... | bash` into a terminal. It runs **the same public scanner**
the census page hands out, fetched live from gosubstrate.com on every run, so the app and the one-liner can never
disagree.

| | |
|---|---|
| macOS (Apple Silicon and Intel) | `.dmg` |
| Windows | `-setup.exe` (per-user, no admin) or `.msi` |
| Linux | `.AppImage`, `.deb`, `.rpm` |

x64 and arm64 builds for Windows and Linux (the arm64 runners are experimental: a failed arm64 build never fails
the run). Downloads: [gosubstrate.com/census](https://gosubstrate.com/census/) once a release is mirrored there,
and [Releases](https://github.com/GoSubstrate/census-app/releases/latest).

## What it runs

| OS | Command (the census page's own, plus `--events`) |
|---|---|
| macOS | `curl -fsSL https://gosubstrate.com/census/substrate-census-osx.sh \| bash -s -- --events` |
| Linux | `curl -fsSL https://gosubstrate.com/census/substrate-census-linux.sh \| bash -s -- --events` |
| Windows | `& ([scriptblock]::Create((irm https://gosubstrate.com/census/substrate-census-windows.ps1))) --events` |

On macOS and Linux the command runs in your own login shell (`$SHELL -i -l -c`), so it sees the same PATH and
environment as a terminal; an app started from Finder or a launcher otherwise gets a bare PATH and would miss tools
installed by Homebrew, npm or in `~/.local/bin`. If the shell does not start the scanner within 60 seconds, the app
retries with plain `/bin/bash`.

What the scanner reads, and what it never reads, is written at the top of each script. Short version: it counts AI
tools and agent setup files, never opens your code, chats or personal folders, and sends one JSON summary.
`SUBSTRATE_DRY_RUN=1` prints that summary without sending it. The app itself reads, stores and sends nothing.

## The optional email

The start screen asks for an email, optional. With one, the scanner's `SUBSTRATE_EMAIL` and `SUBSTRATE_THEME` are set
in its environment (never on a command line), and gosubstrate.com emails the dashboard link and follows up. It is
remembered on this computer for the next run.

## While it runs

The space under the progress bar is used:

- **Scribe or Minutes missing** (and gosubstrate.com has a build for this computer): it cycles through the missing
  ones, each with what it does, **Free**, and **Install**. Install asks `gosubstrate.com/api/apps/<app>/latest` for
  this platform's installer, downloads it, refuses it unless the size and SHA-256 match what the site published,
  then installs it: the `.app` copied into `/Applications` (or `~/Applications`) on macOS, a silent `/S` run of the
  installer on Windows, `~/Applications/<Name>.AppImage` on Linux. Then it opens the app. Never asks for a password.
- **Both installed:** the Substrate Diagnostic, with a button to the inquiry form.

Detection only checks where the apps live (`src-tauri/src/companions.rs`); it never opens their data.
`cargo test --lib companions -- --ignored` runs a real Scribe install into a temp folder.

## The `--events` stream

With `--events` (scanner 1.33.0+) every UI step also writes one stderr line:

```
@@census {"ev":"phase","label":"detection","total":372,"detail":"0 found"}
@@census {"ev":"tick","label":"detection","cur":61,"total":372}
@@census {"ev":"done","label":"detection","summary":"24 AI tools detected across 10 categories"}
@@census {"ev":"result","score":62,"band":"...","dashboard_url":"https://gosubstrate.com/...","headline":"..."}
```

Events: `start`, `phase` (`total` 0 = a step with no count), `tick`, `done`, `error`, `result`. The Rust side
(`src-tauri/src/lib.rs`) forwards every stderr line; `src/progress.ts` maps the labels onto one bar.

## Updates

On launch the app asks `https://gosubstrate.com/api/apps/census/update/{{target}}/{{arch}}/{{current_version}}`
(Tauri updater, `src-tauri/src/updates.rs`). When a newer signed build is registered there, the start screen shows
**Update**; it never updates mid-scan. The plugin installs a bundle only if its minisign signature verifies against
the public key in `tauri.conf.json` (`plugins.updater.pubkey`). The private key is not in this repo.

CI makes updater bundles (`.app.tar.gz`, and `.sig` files for the `-setup.exe` and `.AppImage`) only when the repo
secrets `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` exist: the build then gets
`--config src-tauri/tauri.updater.conf.json`. Without them it builds exactly as before, installers only. A release
reaches gosubstrate.com through a mirror script in the Substrate repo, which verifies every `.sig` before upload.

## Build

```sh
bun install
bun test               # progress mapping (against a recorded run), promo, updater config
bun run tauri dev      # run it
bun run tauri build    # bundle for this OS
```

Tauri 2, Vite, TypeScript, no framework. Every push builds macOS (universal), Windows and Linux (x64 and arm64) in
GitHub Actions; pushing a `v*` tag also publishes a release.

## First launch

The builds are not signed with an Apple Developer ID or a Windows code-signing certificate yet.

- **macOS**: double-click shows "cannot be opened". Open **System Settings > Privacy & Security** and click
  **Open Anyway** (or right-click the app, **Open**). Once only.
- **Windows**: SmartScreen shows "Windows protected your PC". Click **More info**, then **Run anyway**.
