//! The local web server (axum): the Dioxus LiveView UI (same UI in the desktop window and in a browser),
//! static assets, mod uploads (drag & drop) and a small JSON status API.
//!
//! Every request needs the access token: `?token=…` once (it becomes a cookie) or the `hyo_token`
//! cookie. The token is in settings.json and printed in headless mode.

use axum::{
    Router,
    extract::{DefaultBodyLimit, Multipart, Path, Query, Request, State, WebSocketUpgrade},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    middleware::{self, Next},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use dioxus_liveview::LiveViewPool;
use std::collections::HashMap;
use std::net::SocketAddr;

use crate::state::{Shared, VERSION};

const COOKIE: &str = "hyo_token";
const STYLE: &str = include_str!("../assets/style.css");
const APP_JS: &str = include_str!("../assets/app.js");
const LOGO: &str = include_str!("../assets/logo.svg");

#[derive(Clone)]
struct Web {
    state: Shared,
    pool: LiveViewPool,
}

pub fn router(state: Shared) -> Router {
    let web = Web { state, pool: LiveViewPool::new() };
    Router::new()
        .route("/", get(index))
        .route("/ws", get(ws))
        .route("/api/status", get(status))
        .route("/api/instances/{id}/mods", post(upload_mods).layer(DefaultBodyLimit::max(1024 * 1024 * 1024)))
        .route_layer(middleware::from_fn_with_state(web.clone(), auth))
        // public: assets (no secrets in them) and the token prompt
        .route("/assets/style.css", get(|| async { ([(header::CONTENT_TYPE, "text/css; charset=utf-8")], STYLE) }))
        .route("/assets/app.js", get(|| async { ([(header::CONTENT_TYPE, "text/javascript; charset=utf-8")], APP_JS) }))
        .route("/assets/logo.svg", get(|| async { ([(header::CONTENT_TYPE, "image/svg+xml")], LOGO) }))
        .route("/favicon.ico", get(|| async { ([(header::CONTENT_TYPE, "image/svg+xml")], LOGO) }))
        .with_state(web)
}

fn cookie_token(headers: &HeaderMap) -> Option<String> {
    headers.get_all(header::COOKIE).iter().filter_map(|v| v.to_str().ok()).flat_map(|v| v.split(';')).find_map(|kv| {
        let (k, v) = kv.trim().split_once('=')?;
        (k == COOKIE).then(|| v.to_string())
    })
}

/// Constant-time comparison (the token is a secret).
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// WebSocket and API calls must come from our own page (no cross-site requests).
fn same_origin(headers: &HeaderMap) -> bool {
    let Some(origin) = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) else { return true };
    let host = headers.get(header::HOST).and_then(|v| v.to_str().ok()).unwrap_or_default();
    origin.split_once("://").map(|(_, h)| h) == Some(host)
}

async fn auth(State(web): State<Web>, Query(q): Query<HashMap<String, String>>, req: Request, next: Next) -> Response {
    let token = web.state.settings.read().web_token.clone();
    if let Some(t) = q.get("token")
        && same(t, &token)
    {
        // remember it in a cookie and drop it from the address bar
        let mut resp = Redirect::to(req.uri().path()).into_response();
        let cookie = format!("{COOKIE}={token}; Path=/; HttpOnly; SameSite=Strict; Max-Age=31536000");
        resp.headers_mut().insert(header::SET_COOKIE, HeaderValue::from_str(&cookie).unwrap());
        return resp;
    }
    let ok = cookie_token(req.headers()).is_some_and(|c| same(&c, &token));
    if !ok {
        return (StatusCode::UNAUTHORIZED, Html(locked_page())).into_response();
    }
    if !same_origin(req.headers()) {
        return (StatusCode::FORBIDDEN, "cross-origin request refused").into_response();
    }
    next.run(req).await
}

fn locked_page() -> String {
    format!(
        r#"<!doctype html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>HyOrchestrator</title><link rel="icon" href="/assets/logo.svg"><link rel="stylesheet" href="/assets/style.css"></head>
<body class="locked"><main class="lock-card glass"><img src="/assets/logo.svg" width="64" alt="">
<h1>HyOrchestrator</h1><p>This page needs its access link.</p>
<p class="muted">Open the address HyOrchestrator printed when it started (it ends in <code>?token=…</code>), or run
<code>hyorchestrator --print-url</code> on the server.</p><p class="muted small">v{VERSION} · Made by Akulav</p></main></body></html>"#
    )
}

async fn index() -> impl IntoResponse {
    let glue = dioxus_liveview::interpreter_glue("/ws");
    Html(format!(
        r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>HyOrchestrator</title><link rel="icon" href="/assets/logo.svg"><link rel="stylesheet" href="/assets/style.css"></head>
<body><div id="main"><div class="boot"><img src="/assets/logo.svg" width="72" alt=""><p>Connecting…</p></div></div>
<div id="conn-lost">Connection lost - reconnecting…</div>
<script src="/assets/app.js"></script>
{glue}
</body></html>"#
    ))
}

async fn ws(State(web): State<Web>, headers: HeaderMap, upgrade: WebSocketUpgrade) -> Response {
    if !same_origin(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let state = web.state.clone();
    let pool = web.pool.clone();
    upgrade.on_upgrade(move |socket| async move {
        let _ = pool
            .launch_virtualdom(dioxus_liveview::axum_socket(socket), move || {
                dioxus::prelude::VirtualDom::new(crate::ui::App).with_root_context(crate::ui::Ctx(state))
            })
            .await;
    })
}

async fn upload_mods(State(web): State<Web>, Path(id): Path<String>, mut form: Multipart) -> Response {
    let state = web.state.clone();
    let mut saved = Vec::new();
    let mut errors = Vec::new();
    loop {
        match form.next_field().await {
            Ok(Some(field)) => {
                let name = field.file_name().unwrap_or_default().to_string();
                match field.bytes().await {
                    Ok(bytes) => {
                        let (st, i, n) = (state.clone(), id.clone(), name.clone());
                        match tokio::task::spawn_blocking(move || crate::instance::add_mod(&st, &i, &n, &bytes)).await {
                            Ok(Ok(n)) => saved.push(n),
                            Ok(Err(e)) => errors.push(format!("{e:#}")),
                            Err(e) => errors.push(e.to_string()),
                        }
                    }
                    Err(e) => errors.push(format!("{name}: {e}")),
                }
            }
            Ok(None) => break,
            Err(e) => {
                errors.push(e.to_string());
                break;
            }
        }
    }
    if !saved.is_empty() {
        state.system_log(&id, format!("Mods added: {}", saved.join(", ")));
    }
    let code = if errors.is_empty() { StatusCode::OK } else { StatusCode::BAD_REQUEST };
    (code, axum::Json(serde_json::json!({ "saved": saved, "errors": errors }))).into_response()
}

async fn status(State(web): State<Web>) -> impl IntoResponse {
    let st = &web.state;
    let instances: Vec<serde_json::Value> = st
        .instances
        .read()
        .iter()
        .map(|i| {
            let rt = st.runtimes.read();
            let r = rt.get(&i.id);
            serde_json::json!({
                "id": i.id, "name": i.name, "version": i.version, "port": i.port,
                "status": r.map(|r| r.status).unwrap_or_default(),
                "players": r.map(|r| r.players.iter().cloned().collect::<Vec<_>>()).unwrap_or_default(),
                "cpu": r.map(|r| r.cpu).unwrap_or(0.0),
                "ram_mb": r.map(|r| r.ram_mb).unwrap_or(0),
                "uptime_ms": r.and_then(|r| r.started_at).map(|s| crate::state::now_ms() - s),
            })
        })
        .collect();
    let host = st.host.read().clone();
    axum::Json(serde_json::json!({
        "app": "HyOrchestrator", "version": VERSION,
        "host": { "cpu": host.cpu, "ram_used_mb": host.ram_used_mb, "ram_total_mb": host.ram_total_mb },
        "instances": instances,
    }))
}

/// Binds the web server: `port`, or the next free one. Returns the bound address.
pub async fn serve(state: Shared, host: std::net::IpAddr, port: u16) -> anyhow::Result<SocketAddr> {
    let mut last_err = None;
    for p in port..port.saturating_add(20) {
        match tokio::net::TcpListener::bind((host, p)).await {
            Ok(listener) => {
                let addr = listener.local_addr()?;
                let app = router(state.clone());
                state.rt.spawn(async move {
                    if let Err(e) = axum::serve(listener, app.into_make_service()).await {
                        crate::logln(&format!("[web] server stopped: {e}"));
                    }
                });
                return Ok(addr);
            }
            Err(e) => last_err = Some(e),
        }
    }
    Err(anyhow::anyhow!("no free port from {port}: {}", last_err.map(|e| e.to_string()).unwrap_or_default()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_cookie_and_checks_origin() {
        let mut h = HeaderMap::new();
        h.insert(header::COOKIE, HeaderValue::from_static("a=1; hyo_token=abc; b=2"));
        assert_eq!(cookie_token(&h).as_deref(), Some("abc"));
        h.insert(header::HOST, HeaderValue::from_static("127.0.0.1:7520"));
        assert!(same_origin(&h));
        h.insert(header::ORIGIN, HeaderValue::from_static("http://127.0.0.1:7520"));
        assert!(same_origin(&h));
        h.insert(header::ORIGIN, HeaderValue::from_static("http://evil.example"));
        assert!(!same_origin(&h));
        assert!(same("abc", "abc") && !same("abc", "abd") && !same("abc", "ab"));
    }
}
