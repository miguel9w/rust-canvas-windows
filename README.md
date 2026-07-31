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

## System Tray

O app fica na bandeja do sistema (StatusNotifierItem via libappindicator —
funciona no KDE Plasma 6 e GNOME). Fechar todas as janelas **não** encerra o
app: ele continua no tray, de onde você pode:

- 🪟 **Abrir widget...** — abre um seletor de arquivo para escolher um
  `.jsx`/`.html` e cria a janela com o conteúdo
- 🍕 **Janela de exemplo** — o widget do cardápio
- 📋 **Listar janelas** — loga a contagem de janelas ativas
- 🚪 **Sair** — encerra o app (também fecha janelas e o servidor IPC)

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
