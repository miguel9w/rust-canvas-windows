mod config;
mod events;
mod ipc_server;
mod repo;
mod types;
mod widget_renderer;
mod window_manager;

use std::sync::mpsc;

fn main() {
    // Auto-configuração do Wayland: o WebKit2GTK com DMABUF (GPU) falha no
    // Wayland nativo ("Error 71 dispatching to Wayland display" ao criar a
    // webview). Detectamos o Wayland e forçamos o software rendering ANTES
    // do GTK/WebKit inicializarem — o app roda nativo Wayland sem env vars.
    if std::env::var("WAYLAND_DISPLAY").is_ok()
        && std::env::var("WEBKIT_DISABLE_DMABUF_RENDERER").is_err()
    {
        // SAFETY: no início do main, antes de spawnar qualquer thread —
        // nenhuma outra thread lê/escreve env vars concorrentemente.
        unsafe { std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1") };
    }

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();

    if std::env::var("WAYLAND_DISPLAY").is_ok() {
        log::info!("Wayland detectado — software rendering do WebKit ativado");
    }

    let port: u16 = std::env::var("RUST_CANVAS_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8081);

    println!("╭──────────────────────────────────────╮");
    println!("│  WindowLoom                           │");
    println!("│  Native desktop windows for JSX      │");
    println!("│                                      │");
    println!("│  POST http://127.0.0.1:{}         │", port);
    println!("╰──────────────────────────────────────╯");
    println!();

    // Channel: HTTP server thread -> GTK main thread
    let (tx, rx) = mpsc::channel::<ipc_server::GtkCommand>();

    // Initialize window manager on the main thread.
    // Base URI points at the local IPC server so `/vendor/*` scripts
    // (React/Babel bundles) are served by the app itself — no CDN, no network.
    let base_uri = format!("http://127.0.0.1:{}/", port);
    let config = std::sync::Arc::new(std::sync::Mutex::new(config::load()));
    let events = std::sync::Arc::new(events::EventLog::new());
    let wm = window_manager::WindowManager::new(base_uri, config, events.clone());

    // No startup windows: the app lives in the system tray. Open widgets
    // from the tray menu (Abrir widget... / Configurações).
    let startup_windows: Vec<types::WindowState> = vec![];

    // Spawn HTTP server in background thread
    let tx_clone = tx.clone();
    std::thread::spawn(move || {
        if let Err(e) = ipc_server::start_ipc_server(port, tx_clone, events) {
            log::error!("IPC server failed: {}", e);
        }
    });

    // GTK main loop — processes IPC commands via idle_add
    // Use a raw pointer since WindowManager contains GTK types (not Send)
    let wm_ptr: *const window_manager::WindowManager = &wm;

    glib::idle_add_local(move || {
        // SAFETY: wm_ptr is only accessed from the GTK main thread
        let wm = unsafe { &*wm_ptr };

        while let Ok(cmd) = rx.try_recv() {
            match cmd {
                ipc_server::GtkCommand::CreateWindow(c, reply) => {
                    let state = types::WindowState {
                        id: c.id.unwrap_or_default(),
                        title: c.title.unwrap_or_else(|| "Widget".into()),
                        jsx: c.jsx.unwrap_or_else(crate::widget_renderer::blank_widget),
                        width: c.width.unwrap_or(600),
                        height: c.height.unwrap_or(400),
                        x: c.x.unwrap_or(100),
                        y: c.y.unwrap_or(100),
                    };
                    match wm.create_window(state) {
                        Ok(id) => {
                            log::info!("Window created: {}", id);
                            let _ = reply.send(serde_json::json!({"success": true, "id": id}));
                        }
                        Err(e) => {
                            log::error!("Failed to create window: {}", e);
                            let _ = reply.send(serde_json::json!({"success": false, "error": e}));
                        }
                    }
                }
                ipc_server::GtkCommand::UpdateWindow(c, reply) => {
                    if let (Some(id), Some(jsx)) = (c.id, c.jsx) {
                        match wm.update_window(&id, &jsx) {
                            Ok(()) => {
                                log::info!("Window {} updated", id);
                                let _ = reply.send(serde_json::json!({"success": true, "id": id}));
                            }
                            Err(e) => {
                                log::error!("Failed to update window: {}", e);
                                let _ =
                                    reply.send(serde_json::json!({"success": false, "error": e}));
                            }
                        }
                    } else {
                        let _ = reply
                            .send(serde_json::json!({"success": false, "error": "id e jsx são obrigatórios"}));
                    }
                }
                ipc_server::GtkCommand::CloseWindow(c, reply) => {
                    if let Some(id) = c.id {
                        match wm.close_window(&id) {
                            Ok(()) => {
                                log::info!("Window {} closed", id);
                                let _ = reply.send(serde_json::json!({"success": true, "id": id}));
                            }
                            Err(e) => {
                                log::error!("Failed to close window: {}", e);
                                let _ =
                                    reply.send(serde_json::json!({"success": false, "error": e}));
                            }
                        }
                    } else {
                        let _ = reply.send(
                            serde_json::json!({"success": false, "error": "id é obrigatório"}),
                        );
                    }
                }
                ipc_server::GtkCommand::ListWindows(reply) => {
                    let windows = wm.list_windows();
                    log::info!("Active windows: {}", windows.len());
                    let list: Vec<serde_json::Value> = windows
                        .iter()
                        .map(|w| serde_json::json!({"id": w.id, "title": w.title, "width": w.width, "height": w.height}))
                        .collect();
                    let _ = reply.send(serde_json::json!({"success": true, "windows": list}));
                }
                ipc_server::GtkCommand::OpenMainWindow(reply) => {
                    wm.open_main_window();
                    let _ = reply.send(serde_json::json!({"success": true}));
                }
                ipc_server::GtkCommand::Shutdown => {
                    log::info!("Shutting down...");
                    return glib::ControlFlow::Break;
                }
            }
        }
        glib::ControlFlow::Continue
    });

    // Run GTK main loop (blocks)
    wm.run(startup_windows);
}
