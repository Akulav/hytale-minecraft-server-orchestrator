//! The Hytale account used to sign servers in **once, for every server** (Hytale's "Server Provider
//! Authentication Guide", token passthrough):
//!
//! 1. Sign in once with the OAuth device code flow (`client_id=hytale-server`,
//!    `scope=openid offline auth:server`): a code to approve in the browser, then a refresh token
//!    (30-day TTL, rotated on every use) and the account's game profile.
//! 2. On every server start: refresh the OAuth tokens (storing the new refresh token), create a game
//!    session (`sessions.hytale.com/game-session/new`) and start the server with
//!    `HYTALE_SERVER_SESSION_TOKEN` / `HYTALE_SERVER_IDENTITY_TOKEN` and `--owner-uuid`. The server never
//!    asks to sign in and refreshes its session by itself while it runs.
//! 3. A keep-alive refreshes the stored token weekly, so it never reaches its 30-day expiry while
//!    Hyve runs now and then.
//!
//! The login lives in `data/hytale-account.json` (owner-only permissions on Linux / macOS).

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::state::{LoginPrompt, Shared, now_ms, read_json};

const OAUTH: &str = "https://oauth.accounts.hytale.com/oauth2";
const PROFILES: &str = "https://account-data.hytale.com/my-account/get-profiles";
const SESSIONS: &str = "https://sessions.hytale.com/game-session/new";
const CLIENT_ID: &str = "hytale-server";
const SCOPE: &str = "openid offline auth:server";

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Profile {
    pub uuid: String,
    pub username: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Account {
    pub refresh_token: String,
    pub owner: String,
    pub profiles: Vec<Profile>,
    /// the profile servers run as
    pub profile: String,
    pub signed_in_at: i64,
    pub refreshed_at: i64,
}

impl Account {
    pub fn profile_name(&self) -> String {
        self.profiles.iter().find(|p| p.uuid == self.profile).map(|p| p.username.clone()).unwrap_or_else(|| self.profile.clone())
    }
}

/// A short-lived access token kept in memory (1 hour), so starting several servers rotates the
/// refresh token only once.
struct Access {
    token: String,
    expires_at: i64,
}

fn cache() -> &'static parking_lot::Mutex<Option<Access>> {
    static C: OnceLock<parking_lot::Mutex<Option<Access>>> = OnceLock::new();
    C.get_or_init(|| parking_lot::Mutex::new(None))
}

/// Serialises token refreshes: a rotated refresh token must be saved before the next refresh.
fn refresh_lock() -> &'static tokio::sync::Mutex<()> {
    static L: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    L.get_or_init(|| tokio::sync::Mutex::new(()))
}

static SIGNING_IN: AtomicBool = AtomicBool::new(false);

pub fn signing_in() -> bool {
    SIGNING_IN.load(Ordering::Relaxed)
}

fn file(state: &Shared) -> std::path::PathBuf {
    state.paths.root.join("hytale-account.json")
}

pub fn load(state: &Shared) -> Option<Account> {
    read_json::<Account>(&file(state)).filter(|a| !a.refresh_token.is_empty())
}

fn save(state: &Shared, a: &Account) -> Result<()> {
    let path = file(state);
    crate::state::write_json(&path, a)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    state.notify();
    Ok(())
}

pub fn sign_out(state: &Shared) -> Result<()> {
    let _ = std::fs::remove_file(file(state));
    *cache().lock() = None;
    state.notify();
    Ok(())
}

pub fn select_profile(state: &Shared, uuid: &str) -> Result<()> {
    let mut a = load(state).ok_or_else(|| anyhow!("not signed in"))?;
    if !a.profiles.iter().any(|p| p.uuid == uuid) {
        bail!("unknown profile");
    }
    a.profile = uuid.to_string();
    save(state, &a)
}

fn oauth_error(v: &Value) -> Option<String> {
    v["error"].as_str().map(str::to_string)
}

/// Signs in with the device code flow (a background task with the sign-in card). Returns a summary.
pub async fn sign_in(state: Shared, task: String) -> Result<String> {
    if SIGNING_IN.swap(true, Ordering::SeqCst) {
        bail!("a sign-in is already waiting for approval");
    }
    let r = sign_in_inner(&state, &task).await;
    SIGNING_IN.store(false, Ordering::SeqCst);
    state.task_login(&task, None);
    r
}

async fn sign_in_inner(state: &Shared, task: &str) -> Result<String> {
    let dev: Value = state
        .http
        .post(format!("{OAUTH}/device/auth"))
        .form(&[("client_id", CLIENT_ID), ("scope", SCOPE)])
        .send()
        .await
        .context("cannot reach Hytale's sign-in service")?
        .error_for_status()?
        .json()
        .await?;
    let device_code = dev["device_code"].as_str().ok_or_else(|| anyhow!("unexpected answer from Hytale: {dev}"))?.to_string();
    let code = dev["user_code"].as_str().unwrap_or_default().to_string();
    let url = dev["verification_uri_complete"].as_str().or_else(|| dev["verification_uri"].as_str()).unwrap_or_default().to_string();
    let mut interval = dev["interval"].as_u64().unwrap_or(5).max(1);
    let expires = now_ms() + dev["expires_in"].as_i64().unwrap_or(900) * 1000;
    state.task_login(task, Some(LoginPrompt { url: url.clone(), code }));
    state.task_update(task, Some("Approve the code in your browser - every server will use this login"), None);
    crate::login::auto_open(state, &url);

    let tokens = loop {
        tokio::time::sleep(Duration::from_secs(interval)).await;
        if now_ms() > expires {
            bail!("the code expired before it was approved - start the sign-in again");
        }
        if state.is_quitting() {
            bail!("cancelled");
        }
        let v: Value = state
            .http
            .post(format!("{OAUTH}/token"))
            .form(&[("client_id", CLIENT_ID), ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"), ("device_code", device_code.as_str())])
            .send()
            .await?
            .json()
            .await
            .unwrap_or_default();
        match oauth_error(&v).as_deref() {
            None if v["refresh_token"].is_string() => break v,
            Some("authorization_pending") | None => continue,
            Some("slow_down") => interval += 5,
            Some("access_denied") => bail!("the sign-in was declined"),
            Some("expired_token") => bail!("the code expired - start the sign-in again"),
            Some(other) => bail!("Hytale sign-in failed: {other}"),
        }
    };
    state.task_login(task, None);
    state.task_update(task, Some("Approved - reading your game profiles…"), None);
    let access = tokens["access_token"].as_str().unwrap_or_default().to_string();
    let refresh = tokens["refresh_token"].as_str().unwrap_or_default().to_string();
    let (owner, profiles) = get_profiles(state, &access).await?;
    if profiles.is_empty() {
        bail!("this Hytale account has no game profile (does it own Hytale?)");
    }
    let account = Account {
        refresh_token: refresh,
        owner,
        profile: profiles[0].uuid.clone(),
        profiles,
        signed_in_at: now_ms(),
        refreshed_at: now_ms(),
    };
    save(state, &account)?;
    *cache().lock() = Some(Access { token: access, expires_at: now_ms() + tokens["expires_in"].as_i64().unwrap_or(3600) * 1000 });
    Ok(format!("Signed in as {} - servers will start without asking", account.profile_name()))
}

async fn get_profiles(state: &Shared, access: &str) -> Result<(String, Vec<Profile>)> {
    let v: Value = state.http.get(PROFILES).bearer_auth(access).send().await?.error_for_status().context("cannot read the game profiles")?.json().await?;
    let profiles = v["profiles"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| Some(Profile { uuid: p["uuid"].as_str()?.to_string(), username: p["username"].as_str().unwrap_or_default().to_string() }))
        .collect();
    Ok((v["owner"].as_str().unwrap_or_default().to_string(), profiles))
}

/// A valid access token: the cached one, or a refresh (which rotates and saves the refresh token).
async fn access_token(state: &Shared) -> Result<String> {
    let _guard = refresh_lock().lock().await;
    if let Some(a) = cache().lock().as_ref()
        && a.expires_at - now_ms() > 5 * 60_000
    {
        return Ok(a.token.clone());
    }
    let mut account = load(state).ok_or_else(|| anyhow!("not signed in"))?;
    let v: Value = state
        .http
        .post(format!("{OAUTH}/token"))
        .form(&[("client_id", CLIENT_ID), ("grant_type", "refresh_token"), ("refresh_token", account.refresh_token.as_str())])
        .send()
        .await
        .context("cannot reach Hytale's sign-in service")?
        .json()
        .await
        .unwrap_or_default();
    if let Some(e) = oauth_error(&v) {
        if e == "invalid_grant" {
            // expired (30 days unused) or revoked: the user has to sign in again
            sign_out(state)?;
            bail!("the saved Hytale login expired - sign in again in Settings → Hytale account");
        }
        bail!("Hytale token refresh failed: {e}");
    }
    let access = v["access_token"].as_str().ok_or_else(|| anyhow!("unexpected answer from Hytale"))?.to_string();
    if let Some(r) = v["refresh_token"].as_str() {
        account.refresh_token = r.to_string();
    }
    account.refreshed_at = now_ms();
    save(state, &account)?;
    *cache().lock() = Some(Access { token: access.clone(), expires_at: now_ms() + v["expires_in"].as_i64().unwrap_or(3600) * 1000 });
    Ok(access)
}

/// Tokens for one server start: (session token, identity token, owner profile uuid). None when no
/// account is signed in.
pub async fn session_for_server(state: &Shared) -> Result<Option<(String, String, String)>> {
    let Some(account) = load(state) else { return Ok(None) };
    let access = access_token(state).await?;
    let v: Value = state
        .http
        .post(SESSIONS)
        .bearer_auth(&access)
        .json(&serde_json::json!({ "uuid": account.profile }))
        .send()
        .await
        .context("cannot reach Hytale's session service")?
        .error_for_status()
        .context("Hytale refused a game session (500 servers per account at most)")?
        .json()
        .await?;
    let session = v["sessionToken"].as_str().ok_or_else(|| anyhow!("no session token in Hytale's answer"))?.to_string();
    let identity = v["identityToken"].as_str().ok_or_else(|| anyhow!("no identity token in Hytale's answer"))?.to_string();
    Ok(Some((session, identity, account.profile)))
}

/// Keeps the saved login alive: refreshes it when it is older than a week (refresh tokens expire after
/// 30 days unused).
pub fn spawn_keepalive(state: Shared) {
    let st = state.clone();
    state.rt.spawn(async move {
        tokio::time::sleep(Duration::from_secs(30)).await;
        loop {
            if st.is_quitting() {
                break;
            }
            if let Some(a) = load(&st)
                && now_ms() - a.refreshed_at > 7 * 86_400_000
                && let Err(e) = access_token(&st).await
            {
                crate::logln(&format!("[account] keep-alive refresh failed: {e:#}"));
            }
            tokio::time::sleep(Duration::from_secs(6 * 3600)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_names_and_defaults() {
        let a = Account {
            refresh_token: "r".into(),
            profiles: vec![Profile { uuid: "u1".into(), username: "Steve".into() }, Profile { uuid: "u2".into(), username: "Alex".into() }],
            profile: "u2".into(),
            ..Default::default()
        };
        assert_eq!(a.profile_name(), "Alex");
        let back: Account = serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
        assert_eq!(back, a);
        let empty: Account = serde_json::from_str("{}").unwrap();
        assert!(empty.refresh_token.is_empty());
    }

    #[test]
    fn oauth_errors_are_read() {
        assert_eq!(oauth_error(&serde_json::json!({"error": "authorization_pending"})).as_deref(), Some("authorization_pending"));
        assert_eq!(oauth_error(&serde_json::json!({"access_token": "x"})), None);
    }
}
