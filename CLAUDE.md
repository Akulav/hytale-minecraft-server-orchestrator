# CLAUDE.md – HyOrchestrator

HyOrchestrator (made by **Akulav**) runs and manages Hytale dedicated servers on **Windows x64, Linux x64 and macOS
(Apple Silicon)**. Pure Rust: tokio + axum backend, Dioxus 0.7 **LiveView** UI (components rendered on the server,
shown over a WebSocket), a wry/tao desktop window that just displays that UI, and a headless mode that serves it
as a web panel. Dev machine: Windows 11.

## Always: bump the version, prove it works, commit
The owner wants every change set to ship as a release. For **every** change (feature, fix, cleanup):
1. Note it under `## [Unreleased]` in `CHANGELOG.md`, then run `scripts\bump-version.ps1` with `patch`
   (fixes / cleanup), `minor` (anything user-visible added) or `major` (only when asked). It updates `Cargo.toml`,
   `Cargo.lock`, `installer/hyorchestrator.iss`, `installer/Info.plist` and turns `[Unreleased]` into the version.
   Not optional, don't ask.
2. Verify before saying it is done, and report each result honestly (paste failures):
   - `cargo clippy --all-targets -- -D warnings` **and** `cargo clippy --all-targets --no-default-features -- -D warnings`
   - `cargo test`
   - UI changes: run `target\debug\hyorchestrator.exe --headless --data-dir <scratch> --port 7600 --host 127.0.0.1`,
     open the printed `?token=` link in the browser pane and exercise the change. A fake `HytaleServer.jar`
     (a tiny Java main that prints `Hytale Server Booted!`, `Adding player 'X'`, echoes stdin and exits on `/stop`)
     is enough to test start/stop/console/players/backups without real Hytale files.
   - `installer/` changed: compile it (`%LOCALAPPDATA%\Programs\Inno Setup 6\ISCC.exe installer\hyorchestrator.iss`).
   - workflow changed: the YAML must parse.
3. Commit as `Release vX.Y.Z: <short summary>` and give the user the ready-to-run git commands. Pushing (which
   publishes the release) stays the user's call; they push from GitHub Desktop.

## Build & run
- `cargo build` (desktop, default feature `desktop`) / `cargo build --no-default-features` (headless only, no GUI
  libs). `run.bat` builds release + setup into `dist\` and launches.
- git is not on PATH: use GitHub Desktop's `%LOCALAPPDATA%\GitHubDesktop\app-*\resources\app\git\cmd\git.exe`.
- `assets/` (style.css, app.js, logo.svg, icon.png) are **embedded at compile time**: rebuild after editing them.
- Icons: `scripts\make-icons.ps1` draws `assets/icon.png`, `icon-1024.png`, `icon.ico` (System.Drawing).
- Linux desktop deps: `libwebkit2gtk-4.1-dev libgtk-3-dev`. Linux/macOS-only code (`cfg(target_os = "linux")`
  in desktop.rs / process.rs, `cfg(unix)` in updater.rs) can't be built here: keep it small and let CI check it.

## Releases
- `.github/workflows/build.yml`: `checks` (versions agree incl. CHANGELOG section, clippy desktop + headless, tests,
  setup compiles) on ubuntu-22.04 / windows / macos → on a push to `main` whose version has no release: build
  Windows (setup), Linux (desktop + `-headless`), macOS arm64 (.app + .dmg, ad-hoc signed) → `SHA256SUMS.txt` →
  tag `vX.Y.Z` + release (notes = the version's CHANGELOG section). **Never create `v*` tags by hand.**
- Release file names are read by installed copies (`updater::asset_name`): `HyOrchestrator-setup.exe`,
  `hyorchestrator-linux-x64`, `hyorchestrator-linux-x64-headless`, `hyorchestrator-macos-arm64`,
  `HyOrchestrator-macos-arm64.dmg`, `SHA256SUMS.txt`. Keep them.
- Inno Setup (`installer/hyorchestrator.iss`): per user, folder page + desktop-icon task, `UsePreviousAppDir` so
  updates find the old folder. **Never change the AppId GUID.** Update mode: `/SILENT … /LAUNCH=1 /DIR=<dir>`.

## Layout
```
src/
  main.rs       CLI (--headless/--desktop/--port/--host/--data-dir/--print-url), mode detection, log file, start-up, headless banner
  state.rs      AppState (Paths, Settings, InstanceConfig registry, per-instance Runtime, tasks, host stats), JSON persistence
  web.rs        axum router: token auth (cookie / ?token), LiveView WebSocket, assets, mod uploads, /api/status
  desktop.rs    tao + wry window on http://127.0.0.1:<port>; close = stop servers + quit; macOS menu (muda)
  ui/           Dioxus components: mod.rs (App, sidebar, toasts, run_job), dashboard, instance (tabs), create, versions, settings (+About), widgets
  process.rs    java command line, spawn (std Command → tokio), console pump + parsing (players, auth link, ready), stop/kill/restart, /op
  instance.rs   create/delete, version files copy, worlds (seed configs), mods
  versions.rs   version cache, Hytale Downloader CLI, launcher import, zip/folder import
  backup.rs     world + mods zips, restore (safety backup first)
  actions.rs    background tasks: 1-click update, version switch, backup/restore, downloads, Java install
  java.rs       Java 25 detection, Temurin 25 install (Adoptium API)
  telemetry.rs  2 s sampler (sysinfo): machine + per-server CPU/RAM/players history
  updater.rs    GitHub releases check, SHA-256 verified install, Windows setup / Unix swap + relaunch
  archive.rs    zip extract/pack, tree copy; download.rs HTTP download with task progress
assets/         style.css (dark glass theme), app.js (drag & drop uploads, connection hint), logo.svg, icons
installer/      hyorchestrator.iss, Info.plist
scripts/        bump-version.ps1, make-icons.ps1
```

## Hytale facts (from the official Hytale Server Manual)
- Java 25. Files: `Server/HytaleServer.jar` (+ `HytaleServer.aot`) and `Assets.zip` (~3.3 GB). Launch:
  `java -XX:AOTCache=HytaleServer.aot -jar HytaleServer.jar --assets Assets.zip --bind 0.0.0.0:5520`.
- QUIC over **UDP**, default port 5520. `--auth-mode offline`, `--disable-sentry`, `--backup*` options exist.
- Downloader CLI (Windows/Linux only): `https://downloader.hytale.com/hytale-downloader.zip`, flags `-patchline`,
  `-print-version`, `-download-path`, `-skip-update-check`; credentials kept next to it (`data/tools/hytale-downloader`).
- First server start needs `/auth login device` (link `accounts.hytale.com/device?user_code=…`).
- Working dir holds `universe/worlds/<name>/config.json` (`"Seed"`), `mods/`, `config.json`, `permissions.json`,
  `whitelist.json`, `bans.json`. Config files are rewritten by a running server: edit them only while stopped.
- Console commands take a leading `/` (`/op add <name>`, `/op remove`, `/stop`). We set `HYTALE_DISABLE_UPDATES=1`.
- Player join/leave lines are matched by heuristics (`process.rs` `JOIN_PATTERNS`); users can override in Settings.

## Conventions & gotchas
- Never hold a `parking_lot` guard across `.await`.
- Server processes end with HyOrchestrator even on a crash: Windows job object (`process::job`, kill-on-close),
  Linux `PR_SET_PDEATHSIG`. A normal quit / window close stops them cleanly with `/stop` first (`stop_all`).
- UI sessions run on dioxus-liveview's own local runtimes: long work goes to the main runtime (`state.rt`, `run_job`,
  `spawn_task`). Call `state.notify()` after changing anything the UI shows.
- LiveView appends the app next to the `.boot` placeholder in `#main` (CSS hides it). `app.js` must load **before**
  the LiveView glue (it wraps `WebSocket` for the connection hint).
- Don't reuse the `.input` CSS class for non-inputs (log lines use `ln cmd`).
- Branding: "Made by Akulav" in the sidebar, About, headless banner, exe/setup version info, Info.plist.
- Windows release builds use the GUI subsystem; `attach_console()` makes `--headless/--version/--help` print.
