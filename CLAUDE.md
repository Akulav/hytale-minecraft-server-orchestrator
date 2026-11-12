# CLAUDE.md – Hyve

Hyve (made by **Akulav**) runs and manages Hytale and Minecraft (Java Edition: vanilla, Fabric, Quilt, Forge,
NeoForge) dedicated servers on **Windows x64, Linux x64 and macOS (Apple Silicon)**. Pure Rust: tokio + axum
backend, Dioxus 0.7 **LiveView** UI (components rendered on the server, shown over a WebSocket), a wry/tao desktop
window on Windows/macOS (a Chromium-family `--app` browser window on Linux) that just displays that UI, and a
headless mode that serves it as a web panel. Dev machine: Windows 11. It was called **HyOrchestrator** before 0.4 (the GitHub repo is still
`Akulav/hytale-server-orchestrator`); see "Rename" below before touching names, folders or release files.

## Always: bump the version, prove it works, commit
The owner wants every change set to ship as a release. For **every** change (feature, fix, cleanup):
1. Note it under `## [Unreleased]` in `CHANGELOG.md`, then run `scripts\bump-version.ps1` with `patch`
   (fixes / cleanup), `minor` (anything user-visible added) or `major` (only when asked). It updates `Cargo.toml`,
   `Cargo.lock`, `installer/hyve.iss`, `installer/Info.plist` and turns `[Unreleased]` into the version.
   Not optional, don't ask.
2. Verify before saying it is done, and report each result honestly (paste failures):
   - `cargo clippy --all-targets -- -D warnings` **and** `cargo clippy --all-targets --no-default-features -- -D warnings`
   - `cargo test`
   - UI changes: run `target\debug\hyve.exe --headless --data-dir <scratch> --port 7600 --host 127.0.0.1`,
     open the printed `?token=` link in the browser pane and exercise the change. A fake `HytaleServer.jar`
     (a tiny Java main that prints `Hytale Server Booted!`, `Adding player 'X'`, echoes stdin and exits on `/stop`)
     is enough to test start/stop/console/players/backups without real Hytale files.
   - `installer/` changed: compile it (`%LOCALAPPDATA%\Programs\Inno Setup 6\ISCC.exe installer\hyve.iss`).
   - workflow changed: the YAML must parse.
3. Commit as `Release vX.Y.Z: <short summary>` and give the user the ready-to-run git commands. Pushing (which
   publishes the release) stays the user's call; they push from GitHub Desktop.

## Build & run
- `cargo build` (default feature `desktop` = native window on Windows/macOS; `cfg(native_window)` from build.rs) /
  `cargo build --no-default-features` (no native window: desktop mode uses `browser.rs` like Linux always does).
  `run.bat` builds release + setup into `dist\` and launches.
- git is not on PATH: use GitHub Desktop's `%LOCALAPPDATA%\GitHubDesktop\app-*\resources\app\git\cmd\git.exe`.
- `assets/` (style.css, app.js, logo.svg, icon.png) are **embedded at compile time**: rebuild after editing them.
- Icons: `scripts\make-icons.ps1` draws `assets/icon.png`, `icon-1024.png`, `icon.ico` (System.Drawing).
- Linux never links GTK/WebKit (wry/tao are `cfg(not(target_os = "linux"))` deps): one Linux binary for desktops
  and servers. Linux/macOS-only code (`cfg(target_os = "linux")` in process.rs / selfinstall.rs, `cfg(unix)` in
  updater.rs / selfinstall.rs) can't be built here (no cross C compiler for `ring`): keep it small, review it by hand,
  and let CI check it.

## Releases
- `.github/workflows/build.yml`: `checks` (versions agree incl. CHANGELOG section, clippy desktop + headless, tests,
  setup compiles) on ubuntu-22.04 / windows / macos → on a push to `main` whose version has no release: build
  Windows (setup), Linux (one binary, checked to not link GTK/WebKit), macOS arm64 (the binary, ad-hoc signed) →
  `SHA256SUMS.txt` → tag `vX.Y.Z` + release (notes = the version's CHANGELOG section). **Never create `v*` tags by hand.**
- **Exactly 3 binaries per release, one per OS, the same file for install and update** (owner's rule, like
  fast-file-recovery): `Hyve-setup.exe`, `hyve-linux-x64`, `hyve-macos-arm64`, plus `SHA256SUMS.txt`. Installed
  copies read these names (`updater::asset_name`): keep them. No .dmg, no `-headless` build, no legacy-name copies
  (dropped in 0.5; pre-0.4 HyOrchestrator and 0.4 `-headless` copies update by hand once).
- Linux/macOS install themselves (`selfinstall.rs`): run from outside the install, desktop mode shows a dashboard
  banner with an **install folder** field (Install / Update an older install / Just run it); `--install
  [--install-dir <folder>]`, `--uninstall [--delete-data]` in a terminal. The folder is remembered in
  `<config dir>/Hyve/install-dir`. Linux: `<folder>/hyve` (default `~/.local/share/Hyve`) + `~/.local/bin/hyve` link
  + `.desktop` + hicolor icon. macOS: builds `<folder>/Hyve.app` (default `~/Applications/Hyve`; Info.plist from
  `installer/Info.plist` with the version, icns from the PNGs, quarantine removed, ad-hoc signed); a pre-0.6.1
  `~/Applications/Hyve.app` and its `~/Library/Application Support/Hyve/data` move into the folder. Installed data =
  `<folder>/data` (owner's rule, like akulav-mc-launcher: **everything in the install folder**); data next to a
  "just run" copy moves along.
- Rename (0.4, HyOrchestrator → Hyve): the setup keeps the AppId, removes `hyorchestrator.exe` + old shortcuts
  (`[InstallDelete]`) and stops either process. `Paths::user_root` keeps using an existing `…/HyOrchestrator/data`
  (never moved). The web cookie stays `hyo_token`.
- Inno Setup (`installer/hyve.iss`): per user, folder page + desktop-icon task, `UsePreviousAppDir` so
  updates find the old folder. **Never change the AppId GUID.** Update mode: `/SILENT … /LAUNCH=1 /DIR=<dir>`.

## Layout
```
src/
  main.rs       CLI (--headless/--desktop/--port/--host/--data-dir/--print-url), mode detection, log file, start-up, headless banner
  state.rs      AppState (Paths, Settings, InstanceConfig registry, per-instance Runtime, tasks, host stats), JSON persistence
  web.rs        axum router: token auth (cookie / ?token), LiveView WebSocket, assets, mod uploads, /api/status
  desktop.rs    tao + wry window on http://127.0.0.1:<port> (Windows/macOS only); close = stop servers + quit; macOS menu (muda)
  browser.rs    Linux desktop mode: Chrome/Chromium/Edge/Brave --app window on its own profile (closing it quits), else default browser
  selfinstall.rs  Linux/macOS self-install / update of an installed copy / --install / --uninstall
  ui/           Dioxus components: mod.rs (App, sidebar, toasts, run_job), dashboard, instance (tabs), minecraft (worlds, properties, software), create, versions, settings (+About), widgets
  process.rs    java command line (Hytale / Minecraft launch), spawn (std Command → tokio), console pump + parsing (players, auth link, ready), stop/kill/restart, op
  minecraft.rs  Minecraft: catalog (Mojang/Fabric/Quilt/Forge/NeoForge metadata), install per loader, launch args (run.sh/run.bat parsing), worlds, backup items, EULA
  mcprops.rs    server.properties read/write in place (Java escapes) + the editor's field schema
  instance.rs   create/delete, version files copy, worlds (seed configs), mods
  versions.rs   version cache, Hytale Downloader CLI, launcher import, zip/folder import
  backup.rs     world + mods zips, restore (safety backup first)
  actions.rs    background tasks: 1-click update, version switch, backup/restore, downloads, Java install
  java.rs       Java 25 detection, Temurin 25 install (Adoptium API)
  account.rs    Hytale account: device-code sign-in once (client hytale-server), refresh-token rotation (saved
                under a lock), game session per server start → HYTALE_SERVER_SESSION/IDENTITY_TOKEN + --owner-uuid
  login.rs      parses sign-in prompts (downloader + server console) for the LoginCard; auto-open in desktop mode
  net.rs        LAN IP, public IP (api.ipify.org, every 30 min) for the Connect panel's Copy buttons
  telemetry.rs  2 s sampler (sysinfo): machine + per-server CPU/RAM/players history
  updater.rs    GitHub releases check, SHA-256 verified install, Windows setup / Unix swap + relaunch
  archive.rs    zip extract/pack, tree copy; download.rs HTTP download with task progress
assets/         style.css (dark glass theme), app.js (drag & drop uploads, connection hint), logo.svg, icons
installer/      hyve.iss, Info.plist
scripts/        bump-version.ps1, make-icons.ps1
```

## Hytale facts (from the official Hytale Server Manual)
- Java 25. Files: `Server/HytaleServer.jar` (+ `HytaleServer.aot`) and `Assets.zip` (~3.3 GB). Launch:
  `java -XX:AOTCache=HytaleServer.aot -jar HytaleServer.jar --assets Assets.zip --bind 0.0.0.0:5520`.
- QUIC over **UDP**, default port 5520. `--auth-mode offline`, `--disable-sentry`, `--backup*` options exist.
- Downloader CLI (Windows/Linux only): `https://downloader.hytale.com/hytale-downloader.zip`, flags `-patchline`,
  `-print-version`, `-download-path`, `-skip-update-check`; credentials kept next to it (`data/tools/hytale-downloader`).
- First server start needs `/auth login device` (link `accounts.hytale.com/device?user_code=…`).
- Real downloader output (checked 2026-11): `Please visit the following URL to authenticate:` /
  `https://oauth.accounts.hytale.com/oauth2/device/verify?user_code=XXXX` / `Authorization code: XXXX`, then it
  continues by itself; credentials in `.hytale-downloader-credentials.json` next to it. Archive contains
  `hytale-downloader-linux-amd64`, `hytale-downloader-windows-amd64.exe`, `QUICKSTART.md`.
- Sign-in prompts are parsed by `login.rs` (downloader + server): `LoginCard` in the UI, auto-open in desktop mode,
  servers auto-start `/auth login device` once per run when they report being unauthenticated and send
  `/auth persistence Encrypted` after success. Output is read with `process::read_segments` (splits on `\r` too).
- Working dir holds `universe/worlds/<name>/config.json` (`"Seed"`), `mods/`, `config.json`, `permissions.json`,
  `whitelist.json`, `bans.json`. Config files are rewritten by a running server: edit them only while stopped.
- Console commands take a leading `/` (`/op add <name>`, `/op remove`, `/stop`). We set `HYTALE_DISABLE_UPDATES=1`.
- Player join/leave lines are matched by heuristics (`process.rs` `JOIN_PATTERNS`); users can override in Settings.

## Minecraft facts (checked 2026-11)
- Instances carry `game` (`hytale` default for old configs), `loader`, `loader_version`, `java_major` (Mojang's
  `javaVersion`), `launch` (args after the JVM options, set by the install task). TCP port, default 25565.
- Mojang: `piston-meta.mojang.com/mc/game/version_manifest_v2.json` (latest release 26.3 - year-based versions since
  2026). Fabric: `meta.fabricmc.net/v2` (server launcher jar URL; only the newest loader is flagged stable). Quilt:
  `meta.quiltmc.org/v3` (per-game lists are unordered: sort) + installer `install server <mc> <loader>
  --install-dir=<dir>` → `quilt-server-launch.jar`. Forge: `promotions_slim.json` + installer `--installServer`.
  NeoForge: maven versions API; `21.1.x` → MC 1.21.1, `26.3.0.x` → 26.3; installer `--installServer`. Forge/NeoForge
  start with the `@libraries/…/{win,unix}_args.txt` from their run.bat/run.sh.
- Java: Forge/NeoForge and versions needing < 17 get exactly their Java (Temurin in `data/java-<n>`); vanilla /
  Fabric / Quilt run on any Java ≥ their requirement. Console commands have no leading `/`; stop = `stop`, save =
  `save-all flush`. Backups skip `session.lock` (locked while running on Windows).
- Tested for real (2026-11-08): vanilla 26.3 catalog, Fabric 0.20.1 / 1.21.4, Quilt / 1.21.1, NeoForge 21.1.265,
  Forge 47.4.28 / 1.20.1 all install, start ("Done (…)!" → Running), stop and back up.

## Conventions & gotchas
- Data folder (`Paths::default_root`): `data/` in the install folder (next to the program; on macOS next to
  `Hyve.app`); per-user folder only for builds from source (`…/target/…`), a pre-0.6.1 `Applications/Hyve.app`, or a
  read-only program folder. Old per-user data is moved only by an installed copy (uninstaller next to it, or the
  remembered Linux/macOS install folder), across drives too (`state::move_tree` copies when a rename can't).
  Temporary files go to `data/tmp`, never the system temp folder. Never test that move against the real `%LOCALAPPDATA%\HyOrchestrator\data` or `%LOCALAPPDATA%\Hyve\data`.
- Server sign-in order: saved Hytale account (token passthrough, no prompt) → else the server's own device prompt
  (auto-started, `/auth persistence Encrypted` after success). Tokens go in env vars, never in logged arguments.
- Never hold a `parking_lot` guard across `.await`.
- Server processes end with Hyve even on a crash: Windows job object (`process::job`, kill-on-close),
  Linux `PR_SET_PDEATHSIG`. A normal quit / window close stops them cleanly with `/stop` first (`stop_all`).
- UI sessions run on dioxus-liveview's own local runtimes: long work goes to the main runtime (`state.rt`, `run_job`,
  `spawn_task`). Call `state.notify()` after changing anything the UI shows.
- LiveView appends the app next to the `.boot` placeholder in `#main` (CSS hides it). `app.js` must load **before**
  the LiveView glue (it wraps `WebSocket` for the connection hint).
- Don't reuse the `.input` CSS class for non-inputs (log lines use `ln cmd`).
- Branding: "Made by Akulav" in the sidebar, About, headless banner, exe/setup version info, Info.plist.
- Windows release builds use the GUI subsystem; `attach_console()` makes `--headless/--version/--help` print.
