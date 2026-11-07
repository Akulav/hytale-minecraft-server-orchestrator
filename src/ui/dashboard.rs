//! Dashboard: the machine's load, Java / version readiness and a card per instance with live telemetry.

use dioxus::prelude::*;

use super::{Banner, Empty, Icon, Page, PageHeader, Spark, Stat, run_job, use_state, use_toast};
use crate::state::{Status, human_duration, now_ms};

#[component]
pub fn Dashboard(nav: Signal<Page>, tick: u64) -> Element {
    let state = use_state();
    let toast = use_toast();
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
    let ram_pct = if host.ram_total_mb > 0 { host.ram_used_mb as f32 * 100.0 / host.ram_total_mb as f32 } else { 0.0 };
    let st_java = state.clone();

    rsx! {
        div { class: "page",
            PageHeader { title: "Dashboard", subtitle: "Every Hytale server on this machine at a glance.",
                button { class: "btn primary", onclick: move |_| nav.set(Page::Create), Icon { name: "plus" } "New instance" }
            }
            if !java.error.is_empty() {
                Banner { kind: "warn", icon: "alert",
                    b { "Java 25 is needed to run Hytale servers. " }
                    span { class: "muted", "{java.error}" }
                    div { class: "banner-actions",
                        button { class: "btn sm primary", onclick: move |_| { crate::actions::install_java(&st_java); toast.ok("Installing Java 25 - see Tasks"); }, "Install Java 25 (Temurin)" }
                        button { class: "btn sm ghost", onclick: move |_| nav.set(Page::Settings), "Set a path" }
                    }
                }
            }
            if !has_versions {
                Banner { kind: "info", icon: "download",
                    b { "Get a Hytale server version first. " }
                    span { class: "muted", "Download it with the official Hytale downloader or import it from your Hytale launcher." }
                    div { class: "banner-actions",
                        button { class: "btn sm primary", onclick: move |_| nav.set(Page::Versions), "Server versions" }
                    }
                }
            }
            div { class: "stats",
                Stat { icon: "cpu", label: "Machine CPU", value: format!("{:.0}%", host.cpu), sub: format!("{} cores", host.cpus),
                    Spark { values: cpu_hist, max: 100.0, class: "violet" }
                }
                Stat { icon: "ram", label: "Machine RAM", value: format!("{:.0}%", ram_pct), sub: format!("{:.1} / {:.1} GB", host.ram_used_mb as f32 / 1024.0, host.ram_total_mb as f32 / 1024.0),
                    Spark { values: ram_hist, max: 100.0, class: "cyan" }
                }
                Stat { icon: "server", label: "Servers running", value: format!("{running} / {}", instances.len()), sub: "instances" }
                Stat { icon: "users", label: "Players online", value: format!("{players}"), sub: "across all servers" }
            }
            if instances.is_empty() {
                Empty { icon: "server", title: "No servers yet", text: "Create an instance: pick a version, a port and a world seed. You can run as many as your machine handles.",
                    button { class: "btn primary", onclick: move |_| nav.set(Page::Create), Icon { name: "plus" } "Create your first server" }
                }
            } else {
                div { class: "cards",
                    for inst in instances {
                        InstanceCard { key: "{inst.id}", id: inst.id.clone(), nav, tick }
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

    rsx! {
        div { class: "card glass inst-card {status_css}",
            div { class: "card-head", onclick: move |_| nav.set(Page::Instance(id1.clone())),
                div {
                    h3 { "{inst.name}" }
                    div { class: "muted small", "{inst.version} · UDP {inst.port}" }
                }
                span { class: "pill {status_css}", "{status_label}" }
            }
            if auth { div { class: "card-note warn", Icon { name: "key" } "Needs a Hytale login - open the instance" } }
            if let Some(t) = busy { div { class: "card-note", Icon { name: "activity" } "{t.title}: {t.message}" } }
            div { class: "mini-stats",
                div { span { class: "muted", "CPU" } b { "{cpu:.0}%" } }
                div { span { class: "muted", "RAM" } b { "{ram_label}" } }
                div { span { class: "muted", "Players" } b { "{players}" } }
                div { span { class: "muted", "Uptime" } b { "{uptime}" } }
            }
            Spark { values: hist, max: max_ram, class: "cyan" }
            div { class: "card-actions",
                if status.is_active() {
                    button { class: "btn sm", disabled: status == Status::Stopping,
                        onclick: move |_| { let id = id2.clone(); run_job(&st1, toast, move |st| async move { crate::process::stop(&st, &id).await.map(|_| String::new()) }) },
                        Icon { name: "stop" } "Stop"
                    }
                } else {
                    button { class: "btn sm primary",
                        onclick: move |_| { let id = id2.clone(); run_job(&st2, toast, move |st| async move { crate::process::start(&st, &id).await.map(|_| String::new()) }) },
                        Icon { name: "play" } "Start"
                    }
                }
                button { class: "btn sm ghost", onclick: move |_| nav.set(Page::Instance(id.clone())), "Open" }
            }
        }
    }
}
