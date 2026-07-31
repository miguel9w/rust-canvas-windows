use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

use gtk::prelude::*;
use gtk::{Application, ApplicationWindow};
use webkit2gtk::{UserContentManagerExt, WebContext, WebView, WebViewExt};

use crate::types::WindowState;
use crate::widget_renderer;

/// Caminho do favicon do app (gerado em `assets/rust-canvas.png`).
const APP_ICON_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/rust-canvas.png");

/// Carrega o pixbuf do favicon (PNG pequeno — carregado por janela).
fn app_icon_pixbuf() -> Option<gtk::gdk_pixbuf::Pixbuf> {
    gtk::gdk_pixbuf::Pixbuf::from_file(APP_ICON_PATH).ok()
}

pub struct WindowManager {
    app: Application,
    base_uri: String,
    windows: Arc<Mutex<HashMap<String, WindowEntry>>>,
}

struct WindowEntry {
    state: WindowState,
    gtk_window: ApplicationWindow,
    webview: WebView,
}

impl WindowManager {
    /// `base_uri` is the base URL for the HTML loaded into webviews (used to
    /// resolve `/vendor/*` script paths against the local IPC server).
    pub fn new(base_uri: String) -> Self {
        // NON_UNIQUE: o GtkApplication é single-instance por id — sem essa
        // flag, uma 2ª instância delega o activate para a 1ª (recria tray e
        // janelas — indicador duplicado) e crasha ao sair.
        let app = Application::new(
            Some("com.canvas.rust-windows"),
            gtk::gio::ApplicationFlags::NON_UNIQUE,
        );
        Self {
            app,
            base_uri,
            windows: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Run the GTK main loop (blocking, should be called from main thread).
    /// Startup windows are created inside the `activate` handler — the only
    /// correct place to create `GtkApplicationWindow`s (creating them before
    /// `app.run()` triggers GTK init-order crashes on some backends, e.g. Wayland).
    pub fn run(&self, startup: Vec<WindowState>) {
        let windows = self.windows.clone();
        let app = self.app.clone();
        let base_uri = self.base_uri.clone();

        // Create a hidden placeholder so GTK doesn't exit
        app.connect_activate(move |app| {
            let hidden = ApplicationWindow::new(app);
            hidden.set_default_size(1, 1);
            hidden.set_opacity(0.0);
            hidden.set_decorated(false);
            hidden.show_all();

            // Create any startup windows now that the app is active
            for state in &startup {
                if let Err(e) = create_window_impl(&windows, app, &base_uri, state) {
                    log::error!("Failed to create startup window {}: {}", state.title, e);
                }
            }

            // System tray (StatusIcon — works on KDE/Plasma via XEmbed proxy)
            setup_tray(app, &windows, &base_uri);
        });

        // `app.run()` registers the application and fires `activate` itself —
        // calling `app.activate()` manually before `run()` hits
        // "application->priv->is_registered" and the handler never runs.
        app.run();
    }

    /// Create a new native window — must be called from the GTK thread.
    pub fn create_window(&self, state: WindowState) -> Result<String, String> {
        let windows = self.windows.clone();
        create_window_impl(&windows, &self.app, &self.base_uri, &state)
    }

    /// Close a window by ID.
    pub fn close_window(&self, id: &str) -> Result<(), String> {
        let mut map = self.windows.lock().map_err(|e| e.to_string())?;
        if let Some(entry) = map.remove(id) {
            entry.gtk_window.close();
            Ok(())
        } else {
            Err(format!("Window {} not found", id))
        }
    }

    /// Update a window's JSX content.
    pub fn update_window(&self, id: &str, new_jsx: &str) -> Result<(), String> {
        let map = self.windows.lock().map_err(|e| e.to_string())?;
        if let Some(entry) = map.get(id) {
            let html = widget_renderer::build_widget_html(new_jsx, "{}");
            entry.webview.load_html(&html, None);
            Ok(())
        } else {
            Err(format!("Window {} not found", id))
        }
    }

    /// List all active windows.
    pub fn list_windows(&self) -> Vec<WindowState> {
        let map = self.windows.lock().unwrap();
        map.values().map(|e| e.state.clone()).collect()
    }
}

/// Shared window-creation logic. Works both from the `activate` handler
/// (startup windows) and from IPC-triggered creation.
fn create_window_impl(
    windows: &Arc<Mutex<HashMap<String, WindowEntry>>>,
    app: &Application,
    base_uri: &str,
    state: &WindowState,
) -> Result<String, String> {
    let id = if state.id.is_empty() {
        Uuid::new_v4().to_string()
    } else {
        state.id.clone()
    };

    let win = ApplicationWindow::new(app);
    win.set_title(&state.title);
    win.set_default_size(state.width as i32, state.height as i32);
    if state.x != 0 || state.y != 0 {
        win.move_(state.x, state.y);
    }
    // Favicon do app em todas as janelas
    if let Some(icon) = app_icon_pixbuf() {
        win.set_icon(Some(&icon));
    }

    let web_context = WebContext::default().unwrap();
    // UserContentManager carrega o bridge do appBus (script messages JS→Rust)
    let ucm = webkit2gtk::UserContentManager::new();
    let webview = WebView::with_user_content_manager(&ucm);
    webview.set_hexpand(true);
    webview.set_vexpand(true);
    win.add(&webview);

    // JSX widgets get wrapped in the React/Babel template; raw HTML
    // (anything starting with `<`) is loaded verbatim.
    let html = if state.jsx.trim_start().starts_with('<') {
        state.jsx.clone()
    } else {
        widget_renderer::build_widget_html(&state.jsx, "{}")
    };
    webview.load_html(&html, Some(base_uri));

    // appBus bridge (IAS-CANVAS-TOOL widgets): a JS `postMessage` on
    // `window.webkit.messageHandlers.canvasBus` is broadcast back into EVERY
    // window as `window.__canvasBus.__localEmit(payload)`. Each window is a
    // separate webview with its own `window`, so the bus must route through
    // the app process.
    {
        let bus_windows = windows.clone();
        // O handler só existe no JS se registrado explicitamente no WebProcess
        ucm.register_script_message_handler("canvasBus");
        ucm.connect_script_message_received(Some("canvasBus"), move |_, result| {
            let payload = result.js_value().map(|v| v.to_string()).unwrap_or_default();
            if payload.is_empty() {
                return;
            }
            log::info!("canvasBus: payload recebido ({} bytes)", payload.len());
            let js = format!(
                "window.__canvasBus && window.__canvasBus.__localEmit({});",
                serde_json::to_string(&payload).unwrap_or_else(|_| "\"\"".into())
            );
            let map = bus_windows.lock().unwrap();
            for entry in map.values() {
                let _ = entry.webview.run_javascript(
                    &js,
                    None::<&webkit2gtk::gio::Cancellable>,
                    |_| {},
                );
            }
            log::info!("canvasBus: broadcast feito para {} janelas", map.len());
        });
    }

    let mut map = windows.lock().unwrap();
    let entry = WindowEntry {
        state: WindowState {
            id: id.clone(),
            ..state.clone()
        },
        gtk_window: win.clone(),
        webview,
    };
    map.insert(id.clone(), entry);

    win.show_all();

    // Handle close
    let windows_clone = windows.clone();
    let id_clone = id.clone();
    win.connect_delete_event(move |_, _| {
        let mut map = windows_clone.lock().unwrap();
        map.remove(&id_clone);
        glib::Propagation::Proceed
    });

    Ok(id)
}

/// System tray icon + context menu (libappindicator / StatusNotifierItem).
/// Closing all windows keeps the app alive in the tray (the hidden
/// placeholder prevents GTK from quitting).
fn setup_tray(
    app: &Application,
    windows: &Arc<Mutex<HashMap<String, WindowEntry>>>,
    base_uri: &str,
) {
    let mut menu = gtk::Menu::new();

    // "Nova janela" → file chooser para selecionar um widget .jsx/.html
    let mi_new = gtk::MenuItem::with_label("Abrir widget...");
    {
        let w = windows.clone();
        let b = base_uri.to_string();
        let a = app.clone();
        mi_new.connect_activate(move |_| {
            open_widget_dialog(&a, &w, &b);
        });
    }
    menu.append(&mi_new);

    // "Janela de exemplo" (cardápio)
    let mi_example = gtk::MenuItem::with_label("Janela de exemplo");
    {
        let w = windows.clone();
        let b = base_uri.to_string();
        let a = app.clone();
        mi_example.connect_activate(move |_| {
            let state = WindowState {
                id: String::new(),
                title: "Exemplo".into(),
                jsx: widget_renderer::exemplo_cardapio(),
                width: 420,
                height: 300,
                x: 140,
                y: 140,
            };
            if let Err(e) = create_window_impl(&w, &a, &b, &state) {
                log::error!("tray: falha ao criar exemplo: {}", e);
            }
        });
    }
    menu.append(&mi_example);

    menu.append(&gtk::SeparatorMenuItem::new());

    // "Listar janelas"
    let mi_list = gtk::MenuItem::with_label("Listar janelas");
    {
        let w = windows.clone();
        mi_list.connect_activate(move |_| {
            let n = w.lock().map(|m| m.len()).unwrap_or(0);
            log::info!("Janelas ativas no tray: {}", n);
        });
    }
    menu.append(&mi_list);

    // "Sair"
    let mi_quit = gtk::MenuItem::with_label("Sair");
    {
        let a = app.clone();
        mi_quit.connect_activate(move |_| {
            log::info!("Saindo pelo tray");
            a.quit();
        });
    }
    menu.append(&mi_quit);

    menu.show_all();

    // StatusNotifierItem via libappindicator (works on KDE Plasma 6 / GNOME).
    // O id precisa ser único por processo: uma 2ª instância com o mesmo id
    // faz o libayatana crashar (segfault no registro do indicador).
    let indicator_id = format!("rust-canvas-windows-{}", std::process::id());
    let mut indicator =
        libappindicator::AppIndicator::new(&indicator_id, "rust-canvas");
    indicator.set_status(libappindicator::AppIndicatorStatus::Active);
    indicator.set_menu(&mut menu);
    indicator.set_title("Rust Canvas Windows");
    // Ícone próprio do app (assets/) em vez do ícone de tema ("martelo")
    if let Some(dir) = std::path::Path::new(APP_ICON_PATH).parent() {
        indicator.set_icon_theme_path(dir.to_string_lossy().as_ref());
    }
    indicator.set_icon("rust-canvas");
    // Keep it alive for the process lifetime (standard tray pattern).
    std::mem::forget(indicator);

    log::info!("System tray ativo");
}

/// Opens a file chooser to pick a `.jsx`/`.html` widget file and creates a
/// window with its contents.
fn open_widget_dialog(
    app: &Application,
    windows: &Arc<Mutex<HashMap<String, WindowEntry>>>,
    base_uri: &str,
) {
    let dialog = gtk::FileChooserDialog::new(
        Some("Selecionar arquivo JSX/HTML"),
        None::<&gtk::Window>,
        gtk::FileChooserAction::Open,
    );
    dialog.add_button("Cancelar", gtk::ResponseType::Cancel);
    dialog.add_button("Abrir", gtk::ResponseType::Accept);
    dialog.set_modal(true);

    let filter = gtk::FileFilter::new();
    filter.set_name(Some("Widgets (JSX/HTML)"));
    filter.add_pattern("*.jsx");
    filter.add_pattern("*.html");
    dialog.add_filter(filter);

    let all = gtk::FileFilter::new();
    all.set_name(Some("Todos os arquivos"));
    all.add_pattern("*");
    dialog.add_filter(all);

    let w = windows.clone();
    let b = base_uri.to_string();
    let a = app.clone();
    dialog.connect_response(move |dialog, response| {
        if response == gtk::ResponseType::Accept {
            if let Some(path) = dialog.file().and_then(|f| f.path()) {
                match std::fs::read_to_string(&path) {
                    Ok(jsx) => {
                        let title = path
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "Widget".into());
                        let state = WindowState {
                            id: String::new(),
                            title,
                            jsx,
                            width: 600,
                            height: 400,
                            x: 120,
                            y: 120,
                        };
                        if let Err(e) = create_window_impl(&w, &a, &b, &state) {
                            log::error!("widget: falha ao criar janela: {}", e);
                        }
                    }
                    Err(e) => log::error!("widget: falha ao ler {}: {}", path.display(), e),
                }
            }
        }
        dialog.close();
    });

    dialog.show_all();
}
