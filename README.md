# WindowLoom

Native desktop windows renderizando JSX dinamicamente — como o IAS-CANVAS-TOOL, mas em **janelas reais do sistema operacional**.

## Arquitetura

```
OpenCode/Pi Agent ──HTTP POST──▶ Rust App (:8081)
                                      │
                            ┌─────────▼─────────┐
                            │  GTK3 + WebKit    │
                            │  Window Manager   │
                            │                   │
                            │  ┌─────────────┐  │
                            │  │ WebView #1  │  │  ← Native OS Window
                            │  │ (JSX Widget)│  │
                            │  └─────────────┘  │
                            │  ┌─────────────┐  │
                            │  │ WebView #2  │  │  ← Native OS Window
                            │  │ (JSX Widget)│  │
                            │  └─────────────┘  │
                            └───────────────────┘
```

## Como usar

```bash
# Build
cargo build --release

# Rodar (auto-configuração: detecta Wayland e ativa o software rendering
# do WebKit — funciona nativo no Wayland e no X11, sem env vars)
cargo run --release

# Criar uma janela via API
curl -X POST http://localhost:8081 \
  -H 'Content-Type: application/json' \
  -d '{
    "action": "CREATE_WINDOW",
    "title": "Meu Widget",
    "jsx": "function Widget() { const [c, setC] = React.useState(0); return React.createElement(\"button\", { onClick: () => setC(c+1) }, \"Clicks: \" + c); }"
  }'
```

> **Offline por padrão:** React 17 e Babel ficam em `vendor/` e são servidos
> pelo próprio app (`GET /vendor/*`). Sem dependência de CDN/network.
>
> **HTML cru:** se o `jsx` começar com `<`, o conteúdo é carregado como HTML
> literal (sem template React) — útil para protótipos rápidos.

## CLI (`windowloom`)

O CLI é um binário Rust (`src/bin/windowloom.rs`), compilado junto com o app.
Link para `~/.local/bin/` para usar direto do terminal:

```bash
# Criar de um arquivo JSX/HTML
windowloom create meu_widget.jsx
windowloom create pagina.html --width 420 --height 260

# Criar do stdin (heredoc) — JSX multi-linha sem escaping
windowloom create - --title "Relogio" <<'EOF'
function Widget() {
  const [t, s] = React.useState(new Date().toLocaleTimeString());
  setInterval(() => s(new Date().toLocaleTimeString()), 1000);
  return React.createElement('div', { style: { fontSize: 28, fontFamily: 'monospace' } }, '⏰ ' + t);
}
EOF

# Abrir o app e gerenciar janelas
windowloom start                    # inicia o app (tray)
windowloom main                     # abre a janela principal (hub GTK)
windowloom list                     # tabela com id/título/tamanho
windowloom update <id> novo.jsx     # troca o conteúdo ao vivo
windowloom close <id>
windowloom events [n]               # últimos n eventos do appBus

# Porta: --port N ou RUST_CANVAS_PORT (default 8081)
```

> O script `scripts/widget.sh` (bash) continua disponível como alternativa
> sem compilação — mesma API.

**No menu do KDE:** `~/.local/share/applications/windowloom.desktop`
(Exec com as env vars + ícone do app) — clique no menu inicia o WindowLoom.

## Widgets do IAS-CANVAS-TOOL (`appBus`)

Widgets no padrão `function Widget({ appBus })` funcionam: o template injeta
um bus de eventos **roteado pelo app** (cada janela é uma webview com
`window` próprio). `appBus.emit('evt', data)` numa janela chega a todos os
ouvintes `appBus.on('evt', fn)` das outras janelas — via bridge nativo
(`window.webkit.messageHandlers.canvasBus` → Rust → `run_javascript`).

Fluxo típico (dashboard corporativo):

```bash
./scripts/widget.sh ~/git_repos/canvas/IAS-CANVAS-TOOL/widgets-database/business/corp-dados.jsx
./scripts/widget.sh ~/git_repos/canvas/IAS-CANVAS-TOOL/widgets-database/business/corp-vendas.jsx
./scripts/widget.sh ~/git_repos/canvas/IAS-CANVAS-TOOL/widgets-database/business/corp-fluxo-caixa.jsx
./scripts/widget.sh ~/git_repos/canvas/IAS-CANVAS-TOOL/widgets-database/business/corp-metas.jsx
./scripts/widget.sh ~/git_repos/canvas/IAS-CANVAS-TOOL/widgets-database/business/corp-precos.jsx
```

Clique em **Carregar ▶** no corp-dados — os outros 4 widgets recebem os
dados via bus e renderizam (gráficos em Canvas 2D).

## Eventos para o agente (HTTP)

Todo `appBus.emit` dos widgets é registrado num log central — o agente
(OpenCode/Hermes) consulta via HTTP:

```bash
# Últimos 100 eventos (JSON, mais novo primeiro)
curl http://127.0.0.1:8081/events

# Polling incremental: só eventos com ts >= <ts> (o ts do último evento)
curl "http://127.0.0.1:8081/events?since=1785536151287"

# Limpar o buffer (após processar)
curl http://127.0.0.1:8081/events/clear
```

Cada evento: `{ "ts", "window", "evt", "data" }` (timestamp, janela de
origem, nome do evento e payload).

## System Tray

O app fica na bandeja do sistema (StatusNotifierItem via libappindicator —
funciona no KDE Plasma 6 e GNOME). **Ao iniciar, nenhuma janela abre** — só o
tray. Fechar todas as janelas também não encerra o app. No menu do tray:

- **Janela principal** — o hub de comando, com HeaderBar + menubar e 3 abas:
  - **Janelas** — lista das abertas com ações (Topo / Fechar) e duplo-clique
    pra trazer à frente; menu Arquivo tem "Fechar todas as janelas"
  - **Repo** — widgets por categoria com 1 clique: kit embutido (Relógio,
    Contador, Gráfico de barras, Gráfico de linha, Tabela e Formulário, que
    emite no appBus) + **zip de widgets** (botão "Selecionar zip de
    widgets..."): extrai `.jsx`/`.html` e usa o `index.json` do zip pra
    separar por categoria (fallback: subpastas = categorias, raiz = "Geral").
    O zip fica salvo na config e recarrega ao reabrir o hub.
  - **Eventos** — feed ao vivo do que os widgets emitiram no appBus
  - Atalhos: **Ctrl+N** (nova janela), **Ctrl+Q** (sair)
  - também via `windowloom main`
- **Abrir widget...** — seletor de arquivo para escolher um `.jsx`/`.html` e
  criar a janela com o conteúdo (tamanho padrão vindo da configuração)
- **Configurações** — janela com:
  - *Iniciar com o sistema* (cria/remove `~/.config/autostart/rust-canvas-windows.desktop`)
  - *Largura/altura padrão* das janelas novas
  - persiste em `~/.config/rust-canvas-windows/config.json` (bridge `configBus`)
- **Janelas** — submenu dinâmico com as janelas abertas; clicar traz para a
  frente
- **Janela de exemplo** — o widget do cardápio
- **Sair** — encerra o app (também fecha janelas e o servidor IPC)

**Menu de contexto no widget** (botão direito): Recarregar, Sempre no topo
(toggle) e Fechar.

O ícone do tray e o favicon das janelas vêm de `assets/rust-canvas.png`
(gerado com ImageMagick).

Dependência de sistema (Arch/CachyOS): `libappindicator` (fornece o
`appindicator3-0.1.pc`).

## API

Todas as requisições via HTTP POST para `http://localhost:8081`.

| Action | Parâmetros | Descrição |
|--------|-------------|-----------|
| `CREATE_WINDOW` | `title`, `jsx`, `width`, `height` | Cria nova janela |
| `UPDATE_WINDOW` | `id`, `jsx` | Atualiza conteúdo de uma janela |
| `CLOSE_WINDOW` | `id` | Fecha uma janela |
| `LIST_WINDOWS` | — | Lista janelas ativas |

## Integração OpenCode Plugin

O plugin em `opencode-plugin/plugin.js` registra as tools `create_window`, `update_window` e `close_window` no OpenCode. Instale:

1. Build e rode o app Rust
2. Copie `opencode-plugin/plugin.js` para seu diretório de plugins OpenCode
3. A IA pode agora criar janelas nativas com JSX dinâmico!

## Stack

- **Linguagem:** Rust (edition 2024)
- **Janelas:** GTK3 (`gtk` crate 0.18)
- **Renderização:** WebKit2GTK (`webkit2gtk` 2.0)
- **IPC:** HTTP server (`tiny_http` 0.12)
- **Serialização:** `serde` + `serde_json`
