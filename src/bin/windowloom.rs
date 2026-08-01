// windowloom — CLI para o app WindowLoom.
// Subcomandos: create, update, close, list, events, pkg.
//
//   windowloom create widget.jsx [--title X] [--width 600] [--height 400]
//   windowloom create - <<'EOF'          # lê o JSX do stdin
//   windowloom update <id> widget.jsx
//   windowloom close <id>
//   windowloom list
//   windowloom events [n]                # últimos n eventos (default 10)
//   windowloom pkg ...                   # gerenciador de pacotes
//
// Porta: --port N ou env RUST_CANVAS_PORT (default 8081).

use rust_canvas_windows::pkg;
use std::io::Read;
use std::path::Path;
use std::process::ExitCode;

struct Opts {
    port: String,
    title: Option<String>,
    width: u32,
    height: u32,
    pos: Vec<String>,
}

fn parse(args: &[String]) -> Opts {
    let port = std::env::var("RUST_CANVAS_PORT").unwrap_or_else(|_| "8081".into());
    let mut o = Opts { port, title: None, width: 600, height: 400, pos: vec![] };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--port" => { if let Some(v) = it.next() { o.port = v.clone(); } }
            "--title" => { if let Some(v) = it.next() { o.title = Some(v.clone()); } }
            "--width" => { if let Some(v) = it.next() { o.width = v.parse().unwrap_or(600); } }
            "--height" => { if let Some(v) = it.next() { o.height = v.parse().unwrap_or(400); } }
            _ => o.pos.push(a.clone()),
        }
    }
    o
}

fn read_source(src: &str) -> Result<String, String> {
    if src == "-" {
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf).map_err(|e| e.to_string())?;
        Ok(buf)
    } else {
        std::fs::read_to_string(src).map_err(|e| format!("{}: {}", src, e))
    }
}

/// POST de um comando e retorna o JSON da resposta (ou a string de erro).
fn post(port: &str, action: &str, body: serde_json::Value) -> Result<serde_json::Value, String> {
    let url = format!("http://127.0.0.1:{}/", port);
    let resp = ureq::post(&url)
        .set("Content-Type", "application/json")
        .send_json(body)
        .map_err(|e| e.to_string())?;
    let text = resp.into_string().map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| format!("resposta inválida: {} ({})", e, text))
}

fn print_help() {
    println!("windowloom — CLI do WindowLoom (janelas JSX nativas)");
    println!();
    println!("Uso:");
    println!("  windowloom create <arquivo.jsx|-> [--title T] [--width N] [--height N]");
    println!("  windowloom update <id> <arquivo.jsx|->");
    println!("  windowloom close <id>");
    println!("  windowloom list");
    println!("  windowloom events [n]");
    println!("  windowloom pkg <comando>        gerenciador de pacotes (windowloom pkg --help)");
    println!("  windowloom start                inicia o app (tray)");
    println!("  windowloom main                 abre o hub");
    println!();
    println!("Porta: --port N ou RUST_CANVAS_PORT (default 8081)");
    println!("Exemplo:");
    println!("  windowloom create - --title Relogio <<'EOF'");
    println!("  function Widget() {{ return React.createElement('div', null, 'oi'); }}");
    println!("  EOF");
}

fn cmd_create(o: &Opts) -> ExitCode {
    let Some(src) = o.pos.first() else {
        eprintln!("uso: windowloom create <arquivo.jsx|-> [--title T] [--width N] [--height N]");
        return ExitCode::FAILURE;
    };
    let jsx = match read_source(src) {
        Ok(s) => s,
        Err(e) => { eprintln!("erro: {}", e); return ExitCode::FAILURE; }
    };
    let title = o.title.clone().unwrap_or_else(|| {
        if src == "-" { "Widget".into() } else { Path::new(src).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Widget".into()) }
    });
    let body = serde_json::json!({
        "action": "CREATE_WINDOW",
        "title": title,
        "jsx": jsx,
        "width": o.width,
        "height": o.height,
    });
    match post(&o.port, "CREATE_WINDOW", body) {
        Ok(v) if v["success"] == true => {
            let id = v["id"].as_str().unwrap_or("?");
            println!("✅ janela '{}' criada (id: {})", title, id);
            ExitCode::SUCCESS
        }
        Ok(v) => { eprintln!("❌ {}", v["error"].as_str().unwrap_or("erro desconhecido")); ExitCode::FAILURE }
        Err(e) => { eprintln!("❌ falha de conexão: {}", e); ExitCode::FAILURE }
    }
}

fn cmd_update(o: &Opts) -> ExitCode {
    if o.pos.len() < 2 {
        eprintln!("uso: windowloom update <id> <arquivo.jsx|->");
        return ExitCode::FAILURE;
    }
    let id = &o.pos[0];
    let jsx = match read_source(&o.pos[1]) {
        Ok(s) => s,
        Err(e) => { eprintln!("erro: {}", e); return ExitCode::FAILURE; }
    };
    let body = serde_json::json!({ "action": "UPDATE_WINDOW", "id": id, "jsx": jsx });
    match post(&o.port, "UPDATE_WINDOW", body) {
        Ok(v) if v["success"] == true => { println!("✅ janela {} atualizada", id); ExitCode::SUCCESS }
        Ok(v) => { eprintln!("❌ {}", v["error"].as_str().unwrap_or("erro desconhecido")); ExitCode::FAILURE }
        Err(e) => { eprintln!("❌ falha de conexão: {}", e); ExitCode::FAILURE }
    }
}

fn cmd_close(o: &Opts) -> ExitCode {
    let Some(id) = o.pos.first() else {
        eprintln!("uso: windowloom close <id>");
        return ExitCode::FAILURE;
    };
    let body = serde_json::json!({ "action": "CLOSE_WINDOW", "id": id });
    match post(&o.port, "CLOSE_WINDOW", body) {
        Ok(v) if v["success"] == true => { println!("✅ janela {} fechada", id); ExitCode::SUCCESS }
        Ok(v) => { eprintln!("❌ {}", v["error"].as_str().unwrap_or("erro desconhecido")); ExitCode::FAILURE }
        Err(e) => { eprintln!("❌ falha de conexão: {}", e); ExitCode::FAILURE }
    }
}

fn cmd_list(o: &Opts) -> ExitCode {
    let body = serde_json::json!({ "action": "LIST_WINDOWS" });
    match post(&o.port, "LIST_WINDOWS", body) {
        Ok(v) if v["success"] == true => {
            let windows = v["windows"].as_array().cloned().unwrap_or_default();
            if windows.is_empty() {
                println!("(nenhuma janela aberta)");
            } else {
                println!("{:<40} {:>12} {:>10}", "ID", "TÍTULO", "TAMANHO");
                println!("{}", "-".repeat(66));
                for w in &windows {
                    let id = w["id"].as_str().unwrap_or("?");
                    let title = w["title"].as_str().unwrap_or("?");
                    let width = w["width"].as_u64().unwrap_or(0);
                    let height = w["height"].as_u64().unwrap_or(0);
                    println!("{:<40} {:>12} {:>4}x{:<4}", id, title, width, height);
                }
            }
            ExitCode::SUCCESS
        }
        Ok(v) => { eprintln!("❌ {}", v["error"].as_str().unwrap_or("erro desconhecido")); ExitCode::FAILURE }
        Err(e) => { eprintln!("❌ falha de conexão: {}", e); ExitCode::FAILURE }
    }
}

/// `windowloom start` — inicia o app (spawna o binário com as env vars).
fn cmd_start(_o: &Opts) -> ExitCode {
    let exe = match std::env::current_exe() {
        Ok(e) => e,
        Err(e) => {
            eprintln!("❌ não foi possível localizar o binário: {}", e);
            return ExitCode::FAILURE;
        }
    };
    let app_bin = exe
        .parent()
        .map(|d| d.join("rust-canvas-windows"))
        .unwrap_or_default();
    if !app_bin.exists() {
        eprintln!("❌ app não encontrado: {}", app_bin.display());
        return ExitCode::FAILURE;
    }
    match std::process::Command::new(&app_bin)
        // O app detecta o Wayland e se auto-configura no startup (software
        // rendering do WebKit) — nenhuma env necessária.
        // Redireciona o stdout/stderr do filho: sem isso o processo segura o
        // pipe do shell e um `windowloom start | grep ...` nunca termina.
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(_) => {
            println!("✅ WindowLoom iniciado (tray). Abra o hub com: windowloom main");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("❌ falha ao iniciar: {}", e);
            ExitCode::FAILURE
        }
    }
}

fn cmd_main(o: &Opts) -> ExitCode {
    let body = serde_json::json!({ "action": "OPEN_MAIN_WINDOW" });
    match post(&o.port, "OPEN_MAIN_WINDOW", body) {
        Ok(v) if v["success"] == true => {
            println!("✅ janela principal aberta");
            ExitCode::SUCCESS
        }
        Ok(v) => {
            eprintln!("❌ {}", v["error"].as_str().unwrap_or("erro desconhecido"));
            ExitCode::FAILURE
        }
        Err(e) => {
            eprintln!("❌ falha de conexão: {}", e);
            ExitCode::FAILURE
        }
    }
}

fn cmd_events(o: &Opts) -> ExitCode {
    let n: usize = o.pos.first().and_then(|s| s.parse().ok()).unwrap_or(10);
    let url = format!("http://127.0.0.1:{}/events", o.port);
    match ureq::get(&url).call() {
        Ok(resp) => {
            let text = resp.into_string().unwrap_or_default();
            let events: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap_or_default();
            if events.is_empty() {
                println!("(nenhum evento registrado)");
            } else {
                for e in events.iter().take(n) {
                    let ts = e["ts"].as_u64().unwrap_or(0);
                    let window = e["window"].as_str().unwrap_or("?");
                    let evt = e["evt"].as_str().unwrap_or("?");
                    println!("[{}] {} :: {}", ts, window, evt);
                }
            }
            ExitCode::SUCCESS
        }
        Err(e) => { eprintln!("❌ falha de conexão: {}", e); ExitCode::FAILURE }
    }
}

// ---------------------------------------------------------------------------
// windowloom pkg — gerenciador de pacotes
// ---------------------------------------------------------------------------

fn pkg_help() {
    println!("windowloom pkg — gerenciador de pacotes do WindowLoom");
    println!();
    println!("Uso:");
    println!("  windowloom pkg list                     pacotes instalados");
    println!("  windowloom pkg search <termo>           busca nos registries");
    println!("  windowloom pkg info <pacote>            detalhes de um pacote");
    println!("  windowloom pkg install <nome|arquivo|url>  instala (registry, .wlpkg, .jsx/.html ou diretório)");
    println!("  windowloom pkg remove <pacote>          desinstala");
    println!("  windowloom pkg update                   verifica os registries");
    println!("  windowloom pkg upgrade [pacote]         atualiza um (ou todos)");
    println!("  windowloom pkg repo list                fontes configuradas");
    println!("  windowloom pkg repo add <nome> <caminho|url>  adiciona fonte");
    println!("  windowloom pkg repo remove <nome>       remove fonte");
    println!("  windowloom pkg create <dir> [-o saida.wlpkg]  empacota diretório com manifest.json");
    println!("  windowloom pkg open <pacote> [--port N] abre o pacote numa janela (app rodando)");
    println!();
    println!("Formatos: .wlpkg (zip com manifest.json) ou .jsx/.html avulso.");
    println!("Registries: local (diretório com index.json) ou http(s).");
    println!("Na 1ª execução o widgets-database do IAS-CANVAS-TOOL é registrado");
    println!("automaticamente como fonte local 'ias-canvas'.");
}

fn pkg_list() -> ExitCode {
    let db = pkg::load_installed_db();
    if db.packages.is_empty() {
        println!("(nenhum pacote instalado)");
        println!("dica: windowloom pkg search <termo>  →  windowloom pkg install <nome>");
        return ExitCode::SUCCESS;
    }
    let mut nomes: Vec<&String> = db.packages.keys().collect();
    nomes.sort();
    println!("{:<28} {:>10}  {:<24} {}", "PACOTE", "VERSÃO", "TÍTULO", "ORIGEM");
    println!("{}", "-".repeat(78));
    for nome in nomes {
        let e = &db.packages[nome];
        println!(
            "{:<28} {:>10}  {:<24} {}",
            nome,
            e.version,
            truncate(&e.title, 24),
            e.source
        );
    }
    println!("{}", "-".repeat(78));
    println!("{} pacote(s) instalado(s)", db.packages.len());
    ExitCode::SUCCESS
}

fn pkg_search(query: &str) -> ExitCode {
    let hits = pkg::search(query);
    if hits.is_empty() {
        println!("(nenhum resultado para '{}')", query);
        return ExitCode::SUCCESS;
    }
    let instalados = pkg::load_installed_db();
    println!(
        "{:<28} {:>10}  {:<22} {:<14} {}",
        "PACOTE", "VERSÃO", "CATEGORIA", "REGISTRY", "TÍTULO"
    );
    println!("{}", "-".repeat(92));
    for (reg, p) in hits {
        let instalado = instalados.packages.contains_key(&p.name);
        let flag = if instalado { "✓ " } else { "  " };
        println!(
            "{}{:<27} {:>10}  {:<22} {:<14} {}",
            flag,
            p.name,
            p.version,
            truncate(&p.category.clone().unwrap_or_default(), 22),
            truncate(&reg.nome, 14),
            p.title.clone().unwrap_or_default()
        );
    }
    ExitCode::SUCCESS
}

fn pkg_info(nome: &str) -> ExitCode {
    // Instalado: mostra manifest local
    if let Some(m) = pkg::installed_manifest(nome) {
        let dir = pkg::pkg_dir(nome);
        println!("📦 {} v{}", m.name, m.version);
        println!("   título:     {}", m.titulo());
        println!("   descrição:  {}", m.description.clone().unwrap_or_else(|| "—".into()));
        println!("   autor:      {}", m.author.clone().unwrap_or_else(|| "—".into()));
        println!("   licença:    {}", m.license.clone().unwrap_or_else(|| "—".into()));
        println!("   entry:      {}", m.entry);
        println!("   tamanho:    {}x{}", m.width.map(|w| w.to_string()).unwrap_or_else(|| "auto".into()), m.height.map(|h| h.to_string()).unwrap_or_else(|| "auto".into()));
        println!("   tags:       {}", if m.tags.is_empty() { "—".into() } else { m.tags.join(", ") });
        println!("   local:      {}", dir.display());
        return ExitCode::SUCCESS;
    }
    // Registry: mostra o que há disponível
    match pkg::find_package(nome) {
        Some((reg, p)) => {
            println!("📦 {} v{} (registry: {})", p.name, p.version, reg.nome);
            println!("   título:     {}", p.title.clone().unwrap_or_else(|| "—".into()));
            println!("   descrição:  {}", p.description.clone().unwrap_or_else(|| "—".into()));
            println!("   autor:      {}", p.author.clone().unwrap_or_else(|| "—".into()));
            println!("   categoria:  {}", p.category.clone().unwrap_or_else(|| "—".into()));
            println!("   artefato:   {}", p.file.clone().unwrap_or_else(|| "avulso (jsx/html)".into()));
            println!("   tamanho:    {}", p.size_bytes.map(bytes_humano).unwrap_or_else(|| "—".into()));
            println!("   tags:       {}", if p.tags.is_empty() { "—".into() } else { p.tags.join(", ") });
            println!();
            println!("   instale com: windowloom pkg install {}", p.name);
            ExitCode::SUCCESS
        }
        None => {
            eprintln!("❌ pacote '{}' não encontrado (instalado ou nos registries)", nome);
            ExitCode::FAILURE
        }
    }
}

fn pkg_install(alvo: &str) -> ExitCode {
    // Caminho local existente → instala do arquivo/diretório
    if Path::new(alvo).exists() {
        return match pkg::install_path(Path::new(alvo)) {
            Ok(()) => { println!("✅ pacote instalado de '{}'", alvo); ExitCode::SUCCESS }
            Err(e) => { eprintln!("❌ {}", e); ExitCode::FAILURE }
        };
    }
    // URL
    if alvo.starts_with("http://") || alvo.starts_with("https://") {
        return match pkg::install_url(alvo) {
            Ok(()) => { println!("✅ pacote instalado de {}", alvo); ExitCode::SUCCESS }
            Err(e) => { eprintln!("❌ {}", e); ExitCode::FAILURE }
        };
    }
    // Nome de pacote no registry
    match pkg::install_from_registry(alvo) {
        Ok(()) => {
            let v = pkg::installed_version(alvo).unwrap_or_default();
            println!("✅ pacote '{}' v{} instalado", alvo, v);
            println!("   abra com: windowloom pkg open {}", alvo);
            ExitCode::SUCCESS
        }
        Err(e) => { eprintln!("❌ {}", e); ExitCode::FAILURE }
    }
}

fn pkg_remove(nome: &str) -> ExitCode {
    match pkg::remove(nome) {
        Ok(()) => { println!("✅ pacote '{}' removido", nome); ExitCode::SUCCESS }
        Err(e) => { eprintln!("❌ {}", e); ExitCode::FAILURE }
    }
}

fn pkg_update() -> ExitCode {
    pkg::ensure_default_repos();
    let cfg = pkg::load_repos_cfg();
    if cfg.repos.is_empty() {
        println!("(nenhum registry configurado)");
        println!("dica: windowloom pkg repo add <nome> <caminho|url>");
        return ExitCode::SUCCESS;
    }
    let mut nomes: Vec<&String> = cfg.repos.keys().collect();
    nomes.sort();
    println!("{:<20} {:<12} {}", "REGISTRY", "TIPO", "PACOTES");
    println!("{}", "-".repeat(64));
    let mut total = 0usize;
    for nome in nomes {
        let fonte = &cfg.repos[nome];
        match pkg::load_registry(nome, fonte) {
            Ok(r) => {
                total += r.pacotes.len();
                let tipo = match fonte {
                    pkg::RepoSource::Local { .. } => "local",
                    pkg::RepoSource::Http { .. } => "http",
                };
                println!("{:<20} {:<12} {}", truncate(nome, 20), tipo, r.pacotes.len());
            }
            Err(e) => println!("{:<20} {:<12} ❌ {}", truncate(nome, 20), "?", e),
        }
    }
    println!("{}", "-".repeat(64));
    println!("{} pacote(s) no total", total);
    ExitCode::SUCCESS
}

fn pkg_upgrade(nome: Option<&str>) -> ExitCode {
    match pkg::upgrade(nome) {
        Ok(atualizados) => {
            if atualizados.is_empty() {
                match nome {
                    Some(n) => println!("✅ '{}' já está na versão mais nova", n),
                    None => println!("✅ todos os pacotes já estão atualizados"),
                }
            } else {
                for (n, v) in &atualizados {
                    println!("⬆️  {} → v{}", n, v);
                }
            }
            ExitCode::SUCCESS
        }
        Err(e) => { eprintln!("❌ {}", e); ExitCode::FAILURE }
    }
}

fn cmd_pkg_repo(args: &[String]) -> ExitCode {
    let Some(sub) = args.first() else {
        eprintln!("uso: windowloom pkg repo list|add <nome> <caminho|url>|remove <nome>");
        return ExitCode::FAILURE;
    };
    pkg::ensure_default_repos();
    let mut cfg = pkg::load_repos_cfg();
    match sub.as_str() {
        "list" => {
            if cfg.repos.is_empty() {
                println!("(nenhum registry configurado)");
                return ExitCode::SUCCESS;
            }
            let mut nomes: Vec<&String> = cfg.repos.keys().collect();
            nomes.sort();
            println!("{:<20} {:<12} {}", "REGISTRY", "TIPO", "ORIGEM");
            println!("{}", "-".repeat(70));
            for nome in nomes {
                let fonte = &cfg.repos[nome];
                let (tipo, origem) = match fonte {
                    pkg::RepoSource::Local { path } => ("local", path.as_str()),
                    pkg::RepoSource::Http { url } => ("http", url.as_str()),
                };
                println!("{:<20} {:<12} {}", truncate(nome, 20), tipo, origem);
            }
            ExitCode::SUCCESS
        }
        "add" => {
            if args.len() < 3 {
                eprintln!("uso: windowloom pkg repo add <nome> <caminho|url>");
                return ExitCode::FAILURE;
            }
            let nome = &args[1];
            let alvo = &args[2];
            let fonte = if alvo.starts_with("http://") || alvo.starts_with("https://") {
                pkg::RepoSource::Http { url: alvo.clone() }
            } else {
                pkg::RepoSource::Local { path: alvo.clone() }
            };
            // Valida antes de gravar
            if let Err(e) = pkg::load_registry(nome, &fonte) {
                eprintln!("❌ registry inválido: {}", e);
                return ExitCode::FAILURE;
            }
            cfg.repos.insert(nome.clone(), fonte);
            match pkg::save_repos_cfg(&cfg) {
                Ok(()) => { println!("✅ registry '{}' adicionado ({})", nome, alvo); ExitCode::SUCCESS }
                Err(e) => { eprintln!("❌ {}", e); ExitCode::FAILURE }
            }
        }
        "remove" => {
            let Some(nome) = args.get(1) else {
                eprintln!("uso: windowloom pkg repo remove <nome>");
                return ExitCode::FAILURE;
            };
            if cfg.repos.remove(nome).is_none() {
                eprintln!("❌ registry '{}' não existe", nome);
                return ExitCode::FAILURE;
            }
            match pkg::save_repos_cfg(&cfg) {
                Ok(()) => { println!("✅ registry '{}' removido", nome); ExitCode::SUCCESS }
                Err(e) => { eprintln!("❌ {}", e); ExitCode::FAILURE }
            }
        }
        _ => {
            eprintln!("subcomando repo desconhecido: {}", sub);
            ExitCode::FAILURE
        }
    }
}

fn cmd_pkg_create(args: &[String]) -> ExitCode {
    let Some(dir) = args.iter().find(|a| !a.starts_with('-')) else {
        eprintln!("uso: windowloom pkg create <dir> [-o saida.wlpkg]");
        return ExitCode::FAILURE;
    };
    let out = match args.iter().position(|a| a == "-o") {
        Some(i) => args
            .get(i + 1)
            .map(|s| std::path::PathBuf::from(s))
            .unwrap_or_else(|| PathBuf_default(dir)),
        None => PathBuf_default(dir),
    };
    match pkg::create_pkg(Path::new(dir), &out) {
        Ok(()) => {
            println!("✅ pacote criado: {}", out.display());
            println!("   instale com: windowloom pkg install {}", out.display());
            ExitCode::SUCCESS
        }
        Err(e) => { eprintln!("❌ {}", e); ExitCode::FAILURE }
    }
}

fn cmd_pkg_open(args: &[String]) -> ExitCode {
    let Some(nome) = args.iter().find(|a| !a.starts_with('-')) else {
        eprintln!("uso: windowloom pkg open <pacote> [--port N]");
        return ExitCode::FAILURE;
    };
    let port = match args.iter().position(|a| a == "--port") {
        Some(i) => args.get(i + 1).cloned().unwrap_or_else(|| "8081".into()),
        None => std::env::var("RUST_CANVAS_PORT").unwrap_or_else(|_| "8081".into()),
    };
    let Some(m) = pkg::installed_manifest(nome) else {
        eprintln!("❌ pacote '{}' não está instalado", nome);
        return ExitCode::FAILURE;
    };
    let entry_path = pkg::pkg_dir(nome).join(&m.entry);
    let jsx = match std::fs::read_to_string(&entry_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("❌ entry {}: {}", entry_path.display(), e);
            return ExitCode::FAILURE;
        }
    };
    let title = m.titulo();
    let width = m.width.unwrap_or(600);
    let height = m.height.unwrap_or(400);
    let body = serde_json::json!({
        "action": "CREATE_WINDOW",
        "title": title,
        "jsx": jsx,
        "width": width,
        "height": height,
    });
    match post(&port, "CREATE_WINDOW", body) {
        Ok(v) if v["success"] == true => {
            let id = v["id"].as_str().unwrap_or("?");
            println!("✅ '{}' aberto (id: {})", title, id);
            ExitCode::SUCCESS
        }
        Ok(v) => { eprintln!("❌ {}", v["error"].as_str().unwrap_or("erro desconhecido")); ExitCode::FAILURE }
        Err(e) => { eprintln!("❌ falha de conexão (app rodando? use: windowloom start): {}", e); ExitCode::FAILURE }
    }
}

fn cmd_pkg(args: &[String]) -> ExitCode {
    if args.is_empty() || args[0] == "-h" || args[0] == "--help" {
        pkg_help();
        return ExitCode::SUCCESS;
    }
    let cmd = args[0].as_str();
    let rest = &args[1..];
    match cmd {
        "list" => pkg_list(),
        "search" => {
            let Some(q) = rest.first() else {
                eprintln!("uso: windowloom pkg search <termo>");
                return ExitCode::FAILURE;
            };
            pkg_search(q)
        }
        "info" => {
            let Some(n) = rest.first() else {
                eprintln!("uso: windowloom pkg info <pacote>");
                return ExitCode::FAILURE;
            };
            pkg_info(n)
        }
        "install" => {
            let Some(alvo) = rest.first() else {
                eprintln!("uso: windowloom pkg install <nome|arquivo|url>");
                return ExitCode::FAILURE;
            };
            pkg_install(alvo)
        }
        "remove" => {
            let Some(n) = rest.first() else {
                eprintln!("uso: windowloom pkg remove <pacote>");
                return ExitCode::FAILURE;
            };
            pkg_remove(n)
        }
        "update" => pkg_update(),
        "upgrade" => pkg_upgrade(rest.first().map(|s| s.as_str())),
        "repo" => cmd_pkg_repo(rest),
        "create" => cmd_pkg_create(rest),
        "open" => cmd_pkg_open(rest),
        _ => {
            eprintln!("comando pkg desconhecido: {}", cmd);
            pkg_help();
            ExitCode::FAILURE
        }
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

fn bytes_humano(b: u64) -> String {
    if b >= 1_048_576 {
        format!("{:.1} MB", b as f64 / 1_048_576.0)
    } else if b >= 1024 {
        format!("{:.1} KB", b as f64 / 1024.0)
    } else {
        format!("{} B", b)
    }
}

fn PathBuf_default(dir: &str) -> std::path::PathBuf {
    let name = Path::new(dir)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "pacote".into());
    std::path::PathBuf::from(format!("{}.{}", name, pkg::PKG_EXT))
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args[0] == "-h" || args[0] == "--help" {
        print_help();
        return ExitCode::SUCCESS;
    }
    if args[0] == "pkg" {
        return cmd_pkg(&args[1..]);
    }
    let cmd = args[0].as_str();
    let opts = parse(&args[1..]);
    match cmd {
        "create" => cmd_create(&opts),
        "update" => cmd_update(&opts),
        "close" => cmd_close(&opts),
        "list" => cmd_list(&opts),
        "start" => cmd_start(&opts),
        "main" => cmd_main(&opts),
        "events" => cmd_events(&opts),
        _ => {
            eprintln!("comando desconhecido: {}", cmd);
            print_help();
            ExitCode::FAILURE
        }
    }
}
