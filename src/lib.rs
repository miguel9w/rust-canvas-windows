//! WindowLoom — native desktop windows rendering dynamic JSX widgets.
//!
//! Biblioteca compartilhada entre o app GTK (`src/main.rs`) e o CLI
//! (`src/bin/windowloom.rs`).

pub mod config;
pub mod events;
pub mod ipc_server;
pub mod pkg;
pub mod repo;
pub mod types;
pub mod widget_renderer;
pub mod window_manager;

/// Testes de vários módulos mexem em env vars globais (XDG). Este lock
/// serializa a suíte para não haver corrida entre testes paralelos.
#[cfg(test)]
pub(crate) static TEST_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
