//! Hytale device sign-in prompts, as printed by the official Hytale Downloader CLI:
//! ```text
//! Please visit the following URL to authenticate:
//! https://oauth.accounts.hytale.com/oauth2/device/verify?user_code=DAdCQ6dY
//! Or visit the following URL and enter the code:
//! https://oauth.accounts.hytale.com/oauth2/device/verify
//! Authorization code: DAdCQ6dY
//! ```
//! and by a server after `/auth login device` (`Visit: https://accounts.hytale.com/device`,
//! `Enter code: ABCD-1234`, `Or visit: …?user_code=ABCD-1234`). The link with `user_code` needs no typing:
//! open it, approve, and the tool / server continues by itself.

use std::sync::OnceLock;

use crate::state::{LoginPrompt, Mode, Shared};

#[derive(Debug, PartialEq)]
pub enum Bit {
    /// a sign-in link (with the code in it when `user_code=` is present)
    Url(String),
    Code(String),
    /// a download progress line: percent 0-100
    Progress(f32),
}

fn re(cell: &'static OnceLock<regex::Regex>, pattern: &str) -> &'static regex::Regex {
    cell.get_or_init(|| regex::Regex::new(pattern).unwrap())
}

pub fn parse(line: &str) -> Vec<Bit> {
    static URL: OnceLock<regex::Regex> = OnceLock::new();
    static CODE: OnceLock<regex::Regex> = OnceLock::new();
    static PARAM: OnceLock<regex::Regex> = OnceLock::new();
    static PCT: OnceLock<regex::Regex> = OnceLock::new();
    let mut out = Vec::new();
    if let Some(m) = re(&URL, r"https://(?:[A-Za-z0-9-]+\.)*hytale\.com/\S*(?:device|verify)\S*").find(line) {
        let url = m.as_str().trim_end_matches(['.', ',', ')', '"', '\'']).to_string();
        if let Some(c) = re(&PARAM, r"[?&]user_code=([A-Za-z0-9-]+)").captures(&url) {
            out.push(Bit::Code(c[1].to_string()));
        }
        out.push(Bit::Url(url));
    } else if let Some(c) = re(&CODE, r"(?i)\b(?:authorization|user|enter|device)\s+code\s*:\s*([A-Za-z0-9-]{4,})").captures(line) {
        out.push(Bit::Code(c[1].to_string()));
    } else if let Some(c) = re(&PCT, r"(\d{1,3}(?:\.\d+)?)\s?%").captures(line)
        && let Ok(p) = c[1].parse::<f32>()
        && p <= 100.0
    {
        out.push(Bit::Progress(p));
    }
    out
}

/// Text that belongs to a sign-in prompt (it does not mean the sign-in is done).
pub fn is_prompt_text(line: &str) -> bool {
    let l = line.to_lowercase();
    ["visit", "authenticat", "authoriz", "code", "url", "waiting", "expire", "browser", "device"].iter().any(|k| l.contains(k))
}

/// Merges a parsed bit into a prompt. Returns true when the prompt changed.
pub fn merge(prompt: &mut LoginPrompt, bit: &Bit) -> bool {
    match bit {
        // the link that carries the code wins over the plain one
        Bit::Url(u) if prompt.url.is_empty() || (u.contains("user_code=") && !prompt.url.contains("user_code=")) => {
            prompt.url = u.clone();
            true
        }
        Bit::Code(c) if prompt.code != *c => {
            prompt.code = c.clone();
            true
        }
        _ => false,
    }
}

/// In the desktop window the browser is on this computer: open the sign-in page right away.
pub fn auto_open(state: &Shared, url: &str) {
    if state.mode == Mode::Desktop && url.starts_with("https://") {
        let _ = open::that_detached(url);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_downloader_prompt() {
        let mut p = LoginPrompt::default();
        for line in [
            "Please visit the following URL to authenticate:",
            "https://oauth.accounts.hytale.com/oauth2/device/verify?user_code=DAdCQ6dY",
            "Or visit the following URL and enter the code:",
            "https://oauth.accounts.hytale.com/oauth2/device/verify",
            "Authorization code: DAdCQ6dY",
        ] {
            for b in parse(line) {
                merge(&mut p, &b);
            }
            assert!(is_prompt_text(line) || line.starts_with("https://"));
        }
        assert_eq!(p.url, "https://oauth.accounts.hytale.com/oauth2/device/verify?user_code=DAdCQ6dY");
        assert_eq!(p.code, "DAdCQ6dY");
    }

    #[test]
    fn reads_the_server_prompt_and_progress() {
        assert_eq!(parse("Enter code: ABCD-1234"), vec![Bit::Code("ABCD-1234".into())]);
        assert_eq!(parse("Visit: https://accounts.hytale.com/device"), vec![Bit::Url("https://accounts.hytale.com/device".into())]);
        assert_eq!(parse("[=====>    ] 45.5% (1.2 GB / 3.3 GB)"), vec![Bit::Progress(45.5)]);
        assert!(parse("Downloading Assets.zip").is_empty());
        assert!(parse("see https://example.com/device").is_empty());
        assert!(!is_prompt_text("2026.01.22-6f8bdbdc4"));
    }
}
