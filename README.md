# 🪟 Rust Canvas Windows

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

# Rodar (X11/XWayland + software rendering do WebKit — necessário em
# setups NVIDIA/Wayland: sem essas env vars as janelas podem ficar pretas)
GDK_BACKEND=x11 WEBKIT_DISABLE_DMABUF_RENDERER=1 cargo run --release

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

## CLI (`scripts/widget.sh`)

Crie janelas de um arquivo ou direto do terminal, sem sofrer com escaping
de JSON:

```bash
# De um arquivo JSX
./scripts/widget.sh meu_widget.jsx

# De um arquivo HTML cru
./scripts/widget.sh pagina.html --width 420 --height 260

# Do stdin (heredoc) — JSX multi-linha sem escaping
./scripts/widget.sh - --title "Relogio" <<'EOF'
function Widget() {
  const [t, s] = React.useState(new Date().toLocaleTimeString());
  setInterval(() => s(new Date().toLocaleTimeString()), 1000);
  return React.createElement('div', { style: { fontSize: 28, fontFamily: 'monospace' } }, '⏰ ' + t);
}
EOF

# Opções: --title T | --width N | --height N | --port N
```

Exemplos prontos em `examples/` (`contador.jsx`, `html-cru.html`).

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
