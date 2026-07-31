use std::sync::mpsc;
use std::thread;

use tiny_http::{Header, Response, Server};

use crate::types::{WindowCommand, WindowResponse};

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
