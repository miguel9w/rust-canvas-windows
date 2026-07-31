use std::fs::File;
use std::path::Path;
use std::sync::mpsc;
use std::thread;

use tiny_http::{Header, Method, Response, Server};

use crate::types::{WindowCommand, WindowResponse};

/// Vendored JS bundles served locally (no CDN dependency at runtime).
const VENDOR_FILES: [&str; 3] = [
    "react.production.min.js",
    "react-dom.production.min.js",
    "babel.min.js",
];

/// Commands sent from the HTTP server to the GTK thread
pub enum GtkCommand {
    CreateWindow(WindowCommand),
    UpdateWindow(WindowCommand),
    CloseWindow(WindowCommand),
    ListWindows,
    Shutdown,
}

/// Starts a simple HTTP server (POST only) that receives window commands.
/// The server runs in a separate thread and sends commands to the GTK thread via channel.
pub fn start_ipc_server(port: u16, tx: mpsc::Sender<GtkCommand>) -> Result<(), String> {
    let addr = format!("127.0.0.1:{}", port);
    let server = Server::http(&addr).map_err(|e| format!("Failed to bind: {}", e))?;
    log::info!("Rust Canvas IPC server listening on http://{}", addr);

    thread::spawn(move || {
        for mut request in server.incoming_requests() {
            // GET /vendor/* → serve vendored JS bundles (local, no CDN)
            if request.method() == &Method::Get {
                serve_vendor(request);
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

            let gtk_cmd = match cmd.action.as_str() {
                "CREATE_WINDOW" | "create_window" => GtkCommand::CreateWindow(cmd),
                "UPDATE_WINDOW" | "update_window" => GtkCommand::UpdateWindow(cmd),
                "CLOSE_WINDOW" | "close_window" => GtkCommand::CloseWindow(cmd),
                "LIST_WINDOWS" | "list_windows" => GtkCommand::ListWindows,
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

            // Acknowledge
            let resp = WindowResponse {
                success: true,
                id: None,
                error: None,
            };
            let json = serde_json::to_string(&resp).unwrap_or_default();
            let h = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
            let _ = request.respond(Response::from_string(json).with_header(h));
        }
    });

    Ok(())
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
