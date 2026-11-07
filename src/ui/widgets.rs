//! Small building blocks: icons, headers, stat cards, sparklines, task cards, confirm buttons.

use dioxus::prelude::*;

use crate::state::{Task, TaskStatus};

/// Feather-style line icons (inline SVG).
fn icon_svg(name: &str) -> &'static str {
    match name {
        "grid" => r#"<rect x="3" y="3" width="7" height="7" rx="1.5"/><rect x="14" y="3" width="7" height="7" rx="1.5"/><rect x="3" y="14" width="7" height="7" rx="1.5"/><rect x="14" y="14" width="7" height="7" rx="1.5"/>"#,
        "plus" => r#"<path d="M12 5v14M5 12h14"/>"#,
        "layers" => r#"<path d="M12 2 2 7l10 5 10-5-10-5z"/><path d="m2 17 10 5 10-5"/><path d="m2 12 10 5 10-5"/>"#,
        "activity" => r#"<path d="M22 12h-4l-3 9L9 3l-3 9H2"/>"#,
        "settings" => r#"<circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/>"#,
        "info" => r#"<circle cx="12" cy="12" r="10"/><path d="M12 16v-4M12 8h.01"/>"#,
        "link" => r#"<path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"/><path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"/>"#,
        "play" => r#"<path d="m6 4 14 8-14 8z"/>"#,
        "stop" => r#"<rect x="5" y="5" width="14" height="14" rx="2"/>"#,
        "restart" => r#"<path d="M3 12a9 9 0 1 0 3-6.7L3 8"/><path d="M3 3v5h5"/>"#,
        "zap" => r#"<path d="M13 2 3 14h9l-1 8 10-12h-9l1-8z"/>"#,
        "cpu" => r#"<rect x="4" y="4" width="16" height="16" rx="2"/><rect x="9" y="9" width="6" height="6"/><path d="M9 1v3M15 1v3M9 20v3M15 20v3M20 9h3M20 14h3M1 9h3M1 14h3"/>"#,
        "ram" => r#"<rect x="2" y="7" width="20" height="10" rx="2"/><path d="M6 11v2M10 11v2M14 11v2M18 11v2M6 17v3M18 17v3"/>"#,
        "users" => r#"<path d="M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2"/><circle cx="9" cy="7" r="4"/><path d="M23 21v-2a4 4 0 0 0-3-3.87M16 3.13a4 4 0 0 1 0 7.75"/>"#,
        "clock" => r#"<circle cx="12" cy="12" r="10"/><path d="M12 6v6l4 2"/>"#,
        "download" => r#"<path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><path d="m7 10 5 5 5-5M12 15V3"/>"#,
        "upload" => r#"<path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><path d="m17 8-5-5-5 5M12 3v12"/>"#,
        "archive" => r#"<path d="M21 8v13H3V8M1 3h22v5H1zM10 12h4"/>"#,
        "globe" => r#"<circle cx="12" cy="12" r="10"/><path d="M2 12h20M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"/>"#,
        "box" => r#"<path d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z"/><path d="M3.27 6.96 12 12.01l8.73-5.05M12 22.08V12"/>"#,
        "terminal" => r#"<path d="m4 17 6-6-6-6M12 19h8"/>"#,
        "shield" => r#"<path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/>"#,
        "trash" => r#"<path d="M3 6h18M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6M10 11v6M14 11v6M9 6V4a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v2"/>"#,
        "folder" => r#"<path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/>"#,
        "refresh" => r#"<path d="M23 4v6h-6M1 20v-6h6"/><path d="M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15"/>"#,
        "alert" => r#"<path d="M10.29 3.86 1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0zM12 9v4M12 17h.01"/>"#,
        "key" => r#"<path d="M21 2l-2 2m-7.61 7.61a5.5 5.5 0 1 1-7.78 7.78 5.5 5.5 0 0 1 7.78-7.78zm0 0L15.5 7.5m0 0 3 3L22 7l-3-3m-3.5 3.5L19 4"/>"#,
        "server" => r#"<rect x="2" y="2" width="20" height="8" rx="2"/><rect x="2" y="14" width="20" height="8" rx="2"/><path d="M6 6h.01M6 18h.01"/>"#,
        _ => r#"<circle cx="12" cy="12" r="9"/>"#,
    }
}

#[component]
pub fn Icon(name: &'static str) -> Element {
    let svg = format!(
        r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">{}</svg>"#,
        icon_svg(name)
    );
    rsx! { span { class: "ico", dangerous_inner_html: svg } }
}

#[component]
pub fn PageHeader(title: String, subtitle: String, children: Element) -> Element {
    rsx! {
        header { class: "page-header",
            div {
                h1 { "{title}" }
                if !subtitle.is_empty() { p { class: "muted", "{subtitle}" } }
            }
            div { class: "header-actions", {children} }
        }
    }
}

#[component]
pub fn Empty(icon: &'static str, title: String, text: String, children: Element) -> Element {
    rsx! {
        div { class: "empty glass",
            div { class: "empty-icon", Icon { name: icon } }
            h3 { "{title}" }
            p { class: "muted", "{text}" }
            {children}
        }
    }
}

#[component]
pub fn Banner(kind: &'static str, icon: &'static str, children: Element) -> Element {
    rsx! {
        div { class: "banner {kind}",
            Icon { name: icon }
            div { class: "banner-body", {children} }
        }
    }
}

/// A small line chart of `values` (0..max).
#[component]
pub fn Spark(values: Vec<f32>, max: f32, class: &'static str) -> Element {
    if values.len() < 2 {
        return rsx! { svg { class: "spark {class}", view_box: "0 0 100 30", preserve_aspect_ratio: "none" } };
    }
    let max = if max > 0.0 { max } else { values.iter().cloned().fold(1.0, f32::max) };
    let n = values.len() as f32 - 1.0;
    let pts: Vec<String> = values.iter().enumerate().map(|(i, v)| format!("{:.2},{:.2}", i as f32 * 100.0 / n, 29.0 - (v / max).clamp(0.0, 1.0) * 27.0)).collect();
    let line = pts.join(" ");
    let area = format!("0,30 {line} 100,30");
    rsx! {
        svg { class: "spark {class}", view_box: "0 0 100 30", preserve_aspect_ratio: "none",
            polygon { class: "area", points: "{area}" }
            polyline { class: "line", points: "{line}" }
        }
    }
}

#[component]
pub fn Stat(icon: &'static str, label: String, value: String, sub: String, children: Element) -> Element {
    rsx! {
        div { class: "stat glass",
            div { class: "stat-head", Icon { name: icon } span { "{label}" } }
            div { class: "stat-value", "{value}" }
            if !sub.is_empty() { div { class: "stat-sub muted", "{sub}" } }
            {children}
        }
    }
}

#[component]
pub fn Progress(value: Option<f32>) -> Element {
    match value {
        Some(v) => {
            let pct = (v.clamp(0.0, 1.0) * 100.0).round();
            rsx! { div { class: "progress", div { class: "bar", style: "width: {pct}%" } } }
        }
        None => rsx! { div { class: "progress indeterminate", div { class: "bar" } } },
    }
}

/// Text with clickable http(s) links (for logs that print a login URL).
#[component]
pub fn Linkified(text: String) -> Element {
    let mut parts: Vec<(bool, String)> = Vec::new();
    let mut rest = text.as_str();
    while let Some(start) = rest.find("https://").or_else(|| rest.find("http://")) {
        if start > 0 {
            parts.push((false, rest[..start].to_string()));
        }
        let end = rest[start..].find(|c: char| c.is_whitespace()).map(|e| start + e).unwrap_or(rest.len());
        parts.push((true, rest[start..end].trim_end_matches(['.', ',', ')']).to_string()));
        rest = &rest[end..];
    }
    if !rest.is_empty() {
        parts.push((false, rest.to_string()));
    }
    rsx! {
        for (i, (is_link, s)) in parts.into_iter().enumerate() {
            if is_link {
                a { key: "{i}", href: "{s}", target: "_blank", rel: "noopener", "{s}" }
            } else {
                span { key: "{i}", "{s}" }
            }
        }
    }
}

/// "Sign in to Hytale": the device code, big, with a button that opens the page (the link already
/// carries the code, so there is nothing to type) and a copy button.
#[component]
pub fn LoginCard(title: String, url: String, code: String, note: String) -> Element {
    let copy = if code.is_empty() { url.clone() } else { code.clone() };
    rsx! {
        div { class: "login-card",
            div { class: "login-icon", Icon { name: "key" } }
            div { class: "login-body",
                b { "{title}" }
                p { class: "muted small", "{note}" }
                if !code.is_empty() {
                    div { class: "login-code-row",
                        span { class: "login-code", "{code}" }
                        button { class: "btn sm ghost", "data-copy": "{copy}", "Copy code" }
                    }
                }
                div { class: "row",
                    if !url.is_empty() {
                        a { class: "btn primary", href: "{url}", target: "_blank", rel: "noopener", Icon { name: "link" } "Open sign-in page" }
                    }
                    span { class: "muted small", "Waiting for your approval… this continues by itself." }
                }
            }
        }
    }
}

#[component]
pub fn TaskCard(task: Task, expanded: bool) -> Element {
    let mut open = use_signal(move || expanded || task.status == TaskStatus::Running);
    let (cls, label) = match task.status {
        TaskStatus::Running => ("running", "Running"),
        TaskStatus::Done => ("done", "Done"),
        TaskStatus::Failed => ("failed", "Failed"),
    };
    let tail: Vec<String> = task.log.iter().rev().take(60).rev().cloned().collect();
    let when = crate::state::fmt_time(task.started_at);
    rsx! {
        div { class: "task glass {cls}",
            div { class: "task-head", onclick: move |_| open.toggle(),
                span { class: "pill {cls}", "{label}" }
                b { "{task.title}" }
                span { class: "muted small grow", "{when}" }
                span { class: "muted small", if open() { "▾" } else { "▸" } }
            }
            div { class: "task-msg", Linkified { text: task.message.clone() } }
            if let Some(l) = task.login.clone() {
                LoginCard { title: "Sign in to Hytale", url: l.url, code: l.code,
                    note: "The official Hytale downloader needs your Hytale account once (the one that owns the game). Open the page, check the code matches and approve. The login is saved for next time." }
            }
            if task.status == TaskStatus::Running && task.login.is_none() { Progress { value: task.progress } }
            if open() && !tail.is_empty() {
                pre { class: "task-log",
                    for (i, l) in tail.into_iter().enumerate() {
                        div { key: "{i}", Linkified { text: l } }
                    }
                }
            }
        }
    }
}

/// A button that asks "Sure?" before running `onconfirm`.
#[component]
pub fn ConfirmButton(label: String, confirm: String, class: &'static str, disabled: bool, onconfirm: EventHandler<()>) -> Element {
    let mut asking = use_signal(|| false);
    if asking() {
        rsx! {
            span { class: "confirm",
                span { class: "confirm-q", "{confirm}" }
                button { class: "btn danger sm", onclick: move |_| { asking.set(false); onconfirm.call(()); }, "Yes" }
                button { class: "btn ghost sm", onclick: move |_| asking.set(false), "No" }
            }
        }
    } else {
        rsx! { button { class: "btn {class}", disabled, onclick: move |_| asking.set(true), "{label}" } }
    }
}
