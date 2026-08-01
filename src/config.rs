use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Configurações do app, persistidas em
/// `~/.config/windowloom/config.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Largura padrão das janelas novas (file chooser)
    pub width: u32,
    /// Altura padrão das janelas novas (file chooser)
    pub height: u32,
    /// Iniciar o app junto com o login (autostart .desktop)
    pub autostart: bool,
    /// Caminho do último zip de widgets carregado na aba Repo do hub
    #[serde(default)]
    pub repo_zip: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            width: 600,
            height: 400,
            autostart: false,
            repo_zip: None,
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
    config_home().join("windowloom").join("config.json")
}

/// Nome antigo do diretório de config (pré-WindowLoom) — removido ao migrar.
fn legacy_config_dir() -> PathBuf {
    config_home().join("rust-canvas-windows")
}

pub fn load() -> Config {
    let new_path = config_path();
    let legacy = legacy_config_dir().join("config.json");
    // Migração: copia a config antiga (se existir) para o diretório novo
    if !new_path.exists() && legacy.exists() {
        if let Ok(cfg) = std::fs::read_to_string(&legacy)
            .and_then(|s| Ok(serde_json::from_str::<Config>(&s).unwrap_or_default()))
        {
            let _ = save(&cfg);
        }
    }
    let _ = std::fs::remove_dir_all(&legacy_config_dir());
    std::fs::read_to_string(new_path)
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

/// Cria/remove `~/.config/autostart/windowloom.desktop`.
/// O Exec é só o binário — o app detecta o Wayland e se auto-configura
/// (software rendering do WebKit) no startup.
pub fn apply_autostart(enabled: bool) -> Result<(), String> {
    let desktop = config_home().join("autostart").join("windowloom.desktop");
    // Remove o autostart com o nome antigo (pré-WindowLoom), se existir
    let _ = std::fs::remove_file(
        config_home()
            .join("autostart")
            .join("rust-canvas-windows.desktop"),
    );
    if enabled {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let content = format!(
            "[Desktop Entry]\n\
             Type=Application\n\
             Name=WindowLoom\n\
             Comment=Janelas JSX nativas\n\
             Exec={}\n\
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
