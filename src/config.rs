use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Configurações do app, persistidas em
/// `~/.config/rust-canvas-windows/config.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Largura padrão das janelas novas (file chooser)
    pub width: u32,
    /// Altura padrão das janelas novas (file chooser)
    pub height: u32,
    /// Iniciar o app junto com o login (autostart .desktop)
    pub autostart: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            width: 600,
            height: 400,
            autostart: false,
        }
    }
}

fn config_home() -> PathBuf {
    std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
            PathBuf::from(home).join(".config")
        })
}

pub fn config_path() -> PathBuf {
    config_home().join("rust-canvas-windows").join("config.json")
}

pub fn load() -> Config {
    std::fs::read_to_string(config_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(cfg: &Config) -> Result<(), String> {
    let path = config_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())
}

/// Cria/remove `~/.config/autostart/rust-canvas-windows.desktop`.
/// O Exec usa `env` para injetar as env vars necessárias (X11 + software
/// rendering do WebKit) — sem elas as janelas ficam pretas.
pub fn apply_autostart(enabled: bool) -> Result<(), String> {
    let desktop = config_home().join("autostart").join("rust-canvas-windows.desktop");
    if enabled {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let content = format!(
            "[Desktop Entry]\n\
             Type=Application\n\
             Name=Rust Canvas Windows\n\
             Comment=Janelas JSX nativas\n\
             Exec=env GDK_BACKEND=x11 WEBKIT_DISABLE_DMABUF_RENDERER=1 {}\n\
             X-GNOME-Autostart-enabled=true\n",
            exe.display()
        );
        if let Some(dir) = desktop.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(&desktop, content).map_err(|e| e.to_string())?;
    } else {
        let _ = std::fs::remove_file(&desktop);
    }
    Ok(())
}
