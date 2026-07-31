use std::fs::File;
use std::path::Path;
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;

use tiny_http::{Header, Method, Response, Server};

use crate::events::EventLog;
use crate::types::{WindowCommand, WindowResponse};

/// Vendored JS bundles served locally (no CDN dependency at runtime).
const VENDOR_FILES: [&str; 3] = [
    "react17.production.min.js",
    "react-dom17.production.min.js",
    "babel.min.js",
];

/// Commands sent from the HTTP server to the GTK thread.
/// Cada comando carrega um canal de resposta (`serde_json::Value`) para
/// devolver o resultado real (ex: o id da janela criada) — antes o ack era
/// cego (respondia "success" sem o id).
pub enum GtkCommand {
    CreateWindow(WindowCommand, mpsc::Sender<serde_json::Value>),
    UpdateWindow(WindowCommand, mpsc::Sender<serde_json::Value>),
    CloseWindow(WindowCommand, mpsc::Sender<serde_json::Value>),
    ListWindows(mpsc::Sender<serde_json::Value>),
    OpenMainWindow(mpsc::Sender<serde_json::Value>),
    Shutdown,
}

/// Starts a simple HTTP server (POST only) that receives window commands.
/// The server runs in a separate thread and sends commands to the GTK thread via channel.
pub fn start_ipc_server(
    port: u16,
    tx: mpsc::Sender<GtkCommand>,
    events: Arc<EventLog>,
) -> Result<(), String> {
    let addr = format!("127.0.0.1:{}", port);
    let server = Server::http(&addr).map_err(|e| format!("Failed to bind: {}", e))?;
    log::info!("WindowLoom IPC server listening on http://{}", addr);

    thread::spawn(move || {
        for mut request in server.incoming_requests() {
            // GET: rotas de consulta (vendor, eventos)
            if request.method() == &Method::Get {
                let url = request.url().to_string();
                if url.starts_with("/events") {
                    if url == "/events/clear" {
                        events.clear();
                        let _ = request.respond(Response::from_string("ok"));
                    } else {
                        serve_events(request, &events);
                    }
                } else if url.starts_with("/vendor/") {
                    serve_vendor(request);
                } else {
                    let _ = request.respond(Response::from_string("not found").with_status_code(404));
                }
                continue;
            }

            let mut body = String::new();
            if let Err(e) = request.as_reader().read_to_string(&mut body) {
                log::error!("Failed to read request body: {}", e);
                continue;
            }

            let cmd: WindowCommand = match serde_json::from_str(&body) {
                Ok(c) => c,
                Err(e) => {
                    let resp = WindowResponse {
                        success: false,
                        id: None,
                        error: Some(format!("Invalid JSON: {}", e)),
                    };
                    let json = serde_json::to_string(&resp).unwrap_or_default();
                    let h = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
                    let _ = request.respond(Response::from_string(json).with_header(h));
                    continue;
                }
            };

            let (reply_tx, reply_rx) = mpsc::channel::<serde_json::Value>();
            let gtk_cmd = match cmd.action.as_str() {
                "CREATE_WINDOW" | "create_window" => GtkCommand::CreateWindow(cmd, reply_tx),
                "UPDATE_WINDOW" | "update_window" => GtkCommand::UpdateWindow(cmd, reply_tx),
                "CLOSE_WINDOW" | "close_window" => GtkCommand::CloseWindow(cmd, reply_tx),
                "LIST_WINDOWS" | "list_windows" => GtkCommand::ListWindows(reply_tx),
                "OPEN_MAIN_WINDOW" | "open_main_window" | "main" => {
                    GtkCommand::OpenMainWindow(reply_tx)
                }
                other => {
                    let resp = WindowResponse {
                        success: false,
                        id: None,
                        error: Some(format!("Unknown action: {}", other)),
                    };
                    let json = serde_json::to_string(&resp).unwrap_or_default();
                    let h = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
                    let _ = request.respond(Response::from_string(json).with_header(h));
                    continue;
                }
            };

            if let Err(e) = tx.send(gtk_cmd) {
                log::error!("Failed to send command to GTK thread: {}", e);
            }

            // Aguarda a resposta real do GTK thread (ex: o id criado).
            // Antes o ack era cego — respondia "success" sem o resultado.
            let json = match reply_rx.recv_timeout(std::time::Duration::from_secs(3)) {
                Ok(v) => serde_json::to_string(&v).unwrap_or_default(),
                Err(_) => r#"{"success":false,"error":"timeout: GTK thread não respondeu"}"#.to_string(),
            };
            let h = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
            let _ = request.respond(Response::from_string(json).with_header(h));
        }
    });

    Ok(())
}

/// `GET /events` e `GET /events?since=<ts>` — eventos do appBus em JSON
/// (mais novo primeiro). Com `since`, só eventos com `ts >= since`
/// (polling incremental para o agente).
fn serve_events(request: tiny_http::Request, events: &EventLog) {
    let url = request.url().to_string();
    let since = url
        .split('?')
        .nth(1)
        .and_then(|q| q.strip_prefix("since="))
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);
    let list = if since > 0 {
        events.since(since, 100)
    } else {
        events.recent(100)
    };
    let json = serde_json::to_string(&list).unwrap_or_else(|_| "[]".into());
    let h = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
    let _ = request.respond(Response::from_string(json).with_header(h));
}

/// Serves a vendored JS bundle from `<project>/vendor/` (whitelisted filenames only).
fn serve_vendor(request: tiny_http::Request) {
    let url = request.url().to_string();
    let name = url.strip_prefix("/vendor/").unwrap_or("").split('/').next().unwrap_or("");

    if !VENDOR_FILES.contains(&name) {
        let _ = request.respond(Response::from_string("not found").with_status_code(404));
        return;
    }

    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("vendor").join(name);
    match File::open(&path) {
        Ok(file) => {
            let h = Header::from_bytes(&b"Content-Type"[..], &b"application/javascript"[..]).unwrap();
            let _ = request.respond(Response::from_file(file).with_header(h));
        }
        Err(_) => {
            let _ = request.respond(Response::from_string("not found").with_status_code(404));
        }
    }
}
