//! One instance: overview with telemetry, console, worlds, players & operators, mods (drag & drop),
//! backups and settings.

use dioxus::prelude::*;

use super::{Banner, ConfirmButton, Empty, Icon, Linkified, Page, Spark, Stat, TaskCard, run_job, use_state, use_toast};
use crate::state::{LogKind, Mode, Status, human_bytes, human_duration, now_ms};
use crate::{actions, backup, instance, process};

#[derive(Clone, Copy, Debug, PartialEq)]
enum Tab {
    Overview,
    Console,
    Worlds,
    Players,
    Mods,
    Backups,
    Settings,
}

const TABS: [(Tab, &str, &str); 7] = [
    (Tab::Overview, "Overview", "grid"),
    (Tab::Console, "Console", "terminal"),
    (Tab::Worlds, "Worlds", "globe"),
    (Tab::Players, "Players", "users"),
    (Tab::Mods, "Mods", "box"),
    (Tab::Backups, "Backups", "archive"),
    (Tab::Settings, "Settings", "settings"),
];

#[component]
pub fn InstancePage(id: String, nav: Signal<Page>, tick: u64) -> Element {
    let state = use_state();
    let toast = use_toast();
    let mut tab = use_signal(|| Tab::Overview);
    let Some(inst) = state.instance(&id) else {
        return rsx! { div { class: "page", Empty { icon: "server", title: "Instance not found", text: "It may have been deleted." } } };
    };
    let status = state.status(&id);
    let task = state.running_task_for(&id);
    let current = tab();
    let (s1, s2, s3, s4, s5, s6) = (state.clone(), state.clone(), state.clone(), state.clone(), state.clone(), state.clone());
    let (i1, i2, i3, i4, i5, i6) = (id.clone(), id.clone(), id.clone(), id.clone(), id.clone(), id.clone());
    let status_css = status.css();
    let status_label = status.label();
    let busy = task.is_some();

    rsx! {
        div { class: "page",
            header { class: "page-header inst-header",
                div {
                    div { class: "title-row",
                        h1 { "{inst.name}" }
                        span { class: "pill {status_css}", "{status_label}" }
                    }
                    p { class: "muted", "Hytale {inst.version} ({inst.patchline}) · UDP port {inst.port} · {inst.max_memory_mb / 1024} GB max RAM" }
                }
                div { class: "header-actions",
                    if status.is_active() {
                        button { class: "btn", disabled: status == Status::Stopping || busy,
                            onclick: move |_| { let id = i1.clone(); run_job(&s1, toast, move |st| async move { process::stop(&st, &id).await.map(|_| String::new()) }) },
                            Icon { name: "stop" } "Stop"
                        }
                        button { class: "btn", disabled: status == Status::Stopping || busy,
                            onclick: move |_| { let id = i2.clone(); run_job(&s2, toast, move |st| async move { process::restart(&st, &id).await.map(|_| String::new()) }) },
                            Icon { name: "restart" } "Restart"
                        }
                        ConfirmButton { label: "Kill", confirm: "Kill without saving?", class: "ghost danger-text", disabled: false,
                            onconfirm: move |_| process::kill(&s3, &i3)
                        }
                    } else {
                        button { class: "btn primary", disabled: busy,
                            onclick: move |_| { let id = i4.clone(); run_job(&s4, toast, move |st| async move { process::start(&st, &id).await.map(|_| String::new()) }) },
                            Icon { name: "play" } "Start"
                        }
                    }
                    button { class: "btn", disabled: busy, title: "Back up, download the latest version of this patchline, swap it in and restart",
                        onclick: move |_| toast.result(actions::update_instance(&s5, &i5), "Update started - progress below"),
                        Icon { name: "zap" } "Update"
                    }
                    button { class: "btn", disabled: busy,
                        onclick: move |_| toast.result(actions::backup_now(&s6, &i6, String::new()), "Backup started"),
                        Icon { name: "archive" } "Back up"
                    }
                }
            }
            if let Some(t) = task { TaskCard { task: t, expanded: true } }
            nav { class: "tabs",
                for (t, label, icon) in TABS {
                    button { key: "{label}", class: if current == t { "tab active" } else { "tab" }, onclick: move |_| tab.set(t),
                        Icon { name: icon } "{label}"
                    }
                }
            }
            match current {
                Tab::Overview => rsx! { Overview { id: id.clone(), tick, tab } },
                Tab::Console => rsx! { Console { id: id.clone(), tick } },
                Tab::Worlds => rsx! { Worlds { id: id.clone(), tick } },
                Tab::Players => rsx! { Players { id: id.clone(), tick } },
                Tab::Mods => rsx! { Mods { id: id.clone(), tick } },
                Tab::Backups => rsx! { Backups { id: id.clone(), tick } },
                Tab::Settings => rsx! { InstanceSettings { id: id.clone(), nav, tick } },
            }
        }
    }
}

#[component]
fn Overview(id: String, tick: u64, tab: Signal<Tab>) -> Element {
    let state = use_state();
    let toast = use_toast();
    let _ = tick;
    let Some(inst) = state.instance(&id) else { return rsx! {} };
    let (status, cpu, ram, players, uptime, hist, needs_auth, auth_url, tail, last_exit) = {
        let rt = state.runtimes.read();
        match rt.get(&id) {
            Some(r) => (
                r.status,
                r.cpu,
                r.ram_mb,
                r.players.iter().cloned().collect::<Vec<_>>(),
                r.started_at.map(|s| human_duration(now_ms() - s)),
                r.history.clone(),
                r.needs_auth,
                r.auth_url.clone(),
                r.log.iter().rev().take(10).rev().cloned().collect::<Vec<_>>(),
                r.last_exit,
            ),
            None => (Status::Stopped, 0.0, 0, Vec::new(), None, Default::default(), false, None, Vec::new(), None),
        }
    };
    let cpu_hist: Vec<f32> = hist.iter().map(|s| s.cpu).collect();
    let ram_hist: Vec<f32> = hist.iter().map(|s| s.ram_mb as f32).collect();
    let pl_hist: Vec<f32> = hist.iter().map(|s| s.players as f32).collect();
    let lan = local_ip();
    let (s1, s2, i1, i2) = (state.clone(), state.clone(), id.clone(), id.clone());
    let worlds = instance::list_worlds(&state, &id).len();
    let mods = instance::list_mods(&state, &id).iter().filter(|m| m.enabled).count();
    let mut tab = tab;

    rsx! {
        if needs_auth && status.is_active() {
            Banner { kind: "warn", icon: "key",
                b { "This server needs to log in to Hytale before players can join. " }
                if let Some(url) = auth_url.clone() {
                    span { "Open " a { href: "{url}", target: "_blank", rel: "noopener", "{url}" } " and approve the code shown in the console." }
                } else {
                    span { class: "muted", "Start a device login and approve it in your browser." }
                }
                div { class: "banner-actions",
                    button { class: "btn sm primary", onclick: move |_| toast.result(process::send_command(&s1, &i1, "/auth login device"), "Login started - the link appears here"), "Log in (device code)" }
                    button { class: "btn sm ghost", title: "Stores the login encrypted so it survives restarts",
                        onclick: move |_| toast.result(process::send_command(&s2, &i2, "/auth persistence Encrypted"), "Sent /auth persistence Encrypted"),
                        "Remember login"
                    }
                }
            }
        }
        if status == Status::Crashed {
            Banner { kind: "error", icon: "alert",
                b { "The server crashed" }
                span { class: "muted", if let Some(c) = last_exit { " (exit code {c})" } ". The console shows the last lines it printed." }
            }
        }
        div { class: "stats",
            Stat { icon: "cpu", label: "CPU", value: format!("{cpu:.1}%"), sub: "of the whole machine", Spark { values: cpu_hist, max: 100.0, class: "violet" } }
            Stat { icon: "ram", label: "RAM", value: if ram > 0 { format!("{:.2} GB", ram as f32 / 1024.0) } else { "-".to_string() }, sub: format!("max {} GB (-Xmx)", inst.max_memory_mb / 1024),
                Spark { values: ram_hist, max: inst.max_memory_mb as f32 * 1.15, class: "cyan" }
            }
            Stat { icon: "users", label: "Players", value: format!("{}", players.len()), sub: if players.is_empty() { "nobody online".to_string() } else { players.join(", ") },
                Spark { values: pl_hist, max: 0.0, class: "green" }
            }
            Stat { icon: "clock", label: "Uptime", value: uptime.unwrap_or_else(|| "-".into()), sub: status.label().to_string() }
        }
        div { class: "grid2",
            div { class: "panel glass",
                h3 { "Connect" }
                p { "Players join with " b { "{lan}:{inst.port}" } " on your network, or your public IP / domain with the same port." }
                p { class: "muted small", "Hytale uses QUIC over UDP: forward and allow UDP {inst.port} (not TCP)." }
                div { class: "kv",
                    span { "Worlds" } b { "{worlds}" }
                    span { "Active mods" } b { "{mods}" }
                    span { "Operators" } b { "{inst.operators.len()}" }
                    span { "Auth mode" } b { if inst.offline_auth { "offline (LAN tests)" } else { "authenticated" } }
                }
            }
            div { class: "panel glass",
                div { class: "panel-head", h3 { "Latest console output" } button { class: "btn sm ghost", onclick: move |_| tab.set(Tab::Console), "Open console" } }
                pre { class: "log mini",
                    if tail.is_empty() { span { class: "muted", "The server has not printed anything yet." } }
                    for l in tail {
                        div { key: "{l.seq}", class: log_class(l.kind), Linkified { text: l.text.clone() } }
                    }
                }
            }
        }
    }
}

fn log_class(kind: LogKind) -> &'static str {
    match kind {
        LogKind::Out => "ln",
        LogKind::Err => "ln err",
        LogKind::System => "ln sys",
        LogKind::Input => "ln cmd",
    }
}

fn local_ip() -> String {
    let ip = std::net::UdpSocket::bind("0.0.0.0:0").ok().and_then(|s| s.connect("192.0.2.1:9").ok().map(|_| s)).and_then(|s| s.local_addr().ok()).map(|a| a.ip().to_string());
    ip.unwrap_or_else(|| "this-computer".into())
}

#[component]
fn Console(id: String, tick: u64) -> Element {
    let state = use_state();
    let toast = use_toast();
    let _ = tick;
    let mut input = use_signal(String::new);
    let mut filter = use_signal(String::new);
    let (status, lines) = {
        let rt = state.runtimes.read();
        match rt.get(&id) {
            Some(r) => (r.status, r.log.iter().rev().take(600).cloned().collect::<Vec<_>>()),
            None => (Status::Stopped, Vec::new()),
        }
    };
    let f = filter().to_lowercase();
    let lines: Vec<_> = if f.is_empty() { lines } else { lines.into_iter().filter(|l| l.text.to_lowercase().contains(&f)).collect() };
    let running = status.is_active();
    let (s1, s2, i1, i2) = (state.clone(), state.clone(), id.clone(), id.clone());
    let mut send = move |text: String| {
        if text.trim().is_empty() {
            return;
        }
        match process::send_command(&s1, &i1, &text) {
            Ok(()) => input.set(String::new()),
            Err(e) => toast.err(format!("{e:#}")),
        }
    };
    let mut send2 = send.clone();
    let quick = ["/who", "/save", "/say Hello from the orchestrator!", "/help"];

    rsx! {
        div { class: "panel glass console",
            div { class: "console-bar",
                input { class: "input grow", placeholder: "Filter output…", value: "{filter}", oninput: move |e| filter.set(e.value()) }
                for q in quick {
                    button { key: "{q}", class: "btn sm ghost", disabled: !running,
                        onclick: {
                            let (s, i) = (s2.clone(), i2.clone());
                            move |_| toast.result(process::send_command(&s, &i, q), "")
                        },
                        "{q.split(' ').next().unwrap_or(q)}"
                    }
                }
            }
            // column-reverse keeps the view pinned to the newest line
            pre { class: "log",
                for l in lines {
                    div { key: "{l.seq}", class: log_class(l.kind), Linkified { text: l.text.clone() } }
                }
            }
            div { class: "console-input",
                span { class: "prompt", ">" }
                input { class: "input grow mono", disabled: !running,
                    placeholder: if running { "Type a server command, e.g. /op add Steve or /say hi - Enter sends" } else { "Start the server to use its console" },
                    value: "{input}",
                    oninput: move |e| input.set(e.value()),
                    onkeydown: move |e: KeyboardEvent| if e.key() == Key::Enter { send(input()) },
                }
                button { class: "btn primary", disabled: !running, onclick: move |_| send2(input()), "Send" }
            }
        }
    }
}

#[component]
fn Worlds(id: String, tick: u64) -> Element {
    let state = use_state();
    let toast = use_toast();
    let _ = tick;
    let mut name = use_signal(String::new);
    let mut seed = use_signal(String::new);
    let worlds = instance::list_worlds(&state, &id);
    let stopped = !state.status(&id).is_active();
    let (s1, i1) = (state.clone(), id.clone());

    rsx! {
        div { class: "grid2 wide-left",
            div { class: "panel glass",
                h3 { "Worlds" }
                p { class: "muted small", "Every folder in universe/worlds is a world the server loads. Each runs on its own thread." }
                if worlds.is_empty() {
                    p { class: "muted", "No worlds yet - the server creates its default world on the first start, or create one with a seed." }
                }
                table { class: "table",
                    thead { tr { th { "Name" } th { "Seed" } th { "PvP" } th { "Size" } th {} } }
                    tbody {
                        for w in worlds {
                            {
                                let (s2, s3, i2, i3, n2, n3) = (state.clone(), state.clone(), id.clone(), id.clone(), w.name.clone(), w.name.clone());
                                let seed_text = w.seed.map(|s| s.to_string()).unwrap_or_else(|| "-".into());
                                let pvp = w.pvp.unwrap_or(false);
                                rsx! {
                                    tr { key: "{w.name}",
                                        td { b { "{w.name}" } if w.pending { span { class: "pill busy sm", "generated on next start" } } }
                                        td { class: "mono", "{seed_text}" }
                                        td {
                                            label { class: "switch", title: if stopped { "" } else { "Stop the server to change" },
                                                input { r#type: "checkbox", checked: pvp, disabled: !stopped || w.pvp.is_none(),
                                                    onchange: move |e| toast.result(instance::set_world_pvp(&s2, &i2, &n2, e.checked()), "")
                                                }
                                                span { class: "slider" }
                                            }
                                        }
                                        td { "{human_bytes(w.size)}" }
                                        td { class: "right",
                                            ConfirmButton { label: "Delete", confirm: "Delete this world for good?", class: "sm ghost danger-text", disabled: !stopped,
                                                onconfirm: move |_| toast.result(instance::delete_world(&s3, &i3, &n3), "World deleted")
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            div { class: "panel glass",
                h3 { "Generate a world" }
                label { class: "field", span { "Name" }
                    input { class: "input", placeholder: "e.g. survival, creative_2", value: "{name}", oninput: move |e| name.set(e.value()) }
                }
                label { class: "field", span { "Seed" }
                    div { class: "row",
                        input { class: "input grow mono", placeholder: "blank = random; numbers or words", value: "{seed}", oninput: move |e| seed.set(e.value()) }
                        button { class: "btn ghost", onclick: move |_| seed.set(instance::random_seed().to_string()), "🎲" }
                    }
                }
                button { class: "btn primary block",
                    onclick: move |_| match instance::create_world(&s1, &i1, &name(), instance::parse_seed(&seed())) {
                        Ok(s) => { toast.ok(format!("World {} created with seed {s} - it is generated on the next start", name())); name.set(String::new()); seed.set(String::new()); }
                        Err(e) => toast.err(format!("{e:#}")),
                    },
                    Icon { name: "plus" } "Create world"
                }
                p { class: "muted small", "Words are turned into a number the same way every time, so \"hello\" is always the same world." }
            }
        }
    }
}

#[component]
fn Players(id: String, tick: u64) -> Element {
    let state = use_state();
    let toast = use_toast();
    let _ = tick;
    let mut name = use_signal(String::new);
    let Some(inst) = state.instance(&id) else { return rsx! {} };
    let online: Vec<String> = state.runtimes.read().get(&id).map(|r| r.players.iter().cloned().collect()).unwrap_or_default();
    let running = state.status(&id).is_active();
    let (s1, i1) = (state.clone(), id.clone());

    rsx! {
        div { class: "grid2",
            div { class: "panel glass",
                h3 { "Online now ({online.len()})" }
                if !running { p { class: "muted", "The server is not running." } }
                else if online.is_empty() { p { class: "muted", "Nobody is online. Players are read from the console as they join and leave." } }
                ul { class: "list",
                    for p in online {
                        {
                            let is_op = inst.operators.iter().any(|o| o.eq_ignore_ascii_case(&p));
                            let (s2, i2, p2) = (state.clone(), id.clone(), p.clone());
                            let (s3, i3, p3) = (state.clone(), id.clone(), p.clone());
                            rsx! {
                                li { key: "{p}",
                                    span { class: "avatar", "{p.chars().next().unwrap_or('?')}" }
                                    b { class: "grow", "{p}" }
                                    if is_op { span { class: "pill running sm", "OP" } }
                                    if !is_op {
                                        button { class: "btn sm", onclick: move |_| toast.result(process::set_operator(&s2, &i2, &p2, true), "Operator added"), Icon { name: "shield" } "Make OP" }
                                    }
                                    ConfirmButton { label: "Kick", confirm: "Kick?", class: "sm ghost", disabled: false,
                                        onconfirm: move |_| toast.result(process::send_command(&s3, &i3, &format!("/kick {p3}")), "")
                                    }
                                }
                            }
                        }
                    }
                }
            }
            div { class: "panel glass",
                h3 { "Operators" }
                p { class: "muted small", "Operators can use admin commands in game. Changes go through the server console (/op add, /op remove), so the server must be running. Use a UUID for players who are offline." }
                div { class: "row",
                    input { class: "input grow", placeholder: "Player name or UUID", value: "{name}", oninput: move |e| name.set(e.value()), disabled: !running }
                    button { class: "btn primary", disabled: !running,
                        onclick: move |_| match process::set_operator(&s1, &i1, &name(), true) {
                            Ok(()) => { toast.ok(format!("{} is now an operator", name())); name.set(String::new()); }
                            Err(e) => toast.err(format!("{e:#}")),
                        },
                        Icon { name: "shield" } "Make OP"
                    }
                }
                ul { class: "list",
                    if inst.operators.is_empty() { li { class: "muted", "No operators set from here yet." } }
                    for op in inst.operators.clone() {
                        {
                            let (s2, i2, o2) = (state.clone(), id.clone(), op.clone());
                            rsx! {
                                li { key: "{op}",
                                    span { class: "avatar op", Icon { name: "shield" } }
                                    b { class: "grow", "{op}" }
                                    button { class: "btn sm ghost", disabled: !running, onclick: move |_| toast.result(process::set_operator(&s2, &i2, &o2, false), "Operator removed"), "Remove" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn Mods(id: String, tick: u64) -> Element {
    let state = use_state();
    let toast = use_toast();
    let _ = tick;
    let mods = instance::list_mods(&state, &id);
    let running = state.status(&id).is_active();
    let desktop = state.mode == Mode::Desktop;
    let mods_dir = state.paths.server_dir(&id).join("mods");

    rsx! {
        div { class: "dropzone glass", "data-dropzone": "mods", "data-instance": "{id}",
            Icon { name: "upload" }
            div {
                b { "Drop .jar / .zip mods here" }
                p { class: "muted small", "or " label { class: "link", "choose files" input { r#type: "file", multiple: true, accept: ".jar,.zip", "data-upload": "mods", "data-instance": "{id}", hidden: true } } " · they go into this instance's mods folder" }
            }
            div { class: "drop-status", id: "drop-status-{id}" }
        }
        if running { Banner { kind: "info", icon: "info", "Mod changes apply when the server restarts." } }
        div { class: "panel glass",
            div { class: "panel-head",
                h3 { "Installed mods ({mods.len()})" }
                if desktop {
                    button { class: "btn sm ghost", onclick: move |_| { let _ = open::that_detached(&mods_dir); }, Icon { name: "folder" } "Open folder" }
                }
            }
            if mods.is_empty() { p { class: "muted", "No mods yet. Find Hytale mods on CurseForge and drop them above." } }
            table { class: "table",
                tbody {
                    for m in mods {
                        {
                            let (s1, s2, i1, i2, f1, f2) = (state.clone(), state.clone(), id.clone(), id.clone(), m.file.clone(), m.file.clone());
                            rsx! {
                                tr { key: "{m.file}", class: if m.enabled { "" } else { "dim" },
                                    td { Icon { name: "box" } " " b { "{m.name}" } }
                                    td { class: "muted", "{human_bytes(m.size)}" }
                                    td {
                                        label { class: "switch",
                                            input { r#type: "checkbox", checked: m.enabled, onchange: move |_| toast.result(instance::toggle_mod(&s1, &i1, &f1), "") }
                                            span { class: "slider" }
                                        }
                                    }
                                    td { class: "right",
                                        ConfirmButton { label: "Remove", confirm: "Delete this mod?", class: "sm ghost danger-text", disabled: false,
                                            onconfirm: move |_| toast.result(instance::delete_mod(&s2, &i2, &f2), "Mod removed")
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn Backups(id: String, tick: u64) -> Element {
    let state = use_state();
    let toast = use_toast();
    let _ = tick;
    let mut label = use_signal(String::new);
    let list = backup::list(&state, &id);
    let running = state.status(&id).is_active();
    let busy = state.running_task_for(&id).is_some();
    let desktop = state.mode == Mode::Desktop;
    let dir = state.paths.backup_dir(&id);
    let (s1, i1) = (state.clone(), id.clone());

    rsx! {
        div { class: "panel glass",
            h3 { "Back up world + mods" }
            p { class: "muted small", "Packs universe/ (all worlds and player data), mods/ and the server's config, permissions, whitelist and bans into one zip. Updates and restores make one automatically first." }
            div { class: "row",
                input { class: "input grow", placeholder: "Label (optional), e.g. before the castle", value: "{label}", oninput: move |e| label.set(e.value()) }
                button { class: "btn primary", disabled: busy,
                    onclick: move |_| { toast.result(actions::backup_now(&s1, &i1, label()), "Backup started"); label.set(String::new()); },
                    Icon { name: "archive" } "Back up now"
                }
                if desktop { button { class: "btn ghost", onclick: move |_| { let _ = std::fs::create_dir_all(&dir); let _ = open::that_detached(&dir); }, Icon { name: "folder" } "Folder" } }
            }
            if running { p { class: "muted small", "The server is running: it is asked to save first. For a guaranteed-consistent copy, stop it before backing up." } }
        }
        div { class: "panel glass",
            h3 { "Backups ({list.len()})" }
            if list.is_empty() { p { class: "muted", "No backups yet." } }
            table { class: "table",
                tbody {
                    for b in list {
                        {
                            let (s2, s3, i2, i3, f2, f3) = (state.clone(), state.clone(), id.clone(), id.clone(), b.file.clone(), b.file.clone());
                            let when = crate::state::fmt_time(b.created);
                            rsx! {
                                tr { key: "{b.file}",
                                    td { b { "{when}" } if !b.label.is_empty() { span { class: "pill sm", "{b.label}" } } }
                                    td { class: "muted mono small", "{b.file}" }
                                    td { class: "muted", "{human_bytes(b.size)}" }
                                    td { class: "right nowrap",
                                        ConfirmButton { label: "Restore", confirm: "Replace the current world + mods?", class: "sm", disabled: running || busy,
                                            onconfirm: move |_| toast.result(actions::restore(&s2, &i2, f2.clone()), "Restore started")
                                        }
                                        ConfirmButton { label: "Delete", confirm: "Delete?", class: "sm ghost danger-text", disabled: false,
                                            onconfirm: move |_| toast.result(backup::delete(&s3, &i3, &f3), "Backup deleted")
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if running && !busy { p { class: "muted small", "Stop the server to restore a backup." } }
        }
    }
}

#[component]
fn InstanceSettings(id: String, nav: Signal<Page>, tick: u64) -> Element {
    let state = use_state();
    let toast = use_toast();
    let _ = tick;
    let Some(inst) = state.instance(&id) else { return rsx! {} };
    // the form edits a copy; Save writes it back
    let mut form = use_signal(|| inst.clone());
    let mut del_backups = use_signal(|| false);
    let mut new_version = use_signal(|| inst.version.clone());
    let versions = crate::versions::list(&state);
    let running = state.status(&id).is_active();
    let f = form();
    let (s1, s2, s3, i1, i2, i3) = (state.clone(), state.clone(), state.clone(), id.clone(), id.clone(), id.clone());
    let desktop = state.mode == Mode::Desktop;
    let dir = state.paths.server_dir(&id);
    let mems = [1024u32, 2048, 3072, 4096, 6144, 8192, 12288, 16384, 24576, 32768];

    rsx! {
        div { class: "grid2",
            div { class: "panel glass",
                h3 { "Server" }
                label { class: "field", span { "Name" } input { class: "input", value: "{f.name}", oninput: move |e| form.write().name = e.value() } }
                label { class: "field", span { "UDP port" }
                    input { class: "input", r#type: "number", min: "1", max: "65535", value: "{f.port}", oninput: move |e| if let Ok(p) = e.value().parse() { form.write().port = p } }
                }
                div { class: "row",
                    label { class: "field grow", span { "Min RAM" }
                        select { class: "input", onchange: move |e| if let Ok(v) = e.value().parse() { form.write().min_memory_mb = v },
                            for m in mems { option { key: "{m}", value: "{m}", selected: m == f.min_memory_mb, "{m / 1024} GB" } }
                        }
                    }
                    label { class: "field grow", span { "Max RAM" }
                        select { class: "input", onchange: move |e| if let Ok(v) = e.value().parse() { form.write().max_memory_mb = v },
                            for m in mems { option { key: "{m}", value: "{m}", selected: m == f.max_memory_mb, "{m / 1024} GB" } }
                        }
                    }
                }
                label { class: "field", span { "Update channel" }
                    select { class: "input", onchange: move |e| form.write().patchline = e.value(),
                        for p in crate::versions::PATCHLINES { option { key: "{p}", value: "{p}", selected: f.patchline == p, "{p}" } }
                    }
                }
                Check { label: "Offline auth mode (--auth-mode offline, for LAN tests)", checked: f.offline_auth, onchange: move |v| form.write().offline_auth = v }
                Check { label: "Use the AOT cache for faster starts (HytaleServer.aot)", checked: f.use_aot, onchange: move |v| form.write().use_aot = v }
                Check { label: "Disable Sentry crash reports (--disable-sentry, for mod development)", checked: f.disable_sentry, onchange: move |v| form.write().disable_sentry = v }
                Check { label: "Restart automatically after a crash", checked: f.auto_restart, onchange: move |v| form.write().auto_restart = v }
                label { class: "field", span { "Extra JVM arguments" } input { class: "input mono", placeholder: "-XX:+UseG1GC", value: "{f.jvm_args}", oninput: move |e| form.write().jvm_args = e.value() } }
                label { class: "field", span { "Extra server arguments" } input { class: "input mono", placeholder: "--backup --backup-frequency 30", value: "{f.server_args}", oninput: move |e| form.write().server_args = e.value() } }
                div { class: "row",
                    button { class: "btn primary",
                        onclick: move |_| {
                            let mut f = form();
                            let r = (|| {
                                f.name = instance::validate_name(&f.name)?;
                                if f.port == 0 { anyhow::bail!("pick a port between 1 and 65535"); }
                                if let Some(other) = instance::port_conflict(&s1, &i1, f.port) { anyhow::bail!("port {} is used by \"{other}\"", f.port); }
                                if f.min_memory_mb > f.max_memory_mb { f.min_memory_mb = f.max_memory_mb; }
                                s1.update_instance(&i1, |i| {
                                    i.name = f.name.clone(); i.port = f.port; i.min_memory_mb = f.min_memory_mb; i.max_memory_mb = f.max_memory_mb;
                                    i.patchline = f.patchline.clone(); i.offline_auth = f.offline_auth; i.use_aot = f.use_aot;
                                    i.disable_sentry = f.disable_sentry; i.auto_restart = f.auto_restart; i.jvm_args = f.jvm_args.clone(); i.server_args = f.server_args.clone();
                                })
                            })();
                            toast.result(r, if running { "Saved - restart the server to apply" } else { "Saved" });
                        },
                        "Save"
                    }
                    if desktop { button { class: "btn ghost", onclick: move |_| { let _ = open::that_detached(&dir); }, Icon { name: "folder" } "Open server folder" } }
                }
            }
            div { class: "col",
                div { class: "panel glass",
                    h3 { "Server version" }
                    p { class: "muted small", "Switch to any version in the cache (newer or older). A backup is made first; a running server is restarted." }
                    div { class: "row",
                        select { class: "input grow", onchange: move |e| new_version.set(e.value()),
                            for v in versions {
                                option { key: "{v.version}", value: "{v.version}", selected: v.version == new_version(), "{v.version} ({v.patchline})" }
                            }
                        }
                        button { class: "btn", disabled: new_version() == inst.version,
                            onclick: move |_| toast.result(actions::change_version(&s2, &i2, &new_version()), "Switching version - progress above"),
                            "Switch"
                        }
                    }
                }
                div { class: "panel glass danger-zone",
                    h3 { "Delete instance" }
                    p { class: "muted small", "Deletes the server folder: worlds, mods and configs. Backups are kept unless you tick the box." }
                    Check { label: "Also delete its backups", checked: del_backups(), onchange: move |v| del_backups.set(v) }
                    ConfirmButton { label: "Delete instance", confirm: "Delete everything of this instance?", class: "danger", disabled: running,
                        onconfirm: move |_| match instance::delete(&s3, &i3, del_backups()) {
                            Ok(()) => { toast.ok("Instance deleted"); nav.set(Page::Dashboard); }
                            Err(e) => toast.err(format!("{e:#}")),
                        }
                    }
                    if running { p { class: "muted small", "Stop the server first." } }
                }
            }
        }
    }
}

#[component]
pub fn Check(label: String, checked: bool, onchange: EventHandler<bool>) -> Element {
    rsx! {
        label { class: "check",
            input { r#type: "checkbox", checked, onchange: move |e| onchange.call(e.checked()) }
            span { "{label}" }
        }
    }
}
