use serde::Deserialize;
use std::collections::hash_map::DefaultHasher;
use std::fs::File;
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::path::{Path, PathBuf};

/// Um widget de um repositório (zip de widgets).
#[derive(Debug, Clone)]
pub struct RepoWidget {
    /// Nome para exibição (nome do arquivo sem extensão)
    pub nome: String,
    /// Caminho do arquivo extraído (.jsx ou .html)
    pub path: PathBuf,
    /// JSX embutido (para widgets sem arquivo, ex: modelos embutidos)
    pub jsx_inline: Option<String>,
}

impl RepoWidget {
    pub fn de_arquivo(nome: String, path: PathBuf) -> Self {
        Self {
            nome,
            path,
            jsx_inline: None,
        }
    }
    pub fn embutido(nome: String, jsx: String) -> Self {
        Self {
            nome,
            path: PathBuf::new(),
            jsx_inline: Some(jsx),
        }
    }
}

/// Categoria do repositório: nome + widgets.
#[derive(Debug, Clone)]
pub struct RepoCategoria {
    pub nome: String,
    pub widgets: Vec<RepoWidget>,
}

/// Formato do `index.json` opcional dentro do zip. Suporta dois esquemas:
///
/// 1. Genérico: `{ "name": "Meu repo", "categories": { "business": ["corp-dados.jsx"] } }`
/// 2. IAS-CANVAS-TOOL (widgets-database):
///    `{ "version": 1, "widgets": [ { "id": "ai-ml/x", "title": "X",
///    "category": "ai-ml", "file": "ai-ml/x.jsx" } ], "total": N }`
#[derive(Debug, Deserialize)]
struct RepoIndex {
    #[serde(default)]
    #[allow(dead_code)] // metadado informativo, não usado na navegação
    name: Option<String>,
    #[serde(default)]
    categories: std::collections::HashMap<String, Vec<String>>,
    /// Formato IAS-CANVAS-TOOL: entrada por widget.
    #[serde(default)]
    widgets: Vec<RepoIndexWidget>,
}

/// Uma entrada do formato IAS-CANVAS-TOOL.
#[derive(Debug, Deserialize)]
struct RepoIndexWidget {
    #[serde(default)]
    id: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    file: Option<String>,
}

/// Diretório onde os zips são extraídos (cache).
pub fn repos_dir() -> PathBuf {
    let base = std::env::var("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
            PathBuf::from(home).join(".local").join("share")
        });
    base.join("windowloom").join("repos")
}

/// Extrai um zip para `repos_dir()/<nome-do-zip>/` e devolve o diretório.
pub fn extrair_zip(zip_path: &Path) -> Result<PathBuf, String> {
    let file = File::open(zip_path).map_err(|e| format!("zip: {}", e))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("zip inválido: {}", e))?;

    let nome = zip_path
        .file_stem()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "repo".into());
    // Destino único por caminho do zip: dois zips com o mesmo nome
    // (ex: widgets.zip em pastas diferentes) não colidem no cache.
    let dest = repos_dir().join(format!("{}-{}", hash_do_caminho(zip_path), nome));
    std::fs::create_dir_all(&dest).map_err(|e| e.to_string())?;

    // Limpa o diretório antes de extrair de novo (re-seleção do mesmo zip)
    if let Ok(entries) = std::fs::read_dir(&dest) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                let _ = std::fs::remove_dir_all(&p);
            } else {
                let _ = std::fs::remove_file(&p);
            }
        }
    }

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("zip entry {}: {}", i, e))?;
        let entry_name = entry.name().to_string();
        // Ignora diretórios e caminhos maliciosos (traversal)
        if entry.is_dir() || entry_name.contains("..") {
            continue;
        }
        let out_path = dest.join(&entry_name);
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut buf = Vec::new();
        entry.read_to_end(&mut buf).map_err(|e| e.to_string())?;
        std::fs::write(&out_path, buf).map_err(|e| e.to_string())?;
    }

    Ok(dest)
}

/// Escaneia um repositório extraído: usa o `index.json` se existir; senão,
/// usa as subpastas como categorias e os `.jsx`/`.html` como widgets.
pub fn scan_repo(dir: &Path) -> Vec<RepoCategoria> {
    // index.json tem prioridade
    let index_path = dir.join("index.json");
    if let Ok(content) = std::fs::read_to_string(&index_path) {
        if let Ok(index) = serde_json::from_str::<RepoIndex>(&content) {
            // IAS-CANVAS-TOOL (widgets[]) ou genérico (categories), ambos aqui
            let cats = categorias_do_index(dir, &index);
            if !cats.is_empty() {
                return cats;
            }
        }
    }

    // Fallback: subpastas = categorias; raiz = categoria "Geral"
    let mut cats = Vec::new();
    let mut geral = Vec::new();
    let mut subpastas = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                subpastas.push(p);
            } else if is_widget_file(&p) {
                geral.push(RepoWidget::de_arquivo(
                    nome_do_arquivo(&p.to_string_lossy()),
                    p,
                ));
            }
        }
    }
    if !geral.is_empty() {
        cats.push(RepoCategoria {
            nome: "Geral".into(),
            widgets: geral,
        });
    }
    for sub in subpastas {
        let mut widgets = Vec::new();
        let cat = sub
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Categoria".into());
        if let Ok(entries) = std::fs::read_dir(&sub) {
            for e in entries.flatten() {
                let p = e.path();
                if is_widget_file(&p) {
                    widgets.push(RepoWidget::de_arquivo(
                        nome_do_arquivo(&p.to_string_lossy()),
                        p,
                    ));
                }
            }
        }
        if !widgets.is_empty() {
            cats.push(RepoCategoria { nome: cat, widgets });
        }
    }
    cats
}

/// Constrói as categorias a partir do `index.json` (IAS ou genérico).
fn categorias_do_index(dir: &Path, index: &RepoIndex) -> Vec<RepoCategoria> {
    // Formato IAS-CANVAS-TOOL: um widget por entrada, com category/file/title
    if !index.widgets.is_empty() {
        let mut cats: Vec<RepoCategoria> = Vec::new();
        for w in &index.widgets {
            let Some(file) = &w.file else { continue };
            let p = dir.join(file);
            if !p.is_file() {
                continue;
            }
            let nome = w.title.clone().unwrap_or_else(|| nome_do_arquivo(file));
            let cat_nome = w.category.clone().unwrap_or_else(|| "Geral".into());
            match cats.iter_mut().find(|c| c.nome == cat_nome) {
                Some(c) => c.widgets.push(RepoWidget::de_arquivo(nome, p)),
                None => cats.push(RepoCategoria {
                    nome: cat_nome,
                    widgets: vec![RepoWidget::de_arquivo(nome, p)],
                }),
            }
        }
        return cats;
    }

    // Formato genérico: categories { nome: [arquivos] }
    let mut cats = Vec::new();
    for (cat, files) in &index.categories {
        let widgets = files
            .iter()
            .filter_map(|f| {
                let p = dir.join(f);
                if p.is_file() {
                    Some(RepoWidget::de_arquivo(nome_do_arquivo(f), p))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        if !widgets.is_empty() {
            cats.push(RepoCategoria {
                nome: cat.clone(),
                widgets,
            });
        }
    }
    cats
}

fn is_widget_file(p: &Path) -> bool {
    matches!(
        p.extension().and_then(|e| e.to_str()),
        Some("jsx") | Some("html")
    )
}

/// Hash curto e estável do caminho canônico do zip — usado no nome do
/// diretório de extração (evita colisão entre zips homônimos).
fn hash_do_caminho(zip_path: &Path) -> String {
    let canon = zip_path
        .canonicalize()
        .unwrap_or_else(|_| zip_path.to_path_buf());
    let mut h = DefaultHasher::new();
    canon.hash(&mut h);
    format!("{:x}", h.finish() & 0xffff_ffff)
}

fn nome_do_arquivo(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn cria_zip(caminho: &Path, arquivos: &[(&str, &str)]) {
        let file = File::create(caminho).unwrap();
        let mut w = zip::ZipWriter::new(file);
        let opts: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
        for (nome, conteudo) in arquivos {
            w.start_file(*nome, opts).unwrap();
            w.write_all(conteudo.as_bytes()).unwrap();
        }
        w.finish().unwrap();
    }

    #[test]
    fn extrai_e_escaneia_por_subpastas() {
        let _env = crate::TEST_ENV_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join("wl-repo-test-sub");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let zip_path = dir.join("widgets.zip");
        cria_zip(
            &zip_path,
            &[
                (
                    "business/corp-dados.jsx",
                    "function Widget() { return null; }",
                ),
                (
                    "business/corp-vendas.jsx",
                    "function Widget() { return null; }",
                ),
                ("games/meu-jogo.jsx", "function Widget() { return null; }"),
                ("pagina.html", "<html><body>oi</body></html>"),
                ("index.json", r#"{"name":"x"}"#),
            ],
        );

        let dest = extrair_zip(&zip_path).unwrap();
        let cats = scan_repo(&dest);
        assert_eq!(cats.len(), 3, "business, games e Geral");
        let business = cats.iter().find(|c| c.nome == "business").unwrap();
        assert_eq!(business.widgets.len(), 2);
        assert!(business.widgets.iter().any(|w| w.nome == "corp-dados"));
        assert!(cats.iter().any(|c| c.nome == "Geral"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn usa_index_json_com_prioridade() {
        let _env = crate::TEST_ENV_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join("wl-repo-test-idx");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let zip_path = dir.join("widgets.zip");
        cria_zip(
            &zip_path,
            &[
                ("corp-dados.jsx", "function Widget() { return null; }"),
                ("corp-vendas.jsx", "function Widget() { return null; }"),
                (
                    "index.json",
                    r#"{"name":"Corp","categories":{"financeiro":["corp-dados.jsx"]}}"#,
                ),
            ],
        );

        let dest = extrair_zip(&zip_path).unwrap();
        let cats = scan_repo(&dest);
        assert_eq!(cats.len(), 1);
        assert_eq!(cats[0].nome, "financeiro");
        assert_eq!(cats[0].widgets.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn usa_index_json_formato_ias() {
        let _env = crate::TEST_ENV_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join("wl-repo-test-ias");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let zip_path = dir.join("widgets-ias.zip");
        cria_zip(
            &zip_path,
            &[
                (
                    "ai-ml/decision-tree.jsx",
                    "function Widget() { return null; }",
                ),
                (
                    "ai-ml/gradient-descent.jsx",
                    "function Widget() { return null; }",
                ),
                ("games/snake.jsx", "function Widget() { return null; }"),
                (
                    "index.json",
                    r#"{"version":1,"total":3,"widgets":[
                        {"id":"ai-ml/decision-tree","title":"Decision Tree","category":"ai-ml","file":"ai-ml/decision-tree.jsx"},
                        {"id":"ai-ml/gradient-descent","title":"Gradient Descent","category":"ai-ml","file":"ai-ml/gradient-descent.jsx"},
                        {"id":"games/snake","title":"Snake","category":"games","file":"games/snake.jsx"}
                    ]}"#,
                ),
            ],
        );

        let dest = extrair_zip(&zip_path).unwrap();
        let cats = scan_repo(&dest);
        assert_eq!(cats.len(), 2, "ai-ml e games");
        let ai = cats.iter().find(|c| c.nome == "ai-ml").unwrap();
        assert_eq!(ai.widgets.len(), 2);
        // Usa o `title` do index (não o nome do arquivo)
        assert!(ai.widgets.iter().any(|w| w.nome == "Decision Tree"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn zips_homonimos_nao_colidem_no_cache() {
        let _env = crate::TEST_ENV_LOCK.lock().unwrap();
        // Dois zips com o mesmo nome em pastas diferentes → destinos
        // distintos (o hash do caminho entra no nome do diretório).
        let dir = std::env::temp_dir().join("wl-repo-test-hash");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir.join("a")).unwrap();
        std::fs::create_dir_all(&dir.join("b")).unwrap();
        let z1 = dir.join("a/widgets.zip");
        let z2 = dir.join("b/widgets.zip");
        cria_zip(&z1, &[("um.jsx", "function Widget() { return null; }")]);
        cria_zip(&z2, &[("dois.jsx", "function Widget() { return null; }")]);

        let d1 = extrair_zip(&z1).unwrap();
        let d2 = extrair_zip(&z2).unwrap();
        assert_ne!(
            d1, d2,
            "mesmo nome, caminhos diferentes → diretórios diferentes"
        );
        // Re-extração do MESMO zip → mesmo destino (limpeza/re-seleção ok)
        let d1b = extrair_zip(&z1).unwrap();
        assert_eq!(d1, d1b);

        let c1 = scan_repo(&d1);
        assert_eq!(c1.len(), 1);
        assert_eq!(c1[0].widgets[0].nome, "um");
        let c2 = scan_repo(&d2);
        assert_eq!(c2[0].widgets[0].nome, "dois");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// E2E opcional contra o widgets-database real do IAS-CANVAS-TOOL
    /// (formato IAS no index.json). Roda com:
    /// `cargo test -- --ignored e2e_widgets_database_real`
    #[test]
    #[ignore]
    fn e2e_widgets_database_real() {
        let db = Path::new(env!("HOME")).join("git_repos/canvas/IAS-CANVAS-TOOL/widgets-database");
        if !db.join("index.json").is_file() {
            eprintln!("pulando: {} não existe", db.display());
            return;
        }
        let cats = scan_repo(&db);
        assert!(
            cats.len() >= 5,
            "esperava várias categorias, veio {}",
            cats.len()
        );
        // Formato IAS: title do index, não nome de arquivo
        let total: usize = cats.iter().map(|c| c.widgets.len()).sum();
        assert!(total >= 10, "esperava muitos widgets, veio {}", total);
        let nomes: Vec<&str> = cats
            .iter()
            .flat_map(|c| c.widgets.iter().map(|w| w.nome.as_str()))
            .collect();
        assert!(
            nomes.iter().any(|n| n.contains(' ')),
            "titles do index deveriam aparecer (ex: 'Decision Tree'), veio: {:?}",
            &nomes[..nomes.len().min(5)]
        );
        println!(
            "✅ widgets-database real: {} categorias, {} widgets",
            cats.len(),
            total
        );
        for c in &cats {
            println!("  {} ({} widgets)", c.nome, c.widgets.len());
        }
    }
}
