//! Minecraft's `server.properties`: a Java properties file read and written in place (comments and the
//! order of keys are kept), plus the list of settings the UI shows as switches, menus and numbers.

use anyhow::{Context, Result};
use std::path::Path;

#[derive(Clone, Debug, PartialEq)]
enum Line {
    /// a comment or blank line, kept as it is
    Raw(String),
    Pair(String, String),
}

/// A properties file. Unknown keys and comments survive a round trip.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Properties {
    lines: Vec<Line>,
}

impl Properties {
    pub fn parse(text: &str) -> Self {
        let mut lines = Vec::new();
        let mut pending = String::new();
        for raw in text.lines() {
            // a line ending in an odd number of backslashes continues on the next one
            let trailing = raw.chars().rev().take_while(|c| *c == '\\').count();
            if trailing % 2 == 1 {
                pending.push_str(&raw[..raw.len() - 1]);
                continue;
            }
            let logical = if pending.is_empty() { raw.to_string() } else { std::mem::take(&mut pending) + raw.trim_start() };
            let t = logical.trim_start();
            if t.is_empty() || t.starts_with('#') || t.starts_with('!') {
                lines.push(Line::Raw(logical));
                continue;
            }
            let (k, v) = split_pair(t);
            lines.push(Line::Pair(unescape(&k), unescape(&v)));
        }
        if !pending.is_empty() {
            let (k, v) = split_pair(pending.trim_start());
            lines.push(Line::Pair(unescape(&k), unescape(&v)));
        }
        Self { lines }
    }

    pub fn load(path: &Path) -> Self {
        // Minecraft writes UTF-8 (newer) or Latin-1 with \u escapes (older): both read fine as lossy UTF-8
        std::fs::read(path).map(|b| Self::parse(&String::from_utf8_lossy(&b))).unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let tmp = path.with_extension("properties.tmp");
        std::fs::write(&tmp, self.to_text()).with_context(|| format!("cannot write {}", tmp.display()))?;
        std::fs::rename(&tmp, path).with_context(|| format!("cannot write {}", path.display()))?;
        Ok(())
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.lines.iter().rev().find_map(|l| match l {
            Line::Pair(k, v) if k == key => Some(v.as_str()),
            _ => None,
        })
    }

    /// Sets a key, in place when it exists, else appended.
    pub fn set(&mut self, key: &str, value: &str) {
        for l in self.lines.iter_mut().rev() {
            if let Line::Pair(k, v) = l
                && k == key
            {
                *v = value.to_string();
                return;
            }
        }
        self.lines.push(Line::Pair(key.to_string(), value.to_string()));
    }

    pub fn pairs(&self) -> Vec<(String, String)> {
        self.lines
            .iter()
            .filter_map(|l| match l {
                Line::Pair(k, v) => Some((k.clone(), v.clone())),
                Line::Raw(_) => None,
            })
            .collect()
    }

    pub fn to_text(&self) -> String {
        let mut out = String::new();
        for l in &self.lines {
            match l {
                Line::Raw(r) => out.push_str(r),
                Line::Pair(k, v) => {
                    out.push_str(&escape(k, true));
                    out.push('=');
                    out.push_str(&escape(v, false));
                }
            }
            out.push('\n');
        }
        out
    }
}

/// `key=value`, `key: value` or `key value` (backslash escapes the separator).
fn split_pair(t: &str) -> (String, String) {
    let mut key = String::new();
    let mut chars = t.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                key.push('\\');
                if let Some(n) = chars.next() {
                    key.push(n);
                }
            }
            '=' | ':' => break,
            c if c.is_whitespace() => {
                // whitespace, then optionally one = or :
                while chars.peek().is_some_and(|c| c.is_whitespace()) {
                    chars.next();
                }
                if chars.peek().is_some_and(|c| *c == '=' || *c == ':') {
                    chars.next();
                }
                break;
            }
            c => key.push(c),
        }
    }
    let rest: String = chars.collect();
    (key, rest.trim_start().to_string())
}

fn unescape(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('f') => out.push('\u{c}'),
            Some('u') => {
                let hex: String = chars.by_ref().take(4).collect();
                match u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                    Some(ch) => out.push(ch),
                    None => out.push_str(&hex),
                }
            }
            Some(o) => out.push(o),
            None => {}
        }
    }
    out
}

/// Escapes like `java.util.Properties.store` (non-ASCII as `\uXXXX`, which every Minecraft version reads).
fn escape(s: &str, is_key: bool) -> String {
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '=' | ':' | '#' | '!' => {
                out.push('\\');
                out.push(c);
            }
            ' ' if is_key || i == 0 => out.push_str("\\ "),
            c if (c as u32) < 0x20 || (c as u32) > 0x7e => {
                let mut buf = [0u16; 2];
                for u in c.encode_utf16(&mut buf) {
                    out.push_str(&format!("\\u{:04X}", u));
                }
            }
            c => out.push(c),
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------
// What the editor shows
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Bool,
    Int(i64, i64),
    Choice(&'static [(&'static str, &'static str)]),
    Text,
    Secret,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Field {
    pub key: &'static str,
    pub label: &'static str,
    pub kind: Kind,
    /// Minecraft's default (used when the file has no such key)
    pub default: &'static str,
    pub help: &'static str,
}

const fn f(key: &'static str, label: &'static str, kind: Kind, default: &'static str, help: &'static str) -> Field {
    Field { key, label, kind, default, help }
}

pub const GAMEMODES: &[(&str, &str)] = &[("survival", "Survival"), ("creative", "Creative"), ("adventure", "Adventure"), ("spectator", "Spectator")];
pub const DIFFICULTIES: &[(&str, &str)] = &[("peaceful", "Peaceful"), ("easy", "Easy"), ("normal", "Normal"), ("hard", "Hard")];
/// World presets (1.19+ ids; older versions are translated by `minecraft::level_type_value`).
pub const LEVEL_TYPES: &[(&str, &str)] = &[
    ("minecraft:normal", "Normal"),
    ("minecraft:flat", "Superflat"),
    ("minecraft:large_biomes", "Large biomes"),
    ("minecraft:amplified", "Amplified"),
    ("minecraft:single_biome_surface", "Single biome"),
];

/// The settings shown in groups. `server-port` is Hyve's (instance settings) and is not here.
pub const GROUPS: &[(&str, &[Field])] = &[
    (
        "Gameplay",
        &[
            f("gamemode", "Game mode", Kind::Choice(GAMEMODES), "survival", "Mode for new players"),
            f("force-gamemode", "Force game mode", Kind::Bool, "false", "Players get the game mode above every time they join"),
            f("difficulty", "Difficulty", Kind::Choice(DIFFICULTIES), "easy", ""),
            f("hardcore", "Hardcore", Kind::Bool, "false", "One life: players become spectators when they die"),
            f("pvp", "PvP", Kind::Bool, "true", "Players can hurt each other"),
            f("allow-flight", "Allow flight", Kind::Bool, "false", "Needed by some mods; otherwise flying players get kicked"),
            f("allow-nether", "Allow the Nether", Kind::Bool, "true", ""),
            f("spawn-monsters", "Spawn monsters", Kind::Bool, "true", ""),
            f("spawn-animals", "Spawn animals", Kind::Bool, "true", "Ignored by recent versions (game rules)"),
            f("spawn-npcs", "Spawn villagers", Kind::Bool, "true", "Ignored by recent versions (game rules)"),
            f("enable-command-block", "Command blocks", Kind::Bool, "false", ""),
            f("spawn-protection", "Spawn protection radius", Kind::Int(0, 1000), "16", "Blocks around spawn only operators can change (0 = off)"),
        ],
    ),
    (
        "World",
        &[
            f("level-name", "World folder", Kind::Text, "world", "The world the server loads (Worlds tab lists them)"),
            f("level-seed", "Seed", Kind::Text, "", "Only used when a new world is generated"),
            f("level-type", "World type", Kind::Choice(LEVEL_TYPES), "minecraft:normal", "Only used when a new world is generated"),
            f("generate-structures", "Generate structures", Kind::Bool, "true", "Villages, temples, … in new chunks"),
            f("generator-settings", "Generator settings", Kind::Text, "{}", "JSON for superflat / custom worlds"),
            f("max-world-size", "Max world radius", Kind::Int(1, 29_999_984), "29999984", "World border radius in blocks"),
        ],
    ),
    (
        "Players",
        &[
            f("online-mode", "Online mode", Kind::Bool, "true", "Check accounts with Mojang. Off = cracked/offline players can join (anyone can pick any name)"),
            f("max-players", "Max players", Kind::Int(1, 10_000), "20", ""),
            f("white-list", "Whitelist", Kind::Bool, "false", "Only players on the whitelist can join"),
            f("enforce-whitelist", "Enforce whitelist", Kind::Bool, "false", "Kick online players who are not on the whitelist when it is reloaded"),
            f("motd", "Message of the day", Kind::Text, "A Minecraft Server", "Shown in the server list"),
            f("op-permission-level", "Operator level", Kind::Int(1, 4), "4", "1 bypass spawn protection · 2 cheats · 3 kick/ban · 4 everything"),
            f("player-idle-timeout", "Idle kick (minutes)", Kind::Int(0, 1_000_000), "0", "0 = never"),
            f("enforce-secure-profile", "Require signed chat", Kind::Bool, "true", "Players without Mojang-signed chat keys cannot join"),
            f("hide-online-players", "Hide player list", Kind::Bool, "false", "Don't show who is online in the server list"),
        ],
    ),
    (
        "Performance & network",
        &[
            f("view-distance", "View distance (chunks)", Kind::Int(3, 32), "10", ""),
            f("simulation-distance", "Simulation distance (chunks)", Kind::Int(3, 32), "10", "How far away mobs and crops keep ticking"),
            f("server-ip", "Bind to IP", Kind::Text, "", "Blank = all network interfaces"),
            f("network-compression-threshold", "Compression threshold", Kind::Int(-1, 1_000_000), "256", "-1 = off"),
            f("rate-limit", "Packet rate limit", Kind::Int(0, 1_000_000), "0", "0 = off"),
            f("prevent-proxy-connections", "Block VPN / proxy logins", Kind::Bool, "false", ""),
            f("enable-status", "Show in server list", Kind::Bool, "true", ""),
            f("sync-chunk-writes", "Synchronous chunk writes", Kind::Bool, "true", ""),
            f("enable-query", "GameSpy query", Kind::Bool, "false", ""),
            f("query.port", "Query port", Kind::Int(1, 65535), "25565", ""),
            f("enable-rcon", "RCON", Kind::Bool, "false", "Remote console (needs a password)"),
            f("rcon.port", "RCON port", Kind::Int(1, 65535), "25575", ""),
            f("rcon.password", "RCON password", Kind::Secret, "", ""),
        ],
    ),
];

/// Keys Hyve manages itself (not edited in the properties form).
pub const MANAGED: &[&str] = &["server-port"];

pub fn field(key: &str) -> Option<&'static Field> {
    GROUPS.iter().flat_map(|(_, fs)| fs.iter()).find(|f| f.key == key)
}

/// A value checked against its field (numbers in range, booleans as true/false).
pub fn validate(key: &str, value: &str) -> Result<String> {
    let v = value.trim();
    let Some(f) = field(key) else { return Ok(value.to_string()) };
    match f.kind {
        Kind::Bool => match v {
            "true" | "false" => Ok(v.to_string()),
            _ => anyhow::bail!("{}: true or false", f.label),
        },
        Kind::Int(lo, hi) => match v.parse::<i64>() {
            Ok(n) if (lo..=hi).contains(&n) => Ok(n.to_string()),
            _ => anyhow::bail!("{}: a whole number from {lo} to {hi}", f.label),
        },
        Kind::Choice(_) => Ok(v.to_string()),
        Kind::Text | Kind::Secret => Ok(value.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "#Minecraft server properties\n#Sat Nov 08 10:00:00 CET 2026\nmotd=A Minecraft Server\\: \\u00E9t\\u00E9\nonline-mode=true\nlevel-seed=\nserver-port=25565\ngenerator-settings={}\n";

    #[test]
    fn reads_and_writes_in_place() {
        let mut p = Properties::parse(SAMPLE);
        assert_eq!(p.get("motd"), Some("A Minecraft Server: été"));
        assert_eq!(p.get("level-seed"), Some(""));
        assert_eq!(p.get("missing"), None);
        p.set("online-mode", "false");
        p.set("max-players", "8");
        let text = p.to_text();
        assert!(text.starts_with("#Minecraft server properties\n#Sat"), "{text}");
        assert!(text.contains("\nonline-mode=false\n"), "{text}");
        assert!(text.ends_with("max-players=8\n"), "{text}");
        assert!(text.contains("motd=A Minecraft Server\\: \\u00E9t\\u00E9"), "{text}");
        // round trip
        assert_eq!(Properties::parse(&text).get("motd"), Some("A Minecraft Server: été"));
        assert_eq!(Properties::parse(&text).pairs().len(), 6);
    }

    #[test]
    fn other_separators_and_continuations() {
        let p = Properties::parse("a: 1\nb 2\nc = x \\\n   y\nd\\=e=f\n");
        assert_eq!(p.get("a"), Some("1"));
        assert_eq!(p.get("b"), Some("2"));
        assert_eq!(p.get("c"), Some("x y"));
        assert_eq!(p.get("d=e"), Some("f"));
    }

    #[test]
    fn validates_known_fields() {
        assert_eq!(validate("max-players", " 12 ").unwrap(), "12");
        assert!(validate("max-players", "0").is_err());
        assert!(validate("pvp", "yes").is_err());
        assert_eq!(validate("whatever", "x y").unwrap(), "x y");
        assert!(field("server-port").is_none());
    }
}
