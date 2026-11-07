//! Server versions: download with the official Hytale downloader, import from the launcher or a path,
//! and the local cache.

use dioxus::prelude::*;

use super::{Banner, ConfirmButton, Empty, Icon, Page, PageHeader, TaskCard, use_state, use_toast};
use crate::state::{TaskStatus, human_bytes};
use crate::{actions, versions};

#[component]
pub fn VersionsPage(nav: Signal<Page>, tick: u64) -> Element {
    let state = use_state();
    let toast = use_toast();
    let _ = tick;
    let mut path = use_signal(String::new);
    let list = versions::list(&state);
    let launcher = versions::launcher_installs();
    let instances = state.instances.read().clone();
    let tasks: Vec<_> = state.tasks.read().iter().filter(|t| t.instance.is_none() && (t.status == TaskStatus::Running || crate::state::now_ms() - t.ended_at.unwrap_or(0) < 120_000)).cloned().collect();
    let (s1, s2, s3, s4) = (state.clone(), state.clone(), state.clone(), state.clone());
    let mac = cfg!(target_os = "macos");
    let signed_in = versions::downloader_signed_in(&state);

    rsx! {
        div { class: "page",
            PageHeader { title: "Server versions", subtitle: "Hytale server builds cached on this machine. Every instance picks one; updating is one click." }
            for t in tasks { TaskCard { key: "{t.id}", task: t, expanded: true } }
            div { class: "grid3",
                div { class: "panel glass source",
                    div { class: "source-icon", Icon { name: "download" } }
                    h3 { "Official downloader" }
                    p { class: "muted small", "Fetched and run for you. The first time, approve a sign-in code with the Hytale account that owns the game - no token to paste." }
                    div { class: "source-body",
                        if mac {
                            p { class: "warn-text small", "Hypixel publishes the downloader for Windows and Linux only. Use the launcher import on a Mac." }
                        } else if signed_in {
                            div { class: "account-row",
                                span { class: "pill running sm", "Signed in" }
                                span { class: "muted small grow", "login saved" }
                                ConfirmButton { label: "Sign out", confirm: "Forget it?", class: "sm ghost", disabled: false,
                                    onconfirm: move |_| toast.result(versions::downloader_sign_out(&s4), "Signed out - the next download asks again")
                                }
                            }
                        } else {
                            div { class: "account-row",
                                span { class: "pill sm", "Not signed in" }
                                span { class: "muted small grow", "asked on the first download" }
                            }
                        }
                    }
                    div { class: "source-actions",
                        button { class: "btn primary", disabled: mac, onclick: move |_| { actions::download_version(&s1, "release"); toast.ok("Download started"); }, "Latest release" }
                        button { class: "btn", disabled: mac, onclick: move |_| { actions::download_version(&s2, "pre-release"); toast.ok("Download started"); }, "Pre-release" }
                    }
                }
                div { class: "panel glass source",
                    div { class: "source-icon", Icon { name: "server" } }
                    h3 { "Hytale launcher" }
                    p { class: "muted small", "Copies the server and Assets.zip from the game installed on this computer." }
                    div { class: "source-body",
                        if launcher.is_empty() { p { class: "muted small", "No Hytale launcher install found here." } }
                    }
                    div { class: "source-actions",
                        for (patchline, folder) in launcher {
                            {
                                let st = state.clone();
                                let (p2, f2) = (patchline.clone(), folder.clone());
                                rsx! {
                                    button { key: "{patchline}", class: "btn", title: "{folder.display()}",
                                        onclick: move |_| { actions::import_launcher(&st, p2.clone(), f2.clone()); toast.ok("Import started"); },
                                        "Import {patchline}"
                                    }
                                }
                            }
                        }
                    }
                }
                div { class: "panel glass source",
                    div { class: "source-icon", Icon { name: "archive" } }
                    h3 { "Zip or folder" }
                    p { class: "muted small", "A path on the machine running HyOrchestrator: a server zip, or a folder with Server/ and Assets.zip." }
                    div { class: "source-body",
                        input { class: "input mono", placeholder: "/srv/hytale/game.zip", value: "{path}", oninput: move |e| path.set(e.value()) }
                    }
                    div { class: "source-actions",
                        button { class: "btn", disabled: path().trim().is_empty(), onclick: move |_| { toast.result(actions::import_path(&s3, &path()), "Import started"); }, "Import" }
                    }
                }
            }
            div { class: "panel glass",
                h3 { "In the cache ({list.len()})" }
                if list.is_empty() {
                    Empty { icon: "layers", title: "No versions yet", text: "Download the latest release above. The download is several GB (mostly Assets.zip)." }
                } else {
                    table { class: "table",
                        thead { tr { th { "Version" } th { "Channel" } th { "From" } th { "Size" } th { "Added" } th { "Used by" } th {} } }
                        tbody {
                            for v in list {
                                {
                                    let users: Vec<String> = instances.iter().filter(|i| i.version == v.version).map(|i| i.name.clone()).collect();
                                    let used = users.join(", ");
                                    let st = state.clone();
                                    let ver = v.version.clone();
                                    let when = crate::state::fmt_time(v.added_at);
                                    rsx! {
                                        tr { key: "{v.version}",
                                            td { b { class: "mono", "{v.version}" } }
                                            td { span { class: "pill sm", "{v.patchline}" } }
                                            td { class: "muted", "{v.source}" }
                                            td { class: "muted", "{human_bytes(v.size)}" }
                                            td { class: "muted small", "{when}" }
                                            td { class: "small", if used.is_empty() { span { class: "muted", "-" } } else { "{used}" } }
                                            td { class: "right",
                                                ConfirmButton { label: "Delete", confirm: "Delete this version?", class: "sm ghost danger-text", disabled: !users.is_empty(),
                                                    onconfirm: move |_| toast.result(versions::delete(&st, &ver), "Version deleted")
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Banner { kind: "info", icon: "info",
                "Keep a version only while an instance uses it: each takes about 3-4 GB. "
                button { class: "btn sm ghost", onclick: move |_| nav.set(Page::Create), "Create an instance" }
            }
        }
    }
}
