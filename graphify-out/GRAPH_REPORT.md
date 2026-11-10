# Graph Report - hytale-minecraft-server-orchestrator  (2026-11-10)

## Corpus Check
- 41 files · ~51,693 words
- Verdict: corpus is large enough that graph structure adds value.
- Unclassified: 8 file(s) not represented in the graph (top: (none) 3, .ico 1, .css 1)

## Summary
- 906 nodes · 2161 edges · 48 communities (45 shown, 3 thin omitted)
- Extraction: 92% EXTRACTED · 8% INFERRED · 0% AMBIGUOUS · INFERRED: 181 edges (avg confidence: 0.84)
- Token cost: 281,598 input · 0 output

## Community Hubs (Navigation)
- Background Tasks & Account
- Minecraft Install & Downloads
- Docs: Changelog & Rules
- Windows Job Object
- Telemetry Sampler
- Instances, Worlds & Mods
- Web Server & Auth
- server.properties Editor
- Backups & Zip Archives
- Java & Browser Launch
- Hytale Account Tokens
- Linux/macOS Self-Install
- CLI & Startup
- Sign-in Prompt Parsing
- State Constants & Formatting
- AppState Core
- Instance Page UI
- Desktop Window (wry/tao)
- UI Root & Toasts
- Games & Ports
- UI Widgets
- Release Workflow Steps
- Dashboard UI
- Create Instance Form
- Console Log Runtime
- CI Workflow Triggers
- Artifact Upload Steps
- Settings Persistence
- Server Status
- Data Folder Paths
- Instance Tabs
- Settings & About UI
- Drag-and-Drop Uploads (app.js)
- CI Check Matrix
- Release Prepare Job
- Logo SVG Design
- Linux Build Job
- macOS Build Job
- Windows Build Job
- Release Publish Job
- Versions Page UI
- 1024px App Icon
- Icon Generator Script
- Task Status
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
9. `main()` - 19 edges
10. `spawn_task()` - 19 edges

## Surprising Connections (you probably didn't know these)
- `MinecraftForm()` --references--> `version`  [INFERRED]
  src/ui/create.rs → .github/workflows/build.yml
- `McSoftware()` --references--> `version`  [INFERRED]
  src/ui/minecraft.rs → .github/workflows/build.yml
- `sign_in_inner()` --calls--> `now_ms()`  [INFERRED]
  src/account.rs → src/state.rs
- `access_token()` --calls--> `now_ms()`  [INFERRED]
  src/account.rs → src/state.rs
- `spawn_keepalive()` --calls--> `now_ms()`  [INFERRED]
  src/account.rs → src/state.rs

## Import Cycles
- None detected.

## Hyperedges (group relationships)
- **Release pipeline: changelog, version bump, CI build, three binaries, updater** — changelog_changelog, scripts_bump_version, claude_build_workflow, claude_three_binaries_rule, readme_in_app_updater [INFERRED 0.85]
- **Hytale sign-in flow (downloader, device code, sign-in card, saved account)** — claude_hytale_downloader_cli, claude_device_code_signin, changelog_sign_in_card, claude_server_signin_order, readme_hytale_account_file [INFERRED 0.85]
- **Data safety guards introduced in 0.5.1** — changelog_per_instance_task_guard, changelog_broken_config_preservation, changelog_download_stall_timeout, changelog_entry_name_validation [EXTRACTED 1.00]
- **Logo composition: hexagon frame + H monogram + brand gradient** — assets_logo_hyve_logo, assets_logo_hexagon_frame, assets_logo_h_monogram, assets_logo_brand_gradient [EXTRACTED 1.00]

## Communities (48 total, 3 thin omitted)

### Community 0 - "Background Tasks & Account"
Cohesion: 0.06
Nodes (76): get_profiles(), sign_in(), sign_in_inner(), backup_now(), busy(), change_minecraft(), change_version(), download_version() (+68 more)

### Community 1 - "Minecraft Install & Downloads"
Cohesion: 0.07
Nodes (52): to_file(), write_body(), data_items(), accept_eula(), Catalog, create_world(), data_items(), detect_launch() (+44 more)

### Community 2 - "Docs: Changelog & Rules"
Cohesion: 0.06
Nodes (45): Hyve Changelog, Connect panel (LAN / internet address Copy buttons), Dashboard Get started strip, Semantic Versioning, Sign in to Hytale card (LoginCard), Release 0.1.0 (first HyOrchestrator release), Release 0.2.0 (effortless Hytale sign-in card), Release 0.3.0 (Hytale account, Connect panel, data next to program) (+37 more)

### Community 3 - "Windows Job Object"
Cohesion: 0.07
Nodes (31): eula_accepted(), launch_args(), adopt(), builds_server_command_line(), classify(), first_group(), Follow, Job (+23 more)

### Community 4 - "Telemetry Sampler"
Cohesion: 0.09
Nodes (32): HISTORY_CAP, sample(), spawn(), app_bundle(), asset_name(), check(), CHECK_EVERY, cleanup() (+24 more)

### Community 5 - "Instances, Worlds & Mods"
Cohesion: 0.10
Nodes (37): add_mod(), base_name(), create(), create_minecraft(), create_world(), DATA_ITEMS, delete(), delete_mod() (+29 more)

### Community 6 - "Web Server & Auth"
Cohesion: 0.09
Nodes (17): APP_JS, auth(), COOKIE, cookie_token(), index(), locked_page(), LOGO, reads_cookie_and_checks_origin() (+9 more)

### Community 7 - "server.properties Editor"
Cohesion: 0.10
Nodes (24): escape(), f(), Field, field(), GROUPS, Kind, Bool, Choice (+16 more)

### Community 8 - "Backups & Zip Archives"
Cohesion: 0.10
Nodes (17): add_file(), copy_tree(), extract_zip(), read_entry(), skip_in_backup(), walk_files(), zip_into(), zip_paths() (+9 more)

### Community 9 - "Java & Browser Launch"
Cohesion: 0.13
Nodes (25): candidates(), open(), adoptium_os_arch(), candidates(), detect(), ensure_for_minecraft(), exact_major(), exe_name() (+17 more)

### Community 10 - "Hytale Account Tokens"
Cohesion: 0.12
Nodes (21): Access, access_token(), Account, cache(), CLIENT_ID, file(), load(), OAUTH (+13 more)

### Community 11 - "Linux/macOS Self-Install"
Cohesion: 0.15
Nodes (27): builds_the_mac_app_files(), check(), desktop_entry(), icns(), ICON_1024, ICON_256, INFO_PLIST, info_plist() (+19 more)

### Community 12 - "CLI & Startup"
Cohesion: 0.11
Nodes (20): spawn_keepalive(), Args, attach_console(), banner(), has_display(), headless_flag_wins(), HELP, init_log() (+12 more)

### Community 13 - "Sign-in Prompt Parsing"
Cohesion: 0.16
Nodes (16): Bit, Code, Progress, Url, is_prompt_text(), merge(), parse(), re() (+8 more)

### Community 14 - "State Constants & Formatting"
Cohesion: 0.11
Nodes (12): DEFAULT_GAME_PORT, DEFAULT_MC_PORT, DEFAULT_WEB_PORT, formats_sizes_and_durations(), human_bytes(), human_duration(), LOG_CAP, new_token() (+4 more)

### Community 15 - "AppState Core"
Cohesion: 0.13
Nodes (3): AppState, HostStats, Task

### Community 16 - "Instance Page UI"
Cohesion: 0.25
Nodes (16): list_worlds(), load_properties(), Backups(), Console(), InstancePage(), InstanceSettings(), Mods(), Overview() (+8 more)

### Community 17 - "Desktop Window (wry/tao)"
Cohesion: 0.14
Nodes (7): ICON_PNG, open_external(), run(), UserEvent, Menu, Quit, window_icon()

### Community 18 - "UI Root & Toasts"
Cohesion: 0.20
Nodes (8): App(), Ctx, Notice, run_job(), Sidebar(), TasksPage(), Toast, ToastView()

### Community 19 - "Games & Ports"
Cohesion: 0.17
Nodes (11): suggest_port(), port_free(), save_command(), udp_port_free(), Game, Hytale, Minecraft, CreatePage() (+3 more)

### Community 20 - "UI Widgets"
Cohesion: 0.28
Nodes (12): ConfirmButton(), Empty(), Icon(), icon_svg(), Linkified(), LoginCard(), Meter(), PanelHead() (+4 more)

### Community 21 - "Release Workflow Steps"
Cohesion: 0.26
Nodes (15): steps, steps, steps, steps, GH_TOKEN, VERSION, steps, steps (+7 more)

### Community 22 - "Dashboard UI"
Cohesion: 0.22
Nodes (11): Dashboard(), GetStarted(), InstanceCard(), Page, About, Create, Dashboard, Instance (+3 more)

### Community 23 - "Create Instance Form"
Cohesion: 0.23
Nodes (6): DIFFICULTIES, GAMEMODES, LEVEL_TYPES, MEMS, Banner(), PageHeader()

### Community 24 - "Console Log Runtime"
Cohesion: 0.18
Nodes (9): LogKind, Err, Input, Out, System, LogLine, Runtime, Sample (+1 more)

### Community 25 - "CI Workflow Triggers"
Cohesion: 0.17
Nodes (11): env, CARGO_TERM_COLOR, jobs, name, on, pull_request, push, workflow_dispatch (+3 more)

### Community 26 - "Artifact Upload Steps"
Cohesion: 0.17
Nodes (12): with, body_path, components, fail_on_unmatched_files, files, if-no-files-found, merge-multiple, name (+4 more)

### Community 27 - "Settings Persistence"
Cohesion: 0.24
Nodes (4): json_roundtrip_is_atomic(), load_or_keep(), read_json(), write_json()

### Community 28 - "Server Status"
Cohesion: 0.20
Nodes (6): Status, Crashed, Running, Starting, Stopped, Stopping

### Community 29 - "Data Folder Paths"
Cohesion: 0.27
Nodes (3): data_lives_next_to_the_installed_program(), Paths, writable()

### Community 30 - "Instance Tabs"
Cohesion: 0.22
Nodes (9): Tab, Backups, Console, Mods, Overview, Players, Properties, Settings (+1 more)

### Community 31 - "Settings & About UI"
Cohesion: 0.39
Nodes (5): fmt_time(), Check(), AboutPage(), AccountPanel(), SettingsPage()

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

### Community 40 - "Versions Page UI"
Cohesion: 0.40
Nodes (3): cached_jars(), VersionsPage(), launcher_installs()

### Community 41 - "1024px App Icon"
Cohesion: 0.67
Nodes (4): Dark Theme Brand Palette (navy, violet, sky blue), Stylized 'H' Glyph with Connected Nodes, Purple-to-Blue Gradient Hexagon Frame, Hyve App Icon (1024px)

### Community 42 - "Icon Generator Script"
Cohesion: 0.83
Nodes (3): Draw(), Hex(), PngBytes()

### Community 43 - "Task Status"
Cohesion: 0.50
Nodes (4): TaskStatus, Done, Failed, Running

### Community 44 - "App Icon PNG"
Cohesion: 1.00
Nodes (3): Hyve App Icon (icon.png), Hexagon H Logo Design (purple-blue gradient hexagon, light-blue H with node dots), Hyve Branding

## Knowledge Gaps
- **55 isolated node(s):** `hyve`, `Quit`, `Bool`, `Int`, `Choice` (+50 more)
  These have ≤1 connection - possible missing edges or undocumented components. (Counts symbols only; 240 node(s) total have ≤1 connection when file, concept and rationale nodes are included.)
- **3 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `Shared` connect `Background Tasks & Account` to `Minecraft Install & Downloads`, `Windows Job Object`, `Telemetry Sampler`, `Instances, Worlds & Mods`, `Web Server & Auth`, `Backups & Zip Archives`, `Java & Browser Launch`, `Hytale Account Tokens`, `Versions Page UI`, `CLI & Startup`, `Sign-in Prompt Parsing`, `Linux/macOS Self-Install`, `State Constants & Formatting`, `Instance Page UI`, `Desktop Window (wry/tao)`, `UI Root & Toasts`, `Games & Ports`, `Settings Persistence`?**
  _High betweenness centrality (0.327) - this node is a cross-community bridge._
- **Are the 2 inferred relationships involving `Shared` (e.g. with `install_task()` and `file_map()`) actually correct?**
  _`Shared` has 2 INFERRED edges - model-reasoned connections that need verification._
- **What connects `hyve`, `Quit`, `Bool` to the rest of the system?**
  _55 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Background Tasks & Account` be split into smaller, more focused modules?**
  _Cohesion score 0.05938375350140056 - nodes in this community are weakly interconnected._
- **Why does `version` connect `Release Prepare Job` to `Instance Page UI`, `Games & Ports`?**
  _High betweenness centrality (0.130) - this node is a cross-community bridge._
- **Are the 3 inferred relationships involving `AppState` (e.g. with `Catalog` and `Offer`) actually correct?**
  _`AppState` has 3 INFERRED edges - model-reasoned connections that need verification._
- **Should `Minecraft Install & Downloads` be split into smaller, more focused modules?**
  _Cohesion score 0.06502732240437159 - nodes in this community are weakly interconnected._