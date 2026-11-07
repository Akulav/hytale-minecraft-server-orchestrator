//! Desktop mode: a native window (tao + wry: WebView2 on Windows, WKWebView on macOS, WebKitGTK on Linux)
//! showing the same UI the web server serves. Closing the window stops the servers cleanly and quits.

use tao::dpi::LogicalSize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::window::{Icon, WindowBuilder};
use wry::{NewWindowResponse, WebViewBuilder};

use crate::state::Shared;

const ICON_PNG: &[u8] = include_bytes!("../assets/icon.png");

enum UserEvent {
    Quit,
    #[cfg(target_os = "macos")]
    Menu(muda::MenuId),
}

fn window_icon() -> Option<Icon> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(ICON_PNG));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::ALPHA);
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        return None;
    }
    buf.truncate(info.buffer_size());
    Icon::from_rgba(buf, info.width, info.height).ok()
}

/// Opens a link in the system browser (links that leave the app).
fn open_external(url: &str) {
    if url.starts_with("http://") || url.starts_with("https://") {
        let _ = open::that_detached(url);
    }
}

/// Runs the window until the program quits. Never returns.
pub fn run(state: Shared, url: String) -> ! {
    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    #[cfg(target_os = "macos")]
    let quit_item = {
        use muda::{Menu, MenuItem, PredefinedMenuItem, Submenu, accelerator::Accelerator};
        let menu = Menu::new();
        let app = Submenu::new("HyOrchestrator", true);
        let quit = MenuItem::new("Quit HyOrchestrator", true, "cmdorctrl+q".parse::<Accelerator>().ok());
        let _ = app.append_items(&[&PredefinedMenuItem::hide(None), &PredefinedMenuItem::separator(), &quit]);
        let edit = Submenu::new("Edit", true);
        let _ = edit.append_items(&[
            &PredefinedMenuItem::undo(None),
            &PredefinedMenuItem::redo(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::cut(None),
            &PredefinedMenuItem::copy(None),
            &PredefinedMenuItem::paste(None),
            &PredefinedMenuItem::select_all(None),
        ]);
        let _ = menu.append_items(&[&app, &edit]);
        menu.init_for_nsapp();
        std::mem::forget(menu);
        let p = proxy.clone();
        muda::MenuEvent::set_event_handler(Some(move |e: muda::MenuEvent| {
            let _ = p.send_event(UserEvent::Menu(e.id));
        }));
        quit.id().clone()
    };

    let window = WindowBuilder::new()
        .with_title("HyOrchestrator")
        .with_inner_size(LogicalSize::new(1320.0, 860.0))
        .with_min_inner_size(LogicalSize::new(900.0, 600.0))
        .with_window_icon(window_icon())
        .build(&event_loop)
        .unwrap_or_else(|e| {
            crate::logln(&format!("cannot open a window: {e}; run with --headless"));
            std::process::exit(1)
        });

    let origin = url.split('/').take(3).collect::<Vec<_>>().join("/");
    let builder = WebViewBuilder::new()
        .with_url(&url)
        .with_background_color((12, 14, 22, 255))
        .with_devtools(cfg!(debug_assertions))
        .with_navigation_handler(move |target| {
            let ours = target.starts_with(&origin) || target.starts_with("about:") || target.starts_with("data:");
            if !ours {
                open_external(&target);
            }
            ours
        })
        .with_new_window_req_handler(|target, _| {
            open_external(&target);
            NewWindowResponse::Deny
        });

    #[cfg(not(target_os = "linux"))]
    let webview = builder.build(&window);
    #[cfg(target_os = "linux")]
    let webview = {
        use tao::platform::unix::WindowExtUnix;
        use wry::WebViewBuilderExtUnix;
        builder.build_gtk(window.default_vbox().expect("gtk window"))
    };
    let webview = webview.unwrap_or_else(|e| {
        crate::logln(&format!("cannot create the web view: {e}. Windows needs the WebView2 runtime; Linux needs WebKitGTK 4.1. Or run with --headless."));
        std::process::exit(1)
    });

    // quit requests (window closed, Cmd+Q, an update being installed): stop the servers, then end
    {
        let st = state.clone();
        state.rt.spawn(async move {
            st.quit.notified().await;
            crate::process::stop_all(&st).await;
            let _ = proxy.send_event(UserEvent::Quit);
        });
    }

    let mut closing = false;
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        let _ = &webview;
        match event {
            #[cfg(target_os = "macos")]
            Event::UserEvent(UserEvent::Menu(id)) => {
                if id == quit_item && !closing {
                    closing = true;
                    window.set_title("HyOrchestrator - stopping servers…");
                    state.request_quit();
                }
            }
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                if !closing {
                    closing = true;
                    let running = state.runtimes.read().values().filter(|r| r.status.is_active()).count();
                    if running > 0 {
                        window.set_title(&format!("HyOrchestrator - stopping {running} server(s)…"));
                    }
                    state.request_quit();
                }
            }
            Event::UserEvent(UserEvent::Quit) => {
                crate::logln("bye");
                *control_flow = ControlFlow::Exit;
            }
            _ => {}
        }
    })
}
