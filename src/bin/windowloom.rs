// windowloom — CLI para o app WindowLoom.
// Subcomandos: create, update, close, list, events.
//
//   windowloom create widget.jsx [--title X] [--width 600] [--height 400]
//   windowloom create - <<'EOF'          # lê o JSX do stdin
//   windowloom update <id> widget.jsx
//   windowloom close <id>
//   windowloom list
//   windowloom events [n]                # últimos n eventos (default 10)
//
// Porta: --port N ou env RUST_CANVAS_PORT (default 8081).

use std::io::Read;
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
        if src == "-" { "Widget".into() } else { std::path::Path::new(src).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Widget".into()) }
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

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args[0] == "-h" || args[0] == "--help" {
        print_help();
        return ExitCode::SUCCESS;
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
