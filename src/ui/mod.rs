//! The user interface: Dioxus components rendered on the server (LiveView). The same UI is shown in the
//! desktop window and in any browser. Every session reads the shared `AppState` directly and re-renders
//! when it changes (or once a second for telemetry).

mod create;
mod dashboard;
mod instance;
mod settings;
mod versions;
mod widgets;

use dioxus::prelude::*;
use std::future::Future;
use std::time::Duration;

use crate::state::{Mode, Shared, TaskStatus, VERSION};
pub use widgets::*;

/// Root context: the shared application state.
#[derive(Clone)]
pub struct Ctx(pub Shared);

#[derive(Clone, Debug, PartialEq)]
pub enum Page {
    Dashboard,
    Instance(String),
    Create,
    Versions,
    Tasks,
    Settings,
    About,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Notice {
    pub ok: bool,
    pub text: String,
    pub at: i64,
}

/// Toast messages for the current session.
#[derive(Clone, Copy)]
pub struct Toast(pub Signal<Option<Notice>>);

impl Toast {
    pub fn ok(mut self, text: impl Into<String>) {
        self.0.set(Some(Notice { ok: true, text: text.into(), at: crate::state::now_ms() }));
    }
    pub fn err(mut self, text: impl Into<String>) {
        self.0.set(Some(Notice { ok: false, text: text.into(), at: crate::state::now_ms() }));
    }
    /// Shows an error, or `ok_text` on success.
    pub fn result<T>(self, r: anyhow::Result<T>, ok_text: &str) {
        match r {
            Ok(_) if ok_text.is_empty() => {}
            Ok(_) => self.ok(ok_text),
            Err(e) => self.err(format!("{e:#}")),
        }
    }
}

pub fn use_state() -> Shared {
    use_context::<Ctx>().0
}

pub fn use_toast() -> Toast {
    use_context::<Toast>()
}

/// Runs `job` on the main runtime (it may take a while) and toasts its result.
pub fn run_job<F, Fut>(state: &Shared, toast: Toast, job: F)
where
    F: FnOnce(Shared) -> Fut + Send + 'static,
    Fut: Future<Output = anyhow::Result<String>> + Send + 'static,
{
    let st = state.clone();
    let handle = state.rt.spawn(async move { job(st).await });
    spawn(async move {
        match handle.await {
            Ok(Ok(msg)) if msg.is_empty() => {}
            Ok(Ok(msg)) => toast.ok(msg),
            Ok(Err(e)) => toast.err(format!("{e:#}")),
            Err(e) => toast.err(format!("internal error: {e}")),
        }
    });
}

#[component]
pub fn App() -> Element {
    let state = use_state();
    let nav = use_signal(|| Page::Dashboard);
    use_context_provider(|| Toast(Signal::new(None)));
    let mut tick = use_signal(|| 0u64);

    // re-render on every state change, and at least once a second (uptime, telemetry)
    let st = state.clone();
    use_future(move || {
        let st = st.clone();
        async move {
            let mut rx = st.subscribe();
            loop {
                let _ = tokio::time::timeout(Duration::from_millis(1000), rx.changed()).await;
                // coalesce bursts of console output into one render
                tokio::time::sleep(Duration::from_millis(120)).await;
                rx.borrow_and_update();
                tick += 1;
            }
        }
    });
    let t = tick();
    let page = nav();

    rsx! {
        div { class: "app",
            Sidebar { nav, tick: t }
            main { class: "content",
                match page {
                    Page::Dashboard => rsx! { dashboard::Dashboard { nav, tick: t } },
                    Page::Instance(id) => rsx! { instance::InstancePage { key: "{id}", id: id.clone(), nav, tick: t } },
                    Page::Create => rsx! { create::CreatePage { nav, tick: t } },
                    Page::Versions => rsx! { versions::VersionsPage { nav, tick: t } },
                    Page::Tasks => rsx! { TasksPage { tick: t } },
                    Page::Settings => rsx! { settings::SettingsPage { tick: t } },
                    Page::About => rsx! { settings::AboutPage { tick: t } },
                }
            }
            ToastView { tick: t }
        }
    }
}

#[component]
fn ToastView(tick: u64) -> Element {
    let mut toast = use_toast();
    let _ = tick;
    let Some(n) = toast.0() else { return rsx! {} };
    if crate::state::now_ms() - n.at > if n.ok { 5000 } else { 12000 } {
        return rsx! {};
    }
    rsx! {
        div { class: if n.ok { "toast ok" } else { "toast err" },
            onclick: move |_| toast.0.set(None),
            span { class: "toast-icon", if n.ok { "✓" } else { "!" } }
            span { "{n.text}" }
        }
    }
}

#[component]
fn Sidebar(nav: Signal<Page>, tick: u64) -> Element {
    let state = use_state();
    let toast = use_toast();
    let _ = tick;
    let instances = state.instances.read().clone();
    let running_tasks = state.tasks.read().iter().filter(|t| t.status == TaskStatus::Running).count();
    let update = state.updates.read().available.clone();
    let urls = state.urls.read().clone();
    let current = nav();
    let item = |p: Page| if current == p { "nav-item active" } else { "nav-item" };
    let desktop = state.mode == Mode::Desktop;
    let no_instances = instances.is_empty();
    let create_class = format!("{} add", item(Page::Create));
    let first_url = urls.first().cloned().unwrap_or_default();
    let net_url = urls.get(1).cloned();

    rsx! {
        aside { class: "sidebar glass",
            div { class: "brand",
                img { src: "/assets/logo.svg", width: "36", height: "36", alt: "" }
                div {
                    div { class: "brand-name", "HyOrchestrator" }
                    div { class: "brand-sub", "Hytale server orchestrator" }
                }
            }
            nav { class: "nav",
                button { class: item(Page::Dashboard), onclick: move |_| nav.set(Page::Dashboard), Icon { name: "grid" } "Dashboard" }
                div { class: "nav-section",
                    span { "Instances" }
                    button { class: "nav-add", title: "New instance", onclick: move |_| nav.set(Page::Create), Icon { name: "plus" } }
                }
                for inst in instances {
                    {
                        let id = inst.id.clone();
                        let status = state.status(&inst.id);
                        let active = current == Page::Instance(inst.id.clone());
                        let players = state.runtimes.read().get(&inst.id).map(|r| r.players.len()).unwrap_or(0);
                        rsx! {
                            button { key: "{inst.id}", class: if active { "nav-item inst active" } else { "nav-item inst" },
                                onclick: move |_| nav.set(Page::Instance(id.clone())),
                                span { class: "dot {status.css()}" }
                                span { class: "nav-label", "{inst.name}" }
                                if players > 0 { span { class: "badge", "{players}" } }
                            }
                        }
                    }
                }
                if no_instances {
                    button { class: create_class, onclick: move |_| nav.set(Page::Create), Icon { name: "plus" } "New instance" }
                }
                div { class: "nav-section", "Manage" }
                button { class: item(Page::Versions), onclick: move |_| nav.set(Page::Versions), Icon { name: "layers" } "Server versions" }
                button { class: item(Page::Tasks), onclick: move |_| nav.set(Page::Tasks), Icon { name: "activity" } "Tasks"
                    if running_tasks > 0 { span { class: "badge pulse", "{running_tasks}" } }
                }
                button { class: item(Page::Settings), onclick: move |_| nav.set(Page::Settings), Icon { name: "settings" } "Settings" }
                button { class: item(Page::About), onclick: move |_| nav.set(Page::About), Icon { name: "info" } "About"
                    if update.is_some() { span { class: "badge accent", "update" } }
                }
            }
            div { class: "sidebar-foot",
                if desktop {
                    button { class: "web-link", title: "Open this UI in your browser",
                        onclick: move |_| {
                            if let Err(e) = open::that_detached(&first_url) { toast.err(format!("cannot open the browser: {e}")) }
                        },
                        Icon { name: "link" } "Open in browser"
                    }
                } else if let Some(u) = net_url {
                    div { class: "web-link static", title: "{u}", Icon { name: "link" } "Web UI on the network" }
                }
                div { class: "made-by", "v{VERSION} · Made by Akulav" }
            }
        }
    }
}

#[component]
fn TasksPage(tick: u64) -> Element {
    let state = use_state();
    let _ = tick;
    let tasks = state.tasks.read().clone();
    rsx! {
        div { class: "page",
            PageHeader { title: "Tasks", subtitle: "Downloads, updates, backups and restores running in the background." }
            if tasks.is_empty() {
                Empty { icon: "activity", title: "Nothing has run yet", text: "Downloads, updates and backups show up here with their progress and logs." }
            }
            for t in tasks {
                TaskCard { key: "{t.id}", task: t, expanded: false }
            }
        }
    }
}
