# Changelog

All notable changes to HyOrchestrator. Versions follow [Semantic Versioning](https://semver.org/); each pushed
version becomes a GitHub release with this file's section as its notes.

## [Unreleased]

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
