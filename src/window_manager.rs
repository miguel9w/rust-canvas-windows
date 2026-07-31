use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

use gtk::prelude::*;
use gtk::{Application, ApplicationWindow};
use webkit2gtk::{WebView, WebViewExt, WebContext};

use crate::types::WindowState;
use crate::widget_renderer;

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
        let app = Application::new(Some("com.canvas.rust-windows"), Default::default());
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

    let web_context = WebContext::default().unwrap();
    let webview = WebView::with_context(&web_context);
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
