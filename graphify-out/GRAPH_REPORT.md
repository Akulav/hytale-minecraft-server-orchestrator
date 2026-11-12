# Graph Report - hytale-minecraft-server-orchestrator  (2026-11-13)

## Corpus Check
- 38 files · ~51,339 words
- Verdict: corpus is large enough that graph structure adds value.
- Unclassified: 8 file(s) not represented in the graph (top: (none) 3, .ico 1, .css 1)

## Summary
- 926 nodes · 2160 edges · 53 communities (40 shown, 13 thin omitted)
- Extraction: 92% EXTRACTED · 8% INFERRED · 0% AMBIGUOUS · INFERRED: 183 edges (avg confidence: 0.84)
- Token cost: 0 input · 0 output

## Community Hubs (Navigation)
- Shared
- Minecraft Install & Downloads
- Docs: Changelog & Rules
- process.rs
- Telemetry Sampler
- Instances, Worlds & Mods
- Web Server & Auth
- server.properties Editor
- Backups & Zip Archives
- Java & Browser Launch
- Hytale Account Tokens
- Linux/macOS Self-Install
- CLI & Startup
- install
- State Constants & Formatting
- AppState Core
- ui/instance.rs
- Desktop Window (wry/tao)
- InstanceConfig
- Game
- classify
- Release Workflow Steps
- run_downloader
- Console Log Runtime
- CI Workflow Triggers
- Artifact Upload Steps
- Server Status
- Data Folder Paths
- Instance Tabs
- Drag-and-Drop Uploads (app.js)
- CI Check Matrix
- Release Prepare Job
- Logo SVG Design
- Linux Build Job
- macOS Build Job
- Windows Build Job
- Release Publish Job
- 1024px App Icon
- Icon Generator Script
- App Icon PNG
- IDs & Task Start
- Crate Root

## God Nodes (most connected - your core abstractions)
1. `Shared` - 153 edges
2. `AppState` - 37 edges
3. `use_state()` - 31 edges
4. `use_toast()` - 30 edges
5. `now_ms()` - 29 edges
6. `Loader` - 28 edges
7. `InstanceConfig` - 23 edges
8. `Page` - 22 edges
9. `spawn_task()` - 19 edges
10. `Game` - 18 edges

## Surprising Connections (you probably didn't know these)
- `McSoftware()` --references--> `version`  [INFERRED]
  src/ui/minecraft.rs → .github/workflows/build.yml
- `MinecraftForm()` --references--> `version`  [INFERRED]
  src/ui/create.rs → .github/workflows/build.yml
- `data_items()` --references--> `Shared`  [EXTRACTED]
  src/instance.rs → src/state.rs
- `create_minecraft()` --calls--> `now_ms()`  [INFERRED]
  src/instance.rs → src/state.rs
- `create()` --calls--> `now_ms()`  [INFERRED]
  src/instance.rs → src/state.rs

## Import Cycles
- None detected.

## Hyperedges (group relationships)
- **Logo composition: hexagon frame + H monogram + brand gradient** — assets_logo_hyve_logo, assets_logo_hexagon_frame, assets_logo_h_monogram, assets_logo_brand_gradient [EXTRACTED 1.00]
- **Data safety guards introduced in 0.5.1** — changelog_per_instance_task_guard, changelog_broken_config_preservation, changelog_download_stall_timeout, changelog_entry_name_validation [EXTRACTED 1.00]
- **Hytale sign-in flow (downloader, device code, sign-in card, saved account)** — claude_hytale_downloader_cli, claude_device_code_signin, changelog_sign_in_card, claude_server_signin_order, readme_hytale_account_file [INFERRED 0.85]
- **Release pipeline: changelog, version bump, CI build, three binaries, updater** — changelog_changelog, scripts_bump_version, claude_build_workflow, claude_three_binaries_rule, readme_in_app_updater [INFERRED 0.85]

## Communities (53 total, 13 thin omitted)

### Community 0 - "Shared"
Cohesion: 0.22
Nodes (24): backup_now(), busy(), change_minecraft(), change_version(), download_version(), import_launcher(), install_java(), install_java_major() (+16 more)

### Community 1 - "Minecraft Install & Downloads"
Cohesion: 0.06
Nodes (53): NewMinecraft, accept_eula(), Catalog, create_world(), data_items(), detect_launch(), detects_launch_files_for_this_system(), ensure_catalog() (+45 more)

### Community 2 - "Docs: Changelog & Rules"
Cohesion: 0.06
Nodes (45): Hyve Changelog, Connect panel (LAN / internet address Copy buttons), Dashboard Get started strip, Semantic Versioning, Sign in to Hytale card (LoginCard), Release 0.1.0 (first HyOrchestrator release), Release 0.2.0 (effortless Hytale sign-in card), Release 0.3.0 (Hytale account, Connect panel, data next to program) (+37 more)

### Community 3 - "process.rs"
Cohesion: 0.09
Nodes (19): Follow, game_command(), Job, job(), JOIN_PATTERNS, kill(), LEAVE_PATTERNS, MAX_CRASHES (+11 more)

### Community 4 - "Telemetry Sampler"
Cohesion: 0.07
Nodes (36): lan_ip(), parse_public_ip(), public_ip_is_validated(), spawn_public_ip(), HISTORY_CAP, sample(), spawn(), app_bundle() (+28 more)

### Community 5 - "Instances, Worlds & Mods"
Cohesion: 0.11
Nodes (28): add_mod(), base_name(), create_minecraft(), data_items(), DATA_ITEMS, delete(), delete_mod(), delete_world() (+20 more)

### Community 6 - "Web Server & Auth"
Cohesion: 0.08
Nodes (18): Ctx, APP_JS, auth(), COOKIE, cookie_token(), index(), locked_page(), LOGO (+10 more)

### Community 7 - "server.properties Editor"
Cohesion: 0.10
Nodes (23): escape(), f(), Field, field(), GROUPS, Kind, Bool, Choice (+15 more)

### Community 8 - "Backups & Zip Archives"
Cohesion: 0.06
Nodes (37): add_file(), copy_tree(), read_entry(), skip_in_backup(), walk_files(), zip_into(), zip_paths(), zip_roundtrip_keeps_tree_and_skips_missing() (+29 more)

### Community 9 - "Java & Browser Launch"
Cohesion: 0.13
Nodes (21): candidates(), open(), adoptium_os_arch(), candidates(), detect(), exact_major(), exe_name(), find_java_in() (+13 more)

### Community 10 - "Hytale Account Tokens"
Cohesion: 0.08
Nodes (39): Access, access_token(), Account, cache(), CLIENT_ID, file(), get_profiles(), load() (+31 more)

### Community 11 - "Linux/macOS Self-Install"
Cohesion: 0.14
Nodes (35): builds_the_mac_app_files(), check(), checks_typed_folders(), default_folder(), desktop_entry(), exe_in(), forget(), icns() (+27 more)

### Community 12 - "CLI & Startup"
Cohesion: 0.13
Nodes (19): Args, attach_console(), banner(), has_display(), headless_flag_wins(), HELP, init_log(), LOG (+11 more)

### Community 13 - "install"
Cohesion: 0.15
Nodes (18): import_path(), extract_zip(), to_file(), write_body(), ensure_for_minecraft(), install_major(), managed_dir(), delete_cached_jar() (+10 more)

### Community 14 - "State Constants & Formatting"
Cohesion: 0.11
Nodes (15): APP_NAME, copy_tree(), DEFAULT_GAME_PORT, DEFAULT_MC_PORT, DEFAULT_WEB_PORT, formats_sizes_and_durations(), human_bytes(), human_duration() (+7 more)

### Community 15 - "AppState Core"
Cohesion: 0.15
Nodes (3): AppState, HostStats, Task

### Community 16 - "ui/instance.rs"
Cohesion: 0.05
Nodes (71): NewInstance, software_label(), suggest_port(), DIFFICULTIES, GAMEMODES, LEVEL_TYPES, cached_jars(), port_free() (+63 more)

### Community 17 - "Desktop Window (wry/tao)"
Cohesion: 0.14
Nodes (7): ICON_PNG, open_external(), run(), UserEvent, Menu, Quit, window_icon()

### Community 18 - "InstanceConfig"
Cohesion: 0.17
Nodes (14): create(), create_world(), install_version_files(), launch_args(), adopt(), builds_server_command_line(), hytale_launch(), Launch (+6 more)

### Community 19 - "Game"
Cohesion: 0.25
Nodes (5): port_shared_with(), save_command(), Game, Hytale, Minecraft

### Community 20 - "classify"
Cohesion: 0.20
Nodes (13): classify(), first_group(), LineEvent, AuthCode, AuthNeeded, AuthOk, AuthUrl, Joined (+5 more)

### Community 21 - "Release Workflow Steps"
Cohesion: 0.26
Nodes (15): steps, steps, steps, steps, GH_TOKEN, VERSION, steps, steps (+7 more)

### Community 22 - "run_downloader"
Cohesion: 0.22
Nodes (8): run_java(), read_segments(), strip_ansi(), strips_terminal_colours(), downloader_dir(), downloader_sign_out(), downloader_signed_in(), run_downloader()

### Community 24 - "Console Log Runtime"
Cohesion: 0.18
Nodes (9): LogKind, Err, Input, Out, System, LogLine, Runtime, Sample (+1 more)

### Community 25 - "CI Workflow Triggers"
Cohesion: 0.17
Nodes (11): env, CARGO_TERM_COLOR, jobs, name, on, pull_request, push, workflow_dispatch (+3 more)

### Community 26 - "Artifact Upload Steps"
Cohesion: 0.17
Nodes (12): with, body_path, components, fail_on_unmatched_files, files, if-no-files-found, merge-multiple, name (+4 more)

### Community 28 - "Server Status"
Cohesion: 0.20
Nodes (6): Status, Crashed, Running, Starting, Stopped, Stopping

### Community 29 - "Data Folder Paths"
Cohesion: 0.15
Nodes (4): data_lives_next_to_the_installed_program(), load_or_keep(), Paths, writable()

### Community 30 - "Instance Tabs"
Cohesion: 0.20
Nodes (10): Tab, Backups, Console, Mods, Overview, Players, Properties, Settings (+2 more)

### Community 32 - "Drag-and-Drop Uploads (app.js)"
Cohesion: 0.38
Nodes (4): done(), fallback(), status(), upload()

### Community 33 - "CI Check Matrix"
Cohesion: 0.29
Nodes (7): name, runs-on, strategy, checks, os, fail-fast, matrix

### Community 34 - "Release Prepare Job"
Cohesion: 0.33
Nodes (6): prepare, release, version, if, outputs, runs-on

### Community 35 - "Logo SVG Design"
Cohesion: 0.70
Nodes (5): Brand Gradient (violet #a78bfa/#7c3aed to cyan #22d3ee), H Monogram (stroked 'H' letterform with node dots, network/hive motif), Hexagon Frame (purple-to-cyan gradient outer hexagon with dark inner hexagon), Hyve Brand Identity, Hyve Logo (logo.svg)

### Community 36 - "Linux Build Job"
Cohesion: 0.40
Nodes (5): if, name, needs, runs-on, build-linux

### Community 37 - "macOS Build Job"
Cohesion: 0.40
Nodes (5): if, name, needs, runs-on, build-mac

### Community 38 - "Windows Build Job"
Cohesion: 0.40
Nodes (5): if, name, needs, runs-on, build-windows

### Community 39 - "Release Publish Job"
Cohesion: 0.40
Nodes (5): release, if, name, needs, runs-on

### Community 41 - "1024px App Icon"
Cohesion: 0.67
Nodes (4): Dark Theme Brand Palette (navy, violet, sky blue), Stylized 'H' Glyph with Connected Nodes, Purple-to-Blue Gradient Hexagon Frame, Hyve App Icon (1024px)

### Community 42 - "Icon Generator Script"
Cohesion: 0.83
Nodes (3): Draw(), Hex(), PngBytes()

### Community 44 - "App Icon PNG"
Cohesion: 1.00
Nodes (3): Hyve App Icon (icon.png), Hexagon H Logo Design (purple-blue gradient hexagon, light-blue H with node dots), Hyve Branding

## Knowledge Gaps
- **55 isolated node(s):** `hyve`, `AuthNeeded`, `AuthOk`, `Ready`, `Hytale` (+50 more)
  These have ≤1 connection - possible missing edges or undocumented components. (Counts symbols only; 250 node(s) total have ≤1 connection when file, concept and rationale nodes are included.)
- **13 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `Shared` connect `Shared` to `Minecraft Install & Downloads`, `process.rs`, `Telemetry Sampler`, `Instances, Worlds & Mods`, `Web Server & Auth`, `Backups & Zip Archives`, `Java & Browser Launch`, `Hytale Account Tokens`, `Linux/macOS Self-Install`, `CLI & Startup`, `install`, `State Constants & Formatting`, `ui/instance.rs`, `Desktop Window (wry/tao)`, `InstanceConfig`, `Game`, `classify`, `run_downloader`, `Data Folder Paths`?**
  _High betweenness centrality (0.321) - this node is a cross-community bridge._
- **Are the 2 inferred relationships involving `Shared` (e.g. with `install_task()` and `file_map()`) actually correct?**
  _`Shared` has 2 INFERRED edges - model-reasoned connections that need verification._
- **What connects `hyve`, `AuthNeeded`, `AuthOk` to the rest of the system?**
  _55 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Minecraft Install & Downloads` be split into smaller, more focused modules?**
  _Cohesion score 0.05721153846153846 - nodes in this community are weakly interconnected._
- **Why does `version` connect `Release Prepare Job` to `ui/instance.rs`?**
  _High betweenness centrality (0.126) - this node is a cross-community bridge._
- **Are the 3 inferred relationships involving `AppState` (e.g. with `Catalog` and `Offer`) actually correct?**
  _`AppState` has 3 INFERRED edges - model-reasoned connections that need verification._
- **Should `Docs: Changelog & Rules` be split into smaller, more focused modules?**
  _Cohesion score 0.056261343012704176 - nodes in this community are weakly interconnected._