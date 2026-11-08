# Changelog

All notable changes to HyOrchestrator. Versions follow [Semantic Versioning](https://semver.org/); each pushed
version becomes a GitHub release with this file's section as its notes.

## [Unreleased]

## [0.5.0] - 2026-11-08
### Added
- **Minecraft (Java Edition) servers**, next to Hytale ones: New instance → **Minecraft**. Any release (or snapshot),
  as **vanilla, Fabric, Quilt, Forge or NeoForge** - the newest stable / recommended build, or the one you type.
  Hyve downloads Mojang's server (SHA-1 checked), runs the loader's installer, and installs the Java the version
  needs by itself (Temurin 8 / 17 / 21 / 25; Forge and NeoForge get exactly theirs). EULA acceptance is asked once.
- **Properties tab** (Minecraft): every `server.properties` setting as a switch, menu or number - online mode, game
  mode, difficulty, hardcore, PvP, whitelist, max players, MOTD, world type, seed, view / simulation distance, RCON,
  … - plus any other key; saved in place (comments and order kept). Online mode also shows on the Overview.
- Minecraft **Worlds** (switch the active world, generate a new one with a seed and world type, delete), **mods**
  (drag & drop for the loader; vanilla explains how to switch), **operators** (`op` / `deop`), console quick
  commands, **backups** (worlds, mods, config, player lists; `save-all flush` first), **1-click Update** (modded:
  newest loader build for the same version; vanilla: newest release) and **Settings → Server software** to switch
  version or loader with a backup first. Server versions lists the cached Mojang jars.
- Per-instance Java path (Settings), for Hytale and Minecraft.
- Linux and macOS **install themselves**: run the downloaded program and the dashboard offers to install it for your
  user (Linux: `~/.local/share/Hyve` with an app menu entry, icon and `~/.local/bin/hyve`; macOS:
  `~/Applications/Hyve.app`), or to update an older installed copy. `--install` and `--uninstall [--delete-data]`
  do the same from a terminal.

### Changed
- **Releases have exactly three programs**, one per system, each the same file for the first install and for updates:
  `Hyve-setup.exe`, `hyve-linux-x64`, `hyve-macos-arm64` (+ `SHA256SUMS.txt`). The macOS `.dmg`, the Linux
  `-headless` build and the copies under the pre-0.4 HyOrchestrator names are gone: those older copies need one
  manual download of the new file (it then updates itself as before).
- Linux no longer needs GTK / WebKit: one binary runs on desktops and servers. With a display the UI opens as a
  Chrome / Chromium / Edge / Brave **app window** (closing it quits Hyve), else in the default browser with a
  **Quit Hyve** button in the sidebar; without a display it runs headless as before.
- Backups skip lock files a running server holds (Minecraft's `session.lock`) and never leave a half-written zip.

## [0.4.0] - 2026-11-07
### Changed
- **HyOrchestrator is now Hyve.** New program and release file names (`hyve.exe`, `Hyve-setup.exe`,
  `hyve-linux-x64`, `hyve-macos-arm64`, `Hyve-macos-arm64.dmg`). Installed copies update into Hyve by themselves:
  releases also carry the files under their old names, the Windows setup keeps the same install (folder, data, Apps
  entry) and removes the old program and shortcuts. Existing data folders keep working where they are.

## [0.3.1] - 2026-11-07
### Changed
- Instance overview: the console preview is gone (the Console tab has it). Next to **Connect** there is now an
  **At a glance** panel - worlds, active mods, operators and auth mode - each tile opens its tab. A crashed server's
  banner has an **Open console** button.

## [0.3.0] - 2026-11-07
### Added
- **Hytale account - sign in once for every server** (Settings → Hytale account, or step 3 of Get started). Uses
  Hytale's official server token flow: each server start gets a fresh game session passed in through
  `HYTALE_SERVER_SESSION_TOKEN` / `HYTALE_SERVER_IDENTITY_TOKEN` and `--owner-uuid`, so servers no longer ask for a
  code when they start. The login refreshes itself; it only needs renewing after 30 days without HyOrchestrator.
- Connect panel: the **local network** and **internet** address (IP + port) of each server, each with a Copy button.
- Dashboard **Get started** strip: Java → server files → Hytale account → your server, with the next action on each step.

### Changed
- Data (instances, worlds, versions, backups) now lives in a `data` folder **next to the program**, i.e. in the
  folder chosen at install time. Data from older versions (`%LOCALAPPDATA%\HyOrchestrator\data`) is moved there on
  the first start of an installed copy when both are on the same drive; `--data-dir` still overrides it.
- Refreshed design: one spacing scale and control height, stat cards with a fixed footer (chart or meter) in a
  strict 4 / 2 / 1 column grid, equal-height panels, an instance header with status avatar, chips and one action
  bar, full-width equal tabs, symmetric instance cards (Start / Manage), identical source cards on Server versions,
  and two-column forms. Sidebar: a "+" next to Instances.

## [0.2.0] - 2026-11-07
### Added
- Effortless Hytale sign-in: when the official downloader or a server needs your Hytale account, a
  "Sign in to Hytale" card shows the code with **Open sign-in page** (the link already carries the code, nothing
  to type) and **Copy code**; the desktop app opens the page in your browser by itself. The downloader then
  continues on its own.
- Servers that start unauthenticated begin the device sign-in automatically and save the login after it
  succeeds (`/auth persistence Encrypted`), so restarts stay signed in. "Sign in again" starts a new one.
- Server versions shows whether the downloader is signed in, with **Sign out**.
- The downloader's progress fills the task's progress bar; the downloader refreshes itself weekly.

### Fixed
- Linux / macOS: mod and backup file names written with `\` (from a Windows browser) were not cut down to the
  file name - this also made the CI tests fail on Linux and macOS.
- The Hytale downloader is ended with HyOrchestrator if it is closed while the downloader waits for a sign-in.
- CI: actions/checkout v5 (Node 24).

## [0.1.0] - 2026-11-07
### Added
- First release of HyOrchestrator, a pure-Rust Hytale server orchestrator. Made by Akulav.
- Desktop window (wry/tao) and a headless mode that serves the same Dioxus LiveView UI on a local web
  server, picked automatically when there is no display; access links carry a secret token.
- Instances: create any number of isolated servers with their own version, UDP port, RAM limits, auth mode,
  JVM / server arguments and auto-restart after crashes.
- Server versions: download the latest release / pre-release with the official Hytale Downloader CLI, import
  from the Hytale launcher or from a zip / folder; switch any instance to any cached version.
- Worlds: generate any number of worlds with a chosen or random seed (numbers or words), toggle PvP, delete.
- Players: online list read from the console, make / remove operators, kick.
- Mods: drag & drop .jar / .zip mods onto an instance, enable / disable, remove.
- Telemetry: machine CPU / RAM, and per server CPU, RAM, players and uptime with live charts.
- Console: live output with filter, command input, clickable Hytale device-login links and a login helper.
- 1-click update of an instance (backup → latest version → swap → restart) and full "world + mods" backups
  with restore (a safety backup is made first).
- Servers are stopped cleanly when HyOrchestrator quits, and never left orphaned if it crashes.
- Java 25 detection and one-click install of Eclipse Temurin 25.
- In-app updater from GitHub releases (SHA-256 verified): Windows runs the setup silently on the install
  folder, Linux / macOS swap the program in place.
- Windows setup (Inno Setup): choose the folder, optional desktop icon, updates find the previous install.
- GitHub Actions: checks on Windows / Linux / macOS, release builds for Windows x64, Linux x64 (desktop and
  headless) and macOS Apple Silicon.
