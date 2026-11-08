//! New instance form: pick the game, then its settings.

use dioxus::prelude::*;

use super::instance::Check;
use super::{Banner, Icon, Page, PageHeader, use_state, use_toast};
use crate::instance::{self, NewInstance, NewMinecraft};
use crate::mcprops::{DIFFICULTIES, GAMEMODES, LEVEL_TYPES};
use crate::state::{Game, Loader};

const MEMS: [u32; 7] = [2048, 3072, 4096, 6144, 8192, 12288, 16384];

#[component]
pub fn CreatePage(nav: Signal<Page>, tick: u64) -> Element {
    let mut game = use_signal(|| Game::Hytale);
    let g = game();
    let subtitle = match g {
        Game::Hytale => "An isolated Hytale server with its own worlds, mods, port and settings.",
        Game::Minecraft => "A Minecraft Java server - vanilla or modded with Fabric, Quilt, Forge or NeoForge.",
    };
    rsx! {
        div { class: "page narrow",
            PageHeader { title: "New instance", subtitle }
            nav { class: "tabs game-pick",
                for (choice, icon) in [(Game::Hytale, "server"), (Game::Minecraft, "box")] {
                    button { key: "{choice.label()}", class: if g == choice { "tab active" } else { "tab" }, onclick: move |_| game.set(choice),
                        Icon { name: icon } span { "{choice.label()}" }
                    }
                }
            }
            match g {
                Game::Hytale => rsx! { HytaleForm { nav, tick } },
                Game::Minecraft => rsx! { MinecraftForm { nav, tick } },
            }
        }
    }
}

#[component]
fn HytaleForm(nav: Signal<Page>, tick: u64) -> Element {
    let state = use_state();
    let toast = use_toast();
    let _ = tick;
    let versions = crate::versions::list(&state);
    let count = state.instances.read().len();
    let first_version = versions.first().map(|v| v.version.clone()).unwrap_or_default();
    let first_patchline = versions.first().map(|v| v.patchline.clone()).filter(|p| !p.is_empty()).unwrap_or_else(|| "release".into());
    let suggested_port = instance::suggest_port(&state, Game::Hytale);
    let mut name = use_signal(move || format!("Server {}", count + 1));
    let mut version = use_signal(move || first_version.clone());
    let mut patchline = use_signal(move || first_patchline.clone());
    let mut port = use_signal(move || suggested_port.to_string());
    let mut memory = use_signal(|| 4096u32);
    let mut seed = use_signal(String::new);
    let mut offline = use_signal(|| false);
    let st = state.clone();
    let no_versions = versions.is_empty();
    let port_free = port().parse::<u16>().map(|p| crate::process::port_free(Game::Hytale, p)).unwrap_or(false);

    rsx! {
        if no_versions {
            Banner { kind: "info", icon: "download",
                b { "No Hytale server version in the cache yet. " }
                "Download one (or import it from your Hytale launcher) first."
                div { class: "banner-actions", button { class: "btn sm primary", onclick: move |_| nav.set(Page::Versions), "Server versions" } }
            }
        }
        div { class: "panel glass form",
            label { class: "field", span { "Name" } input { class: "input", value: "{name}", oninput: move |e| name.set(e.value()) } }
            div { class: "row",
                label { class: "field grow", span { "Server version" }
                    select { class: "input", disabled: no_versions, onchange: move |e| version.set(e.value()),
                        for v in versions.clone() {
                            option { key: "{v.version}", value: "{v.version}", selected: v.version == version(), "{v.version} ({v.patchline})" }
                        }
                    }
                }
                label { class: "field", span { "Update channel" }
                    select { class: "input", onchange: move |e| patchline.set(e.value()),
                        for p in crate::versions::PATCHLINES { option { key: "{p}", value: "{p}", selected: patchline() == p, "{p}" } }
                    }
                }
            }
            div { class: "row",
                label { class: "field grow", span { "UDP port" }
                    input { class: "input", r#type: "number", min: "1", max: "65535", value: "{port}", oninput: move |e| port.set(e.value()) }
                    if !port_free { small { class: "warn-text", "This port is in use right now." } }
                }
                label { class: "field grow", span { "Max RAM" }
                    select { class: "input", onchange: move |e| if let Ok(v) = e.value().parse() { memory.set(v) },
                        for m in MEMS { option { key: "{m}", value: "{m}", selected: m == memory(), "{m / 1024} GB" } }
                    }
                }
            }
            label { class: "field", span { "World seed" }
                div { class: "row",
                    input { class: "input grow mono", placeholder: "blank = random (chosen by the server)", value: "{seed}", oninput: move |e| seed.set(e.value()) }
                    button { class: "btn ghost", onclick: move |_| seed.set(instance::random_seed().to_string()), "🎲" }
                }
                small { class: "muted", "Sets the seed of the default world. Add more worlds with their own seeds later in the Worlds tab." }
            }
            Check { label: "Offline auth mode (LAN tests without a Hytale login)", checked: offline(), onchange: move |v| offline.set(v) }
            div { class: "row end",
                button { class: "btn ghost", onclick: move |_| nav.set(Page::Dashboard), "Cancel" }
                button { class: "btn primary", disabled: no_versions,
                    onclick: move |_| {
                        let req = NewInstance {
                            name: name(),
                            version: version(),
                            patchline: patchline(),
                            port: port().trim().parse().unwrap_or(0),
                            max_memory_mb: memory(),
                            offline_auth: offline(),
                            seed: instance::parse_seed(&seed()),
                        };
                        match instance::create(&st, req) {
                            Ok(inst) => { toast.ok(format!("{} created - press Start", inst.name)); nav.set(Page::Instance(inst.id)); }
                            Err(e) => toast.err(format!("{e:#}")),
                        }
                    },
                    Icon { name: "plus" } "Create instance"
                }
            }
        }
    }
}

#[component]
fn MinecraftForm(nav: Signal<Page>, tick: u64) -> Element {
    let state = use_state();
    let toast = use_toast();
    let _ = tick;
    crate::minecraft::ensure_catalog(&state);
    let cat = state.mc_catalog.read().clone();
    let count = state.instances.read().len();
    let suggested_port = instance::suggest_port(&state, Game::Minecraft);
    let mut name = use_signal(move || format!("Minecraft {}", count + 1));
    let mut version = use_signal(String::new);
    let mut loader = use_signal(|| Loader::Vanilla);
    let mut loader_version = use_signal(String::new);
    let mut snapshots = use_signal(|| false);
    let mut port = use_signal(move || suggested_port.to_string());
    let mut memory = use_signal(|| 4096u32);
    let mut seed = use_signal(String::new);
    let mut level_type = use_signal(|| "minecraft:normal".to_string());
    let mut gamemode = use_signal(|| "survival".to_string());
    let mut difficulty = use_signal(|| "easy".to_string());
    let mut max_players = use_signal(|| "20".to_string());
    let mut motd = use_signal(|| "A Minecraft Server".to_string());
    let mut online = use_signal(|| true);
    let mut eula = use_signal(|| false);

    let l = loader();
    let games = cat.game_versions(l, snapshots());
    // the picked version, or the newest one the loader supports
    let picked = if !cat.ready() || games.contains(&version()) { version() } else { games.first().cloned().unwrap_or_default() };
    let suggestions = cat.loader_versions(l, &picked);
    let port_free = port().parse::<u16>().map(|p| crate::process::port_free(Game::Minecraft, p)).unwrap_or(false);
    let st = state.clone();
    let picked2 = picked.clone();
    let can_create = eula() && !picked.trim().is_empty();
    let lv_placeholder = match l {
        Loader::Forge => "blank = recommended build",
        _ => "blank = newest stable build",
    };

    rsx! {
        if cat.loading && !cat.ready() {
            Banner { kind: "info", icon: "refresh", "Loading the Minecraft version lists…" }
        }
        if !cat.errors.is_empty() {
            Banner { kind: "warn", icon: "alert",
                b { "Some version lists could not be loaded. " }
                span { class: "muted", "{cat.errors.join(\" · \")}" }
            }
        }
        div { class: "panel glass form",
            label { class: "field", span { "Name" } input { class: "input", value: "{name}", oninput: move |e| name.set(e.value()) } }
            div { class: "row",
                label { class: "field grow", span { "Server software" }
                    select { class: "input", onchange: move |e| { loader.set(Loader::from_key(&e.value())); loader_version.set(String::new()); },
                        for x in Loader::ALL { option { key: "{x.key()}", value: "{x.key()}", selected: x == l, "{x.label()}" } }
                    }
                }
                label { class: "field grow", span { "Minecraft version" }
                    if cat.ready() {
                        select { class: "input", onchange: move |e| { version.set(e.value()); loader_version.set(String::new()); },
                            for v in games.iter().take(300) { option { key: "{v}", value: "{v}", selected: *v == picked, "{v}" } }
                        }
                    } else {
                        input { class: "input mono", placeholder: "e.g. 1.21.4", value: "{version}", oninput: move |e| version.set(e.value()) }
                    }
                }
            }
            div { class: "row",
                if l != Loader::Vanilla {
                    label { class: "field grow", span { "{l.label()} version" }
                        input { class: "input mono", list: "loader-versions", placeholder: lv_placeholder, value: "{loader_version}", oninput: move |e| loader_version.set(e.value()) }
                        datalist { id: "loader-versions",
                            for v in suggestions.iter() { option { key: "{v}", value: "{v}" } }
                        }
                    }
                } else {
                    div { class: "field grow" }
                }
                div { class: "field grow",
                    span { "\u{a0}" }
                    Check { label: "Show snapshots", checked: snapshots(), onchange: move |v| snapshots.set(v) }
                }
            }
            div { class: "row",
                label { class: "field grow", span { "TCP port" }
                    input { class: "input", r#type: "number", min: "1", max: "65535", value: "{port}", oninput: move |e| port.set(e.value()) }
                    if !port_free { small { class: "warn-text", "This port is in use right now." } }
                }
                label { class: "field grow", span { "Max RAM" }
                    select { class: "input", onchange: move |e| if let Ok(v) = e.value().parse() { memory.set(v) },
                        for m in MEMS { option { key: "{m}", value: "{m}", selected: m == memory(), "{m / 1024} GB" } }
                    }
                }
            }
            div { class: "row",
                label { class: "field grow", span { "World seed" }
                    div { class: "row",
                        input { class: "input grow mono", placeholder: "blank = random", value: "{seed}", oninput: move |e| seed.set(e.value()) }
                        button { class: "btn ghost", onclick: move |_| seed.set(instance::random_seed().to_string()), "🎲" }
                    }
                }
                label { class: "field grow", span { "World type" }
                    select { class: "input", onchange: move |e| level_type.set(e.value()),
                        for (v, label) in LEVEL_TYPES.iter() { option { key: "{v}", value: "{v}", selected: level_type() == *v, "{label}" } }
                    }
                }
            }
            div { class: "row",
                label { class: "field grow", span { "Game mode" }
                    select { class: "input", onchange: move |e| gamemode.set(e.value()),
                        for (v, label) in GAMEMODES.iter() { option { key: "{v}", value: "{v}", selected: gamemode() == *v, "{label}" } }
                    }
                }
                label { class: "field grow", span { "Difficulty" }
                    select { class: "input", onchange: move |e| difficulty.set(e.value()),
                        for (v, label) in DIFFICULTIES.iter() { option { key: "{v}", value: "{v}", selected: difficulty() == *v, "{label}" } }
                    }
                }
            }
            div { class: "row",
                label { class: "field grow", span { "Max players" }
                    input { class: "input", r#type: "number", min: "1", value: "{max_players}", oninput: move |e| max_players.set(e.value()) }
                }
                label { class: "field grow", span { "Message of the day" }
                    input { class: "input", value: "{motd}", oninput: move |e| motd.set(e.value()) }
                }
            }
            Check { label: "Online mode - check accounts with Mojang (turn off for offline / LAN players)", checked: online(), onchange: move |v| online.set(v) }
            label { class: "check",
                input { r#type: "checkbox", checked: eula(), onchange: move |e| eula.set(e.checked()) }
                span { "I accept the " a { href: crate::minecraft::EULA_URL, target: "_blank", rel: "noopener", "Minecraft EULA" } " (required to run a server)" }
            }
            p { class: "muted small", "Every setting can be changed later in the Properties tab. Hyve downloads the server and the Java version it needs." }
            div { class: "row end",
                button { class: "btn ghost", onclick: move |_| nav.set(Page::Dashboard), "Cancel" }
                button { class: "btn primary", disabled: !can_create, title: if eula() { "" } else { "Accept the EULA first" },
                    onclick: move |_| {
                        let req = NewMinecraft {
                            name: name(),
                            version: picked2.clone(),
                            loader: loader(),
                            loader_version: loader_version(),
                            port: port().trim().parse().unwrap_or(0),
                            max_memory_mb: memory(),
                            eula: eula(),
                            world: crate::minecraft::WorldSetup {
                                seed: seed(),
                                level_type: level_type(),
                                gamemode: gamemode(),
                                difficulty: difficulty(),
                                max_players: max_players().trim().parse().unwrap_or(20),
                                motd: motd(),
                                online_mode: online(),
                            },
                        };
                        let r = instance::create_minecraft(&st, req).and_then(|inst| crate::actions::install_minecraft(&st, &inst.id).map(|_| inst));
                        match r {
                            Ok(inst) => { toast.ok(format!("{} created - installing the server", inst.name)); nav.set(Page::Instance(inst.id)); }
                            Err(e) => toast.err(format!("{e:#}")),
                        }
                    },
                    Icon { name: "plus" } "Create instance"
                }
            }
        }
    }
}
