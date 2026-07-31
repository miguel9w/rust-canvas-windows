use serde::{Deserialize, Serialize};

/// Command received via WebSocket to create/update a window
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowCommand {
    pub action: String, // "CREATE_WINDOW" | "UPDATE_WINDOW" | "CLOSE_WINDOW"
    pub id: Option<String>,
    pub title: Option<String>,
    pub jsx: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub props: Option<serde_json::Value>,
}

/// Response sent back via WebSocket
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowResponse {
    pub success: bool,
    pub id: Option<String>,
    pub error: Option<String>,
}

/// Internal state for a managed window
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowState {
    pub id: String,
    pub title: String,
    pub jsx: String,
    pub width: u32,
    pub height: u32,
    pub x: i32,
    pub y: i32,
}
