//! pkg — gerenciador de pacotes do WindowLoom.
//!
//! Um pacote é um widget/janela distribuído como `.wlpkg` (zip com
//! `manifest.json`) ou como arquivo `.jsx`/`.html` avulso vindo de um
//! registry (fonte de pacotes).
//!
//! Registries:
//!   - **local**: diretório com `index.json` no formato WindowLoom
//!     (`{ "packages": [...] }`) ou IAS-CANVAS-TOOL (`{ "widgets": [...] }`)
//!     — ex: o widgets-database do IAS-CANVAS-TOOL;
//!   - **http(s)**: URL que serve `index.json` + artefatos `.wlpkg`
//!     (ou `.jsx`/`.html` avulsos, resolvidos relativos ao index).
//!
//! Layout em disco:
//!   - Instalados: `~/.local/share/windowloom/pkgs/<nome>/` (cada pacote
//!     tem `manifest.json` + arquivos);
//!   - Estado: `~/.local/share/windowloom/pkgs/installed.json`;
//!   - Fontes: `~/.config/windowloom/repos.json`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Extensão dos pacotes empacotados (zip + manifest).
pub const PKG_EXT: &str = "wlpkg";

// ---------------------------------------------------------------------------
// Manifest
// ---------------------------------------------------------------------------

/// `manifest.json` dentro de um pacote (ou sintetizado na instalação).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub name: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    /// Arquivo de entrada (relativo à raiz do pacote).
    pub entry: String,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub license: Option<String>,
}

fn default_version() -> String {
    "0.0.0".into()
}

impl Manifest {
    pub fn titulo(&self) -> String {
        self.title
            .clone()
            .unwrap_or_else(|| self.name.clone())
    }
}

// ---------------------------------------------------------------------------
// Fontes (registries)
// ---------------------------------------------------------------------------

/// Fonte de pacotes configurada em `repos.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum RepoSource {
    /// Diretório local com `index.json`.
    Local { path: String },
    /// URL base com `index.json` (o index é resolvido em `<url>/index.json`).
    Http { url: String },
}

impl RepoSource {
    /// `index.json` de cada fonte.
    pub fn index_url(&self) -> String {
        match self {
            RepoSource::Local { path } => format!("{}/index.json", path.trim_end_matches('/')),
            RepoSource::Http { url } => format!("{}/index.json", url.trim_end_matches('/')),
        }
    }

    /// Resolve o caminho/URL de um arquivo referenciado no index
    /// (`file` = artefato .wlpkg, `entry` = .jsx/.html avulso).
    fn resolve(&self, arquivo: &str) -> String {
        match self {
            RepoSource::Local { path } => {
                Path::new(path).join(arquivo).to_string_lossy().into_owned()
            }
            RepoSource::Http { url } => format!("{}/{}", url.trim_end_matches('/'), arquivo),
        }
    }
}

/// Configuração de fontes: `~/.config/windowloom/repos.json`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReposCfg {
    #[serde(default)]
    pub repos: HashMap<String, RepoSource>,
}

/// Entrada de um registry (formato WindowLoom `packages[]`).
#[derive(Debug, Clone, Deserialize, Default)]
pub struct RegistryPackage {
    #[serde(default)]
    pub name: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    /// `.jsx`/`.html` avulso (instala copiando).
    #[serde(default)]
    pub entry: Option<String>,
    /// Artefato `.wlpkg` (instala extraindo o zip).
    #[serde(default)]
    pub file: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub size_bytes: Option<u64>,
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Entrada no formato IAS-CANVAS-TOOL (`widgets[]` — sem versão).
#[derive(Debug, Clone, Deserialize, Default)]
struct IasWidget {
    #[serde(default)]
    id: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    file: Option<String>,
    #[serde(default)]
    size_bytes: Option<u64>,
    #[serde(default)]
    tags: Vec<String>,
}

impl From<IasWidget> for RegistryPackage {
    fn from(w: IasWidget) -> Self {
        // Sem versão no formato IAS: "0.0.0" faz qualquer upgrade do registry
        // substituir a cópia local (re-sincroniza do diretório vivo).
        RegistryPackage {
            name: w
                .id
                .split('/')
                .last()
                .map(|s| s.to_string())
                .unwrap_or_default(),
            version: "0.0.0".into(),
            title: w.title,
            description: w.description,
            category: w.category,
            entry: w.file,
            size_bytes: w.size_bytes,
            tags: w.tags,
            ..Default::default()
        }
    }
}

/// `index.json` de um registry — aceita o formato WindowLoom (`packages[]`)
/// e o IAS-CANVAS-TOOL (`widgets[]`).
#[derive(Debug, Deserialize, Default)]
struct RegistryIndex {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    packages: Vec<RegistryPackage>,
    #[serde(default)]
    widgets: Vec<IasWidget>,
}

/// Registry carregado (nome + fonte + pacotes).
#[derive(Debug, Clone)]
pub struct Registry {
    pub nome: String,
    pub fonte: RepoSource,
    pub pacotes: Vec<RegistryPackage>,
}

// ---------------------------------------------------------------------------
// Estado instalado
// ---------------------------------------------------------------------------

/// `installed.json` — pacotes instalados.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InstalledDb {
    #[serde(default)]
    pub packages: HashMap<String, InstalledEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledEntry {
    pub version: String,
    pub title: String,
    /// Origem: nome do registry, "arquivo" ou "url".
    pub source: String,
    /// Caminho do arquivo de entrada (relativo ao diretório do pacote).
    pub entry: String,
    /// Timestamp (epoch secs) da instalação.
    pub installed_at: u64,
}

// ---------------------------------------------------------------------------
// Caminhos XDG
// ---------------------------------------------------------------------------

fn data_home() -> PathBuf {
    std::env::var("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
            PathBuf::from(home).join(".local").join("share")
        })
}

fn config_home() -> PathBuf {
    std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
            PathBuf::from(home).join(".config")
        })
}

/// `~/.local/share/windowloom/pkgs` — raiz dos pacotes instalados.
pub fn pkgs_root() -> PathBuf {
    data_home().join("windowloom").join("pkgs")
}

/// Diretório de um pacote instalado.
pub fn pkg_dir(nome: &str) -> PathBuf {
    pkgs_root().join(nome)
}

fn installed_db_path() -> PathBuf {
    pkgs_root().join("installed.json")
}

fn repos_cfg_path() -> PathBuf {
    config_home().join("windowloom").join("repos.json")
}

// ---------------------------------------------------------------------------
// Persistência
// ---------------------------------------------------------------------------

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn load_installed_db() -> InstalledDb {
    fs::read_to_string(installed_db_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_installed_db(db: &InstalledDb) -> Result<(), String> {
    let path = installed_db_path();
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(db).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| e.to_string())
}

pub fn load_repos_cfg() -> ReposCfg {
    fs::read_to_string(repos_cfg_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_repos_cfg(cfg: &ReposCfg) -> Result<(), String> {
    let path = repos_cfg_path();
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| e.to_string())
}

/// Semeia a configuração de registries na primeira execução: registra o
/// widgets-database do IAS-CANVAS-TOOL (fonte local) se ele existir.
pub fn ensure_default_repos() {
    if !repos_cfg_path().exists() {
        let ias = std::env::var("HOME")
            .map(|h| {
                PathBuf::from(h)
                    .join("git_repos/canvas/IAS-CANVAS-TOOL/widgets-database")
            })
            .unwrap_or_default();
        let mut cfg = ReposCfg::default();
        if ias.join("index.json").is_file() {
            cfg.repos.insert(
                "ias-canvas".into(),
                RepoSource::Local {
                    path: ias.to_string_lossy().into_owned(),
                },
            );
        }
        let _ = save_repos_cfg(&cfg);
    }
}

// ---------------------------------------------------------------------------
// HTTP
// ---------------------------------------------------------------------------

/// GET de um URL (texto). Timeout de 30s.
fn http_get_text(url: &str) -> Result<String, String> {
    let resp = ureq::get(url)
        .timeout(std::time::Duration::from_secs(30))
        .call()
        .map_err(|e| format!("GET {}: {}", url, e))?;
    resp.into_string().map_err(|e| format!("GET {}: {}", url, e))
}

/// GET de um URL (binário — artefatos .wlpkg).
fn http_get_bytes(url: &str) -> Result<Vec<u8>, String> {
    let resp = ureq::get(url)
        .timeout(std::time::Duration::from_secs(120))
        .call()
        .map_err(|e| format!("GET {}: {}", url, e))?;
    let mut buf = Vec::new();
    resp.into_reader()
        .read_to_end(&mut buf)
        .map_err(|e| format!("GET {}: {}", url, e))?;
    Ok(buf)
}

// ---------------------------------------------------------------------------
// Registries
// ---------------------------------------------------------------------------

fn ler_index(fonte: &RepoSource) -> Result<RegistryIndex, String> {
    let texto = match fonte {
        RepoSource::Local { path } => {
            fs::read_to_string(Path::new(path).join("index.json"))
                .map_err(|e| format!("registry local {}: {}", path, e))?
        }
        RepoSource::Http { url } => http_get_text(&fonte.index_url())
            .map_err(|e| format!("registry {}: {}", url, e))?,
    };
    serde_json::from_str(&texto).map_err(|e| format!("index.json inválido: {}", e))
}

/// Carrega um registry completo (nome + fonte + pacotes).
pub fn load_registry(nome: &str, fonte: &RepoSource) -> Result<Registry, String> {
    let index = ler_index(fonte)?;
    let mut pacotes = index.packages;
    pacotes.extend(index.widgets.into_iter().map(RegistryPackage::from));
    Ok(Registry {
        nome: index.name.unwrap_or_else(|| nome.into()),
        fonte: fonte.clone(),
        pacotes,
    })
}

/// Todos os registries configurados, carregados.
pub fn load_all_registries() -> Vec<Registry> {
    ensure_default_repos();
    load_repos_cfg()
        .repos
        .into_iter()
        .filter_map(|(nome, fonte)| match load_registry(&nome, &fonte) {
            Ok(r) => Some(r),
            Err(e) => {
                eprintln!("⚠ registry {}: {}", nome, e);
                None
            }
        })
        .collect()
}

/// Procura um pacote por nome em todos os registries (primeiro vence).
pub fn find_package(nome: &str) -> Option<(Registry, RegistryPackage)> {
    for r in load_all_registries() {
        if let Some(p) = r.pacotes.iter().find(|p| p.name == nome).cloned() {
            return Some((r, p));
        }
    }
    None
}

/// Busca por termo em nome/título/descrição/tags/categoria.
pub fn search(query: &str) -> Vec<(Registry, RegistryPackage)> {
    let q = query.to_lowercase();
    let mut hits = Vec::new();
    for r in load_all_registries() {
        for p in &r.pacotes {
            let hay = format!(
                "{} {} {} {} {}",
                p.name,
                p.title.clone().unwrap_or_default(),
                p.description.clone().unwrap_or_default(),
                p.category.clone().unwrap_or_default(),
                p.tags.join(" ")
            )
            .to_lowercase();
            if hay.contains(&q) {
                hits.push((r.clone(), p.clone()));
            }
        }
    }
    hits
}

// ---------------------------------------------------------------------------
// Extração e empacotamento
// ---------------------------------------------------------------------------

/// Extrai um zip para `dest`, ignorando diretórios e caminhos maliciosos.
fn extrair_zip_para(zip_path: &Path, dest: &Path) -> Result<(), String> {
    let file = File::open(zip_path).map_err(|e| format!("zip: {}", e))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("zip inválido: {}", e))?;
    fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("zip entry {}: {}", i, e))?;
        let name = entry.name().to_string();
        if entry.is_dir() || name.contains("..") || name.starts_with('/') {
            continue;
        }
        let out = dest.join(&name);
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut buf = Vec::new();
        entry.read_to_end(&mut buf).map_err(|e| e.to_string())?;
        fs::write(&out, buf).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn temp_file(sufixo: &str) -> PathBuf {
    let id = uuid::Uuid::new_v4();
    std::env::temp_dir().join(format!("wlpkg-{}-{}", id, sufixo))
}

/// Empacota um diretório (com `manifest.json`) num `.wlpkg` (zip).
pub fn create_pkg(dir: &Path, out: &Path) -> Result<(), String> {
    let manifest_path = dir.join("manifest.json");
    let manifest: Manifest = serde_json::from_str(
        &fs::read_to_string(&manifest_path)
            .map_err(|e| format!("manifest.json: {}", e))?,
    )
    .map_err(|e| format!("manifest.json inválido: {}", e))?;
    if manifest.name.is_empty() || manifest.entry.is_empty() {
        return Err("manifest.json precisa de 'name' e 'entry'".into());
    }
    if !dir.join(&manifest.entry).is_file() {
        return Err(format!("entry '{}' não existe em {}", manifest.entry, dir.display()));
    }

    let file = File::create(out).map_err(|e| format!("{}: {}", out.display(), e))?;
    let mut w = zip::ZipWriter::new(file);
    let opts: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();

    let mut arquivos = Vec::new();
    walk_dir(dir, dir, &mut arquivos);
    arquivos.sort();
    for (rel, abs) in arquivos {
        // Não empacota o próprio .wlpkg de saída (caso esteja dentro do dir)
        if abs.canonicalize().ok() == out.canonicalize().ok() {
            continue;
        }
        let conteudo = fs::read(&abs).map_err(|e| format!("{}: {}", abs.display(), e))?;
        w.start_file(rel, opts).map_err(|e| e.to_string())?;
        w.write_all(&conteudo).map_err(|e| e.to_string())?;
    }
    w.finish().map_err(|e| e.to_string())?;
    Ok(())
}

fn walk_dir(dir: &Path, base: &Path, out: &mut Vec<(String, PathBuf)>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk_dir(&p, base, out);
            } else if let Ok(rel) = p.strip_prefix(base) {
                out.push((rel.to_string_lossy().into_owned(), p));
            }
        }
    }
}

/// Valida o entry de um manifest: não pode escapar do diretório do pacote.
fn entry_valido(manifest: &Manifest) -> bool {
    !manifest.entry.is_empty()
        && !manifest.entry.contains("..")
        && !manifest.entry.starts_with('/')
}

// ---------------------------------------------------------------------------
// Instalação
// ---------------------------------------------------------------------------

/// Instala de um registry pelo nome.
pub fn install_from_registry(nome: &str) -> Result<(), String> {
    let (reg, pkg) = find_package(nome)
        .ok_or_else(|| format!("pacote '{}' não encontrado nos registries", nome))?;
    let fonte = reg.fonte.clone();
    let origem = reg.nome.clone();

    let dest = pkg_dir(&pkg.name);
    // Reinstalação: limpa o diretório antes (como na re-extração de zips)
    let _ = fs::remove_dir_all(&dest);

    // 1) Artefato .wlpkg (zip com manifest)
    if let Some(arquivo) = &pkg.file {
        let resolved = fonte.resolve(arquivo);
        let tmp = temp_file("artefato.wlpkg");
        match &fonte {
            RepoSource::Local { .. } => {
                fs::copy(Path::new(&resolved), &tmp)
                    .map_err(|e| format!("{}: {}", resolved, e))?;
            }
            RepoSource::Http { .. } => {
                let bytes = http_get_bytes(&resolved)?;
                fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
            }
        }
        extrair_zip_para(&tmp, &dest)?;
        let _ = fs::remove_file(&tmp);

        let mut manifest = ler_manifest_do_dir(&dest)?;
        if manifest.name.is_empty() {
            manifest.name = pkg.name.clone();
        }
        if manifest.name != pkg.name {
            return Err(format!(
                "manifest do artefato diz '{}', registry diz '{}'",
                manifest.name, pkg.name
            ));
        }
        if !entry_valido(&manifest) {
            // Fallback: entry apontado no index do registry
            if let Some(e) = &pkg.entry {
                if !e.contains("..") {
                    manifest.entry = e.clone();
                }
            }
        }
        preencher_faltantes(&mut manifest, &pkg);
        if !dir_has_entry(&dest, &manifest.entry) {
            return Err(format!("entry '{}' não existe no pacote", manifest.entry));
        }
        gravar_manifesto(&dest, &manifest)?;
        registrar_instalado(&manifest, &origem)?;
        return Ok(());
    }

    // 2) Arquivo avulso (.jsx/.html) — preserva o caminho relativo do registry
    let entry_arquivo = pkg
        .entry
        .clone()
        .ok_or_else(|| format!("pacote '{}' não tem 'file' nem 'entry'", pkg.name))?;
    if entry_arquivo.contains("..") || entry_arquivo.starts_with('/') {
        return Err(format!("entry inválido: '{}'", entry_arquivo));
    }
    let resolved = fonte.resolve(&entry_arquivo);
    fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
    let destino = dest.join(&entry_arquivo);
    if let Some(pai) = destino.parent() {
        fs::create_dir_all(pai).map_err(|e| e.to_string())?;
    }
    match &fonte {
        RepoSource::Local { .. } => {
            fs::copy(Path::new(&resolved), &destino)
                .map_err(|e| format!("{}: {}", resolved, e))?;
        }
        RepoSource::Http { .. } => {
            let bytes = http_get_bytes(&resolved)?;
            fs::write(&destino, bytes).map_err(|e| e.to_string())?;
        }
    }
    let manifest = Manifest {
        name: pkg.name.clone(),
        version: pkg.version.clone(),
        title: pkg.title.clone(),
        description: pkg.description.clone(),
        author: pkg.author.clone(),
        entry: entry_arquivo,
        width: None,
        height: None,
        tags: pkg.tags.clone(),
        license: None,
    };
    gravar_manifesto(&dest, &manifest)?;
    registrar_instalado(&manifest, &origem)?;
    Ok(())
}

fn ler_manifest_do_dir(dir: &Path) -> Result<Manifest, String> {
    let path = dir.join("manifest.json");
    if path.is_file() {
        serde_json::from_str(&fs::read_to_string(&path).map_err(|e| e.to_string())?)
            .map_err(|e| format!("manifest.json inválido: {}", e))
    } else {
        Ok(Manifest {
            name: String::new(),
            version: default_version(),
            title: None,
            description: None,
            author: None,
            entry: String::new(),
            width: None,
            height: None,
            tags: vec![],
            license: None,
        })
    }
}

fn gravar_manifesto(dir: &Path, manifest: &Manifest) -> Result<(), String> {
    let json = serde_json::to_string_pretty(manifest).map_err(|e| e.to_string())?;
    fs::write(dir.join("manifest.json"), json).map_err(|e| e.to_string())
}

/// Completa campos vazios do manifest com os metadados do registry.
fn preencher_faltantes(manifest: &mut Manifest, pkg: &RegistryPackage) {
    if manifest.title.is_none() {
        manifest.title = pkg.title.clone();
    }
    if manifest.description.is_none() {
        manifest.description = pkg.description.clone();
    }
    if manifest.author.is_none() {
        manifest.author = pkg.author.clone();
    }
    if manifest.tags.is_empty() {
        manifest.tags = pkg.tags.clone();
    }
}

fn dir_has_entry(dir: &Path, entry: &str) -> bool {
    !entry.is_empty() && dir.join(entry).is_file()
}

fn registrar_instalado(manifest: &Manifest, origem: &str) -> Result<(), String> {
    let mut db = load_installed_db();
    db.packages.insert(
        manifest.name.clone(),
        InstalledEntry {
            version: manifest.version.clone(),
            title: manifest.titulo(),
            source: origem.into(),
            entry: manifest.entry.clone(),
            installed_at: now_secs(),
        },
    );
    save_installed_db(&db)
}

/// Instala de um caminho local: `.wlpkg`, diretório com `manifest.json`,
/// ou `.jsx`/`.html` avulso.
pub fn install_path(path: &Path) -> Result<(), String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_lowercase();

    if path.is_dir() {
        // Diretório com manifest.json → cópia direta
        let manifest = ler_manifest_do_dir(path)?;
        if manifest.name.is_empty() {
            return Err("diretório sem manifest.json (use `pkg create`) ".into());
        }
        let dest = pkg_dir(&manifest.name);
        let _ = fs::remove_dir_all(&dest);
        copiar_dir(path, &dest)?;
        if !dir_has_entry(&dest, &manifest.entry) {
            return Err(format!("entry '{}' não existe em {}", manifest.entry, path.display()));
        }
        registrar_instalado(&manifest, "arquivo")?;
        return Ok(());
    }

    if ext == PKG_EXT {
        let dest_tmp = temp_file("extraido");
        let _ = fs::remove_dir_all(&dest_tmp);
        extrair_zip_para(path, &dest_tmp)?;
        let mut manifest = ler_manifest_do_dir(&dest_tmp)?;
        if manifest.name.is_empty() {
            return Err(format!("{} não tem manifest.json", path.display()));
        }
        if !entry_valido(&manifest) {
            return Err("manifest.json precisa de um 'entry' válido".into());
        }
        if !dir_has_entry(&dest_tmp, &manifest.entry) {
            return Err(format!("entry '{}' não existe no pacote", manifest.entry));
        }
        let dest = pkg_dir(&manifest.name);
        let _ = fs::remove_dir_all(&dest);
        // rename falha entre filesystems (ex: /tmp → ~/.local); copia+remove
        if fs::rename(&dest_tmp, &dest).is_err() {
            copiar_dir(&dest_tmp, &dest)?;
            let _ = fs::remove_dir_all(&dest_tmp);
        }
        registrar_instalado(&manifest, "arquivo")?;
        return Ok(());
    }

    if ext == "jsx" || ext == "html" {
        let nome_arquivo = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "widget.jsx".into());
        let nome_pkg = Path::new(&nome_arquivo)
            .file_stem()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "widget".into());
        let dest = pkg_dir(&nome_pkg);
        let _ = fs::remove_dir_all(&dest);
        fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
        fs::copy(path, dest.join(&nome_arquivo))
            .map_err(|e| format!("{}: {}", path.display(), e))?;
        let manifest = Manifest {
            name: nome_pkg,
            version: default_version(),
            title: Some(
                Path::new(&nome_arquivo)
                    .file_stem()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "Widget".into()),
            ),
            description: None,
            author: None,
            entry: nome_arquivo,
            width: None,
            height: None,
            tags: vec![],
            license: None,
        };
        gravar_manifesto(&dest, &manifest)?;
        registrar_instalado(&manifest, "arquivo")?;
        return Ok(());
    }

    Err(format!(
        "formato não suportado: .{} (use .{}, .jsx, .html ou diretório)",
        ext, PKG_EXT
    ))
}

/// Instala de uma URL: `.wlpkg`, `.jsx` ou `.html`.
pub fn install_url(url: &str) -> Result<(), String> {
    let lower = url.to_lowercase();
    let bytes = http_get_bytes(url)?;
    let tmp = temp_file("download");
    fs::write(&tmp, &bytes).map_err(|e| e.to_string())?;
    let result = if lower.ends_with(&format!(".{}", PKG_EXT)) {
        install_path(&tmp)
    } else if lower.ends_with(".jsx") || lower.ends_with(".html") {
        // Grava com a extensão certa para o install_path reconhecer
        let ext = if lower.ends_with(".html") { "html" } else { "jsx" };
        let tmp2 = temp_file(&format!("widget.{}", ext));
        fs::write(&tmp2, &bytes).map_err(|e| e.to_string())?;
        install_path(&tmp2)
    } else {
        Err(format!("URL não parece ser .{}/.jsx/.html: {}", PKG_EXT, url))
    };
    let _ = fs::remove_file(&tmp);
    result
}

fn copiar_dir(from: &Path, to: &Path) -> Result<(), String> {
    fs::create_dir_all(to).map_err(|e| e.to_string())?;
    for e in fs::read_dir(from).map_err(|e| e.to_string())?.flatten() {
        let p = e.path();
        let dest = to.join(p.file_name().unwrap_or_default());
        if p.is_dir() {
            copiar_dir(&p, &dest)?;
        } else {
            fs::copy(&p, &dest).map_err(|e| format!("{}: {}", p.display(), e))?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Remoção / upgrade / consulta
// ---------------------------------------------------------------------------

/// Remove um pacote instalado.
pub fn remove(nome: &str) -> Result<(), String> {
    let mut db = load_installed_db();
    if db.packages.remove(nome).is_none() {
        return Err(format!("pacote '{}' não está instalado", nome));
    }
    save_installed_db(&db)?;
    let dir = pkg_dir(nome);
    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Versão instalada de um pacote, se houver.
pub fn installed_version(nome: &str) -> Option<String> {
    load_installed_db()
        .packages
        .get(nome)
        .map(|e| e.version.clone())
}

/// Comparação semver simplificada (numérica por segmento).
pub fn versao_maior(a: &str, b: &str) -> bool {
    let pa: Vec<u64> = a.split('.').filter_map(|s| s.parse().ok()).collect();
    let pb: Vec<u64> = b.split('.').filter_map(|s| s.parse().ok()).collect();
    for i in 0..pa.len().max(pb.len()) {
        let x = pa.get(i).copied().unwrap_or(0);
        let y = pb.get(i).copied().unwrap_or(0);
        if x != y {
            return x > y;
        }
    }
    false
}

/// Atualiza um pacote (ou todos) para a versão mais nova do registry.
/// Retorna os pacotes atualizados (nome + versão nova).
pub fn upgrade(nome: Option<&str>) -> Result<Vec<(String, String)>, String> {
    let db = load_installed_db();
    let alvos: Vec<String> = match nome {
        Some(n) => vec![n.to_string()],
        None => db.packages.keys().cloned().collect(),
    };
    let mut atualizados = Vec::new();
    for n in alvos {
        let Some((reg, pkg)) = find_package(&n) else {
            if nome.is_some() {
                return Err(format!("pacote '{}' não encontrado nos registries", n));
            }
            continue;
        };
        let atual = db.packages.get(&n).map(|e| e.version.clone());
        if let Some(atual) = atual {
            if !versao_maior(&pkg.version, &atual) {
                continue; // já está atualizado
            }
        }
        // Reinstala (limpa o diretório e extrai/copia de novo)
        let _ = fs::remove_dir_all(pkg_dir(&n));
        install_from_registry(&n)?;
        atualizados.push((n, pkg.version));
        let _ = reg;
    }
    Ok(atualizados)
}

/// Manifest de um pacote instalado (para `info` / `open`).
pub fn installed_manifest(nome: &str) -> Option<Manifest> {
    let path = pkg_dir(nome).join("manifest.json");
    if !path.is_file() {
        return None;
    }
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

/// Categoria "Instalados" para a aba Repo do hub — pacotes gerenciados pelo
/// `windowloom pkg` ficam abertos de lá também.
pub fn instalados_categoria() -> Option<crate::repo::RepoCategoria> {
    let db = load_installed_db();
    if db.packages.is_empty() {
        return None;
    }
    let mut widgets = Vec::new();
    let mut nomes: Vec<&String> = db.packages.keys().collect();
    nomes.sort();
    for nome in nomes {
        let entry = &db.packages[nome];
        let p = pkg_dir(nome).join(&entry.entry);
        if p.is_file() {
            widgets.push(crate::repo::RepoWidget::de_arquivo(
                entry.title.clone(),
                p,
            ));
        }
    }
    if widgets.is_empty() {
        return None;
    }
    Some(crate::repo::RepoCategoria {
        nome: "Instalados".into(),
        widgets,
    })
}

// ---------------------------------------------------------------------------
// Testes
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Os testes mexem em env vars globais (XDG); o lock compartilhado
    /// (crate::TEST_ENV_LOCK) serializa com os testes de outros módulos.
    fn ambiente_isolado() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("wlpkg-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(dir.join("data/windowloom/pkgs")).unwrap();
        fs::create_dir_all(dir.join("config/windowloom")).unwrap();
        unsafe {
            std::env::set_var("XDG_DATA_HOME", dir.join("data"));
            std::env::set_var("XDG_CONFIG_HOME", dir.join("config"));
        }
        dir
    }

    fn manifest_exemplo(nome: &str) -> Manifest {
        Manifest {
            name: nome.into(),
            version: "1.0.0".into(),
            title: Some("Widget Teste".into()),
            description: Some("desc".into()),
            author: Some("miguel9w".into()),
            entry: "widget.jsx".into(),
            width: Some(400),
            height: Some(300),
            tags: vec!["teste".into()],
            license: Some("MIT".into()),
        }
    }

    #[test]
    fn manifest_roundtrip() {
        let m = manifest_exemplo("roundtrip");
        let json = serde_json::to_string(&m).unwrap();
        let back: Manifest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "roundtrip");
        assert_eq!(back.entry, "widget.jsx");
        assert_eq!(back.width, Some(400));
    }

    #[test]
    fn create_e_instala_wlpkg() {
        let _env = crate::TEST_ENV_LOCK.lock().unwrap();
        let root = ambiente_isolado();
        let src = root.join("src-pkg");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("widget.jsx"), "function Widget() { return null; }").unwrap();
        fs::write(
            src.join("manifest.json"),
            serde_json::to_string_pretty(&manifest_exemplo("meu-widget")).unwrap(),
        )
        .unwrap();
        let out = root.join("meu-widget.wlpkg");
        create_pkg(&src, &out).unwrap();
        assert!(out.is_file());

        install_path(&out).unwrap();
        let db = load_installed_db();
        let e = db.packages.get("meu-widget").expect("instalado");
        assert_eq!(e.version, "1.0.0");
        assert_eq!(e.source, "arquivo");
        assert!(pkg_dir("meu-widget").join("widget.jsx").is_file());
        // Reinstalação limpa e regrava
        install_path(&out).unwrap();
        assert!(pkg_dir("meu-widget").join("widget.jsx").is_file());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn instala_jsx_avulso_e_remove() {
        let _env = crate::TEST_ENV_LOCK.lock().unwrap();
        let root = ambiente_isolado();
        let jsx = root.join("relogio.jsx");
        fs::write(&jsx, "function Widget() { return null; }").unwrap();
        install_path(&jsx).unwrap();
        let db = load_installed_db();
        assert!(db.packages.contains_key("relogio"));
        remove("relogio").unwrap();
        assert!(!load_installed_db().packages.contains_key("relogio"));
        assert!(!pkg_dir("relogio").exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn registra_registry_local_ias() {
        let _env = crate::TEST_ENV_LOCK.lock().unwrap();
        let root = ambiente_isolado();
        // Cria um widgets-database fake no formato IAS
        let db_dir = root.join("widgets-database");
        fs::create_dir_all(db_dir.join("produtividade")).unwrap();
        fs::write(
            db_dir.join("produtividade/todo.jsx"),
            "function Widget() { return null; }",
        )
        .unwrap();
        fs::write(
            db_dir.join("index.json"),
            r#"{"version":1,"widgets":[
                {"id":"produtividade/todo","title":"Todo List","category":"produtividade","file":"produtividade/todo.jsx"}
            ]}"#,
        )
        .unwrap();
        let mut cfg = ReposCfg::default();
        cfg.repos.insert(
            "ias-fake".into(),
            RepoSource::Local {
                path: db_dir.to_string_lossy().into_owned(),
            },
        );
        save_repos_cfg(&cfg).unwrap();

        let regs = load_all_registries();
        assert_eq!(regs.len(), 1);
        assert_eq!(regs[0].pacotes.len(), 1);
        assert_eq!(regs[0].pacotes[0].name, "todo");

        let (_, pkg) = find_package("todo").expect("encontra no registry");
        assert_eq!(pkg.entry.as_deref(), Some("produtividade/todo.jsx"));
        assert_eq!(pkg.version, "0.0.0");

        install_from_registry("todo").unwrap();
        let binding = load_installed_db();
        let e = binding.packages.get("todo").expect("instalado");
        assert_eq!(e.source, "ias-fake");
        assert!(pkg_dir("todo").join("produtividade/todo.jsx").is_file());
        // Categoria do hub
        let cat = instalados_categoria().expect("categoria instalados");
        assert_eq!(cat.nome, "Instalados");
        assert_eq!(cat.widgets.len(), 1);
        assert_eq!(cat.widgets[0].nome, "Todo List");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn versao_maior_compra() {
        assert!(versao_maior("1.1.0", "1.0.9"));
        assert!(versao_maior("2.0.0", "1.9.9"));
        assert!(!versao_maior("1.0.0", "1.0.0"));
        assert!(!versao_maior("0.9.0", "1.0.0"));
    }

    #[test]
    fn upgrade_reinstala_versao_nova() {
        let _env = crate::TEST_ENV_LOCK.lock().unwrap();
        let root = ambiente_isolado();
        // Registry local com v1.0.0
        let db_dir = root.join("reg");
        fs::create_dir_all(db_dir.join("pkg")).unwrap();
        fs::write(db_dir.join("pkg/x.jsx"), "v1").unwrap();
        fs::write(
            db_dir.join("index.json"),
            r#"{"packages":[{"name":"x","version":"1.0.0","entry":"pkg/x.jsx"}]}"#,
        )
        .unwrap();
        let mut cfg = ReposCfg::default();
        cfg.repos.insert(
            "reg".into(),
            RepoSource::Local {
                path: db_dir.to_string_lossy().into_owned(),
            },
        );
        save_repos_cfg(&cfg).unwrap();

        install_from_registry("x").unwrap();
        assert_eq!(installed_version("x").as_deref(), Some("1.0.0"));

        // Registry sobe para 2.0.0 → upgrade
        fs::write(
            db_dir.join("index.json"),
            r#"{"packages":[{"name":"x","version":"2.0.0","entry":"pkg/x.jsx"}]}"#,
        )
        .unwrap();
        let atualizados = upgrade(Some("x")).unwrap();
        assert_eq!(atualizados.len(), 1);
        assert_eq!(atualizados[0], ("x".to_string(), "2.0.0".to_string()));
        assert_eq!(installed_version("x").as_deref(), Some("2.0.0"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn extrai_ignora_path_traversal() {
        let _env = crate::TEST_ENV_LOCK.lock().unwrap();
        let root = ambiente_isolado();
        let zip_path = root.join("malicioso.wlpkg");
        let file = File::create(&zip_path).unwrap();
        let mut w = zip::ZipWriter::new(file);
        let opts: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
        w.start_file("../../escape.jsx", opts).unwrap();
        w.write_all(b"x").unwrap();
        w.start_file("ok.jsx", opts).unwrap();
        w.write_all(b"y").unwrap();
        w.finish().unwrap();

        let dest = root.join("extraido");
        extrair_zip_para(&zip_path, &dest).unwrap();
        assert!(dest.join("ok.jsx").is_file());
        assert!(!root.join("escape.jsx").exists());
        assert!(!dest.join("..").join("escape.jsx").exists());
        let _ = fs::remove_dir_all(&root);
    }
}
