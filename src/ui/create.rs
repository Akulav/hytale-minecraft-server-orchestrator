//! New instance form.

use dioxus::prelude::*;

use super::instance::Check;
use super::{Banner, Icon, Page, PageHeader, use_state, use_toast};
use crate::instance::{self, NewInstance};

#[component]
pub fn CreatePage(nav: Signal<Page>, tick: u64) -> Element {
    let state = use_state();
    let toast = use_toast();
    let _ = tick;
    let versions = crate::versions::list(&state);
    let count = state.instances.read().len();
    let first_version = versions.first().map(|v| v.version.clone()).unwrap_or_default();
    let first_patchline = versions.first().map(|v| v.patchline.clone()).filter(|p| !p.is_empty()).unwrap_or_else(|| "release".into());
    let suggested_port = instance::suggest_port(&state);
    let mut name = use_signal(move || format!("Server {}", count + 1));
    let mut version = use_signal(move || first_version.clone());
    let mut patchline = use_signal(move || first_patchline.clone());
    let mut port = use_signal(move || suggested_port.to_string());
    let mut memory = use_signal(|| 4096u32);
    let mut seed = use_signal(String::new);
    let mut offline = use_signal(|| false);
    let st = state.clone();
    let no_versions = versions.is_empty();
    let port_free = port().parse::<u16>().map(crate::process::udp_port_free).unwrap_or(false);

    rsx! {
        div { class: "page narrow",
            PageHeader { title: "New instance", subtitle: "An isolated Hytale server with its own worlds, mods, port and settings." }
            if no_versions {
                Banner { kind: "info", icon: "download",
                    b { "No server version in the cache yet. " }
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
                            for m in [2048u32, 3072, 4096, 6144, 8192, 12288, 16384] {
                                option { key: "{m}", value: "{m}", selected: m == memory(), "{m / 1024} GB" }
                            }
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
}
