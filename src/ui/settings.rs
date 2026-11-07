//! Settings (Java, web UI access, updates, console patterns) and About (version, updates, credits).

use dioxus::prelude::*;

use super::instance::Check;
use super::{Banner, ConfirmButton, Icon, Linkified, PageHeader, PanelHead, TaskCard, run_job, use_state, use_toast};
use crate::state::{Mode, VERSION, fmt_time};

#[component]
pub fn SettingsPage(tick: u64) -> Element {
    let state = use_state();
    let toast = use_toast();
    let _ = tick;
    let saved = state.settings.read().clone();
    let mut form = use_signal(move || saved.clone());
    let java = state.java.read().clone();
    let urls = state.urls.read().clone();
    let f = form();
    let (s1, s2, s3, s4) = (state.clone(), state.clone(), state.clone(), state.clone());
    let desktop = state.mode == Mode::Desktop;
    let data = state.paths.root.clone();

    rsx! {
        div { class: "page narrow",
            PageHeader { title: "Settings", subtitle: "Java, your Hytale account, web access and updates." }
            AccountPanel { tick }
            div { class: "panel glass",
                h3 { "Java" }
                p { class: "muted small", "Hytale servers need Java 25. Hyve uses, in order: the path below, its own Temurin copy, JAVA_HOME, then java on the PATH." }
                if java.major > 0 {
                    div { class: "kv",
                        span { "Found" } b { class: "mono small", "{java.path}" }
                        span { "Version" } b { "{java.version}" }
                    }
                }
                if !java.error.is_empty() { Banner { kind: "warn", icon: "alert", "{java.error}" } }
                label { class: "field", span { "Java path (blank = automatic)" }
                    input { class: "input mono", placeholder: "C:\\Program Files\\Eclipse Adoptium\\jdk-25\\bin\\java.exe", value: "{f.java_path}", oninput: move |e| form.write().java_path = e.value() }
                }
                div { class: "row",
                    button { class: "btn", onclick: move |_| {
                        let p = form().java_path.clone();
                        s1.settings.write().java_path = p;
                        let _ = s1.save_settings();
                        run_job(&s1, toast, |st| async move {
                            let j = crate::java::detect(&st).await;
                            if j.error.is_empty() { Ok(format!("Java {} found", j.major)) } else { Err(anyhow::anyhow!(j.error)) }
                        });
                    }, Icon { name: "refresh" } "Save & detect" }
                    button { class: "btn primary", onclick: move |_| { crate::actions::install_java(&s2); toast.ok("Installing Java 25 - see Tasks"); }, Icon { name: "download" } "Install Java 25 (Temurin)" }
                }
            }
            div { class: "panel glass",
                h3 { "Web UI" }
                p { class: "muted small", "The same interface is served on a local port. Without a display (headless) Hyve only runs this web server and prints these links." }
                ul { class: "list links",
                    for u in urls { li { key: "{u}", Icon { name: "link" } Linkified { text: u.clone() } } }
                }
                div { class: "row",
                    label { class: "field grow", span { "Port (after a restart)" }
                        input { class: "input", r#type: "number", value: "{f.web_port}", oninput: move |e| if let Ok(p) = e.value().parse() { form.write().web_port = p } }
                    }
                }
                Check { label: "Desktop mode: also allow other computers on the network (after a restart)", checked: f.lan_access, onchange: move |v| form.write().lan_access = v }
                p { class: "muted small", "Every page needs the access token in its link. Make a new one if a link leaked: old links stop working." }
                button { class: "btn ghost", onclick: move |_| {
                    let t = crate::state::new_token();
                    s3.settings.write().web_token = t.clone();
                    form.write().web_token = t.clone();
                    let port = s3.urls.read().first().and_then(|u| u.split(':').nth(2)).and_then(|p| p.split('/').next()).unwrap_or("7520").to_string();
                    let new_urls: Vec<String> = s3.urls.read().iter().map(|u| format!("{}?token={t}", u.split('?').next().unwrap_or(u))).collect();
                    *s3.urls.write() = new_urls;
                    toast.result(s3.save_settings(), &format!("New token saved (port {port}). This window stays connected."));
                    s3.notify();
                }, Icon { name: "key" } "New access token" }
            }
            div { class: "panel glass",
                h3 { "Updates" }
                Check { label: "Check GitHub for new Hyve versions (at start and every 6 hours)", checked: f.check_updates, onchange: move |v| form.write().check_updates = v }
            }
            div { class: "panel glass",
                h3 { "Player detection (advanced)" }
                p { class: "muted small", "Players online are read from the server console. If your Hytale version words it differently, set your own regular expressions (group 1 = player name). Blank = built-in patterns." }
                label { class: "field", span { "Join pattern" } input { class: "input mono", placeholder: "e.g. Adding player '([^']+)'", value: "{f.join_pattern}", oninput: move |e| form.write().join_pattern = e.value() } }
                label { class: "field", span { "Leave pattern" } input { class: "input mono", placeholder: "e.g. Removing player '([^']+)'", value: "{f.leave_pattern}", oninput: move |e| form.write().leave_pattern = e.value() } }
            }
            div { class: "panel glass",
                h3 { "Data folder" }
                p { class: "mono small", "{data.display()}" }
                p { class: "muted small", "Instances, server versions, backups, Java and logs. Start with --data-dir <path> to use another folder." }
                if desktop { button { class: "btn ghost", onclick: move |_| { let _ = open::that_detached(&data); }, Icon { name: "folder" } "Open" } }
            }
            div { class: "row end sticky-save",
                button { class: "btn primary", onclick: move |_| {
                    let f = form();
                    for p in [&f.join_pattern, &f.leave_pattern] {
                        if !p.trim().is_empty() && let Err(e) = regex::Regex::new(p) {
                            toast.err(format!("invalid pattern: {e}"));
                            return;
                        }
                    }
                    if f.web_port == 0 { toast.err("pick a web port between 1 and 65535"); return; }
                    {
                        let mut s = s4.settings.write();
                        s.java_path = f.java_path.clone();
                        s.web_port = f.web_port;
                        s.lan_access = f.lan_access;
                        s.check_updates = f.check_updates;
                        s.join_pattern = f.join_pattern.clone();
                        s.leave_pattern = f.leave_pattern.clone();
                    }
                    toast.result(s4.save_settings(), "Settings saved");
                    s4.notify();
                }, "Save settings" }
            }
        }
    }
}

/// The Hytale account every server signs in with: sign in once, then servers start without prompts.
#[component]
pub fn AccountPanel(tick: u64) -> Element {
    let state = use_state();
    let toast = use_toast();
    let _ = tick;
    let account = crate::account::load(&state);
    let task = state.tasks.read().iter().find(|t| t.title.starts_with("Sign in to Hytale (all servers)") && t.status == crate::state::TaskStatus::Running).cloned();
    let (s1, s2, s3) = (state.clone(), state.clone(), state.clone());
    let name = account.as_ref().map(|a| a.profile_name()).unwrap_or_default();
    let initial = name.chars().next().unwrap_or('?').to_string();
    let (signed, refreshed) = account.as_ref().map(|a| (fmt_time(a.signed_in_at), fmt_time(a.refreshed_at))).unwrap_or_default();
    rsx! {
        div { class: "panel glass account",
            PanelHead { title: "Hytale account", icon: Some("key"),
                desc: "Sign in once and every server starts already signed in - no code each time you start one. Uses Hytale's official server token flow; the login refreshes itself and only needs renewing after 30 days without Hyve running." }
            if let Some(t) = task {
                TaskCard { task: t, expanded: false }
            } else if let Some(a) = account {
                div { class: "account-state",
                    div { class: "account-avatar", "{initial}" }
                    div { class: "grow",
                        b { "{name}" }
                        div { class: "muted small", "Signed in {signed} · token refreshed {refreshed}" }
                    }
                    span { class: "pill running", "Servers sign in automatically" }
                }
                if a.profiles.len() > 1 {
                    label { class: "field", span { "Servers run as" }
                        select { class: "input", onchange: move |e| toast.result(crate::account::select_profile(&s1, &e.value()), "Profile saved"),
                            for p in a.profiles.clone() { option { key: "{p.uuid}", value: "{p.uuid}", selected: p.uuid == a.profile, "{p.username}" } }
                        }
                    }
                }
                div { class: "row end",
                    ConfirmButton { label: "Sign out", confirm: "Servers will ask to sign in again. Sign out?", class: "ghost", disabled: false,
                        onconfirm: move |_| toast.result(crate::account::sign_out(&s2), "Signed out")
                    }
                }
            } else {
                div { class: "account-state",
                    div { class: "account-avatar off", Icon { name: "key" } }
                    div { class: "grow",
                        b { "Not signed in" }
                        div { class: "muted small", "Each server asks for its own sign-in code until you sign in here." }
                    }
                    button { class: "btn primary", onclick: move |_| toast.result(crate::actions::sign_in_account(&s3), ""), Icon { name: "key" } "Sign in to Hytale" }
                }
            }
        }
    }
}

#[component]
pub fn AboutPage(tick: u64) -> Element {
    let state = use_state();
    let toast = use_toast();
    let _ = tick;
    let u = state.updates.read().clone();
    let (s1, s2) = (state.clone(), state.clone());
    let checked = if u.checked_at > 0 { fmt_time(u.checked_at) } else { "not yet".into() };

    rsx! {
        div { class: "page narrow",
            div { class: "about glass",
                img { src: "/assets/logo.svg", width: "88", height: "88", alt: "" }
                h1 { "Hyve" }
                p { class: "muted", "Run and manage Hytale servers from one place - on your desktop or headless on a server." }
                p { class: "version", "Version {VERSION}" }
                p { class: "made-by big", "Made by " a { href: "https://github.com/Akulav", target: "_blank", rel: "noopener", "Akulav" } }
                div { class: "row center",
                    a { class: "btn ghost", href: "https://github.com/Akulav/hytale-server-orchestrator", target: "_blank", rel: "noopener", Icon { name: "link" } "GitHub" }
                    a { class: "btn ghost", href: "https://github.com/Akulav/hytale-server-orchestrator/releases", target: "_blank", rel: "noopener", "Releases" }
                }
            }
            div { class: "panel glass",
                h3 { "Updates" }
                div { class: "kv",
                    span { "This version" } b { "{VERSION}" }
                    span { "Last check" } b { "{checked}" }
                }
                if let Some(rel) = u.available.clone() {
                    Banner { kind: "info", icon: "zap",
                        b { "Hyve {rel.version} is available. " }
                        a { href: "{rel.url}", target: "_blank", rel: "noopener", "What's new" }
                        if !rel.notes.is_empty() { pre { class: "notes", "{rel.notes}" } }
                        div { class: "banner-actions",
                            if u.self_built {
                                span { class: "muted small", "This copy was built from source: update it with git pull + cargo build." }
                            } else {
                                button { class: "btn sm primary", disabled: u.installing,
                                    onclick: move |_| {
                                        crate::state::spawn_task(&s2, "Update Hyve", None, crate::updater::install);
                                        toast.ok("Downloading the update - servers are stopped cleanly before it installs");
                                    },
                                    if u.installing { "Installing…" } else { "Download & install" }
                                }
                            }
                        }
                    }
                } else if u.checked_at > 0 && u.error.is_empty() {
                    p { class: "muted", "You have the latest version." }
                }
                if !u.error.is_empty() { p { class: "warn-text small", "Last check failed: {u.error}" } }
                button { class: "btn", disabled: u.checking,
                    onclick: move |_| run_job(&s1, toast, |st| async move {
                        let u = crate::updater::check(&st).await;
                        if !u.error.is_empty() { Err(anyhow::anyhow!(u.error)) }
                        else if let Some(r) = u.available { Ok(format!("Version {} is available", r.version)) }
                        else { Ok("You have the latest version".into()) }
                    }),
                    Icon { name: "refresh" } if u.checking { "Checking…" } else { "Check for updates" }
                }
            }
            div { class: "panel glass small muted",
                p { "Hyve is source-available (see LICENSE). Hytale is a trademark of Hypixel Studios; this tool is not affiliated with them. Server files are downloaded with Hypixel's own downloader under your Hytale account." }
            }
        }
    }
}
