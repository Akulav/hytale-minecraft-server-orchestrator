//! Dashboard: a "get started" strip until everything is ready, the machine's load, and one card per
//! instance with live telemetry.

use dioxus::prelude::*;

use super::{Empty, Icon, Meter, Page, PageHeader, Spark, Stat, TaskCard, run_job, use_state, use_toast};
use crate::state::{Status, human_duration, now_ms};

#[component]
pub fn Dashboard(nav: Signal<Page>, tick: u64) -> Element {
    let state = use_state();
    let _ = tick;
    let host = state.host.read().clone();
    let instances = state.instances.read().clone();
    let java = state.java.read().clone();
    let has_versions = !crate::versions::list(&state).is_empty();
    let (running, players) = {
        let rt = state.runtimes.read();
        (rt.values().filter(|r| r.status.is_active()).count(), rt.values().map(|r| r.players.len()).sum::<usize>())
    };
    let cpu_hist: Vec<f32> = host.history.iter().map(|h| h.0).collect();
    let ram_hist: Vec<f32> = host.history.iter().map(|h| h.1).collect();
    let pl_hist: Vec<f32> = host.history.iter().map(|h| h.2).collect();
    let ram_pct = if host.ram_total_mb > 0 { host.ram_used_mb as f32 * 100.0 / host.ram_total_mb as f32 } else { 0.0 };
    let waiting_login = state.tasks.read().iter().find(|t| t.login.is_some()).cloned();
    let java_ok = java.error.is_empty() && java.major > 0;
    let has_account = crate::account::load(&state).is_some();
    let ready = java_ok && has_versions && has_account && !instances.is_empty();
    let share = if instances.is_empty() { 0.0 } else { running as f32 / instances.len() as f32 };

    rsx! {
        div { class: "page",
            PageHeader { title: "Dashboard", subtitle: "Every Hytale server on this machine at a glance.",
                button { class: "btn primary", onclick: move |_| nav.set(Page::Create), Icon { name: "plus" } "New instance" }
            }
            if let Some(t) = waiting_login {
                TaskCard { task: t, expanded: false }
            }
            if !ready {
                GetStarted { nav, java_ok, java_error: java.error.clone(), has_versions, has_account, has_instances: !instances.is_empty() }
            }
            div { class: "stats",
                Stat { icon: "cpu", label: "Machine CPU", value: format!("{:.0}%", host.cpu), sub: format!("{} cores", host.cpus),
                    Spark { values: cpu_hist, max: 100.0, class: "violet" }
                }
                Stat { icon: "ram", label: "Machine RAM", value: format!("{:.0}%", ram_pct), sub: format!("{:.1} of {:.1} GB", host.ram_used_mb as f32 / 1024.0, host.ram_total_mb as f32 / 1024.0),
                    Spark { values: ram_hist, max: 100.0, class: "cyan" }
                }
                Stat { icon: "server", label: "Servers running", value: format!("{running} / {}", instances.len()), sub: if instances.is_empty() { "no instances yet".to_string() } else { format!("{} stopped", instances.len() - running) },
                    div { class: "foot-meter", Meter { value: share, class: "green" } }
                }
                Stat { icon: "users", label: "Players online", value: format!("{players}"), sub: "across all servers",
                    Spark { values: pl_hist, max: 0.0, class: "green" }
                }
            }
            if !instances.is_empty() {
                div { class: "section-title", h2 { "Instances" } span { class: "muted small", "{instances.len()} total · {running} running" } }
                div { class: "cards",
                    for inst in instances {
                        InstanceCard { key: "{inst.id}", id: inst.id.clone(), nav, tick }
                    }
                }
            } else if ready || (java_ok && has_versions) {
                Empty { icon: "server", title: "No servers yet", text: "Create an instance: pick a version, a port and a world seed. You can run as many as your machine handles.",
                    button { class: "btn primary", onclick: move |_| nav.set(Page::Create), Icon { name: "plus" } "Create your first server" }
                }
            }
        }
    }
}

/// Four equal steps: Java → server files → Hytale account → a server. Done steps are ticked.
#[component]
fn GetStarted(nav: Signal<Page>, java_ok: bool, java_error: String, has_versions: bool, has_account: bool, has_instances: bool) -> Element {
    let state = use_state();
    let toast = use_toast();
    let (st, st2) = (state.clone(), state.clone());
    let signing = crate::account::signing_in();
    let step = |done: bool, current: bool| if done { "step done" } else if current { "step current" } else { "step" };
    let java_text = if java_ok { "Java 25 is ready.".to_string() } else if java_error.is_empty() { "Checking for Java 25…".to_string() } else { "Hytale servers need Java 25. Install it in one click.".to_string() };
    rsx! {
        div { class: "get-started glass",
            div { class: "gs-head", h2 { "Get started" } span { class: "muted small", "Four one-time steps to your first Hytale server" } }
            div { class: "steps",
                div { class: step(java_ok, true),
                    div { class: "step-num", if java_ok { "✓" } else { "1" } }
                    b { "Java 25" }
                    p { class: "muted small", "{java_text}" }
                    if !java_ok {
                        button { class: "btn sm primary", onclick: move |_| { crate::actions::install_java(&st); toast.ok("Installing Java 25 - see Tasks"); }, "Install Java 25" }
                    }
                }
                div { class: step(has_versions, java_ok),
                    div { class: "step-num", if has_versions { "✓" } else { "2" } }
                    b { "Server files" }
                    p { class: "muted small", if has_versions { "A Hytale server version is in the cache." } else { "Download them with the official Hytale downloader, or import them from your launcher." } }
                    if !has_versions {
                        button { class: "btn sm", onclick: move |_| nav.set(Page::Versions), "Get server files" }
                    }
                }
                div { class: step(has_account, java_ok && has_versions),
                    div { class: "step-num", if has_account { "✓" } else { "3" } }
                    b { "Hytale account" }
                    p { class: "muted small", if has_account { "Servers start already signed in." } else { "Sign in once so servers never ask for a code when they start." } }
                    if !has_account {
                        button { class: "btn sm", disabled: signing, onclick: move |_| toast.result(crate::actions::sign_in_account(&st2), ""),
                            if signing { "Waiting for approval…" } else { "Sign in once" }
                        }
                    }
                }
                div { class: step(has_instances, java_ok && has_versions),
                    div { class: "step-num", if has_instances { "✓" } else { "4" } }
                    b { "Your server" }
                    p { class: "muted small", "Create an instance with a name, port and world seed, then press Start." }
                    if !has_instances {
                        button { class: "btn sm", disabled: !has_versions, onclick: move |_| nav.set(Page::Create), "New instance" }
                    }
                }
            }
        }
    }
}

#[component]
fn InstanceCard(id: String, nav: Signal<Page>, tick: u64) -> Element {
    let state = use_state();
    let toast = use_toast();
    let _ = tick;
    let Some(inst) = state.instance(&id) else { return rsx! {} };
    let (status, cpu, ram, players, uptime, hist, auth) = {
        let rt = state.runtimes.read();
        match rt.get(&id) {
            Some(r) => (
                r.status,
                r.cpu,
                r.ram_mb,
                r.players.len(),
                r.started_at.map(|s| human_duration(now_ms() - s)).unwrap_or_else(|| "-".into()),
                r.history.iter().map(|s| s.ram_mb as f32).collect::<Vec<_>>(),
                r.needs_auth,
            ),
            None => (Status::Stopped, 0.0, 0, 0, "-".into(), Vec::new(), false),
        }
    };
    let busy = state.running_task_for(&id);
    let (id1, id2, st1, st2) = (id.clone(), id.clone(), state.clone(), state.clone());
    let status_css = status.css();
    let status_label = status.label();
    let max_ram = inst.max_memory_mb as f32;
    let ram_label = if ram > 0 { format!("{:.1} GB", ram as f32 / 1024.0) } else { "-".into() };
    let cpu_label = if status.is_active() { format!("{cpu:.0}%") } else { "-".into() };
    let note = if auth {
        Some(("warn", "key", "Needs a Hytale sign-in".to_string()))
    } else {
        busy.map(|t| ("", "activity", format!("{}: {}", t.title, t.message)))
    };
    let note_cls = note.as_ref().map(|n| n.0).unwrap_or_default();

    rsx! {
        div { class: "card glass inst-card {status_css}",
            div { class: "card-head", onclick: move |_| nav.set(Page::Instance(id1.clone())),
                div { class: "card-title",
                    span { class: "dot {status_css}" }
                    h3 { "{inst.name}" }
                }
                span { class: "pill {status_css}", "{status_label}" }
            }
            div { class: "chips",
                span { class: "chip", "{inst.version}" }
                span { class: "chip", "UDP {inst.port}" }
                span { class: "chip", "{inst.max_memory_mb / 1024} GB" }
            }
            div { class: "mini-stats",
                div { span { "CPU" } b { "{cpu_label}" } }
                div { span { "RAM" } b { "{ram_label}" } }
                div { span { "Players" } b { "{players}" } }
                div { span { "Uptime" } b { "{uptime}" } }
            }
            Spark { values: hist, max: max_ram, class: "cyan" }
            div { class: "card-note {note_cls}",
                if let Some((_, icon, text)) = note { Icon { name: icon } span { "{text}" } } else { "\u{a0}" }
            }
            div { class: "card-actions",
                if status.is_active() {
                    button { class: "btn", disabled: status == Status::Stopping,
                        onclick: move |_| { let id = id2.clone(); run_job(&st1, toast, move |st| async move { crate::process::stop(&st, &id).await.map(|_| String::new()) }) },
                        Icon { name: "stop" } "Stop"
                    }
                } else {
                    button { class: "btn primary",
                        onclick: move |_| { let id = id2.clone(); run_job(&st2, toast, move |st| async move { crate::process::start(&st, &id).await.map(|_| String::new()) }) },
                        Icon { name: "play" } "Start"
                    }
                }
                button { class: "btn", onclick: move |_| nav.set(Page::Instance(id.clone())), "Manage" Icon { name: "chevron" } }
            }
        }
    }
}
