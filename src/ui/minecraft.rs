//! Minecraft-only parts of the instance page: worlds (level-name), the server.properties editor and the
//! server software panel (version + loader) in Settings.

use dioxus::prelude::*;
use std::collections::BTreeMap;

use super::{Banner, ConfirmButton, Icon, use_state, use_toast};
use crate::mcprops::{self, GROUPS, Kind, LEVEL_TYPES, MANAGED};
use crate::minecraft;
use crate::state::{Loader, human_bytes};

#[component]
pub fn McWorlds(id: String, tick: u64) -> Element {
    let state = use_state();
    let toast = use_toast();
    let _ = tick;
    let mut name = use_signal(String::new);
    let mut seed = use_signal(String::new);
    let mut level_type = use_signal(|| "minecraft:normal".to_string());
    let worlds = minecraft::list_worlds(&state, &id);
    let running = state.status(&id).is_active();
    let (s1, i1) = (state.clone(), id.clone());
    let applies = if running { " - restart the server to switch" } else { "" };

    rsx! {
        div { class: "grid2 wide-left",
            div { class: "panel glass",
                h3 { "Worlds" }
                p { class: "muted small", "A Minecraft server runs one world at a time (level-name in server.properties), with its Nether and End inside the same folder." }
                table { class: "table",
                    thead { tr { th { "Folder" } th { "" } th { "Size" } th {} } }
                    tbody {
                        for w in worlds {
                            {
                                let (s2, s3, i2, i3, n2, n3) = (state.clone(), state.clone(), id.clone(), id.clone(), w.name.clone(), w.name.clone());
                                rsx! {
                                    tr { key: "{w.name}",
                                        td { b { "{w.name}" } }
                                        td {
                                            if w.active { span { class: "pill running sm", "Active" } }
                                            if w.pending { span { class: "pill busy sm", "generated on next start" } }
                                            if !w.active {
                                                button { class: "btn sm", onclick: move |_| toast.result(minecraft::set_active_world(&s2, &i2, &n2), &format!("{n2} is the active world{applies}")), "Make active" }
                                            }
                                        }
                                        td { if w.pending { "-" } else { "{human_bytes(w.size)}" } }
                                        td { class: "right",
                                            if !w.pending {
                                                ConfirmButton { label: "Delete", confirm: "Delete this world for good?", class: "sm ghost danger-text", disabled: running,
                                                    onconfirm: move |_| toast.result(minecraft::delete_world(&s3, &i3, &n3), "World deleted")
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                if running { p { class: "muted small", "Stop the server to delete a world." } }
            }
            div { class: "panel glass",
                h3 { "Generate a world" }
                label { class: "field", span { "Folder name" }
                    input { class: "input", placeholder: "e.g. survival2", value: "{name}", oninput: move |e| name.set(e.value()) }
                }
                label { class: "field", span { "Seed" }
                    div { class: "row",
                        input { class: "input grow mono", placeholder: "blank = random; numbers or words", value: "{seed}", oninput: move |e| seed.set(e.value()) }
                        button { class: "btn ghost", onclick: move |_| seed.set(crate::instance::random_seed().to_string()), "🎲" }
                    }
                }
                label { class: "field", span { "World type" }
                    select { class: "input", onchange: move |e| level_type.set(e.value()),
                        for (v, label) in LEVEL_TYPES.iter() { option { key: "{v}", value: "{v}", selected: level_type() == *v, "{label}" } }
                    }
                }
                button { class: "btn primary block",
                    onclick: move |_| match minecraft::create_world(&s1, &i1, &name(), &seed(), &level_type()) {
                        Ok(()) => { toast.ok(format!("{} becomes the active world and is generated on the next start", name())); name.set(String::new()); seed.set(String::new()); }
                        Err(e) => toast.err(format!("{e:#}")),
                    },
                    Icon { name: "plus" } "Create world"
                }
                p { class: "muted small", "The other worlds stay on disk; switch back with Make active." }
            }
        }
    }
}

fn file_map(state: &crate::state::Shared, id: &str) -> BTreeMap<String, String> {
    minecraft::load_properties(state, id).pairs().into_iter().collect()
}

/// The server.properties editor: known settings as switches / menus / numbers, the rest as text.
#[component]
pub fn McProperties(id: String, tick: u64) -> Element {
    let state = use_state();
    let toast = use_toast();
    let _ = tick;
    let st0 = state.clone();
    let id0 = id.clone();
    let mut form = use_signal(move || file_map(&st0, &id0));
    let mut new_key = use_signal(String::new);
    let mut new_value = use_signal(String::new);
    let on_disk = file_map(&state, &id);
    let f = form();
    let changed: Vec<String> = f.iter().filter(|(k, v)| on_disk.get(*k) != Some(*v)).map(|(k, _)| k.clone()).collect();
    let dirty = !changed.is_empty();
    let running = state.status(&id).is_active();
    let exists = minecraft::properties_path(&state, &id).is_file();
    let others: Vec<(String, String)> = f.iter().filter(|(k, _)| mcprops::field(k).is_none() && !MANAGED.contains(&k.as_str())).map(|(k, v)| (k.clone(), v.clone())).collect();
    let (s1, i1) = (state.clone(), id.clone());
    let (s2, i2) = (state.clone(), id.clone());
    let count = changed.len();

    rsx! {
        div { class: "panel glass props-bar",
            div { class: "row",
                div { class: "grow",
                    b { "server.properties" }
                    p { class: "muted small",
                        if dirty { "{count} unsaved change(s)." } else { "Saved." }
                        if running { " The server reads this file when it starts: restart to apply." }
                        if !exists { " The server writes the full list on its first start." }
                    }
                }
                button { class: "btn ghost", disabled: !dirty, onclick: move |_| form.set(file_map(&s2, &i2)), "Discard" }
                button { class: "btn primary", disabled: !dirty,
                    onclick: move |_| {
                        let values = form();
                        let r = (|| -> anyhow::Result<()> {
                            let path = minecraft::properties_path(&s1, &i1);
                            let mut p = mcprops::Properties::load(&path);
                            let mut clean = Vec::new();
                            for (k, v) in &values {
                                if p.get(k) != Some(v.as_str()) {
                                    clean.push((k.clone(), mcprops::validate(k, v)?));
                                }
                            }
                            for (k, v) in &clean { p.set(k, v); }
                            p.save(&path)?;
                            // the dashboard and Overview show online mode as the auth mode
                            let offline = p.get("online-mode") == Some("false");
                            s1.update_instance(&i1, |i| i.offline_auth = offline)?;
                            Ok(())
                        })();
                        match r {
                            Ok(()) => { form.set(file_map(&s1, &i1)); toast.ok(if running { "Saved - restart the server to apply" } else { "Saved" }); }
                            Err(e) => toast.err(format!("{e:#}")),
                        }
                    },
                    "Save"
                }
            }
        }
        for (group, fields) in GROUPS.iter() {
            div { key: "{group}", class: "panel glass",
                h3 { "{group}" }
                div { class: "props-grid",
                    for fd in fields.iter() {
                        {
                            let key = fd.key;
                            let value = f.get(key).cloned().unwrap_or_else(|| fd.default.to_string());
                            let is_changed = changed.iter().any(|c| c == key);
                            let mut set = move |v: String| { form.write().insert(key.to_string(), v); };
                            let control = match fd.kind {
                                Kind::Bool => rsx! {
                                    label { class: "switch",
                                        input { r#type: "checkbox", checked: value == "true", onchange: move |e| set(e.checked().to_string()) }
                                        span { class: "slider" }
                                    }
                                },
                                Kind::Choice(opts) => {
                                    let unknown = !value.is_empty() && !opts.iter().any(|(v, _)| *v == value);
                                    rsx! {
                                        select { class: "input", onchange: move |e| set(e.value()),
                                            for (v, label) in opts.iter() { option { key: "{v}", value: "{v}", selected: value == *v, "{label}" } }
                                            if unknown { option { value: "{value}", selected: true, "{value}" } }
                                        }
                                    }
                                }
                                Kind::Int(lo, hi) => rsx! {
                                    input { class: "input", r#type: "number", min: "{lo}", max: "{hi}", value: "{value}", oninput: move |e| set(e.value()) }
                                },
                                Kind::Text => rsx! { input { class: "input", value: "{value}", placeholder: "{fd.default}", oninput: move |e| set(e.value()) } },
                                Kind::Secret => rsx! { input { class: "input", r#type: "password", value: "{value}", oninput: move |e| set(e.value()) } },
                            };
                            rsx! {
                                div { key: "{key}", class: if is_changed { "prop changed" } else { "prop" },
                                    div { class: "prop-text",
                                        b { "{fd.label}" }
                                        if !fd.help.is_empty() { small { "{fd.help}" } }
                                        span { class: "prop-key", "{key}" }
                                    }
                                    {control}
                                }
                            }
                        }
                    }
                }
            }
        }
        div { class: "panel glass",
            h3 { "Other properties" }
            p { class: "muted small", "Everything else in the file (mods and newer versions add their own). server-port is set in Settings." }
            div { class: "props-grid",
                for (k, v) in others {
                    {
                        let k2 = k.clone();
                        rsx! {
                            div { key: "{k}", class: "prop",
                                div { class: "prop-text", b { class: "mono", "{k}" } }
                                input { class: "input mono", value: "{v}", oninput: move |e| { form.write().insert(k2.clone(), e.value()); } }
                            }
                        }
                    }
                }
            }
            div { class: "row add-prop",
                input { class: "input mono grow", placeholder: "new-key", value: "{new_key}", oninput: move |e| new_key.set(e.value()) }
                input { class: "input mono grow", placeholder: "value", value: "{new_value}", oninput: move |e| new_value.set(e.value()) }
                button { class: "btn",
                    onclick: move |_| {
                        let k = new_key().trim().to_string();
                        if k.is_empty() || k.contains(char::is_whitespace) || MANAGED.contains(&k.as_str()) {
                            toast.err("enter a property name without spaces (server-port is set in Settings)");
                            return;
                        }
                        form.write().insert(k, new_value());
                        new_key.set(String::new());
                        new_value.set(String::new());
                    },
                    Icon { name: "plus" } "Add"
                }
            }
        }
    }
}

/// Settings → server software: Minecraft version, loader and loader version, reinstalled with a backup.
#[component]
pub fn McSoftware(id: String, tick: u64) -> Element {
    let state = use_state();
    let toast = use_toast();
    let _ = tick;
    let Some(inst) = state.instance(&id) else { return rsx! {} };
    minecraft::ensure_catalog(&state);
    let cat = state.mc_catalog.read().clone();
    let (v0, l0, lv0) = (inst.version.clone(), inst.loader, inst.loader_version.clone());
    let mut version = use_signal(move || v0);
    let mut loader = use_signal(move || l0);
    let mut loader_version = use_signal(move || lv0);
    let mut snapshots = use_signal(|| false);
    let l = loader();
    let mut games = cat.game_versions(l, snapshots());
    if !games.contains(&inst.version) && cat.supports(l, &inst.version) != Some(false) {
        games.insert(0, inst.version.clone());
    }
    let picked = if !cat.ready() || games.contains(&version()) { version() } else { games.first().cloned().unwrap_or_default() };
    let suggestions = cat.loader_versions(l, &picked);
    let busy = state.running_task_for(&id).is_some();
    let same = picked == inst.version && l == inst.loader && loader_version().trim() == inst.loader_version && !inst.launch.is_empty();
    let downgrade = minecraft::parse_game(&picked).zip(minecraft::parse_game(&inst.version)).is_some_and(|(a, b)| a < b);
    let dir = state.paths.server_dir(&id);
    let eula = minecraft::eula_accepted(&dir);
    let (s1, i1, s2) = (state.clone(), id.clone(), state.clone());
    let current = crate::instance::software_label(&inst);
    let installed = !inst.launch.is_empty();

    rsx! {
        div { class: "panel glass",
            h3 { "Server software" }
            div { class: "kv",
                span { "Installed" } b { if installed { "{current}" } else { "not yet" } }
                span { "Java" } b { if inst.java_major > 0 { "Java {inst.java_major}" } else { "-" } }
            }
            div { class: "row",
                label { class: "field grow", span { "Software" }
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
                        input { class: "input mono", value: "{version}", oninput: move |e| version.set(e.value()) }
                    }
                }
            }
            if l != Loader::Vanilla {
                label { class: "field", span { "{l.label()} version (blank = newest)" }
                    input { class: "input mono", list: "sw-loader-versions", value: "{loader_version}", oninput: move |e| loader_version.set(e.value()) }
                    datalist { id: "sw-loader-versions", for v in suggestions.iter() { option { key: "{v}", value: "{v}" } } }
                }
            }
            label { class: "check",
                input { r#type: "checkbox", checked: snapshots(), onchange: move |e| snapshots.set(e.checked()) }
                span { "Show snapshots" }
            }
            if downgrade { p { class: "warn-text small", "Older versions can't always open a world made by a newer one." } }
            if l != inst.loader && installed { p { class: "muted small", "Mods are kept, but mods for {inst.loader.label()} won't load on {l.label()}." } }
            div { class: "row",
                ConfirmButton { label: if installed { "Install" } else { "Install now" }, confirm: "Back up and reinstall?", class: "primary", disabled: busy || same,
                    onconfirm: move |_| {
                        let v = picked.clone();
                        toast.result(crate::actions::change_minecraft(&s1, &i1, &v, loader(), &loader_version()), "Installing - progress above")
                    }
                }
                span { class: "muted small", "A backup is made first; a running server is restarted." }
            }
        }
        if !eula {
            Banner { kind: "warn", icon: "alert",
                b { "The Minecraft EULA is not accepted. " }
                "The server will not start without it. "
                a { href: minecraft::EULA_URL, target: "_blank", rel: "noopener", "Read the EULA" }
                div { class: "banner-actions",
                    button { class: "btn sm primary", onclick: move |_| { let r = minecraft::accept_eula(&dir); s2.notify(); toast.result(r, "EULA accepted"); }, "I accept the EULA" }
                }
            }
        }
    }
}
