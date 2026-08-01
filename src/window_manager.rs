use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

use gtk::prelude::*;
use gtk::{Application, ApplicationWindow};
use webkit2gtk::{UserContentManagerExt, WebContext, WebView, WebViewExt};

use crate::types::WindowState;
use crate::widget_renderer;

/// Caminho do ícone do app (gerado em `assets/windowloom.png`).
const APP_ICON_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/windowloom.png");

/// Carrega o pixbuf do favicon (PNG pequeno — carregado por janela).
fn app_icon_pixbuf() -> Option<gtk::gdk_pixbuf::Pixbuf> {
    gtk::gdk_pixbuf::Pixbuf::from_file(APP_ICON_PATH).ok()
}

pub struct WindowManager {
    app: Application,
    base_uri: String,
    config: std::sync::Arc<std::sync::Mutex<crate::config::Config>>,
    events: std::sync::Arc<crate::events::EventLog>,
    windows: Arc<Mutex<HashMap<String, WindowEntry>>>,
}

struct WindowEntry {
    state: WindowState,
    gtk_window: ApplicationWindow,
    webview: WebView,
    keep_above: bool,
}

impl WindowManager {
    /// `base_uri` is the base URL for the HTML loaded into webviews (used to
    /// resolve `/vendor/*` script paths against the local IPC server).
    pub fn new(
        base_uri: String,
        config: std::sync::Arc<std::sync::Mutex<crate::config::Config>>,
        events: std::sync::Arc<crate::events::EventLog>,
    ) -> Self {
        // NON_UNIQUE: o GtkApplication é single-instance por id — sem essa
        // flag, uma 2ª instância delega o activate para a 1ª (recria tray e
        // janelas — indicador duplicado) e crasha ao sair.
        let app = Application::new(
            Some("com.windowloom.app"),
            gtk::gio::ApplicationFlags::NON_UNIQUE,
        );
        Self {
            app,
            base_uri,
            config,
            events,
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
        let config = self.config.clone();
        let events = self.events.clone();

        app.connect_activate(move |app| {
            // hold(): impede o run() de retornar quando não há janelas — o
            // app vive no tray mesmo com todas fechadas. (Sem o placeholder
            // antigo: a janela 1x1 transparente aparecia no compositor
            // Wayland.)
            app.hold();

            // Create any startup windows now that the app is active
            for state in &startup {
                if let Err(e) =
                    create_window_impl(&windows, app, &base_uri, &config, &events, state)
                {
                    log::error!("Failed to create startup window {}: {}", state.title, e);
                }
            }

            // System tray (StatusIcon — works on KDE/Plasma via XEmbed proxy)
            setup_tray(app, &windows, &base_uri, &config, &events);
        });

        // `app.run()` registers the application and fires `activate` itself —
        // calling `app.activate()` manually before `run()` hits
        // "application->priv->is_registered" and the handler never runs.
        app.run();
    }

    /// Create a new native window — must be called from the GTK thread.
    pub fn create_window(&self, state: WindowState) -> Result<String, String> {
        let windows = self.windows.clone();
        create_window_impl(
            &windows,
            &self.app,
            &self.base_uri,
            &self.config,
            &self.events,
            &state,
        )
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
            let config_json = serde_json::to_string(&*self.config.lock().unwrap())
                .unwrap_or_else(|_| "{}".into());
            let html = widget_renderer::build_widget_html(new_jsx, "{}", &config_json);
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

    /// Abre a janela principal (hub com menubar GTK). Chamado pelo tray ou
    /// via API/CLI (action OPEN_MAIN_WINDOW).
    pub fn open_main_window(&self) {
        open_main_window(
            &self.app,
            &self.windows,
            &self.base_uri,
            &self.config,
            &self.events,
        );
    }
}

/// Shared window-creation logic. Works both from the `activate` handler
/// (startup windows) and from IPC-triggered creation.
fn create_window_impl(
    windows: &Arc<Mutex<HashMap<String, WindowEntry>>>,
    app: &Application,
    base_uri: &str,
    config: &std::sync::Arc<std::sync::Mutex<crate::config::Config>>,
    events: &std::sync::Arc<crate::events::EventLog>,
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
        let config_json =
            serde_json::to_string(&*config.lock().unwrap()).unwrap_or_else(|_| "{}".into());
        widget_renderer::build_widget_html(&state.jsx, "{}", &config_json)
    };
    webview.load_html(&html, Some(base_uri));

    // appBus bridge (IAS-CANVAS-TOOL widgets): a JS `postMessage` on
    // `window.webkit.messageHandlers.canvasBus` is broadcast back into EVERY
    // window as `window.__canvasBus.__localEmit(payload)`. Each window is a
    // separate webview with its own `window`, so the bus must route through
    // the app process.
    {
        let bus_windows = windows.clone();
        let ev_log = events.clone();
        let win_title = state.title.clone();
        // O handler só existe no JS se registrado explicitamente no WebProcess
        ucm.register_script_message_handler("canvasBus");
        ucm.connect_script_message_received(Some("canvasBus"), move |_, result| {
            let payload = result.js_value().map(|v| v.to_string()).unwrap_or_default();
            if payload.is_empty() {
                return;
            }
            log::info!("canvasBus: payload recebido ({} bytes)", payload.len());
            // Registra no EventLog (consultável via GET /events e /events/stream)
            ev_log.push(&win_title, &payload);
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

    // configBus bridge: a janela de Configurações posta o JSON da config e o
    // app persiste (~/.config/rust-canvas-windows/config.json) e aplica
    // (autostart .desktop).
    {
        let cfg = config.clone();
        ucm.register_script_message_handler("configBus");
        ucm.connect_script_message_received(Some("configBus"), move |_, result| {
            let payload = result.js_value().map(|v| v.to_string()).unwrap_or_default();
            match serde_json::from_str::<crate::config::Config>(&payload) {
                Ok(mut new_cfg) => {
                    // O widget de Configurações só manda width/height/autostart —
                    // preserva o repo_zip (selecionado na aba Repo do hub).
                    new_cfg.repo_zip = cfg.lock().unwrap().repo_zip.clone();
                    if let Err(e) = crate::config::apply_autostart(new_cfg.autostart) {
                        log::error!("config: autostart falhou: {}", e);
                    }
                    if let Err(e) = crate::config::save(&new_cfg) {
                        log::error!("config: salvar falhou: {}", e);
                    }
                    *cfg.lock().unwrap() = new_cfg;
                    log::info!("Configurações salvas");
                }
                Err(e) => log::error!("config: JSON inválido: {}", e),
            }
        });
    }

    // Menu de contexto do widget (botão direito): recarregar, sempre no topo,
    // fechar. Suprime o menu padrão do WebKit (retorno true).
    {
        let ctx_windows = windows.clone();
        let ctx_id = id.clone();
        webview.connect_context_menu(move |webview, _menu, _event, _hit| {
            let ctx = gtk::Menu::new();

            let mi_reload = gtk::MenuItem::with_label("Recarregar");
            let wv = webview.clone();
            mi_reload.connect_activate(move |_| {
                wv.reload();
            });
            ctx.append(&mi_reload);

            let mi_top = gtk::MenuItem::with_label("Sempre no topo");
            let w = ctx_windows.clone();
            let wid = ctx_id.clone();
            mi_top.connect_activate(move |_| {
                if let Some(entry) = w.lock().unwrap().get_mut(&wid) {
                    entry.keep_above = !entry.keep_above;
                    entry.gtk_window.set_keep_above(entry.keep_above);
                    log::info!("Janela {} keep_above={}", wid, entry.keep_above);
                }
            });
            ctx.append(&mi_top);

            let mi_close = gtk::MenuItem::with_label("Fechar");
            let w = ctx_windows.clone();
            let wid = ctx_id.clone();
            mi_close.connect_activate(move |_| {
                if let Some(entry) = w.lock().unwrap().get(&wid) {
                    entry.gtk_window.close();
                }
            });
            ctx.append(&mi_close);

            ctx.show_all();
            ctx.popup_at_pointer(None);
            true
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
        keep_above: false,
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

/// Janela principal (hub) com menubar GTK clássica — Arquivo/Janelas/Ajuda —
/// e lista das janelas abertas (duplo-clique traz pra frente). A lista e o
/// submenu "Janelas" são atualizados por timer (2s), como no tray.
fn open_main_window(
    app: &Application,
    windows: &Arc<Mutex<HashMap<String, WindowEntry>>>,
    base_uri: &str,
    config: &std::sync::Arc<std::sync::Mutex<crate::config::Config>>,
    events: &std::sync::Arc<crate::events::EventLog>,
) {
    log::info!("open_main_window: criando a janela principal");
    let win = ApplicationWindow::new(app);
    win.set_title("WindowLoom");
    win.set_default_size(520, 380);
    if let Some(icon) = app_icon_pixbuf() {
        win.set_icon(Some(&icon));
    }

    let menubar = gtk::MenuBar::new();

    // Arquivo → Abrir widget... / Configurações / Sair
    let mi_file = gtk::MenuItem::with_label("Arquivo");
    let file_menu = gtk::Menu::new();

    let mi_open = gtk::MenuItem::with_label("Abrir widget...");
    {
        let w = windows.clone();
        let b = base_uri.to_string();
        let a = app.clone();
        let c = config.clone();
        let e = events.clone();
        mi_open.connect_activate(move |_| {
            open_widget_dialog(&a, &w, &b, &c, &e);
        });
    }
    file_menu.append(&mi_open);

    let mi_settings = gtk::MenuItem::with_label("Configurações");
    {
        let w = windows.clone();
        let b = base_uri.to_string();
        let a = app.clone();
        let c = config.clone();
        let e = events.clone();
        mi_settings.connect_activate(move |_| {
            let (width, height) = {
                let cfg = c.lock().unwrap();
                (cfg.width, cfg.height)
            };
            let state = WindowState {
                id: String::new(),
                title: "Configurações".into(),
                jsx: widget_renderer::config_widget(),
                width,
                height,
                x: 200,
                y: 150,
            };
            if let Err(err) = create_window_impl(&w, &a, &b, &c, &e, &state) {
                log::error!("falha ao abrir configurações: {}", err);
            }
        });
    }
    file_menu.append(&mi_settings);

    file_menu.append(&gtk::SeparatorMenuItem::new());
    // Fechar todas as janelas
    let mi_close_all = gtk::MenuItem::with_label("Fechar todas as janelas");
    {
        let w = windows.clone();
        mi_close_all.connect_activate(move |_| {
            let ids: Vec<String> = {
                let map = w.lock().unwrap();
                map.keys().cloned().collect()
            };
            for id in ids {
                if let Some(entry) = w.lock().unwrap().get(&id) {
                    entry.gtk_window.close();
                }
            }
            log::info!("Todas as janelas fechadas pelo hub");
        });
    }
    file_menu.append(&mi_close_all);
    file_menu.append(&gtk::SeparatorMenuItem::new());
    let mi_quit = gtk::MenuItem::with_label("Sair");
    {
        let a = app.clone();
        mi_quit.connect_activate(move |_| {
            log::info!("Saindo pelo menu principal");
            a.quit();
        });
    }
    file_menu.append(&mi_quit);
    mi_file.set_submenu(Some(&file_menu));
    menubar.append(&mi_file);

    // Janelas → submenu dinâmico
    let mi_windows = gtk::MenuItem::with_label("Janelas");
    let win_sub = gtk::Menu::new();
    mi_windows.set_submenu(Some(&win_sub));
    menubar.append(&mi_windows);

    // Ajuda → Sobre
    let mi_help = gtk::MenuItem::with_label("Ajuda");
    let help_menu = gtk::Menu::new();
    let mi_about = gtk::MenuItem::with_label("Sobre");
    {
        let icon = app_icon_pixbuf();
        mi_about.connect_activate(move |_| {
            let dlg = gtk::AboutDialog::new();
            dlg.set_program_name("WindowLoom");
            dlg.set_version(Some(env!("CARGO_PKG_VERSION")));
            dlg.set_comments(Some(
                "Janelas nativas com widgets JSX — controladas por agente.",
            ));
            dlg.set_website(Some("https://github.com/miguel9w/windowloom"));
            if let Some(ic) = &icon {
                dlg.set_logo(Some(ic));
            }
            dlg.connect_response(|d, _| d.close());
            dlg.show_all();
        });
    }
    help_menu.append(&mi_about);
    mi_help.set_submenu(Some(&help_menu));
    menubar.append(&mi_help);

    // HeaderBar: título + ações rápidas
    let header = gtk::HeaderBar::new();
    header.set_title(Some("WindowLoom"));
    header.set_subtitle(Some("janelas JSX nativas"));
    header.set_show_close_button(true);
    {
        let btn_new = gtk::Button::with_label("Nova janela");
        let w = windows.clone();
        let b = base_uri.to_string();
        let a = app.clone();
        let c = config.clone();
        let e = events.clone();
        btn_new.connect_clicked(move |_| {
            open_widget_dialog(&a, &w, &b, &c, &e);
        });
        header.pack_start(&btn_new);

        let btn_settings = gtk::Button::with_label("Configurações");
        let w = windows.clone();
        let b = base_uri.to_string();
        let a = app.clone();
        let c = config.clone();
        let e = events.clone();
        btn_settings.connect_clicked(move |_| {
            let (width, height) = {
                let cfg = c.lock().unwrap();
                (cfg.width, cfg.height)
            };
            let state = WindowState {
                id: String::new(),
                title: "Configurações".into(),
                jsx: widget_renderer::config_widget(),
                width,
                height,
                x: 200,
                y: 150,
            };
            if let Err(err) = create_window_impl(&w, &a, &b, &c, &e, &state) {
                log::error!("falha ao abrir configurações: {}", err);
            }
        });
        header.pack_end(&btn_settings);
    }
    win.set_titlebar(Some(&header));

    // Notebook: Janelas / Modelos / Eventos
    let notebook = gtk::Notebook::new();

    let page_windows = gtk::ListBox::new();
    notebook.append_page(&page_windows, Some(&gtk::Label::new(Some("Janelas"))));

    // Repo — seleção de zip + widgets por categoria
    let page_repo_scroll = gtk::ScrolledWindow::new(gtk::Adjustment::NONE, gtk::Adjustment::NONE);
    page_repo_scroll.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
    let repo_outer = gtk::Box::new(gtk::Orientation::Vertical, 8);
    let btn_zip = gtk::Button::with_label("Selecionar zip de widgets...");
    repo_outer.pack_start(&btn_zip, false, false, 0);
    let repo_vbox = gtk::Box::new(gtk::Orientation::Vertical, 8);
    repo_outer.pack_start(&repo_vbox, true, true, 0);
    page_repo_scroll.add(&repo_outer);
    notebook.append_page(&page_repo_scroll, Some(&gtk::Label::new(Some("Repo"))));

    let page_events = gtk::ListBox::new();
    notebook.append_page(&page_events, Some(&gtk::Label::new(Some("Eventos"))));

    // Repo: popula os embutidos (modelos) como categoria fixa
    let embutidos = crate::repo::RepoCategoria {
        nome: "Embutidos".into(),
        widgets: widget_renderer::modelos()
            .into_iter()
            .map(|(nome, f)| crate::repo::RepoWidget::embutido(nome.to_string(), f()))
            .collect(),
    };
    populate_repo_categorias(
        app,
        &repo_vbox,
        &[embutidos.clone()],
        &windows,
        &base_uri,
        &config,
        &events,
    );

    // Se a config tem um zip carregado, recarrega (persistência entre abas)
    if let Some(zip) = config.lock().unwrap().repo_zip.clone() {
        if let Ok(dest) = crate::repo::extrair_zip(std::path::Path::new(&zip)) {
            let cats = crate::repo::scan_repo(&dest);
            let mut todas = vec![embutidos];
            todas.extend(cats);
            populate_repo_categorias(
                app, &repo_vbox, &todas, &windows, &base_uri, &config, &events,
            );
        }
    }

    // Botão: selecionar zip → extrair → escanear → popular
    {
        let w = windows.clone();
        let b = base_uri.to_string();
        let a = app.clone();
        let c = config.clone();
        let e = events.clone();
        let vb = repo_vbox.clone();
        btn_zip.connect_clicked(move |_| {
            let dlg = gtk::FileChooserDialog::new(
                Some("Selecionar zip de widgets"),
                None::<&gtk::Window>,
                gtk::FileChooserAction::Open,
            );
            dlg.add_button("Cancelar", gtk::ResponseType::Cancel);
            dlg.add_button("Abrir", gtk::ResponseType::Accept);
            let filter = gtk::FileFilter::new();
            filter.set_name(Some("Zips de widgets (*.zip)"));
            filter.add_pattern("*.zip");
            dlg.add_filter(filter);
            let all = gtk::FileFilter::new();
            all.set_name(Some("Todos os arquivos"));
            all.add_pattern("*");
            dlg.add_filter(all);

            let (w2, b2, a2, c2, e2, vb2) = (
                w.clone(),
                b.clone(),
                a.clone(),
                c.clone(),
                e.clone(),
                vb.clone(),
            );
            dlg.connect_response(move |dlg, resp| {
                if resp == gtk::ResponseType::Accept {
                    if let Some(path) = dlg.file().and_then(|f| f.path()) {
                        log::info!("repo: selecionado {}", path.display());
                        match crate::repo::extrair_zip(&path) {
                            Ok(dest) => {
                                let cats = crate::repo::scan_repo(&dest);
                                log::info!("repo: {} categorias do zip", cats.len());
                                // Persiste o zip na config (recarrega ao reabrir)
                                {
                                    let mut cfg = c2.lock().unwrap();
                                    cfg.repo_zip = Some(path.to_string_lossy().into_owned());
                                    let _ = crate::config::save(&cfg);
                                }
                                let emb = crate::repo::RepoCategoria {
                                    nome: "Embutidos".into(),
                                    widgets: widget_renderer::modelos()
                                        .into_iter()
                                        .map(|(nome, f)| {
                                            crate::repo::RepoWidget::embutido(nome.to_string(), f())
                                        })
                                        .collect(),
                                };
                                let mut todas = vec![emb];
                                todas.extend(cats);
                                populate_repo_categorias(&a2, &vb2, &todas, &w2, &b2, &c2, &e2);
                            }
                            Err(err) => log::error!("repo: {}", err),
                        }
                    }
                }
                dlg.close();
            });
            dlg.show_all();
        });
    }

    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
    vbox.pack_start(&menubar, false, false, 0);
    vbox.pack_start(&notebook, true, true, 0);
    win.add(&vbox);

    // Atalhos: Ctrl+N (nova janela) / Ctrl+Q (sair)
    {
        let w = windows.clone();
        let b = base_uri.to_string();
        let a = app.clone();
        let c = config.clone();
        let e = events.clone();
        win.connect_key_press_event(move |_, ev| {
            if ev.state().contains(gtk::gdk::ModifierType::CONTROL_MASK) {
                let k = ev.keyval();
                if k == gtk::gdk::keys::constants::n {
                    open_widget_dialog(&a, &w, &b, &c, &e);
                    return glib::Propagation::Stop;
                }
                if k == gtk::gdk::keys::constants::q {
                    log::info!("Saindo pelo atalho Ctrl+Q");
                    a.quit();
                    return glib::Propagation::Stop;
                }
            }
            glib::Propagation::Proceed
        });
    }

    // Timer: submenu Janelas + lista com ações + feed de eventos
    let w_owned = windows.clone();
    let w1 = w_owned.clone();
    let w2 = w_owned.clone();
    let ev = events.clone();
    glib::timeout_add_local(std::time::Duration::from_secs(2), move || {
        repopulate_windows_menu(&win_sub, &w1);

        // Lista de janelas (com ações: Topo / Fechar)
        for child in page_windows.children() {
            page_windows.remove(&child);
        }
        let map = w2.lock().unwrap();
        if map.is_empty() {
            let lbl = gtk::Label::new(Some("(nenhuma janela aberta)"));
            page_windows.add(&lbl);
        }
        for (id, entry) in map.iter() {
            let row = gtk::ListBoxRow::new();
            let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            let lbl = gtk::Label::new(Some(&entry.state.title));
            lbl.set_xalign(0.0);
            let dim = gtk::Label::new(Some(&format!(
                "{}x{}",
                entry.state.width, entry.state.height
            )));
            dim.set_xalign(1.0);
            hbox.pack_start(&lbl, true, true, 0);
            hbox.pack_start(&dim, false, false, 0);

            let btn_top = gtk::Button::with_label("Topo");
            let w4 = w_owned.clone();
            let wid4 = id.clone();
            btn_top.connect_clicked(move |_| {
                if let Some(entry) = w4.lock().unwrap().get_mut(&wid4) {
                    entry.keep_above = !entry.keep_above;
                    entry.gtk_window.set_keep_above(entry.keep_above);
                    log::info!("Janela {} keep_above={}", wid4, entry.keep_above);
                }
            });
            hbox.pack_end(&btn_top, false, false, 0);

            let btn_close = gtk::Button::with_label("Fechar");
            let w5 = w_owned.clone();
            let wid5 = id.clone();
            btn_close.connect_clicked(move |_| {
                if let Some(entry) = w5.lock().unwrap().get(&wid5) {
                    entry.gtk_window.close();
                }
            });
            hbox.pack_end(&btn_close, false, false, 0);

            row.add(&hbox);
            let wid6 = id.clone();
            let w6 = w_owned.clone();
            row.connect_activate(move |_| {
                if let Some(e) = w6.lock().unwrap().get(&wid6) {
                    e.gtk_window.present();
                }
            });
            page_windows.add(&row);
        }
        page_windows.show_all();

        // Feed de eventos (do EventLog — polling)
        for child in page_events.children() {
            page_events.remove(&child);
        }
        let recent = ev.recent(15);
        if recent.is_empty() {
            let lbl = gtk::Label::new(Some(
                "(sem eventos — emita algo num widget, ex: Formulário)",
            ));
            page_events.add(&lbl);
        }
        for rec in recent {
            let row = gtk::ListBoxRow::new();
            let lbl = gtk::Label::new(Some(&format!("{} :: {} — {}", rec.window, rec.evt, rec.ts)));
            lbl.set_xalign(0.0);
            row.add(&lbl);
            page_events.add(&row);
        }
        page_events.show_all();

        glib::ControlFlow::Continue
    });

    win.show_all();
}

/// System tray icon + context menu (libappindicator / StatusNotifierItem).
/// Closing all windows keeps the app alive in the tray (the hidden
/// placeholder prevents GTK from quitting).
fn setup_tray(
    app: &Application,
    windows: &Arc<Mutex<HashMap<String, WindowEntry>>>,
    base_uri: &str,
    config: &std::sync::Arc<std::sync::Mutex<crate::config::Config>>,
    events: &std::sync::Arc<crate::events::EventLog>,
) {
    let mut menu = gtk::Menu::new();

    // "Janela principal" — hub com menubar GTK (Arquivo/Janelas/Ajuda)
    let mi_main = gtk::MenuItem::with_label("Janela principal");
    {
        let w = windows.clone();
        let b = base_uri.to_string();
        let a = app.clone();
        let c = config.clone();
        let e = events.clone();
        mi_main.connect_activate(move |_| {
            log::info!("tray: clique em 'Janela principal'");
            // O activate do MenuItem via dbusmenu pode disparar fora da main
            // thread — agendar no main loop (GTK exige a main thread para
            // criar/mostrar janelas e para o timeout_add_local).
            let (a2, w2, b2, c2, e2) = (a.clone(), w.clone(), b.clone(), c.clone(), e.clone());
            glib::idle_add_local(move || {
                open_main_window(&a2, &w2, &b2, &c2, &e2);
                glib::ControlFlow::Break
            });
        });
    }
    menu.append(&mi_main);
    menu.append(&gtk::SeparatorMenuItem::new());

    // "Nova janela" → file chooser para selecionar um widget .jsx/.html
    let mi_new = gtk::MenuItem::with_label("Abrir widget...");
    {
        let w = windows.clone();
        let b = base_uri.to_string();
        let a = app.clone();
        let c = config.clone();
        let e = events.clone();
        mi_new.connect_activate(move |_| {
            open_widget_dialog(&a, &w, &b, &c, &e);
        });
    }
    menu.append(&mi_new);

    // "Configurações" — janela com a config do app (autostart, tamanhos)
    let mi_settings = gtk::MenuItem::with_label("Configurações");
    {
        let w = windows.clone();
        let b = base_uri.to_string();
        let a = app.clone();
        let c = config.clone();
        let e = events.clone();
        mi_settings.connect_activate(move |_| {
            let (width, height) = {
                let cfg = c.lock().unwrap();
                (cfg.width, cfg.height)
            };
            let state = WindowState {
                id: String::new(),
                title: "Configurações".into(),
                jsx: widget_renderer::config_widget(),
                width,
                height,
                x: 200,
                y: 150,
            };
            if let Err(e) = create_window_impl(&w, &a, &b, &c, &e, &state) {
                log::error!("tray: falha ao abrir configurações: {}", e);
            }
        });
    }
    menu.append(&mi_settings);

    // "Janela de exemplo" (cardápio)
    let mi_example = gtk::MenuItem::with_label("Janela de exemplo");
    {
        let w = windows.clone();
        let b = base_uri.to_string();
        let a = app.clone();
        let c = config.clone();
        let e = events.clone();
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
            if let Err(e) = create_window_impl(&w, &a, &b, &c, &e, &state) {
                log::error!("tray: falha ao criar exemplo: {}", e);
            }
        });
    }
    menu.append(&mi_example);

    // Submenu "Janelas" — dinâmico: o libappindicator exporta o menu via
    // dbusmenu (o signal `show` do gtk::Menu nunca dispara), então o
    // submenu é repopulado por um timer (2s) com as janelas atuais.
    let sub = gtk::Menu::new();
    let mi_windows = gtk::MenuItem::with_label("Janelas");
    mi_windows.set_submenu(Some(&sub));
    menu.append(&mi_windows);
    {
        let w = windows.clone();
        // Primeira população imediata
        repopulate_windows_menu(&sub, &w);
        // E a cada 2s (o dbusmenu lê o estado atual do menu GTK)
        let w2 = w.clone();
        glib::timeout_add_local(std::time::Duration::from_secs(2), move || {
            repopulate_windows_menu(&sub, &w2);
            glib::ControlFlow::Continue
        });
    }

    menu.append(&gtk::SeparatorMenuItem::new());

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
    let indicator_id = format!("windowloom-{}", std::process::id());
    let mut indicator = libappindicator::AppIndicator::new(&indicator_id, "windowloom");
    indicator.set_status(libappindicator::AppIndicatorStatus::Active);
    indicator.set_menu(&mut menu);
    indicator.set_title("WindowLoom");
    // Ícone próprio do app (assets/) em vez do ícone de tema ("martelo")
    if let Some(dir) = std::path::Path::new(APP_ICON_PATH).parent() {
        indicator.set_icon_theme_path(dir.to_string_lossy().as_ref());
    }
    indicator.set_icon("windowloom");
    // Keep it alive for the process lifetime (standard tray pattern).
    std::mem::forget(indicator);

    log::info!("System tray ativo");
}

/// Preenche a aba Repo: categorias com botões de widgets (clique cria a
/// janela — lê o arquivo ou usa o JSX embutido).
fn populate_repo_categorias(
    app: &Application,
    vbox: &gtk::Box,
    categorias: &[crate::repo::RepoCategoria],
    windows: &Arc<Mutex<HashMap<String, WindowEntry>>>,
    base_uri: &str,
    config: &std::sync::Arc<std::sync::Mutex<crate::config::Config>>,
    events: &std::sync::Arc<crate::events::EventLog>,
) {
    for child in vbox.children() {
        vbox.remove(&child);
    }
    for cat in categorias {
        let lbl = gtk::Label::new(Some(&cat.nome));
        lbl.set_xalign(0.0);
        lbl.set_markup(&format!("<b>{}</b>", cat.nome));
        vbox.pack_start(&lbl, false, false, 0);

        let fb = gtk::FlowBox::new();
        fb.set_max_children_per_line(4);
        fb.set_selection_mode(gtk::SelectionMode::None);
        for w in &cat.widgets {
            let btn = gtk::Button::with_label(&w.nome);
            let nome = w.nome.clone();
            let jsx_inline = w.jsx_inline.clone();
            let path = w.path.clone();
            let ww = windows.clone();
            let bb = base_uri.to_string();
            let aa = app.clone();
            let cc = config.clone();
            let ee = events.clone();
            btn.connect_clicked(move |_| {
                // JSX: inline (embutidos) ou do arquivo extraído
                let jsx = match &jsx_inline {
                    Some(s) => Some(s.clone()),
                    None => std::fs::read_to_string(&path).ok(),
                };
                if let Some(jsx) = jsx {
                    let (width, height) = {
                        let cfg = cc.lock().unwrap();
                        (cfg.width, cfg.height)
                    };
                    let state = WindowState {
                        id: String::new(),
                        title: nome.clone(),
                        jsx,
                        width,
                        height,
                        x: 120,
                        y: 120,
                    };
                    if let Err(err) = create_window_impl(&ww, &aa, &bb, &cc, &ee, &state) {
                        log::error!("repo {}: {}", nome, err);
                    }
                } else {
                    log::error!("repo: não foi possível ler {}", path.display());
                }
            });
            let child = gtk::FlowBoxChild::new();
            child.add(&btn);
            fb.add(&child);
        }
        vbox.pack_start(&fb, false, false, 0);
    }
    vbox.show_all();
}

/// Reconstrói o submenu "Janelas" com as janelas atuais (título = item;
/// clique traz a janela para frente).
fn repopulate_windows_menu(sub: &gtk::Menu, windows: &Arc<Mutex<HashMap<String, WindowEntry>>>) {
    for child in sub.children() {
        sub.remove(&child);
    }
    let map = windows.lock().unwrap();
    if map.is_empty() {
        let it = gtk::MenuItem::with_label("(nenhuma janela)");
        it.set_sensitive(false);
        sub.append(&it);
    }
    for (id, entry) in map.iter() {
        let it = gtk::MenuItem::with_label(&entry.state.title);
        let wid = id.clone();
        let w2 = windows.clone();
        it.connect_activate(move |_| {
            if let Some(e) = w2.lock().unwrap().get(&wid) {
                e.gtk_window.present();
            }
        });
        sub.append(&it);
    }
    sub.show_all();
}

/// Opens a file chooser to pick a `.jsx`/`.html` widget file and creates a
/// window with its contents.
fn open_widget_dialog(
    app: &Application,
    windows: &Arc<Mutex<HashMap<String, WindowEntry>>>,
    base_uri: &str,
    config: &std::sync::Arc<std::sync::Mutex<crate::config::Config>>,
    events: &std::sync::Arc<crate::events::EventLog>,
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
    let c = config.clone();
    let e = events.clone();
    dialog.connect_response(move |dialog, response| {
        if response == gtk::ResponseType::Accept {
            if let Some(path) = dialog.file().and_then(|f| f.path()) {
                match std::fs::read_to_string(&path) {
                    Ok(jsx) => {
                        let title = path
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "Widget".into());
                        let (width, height) = {
                            let cfg = c.lock().unwrap();
                            (cfg.width, cfg.height)
                        };
                        let state = WindowState {
                            id: String::new(),
                            title,
                            jsx,
                            width,
                            height,
                            x: 120,
                            y: 120,
                        };
                        if let Err(e) = create_window_impl(&w, &a, &b, &c, &e, &state) {
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
