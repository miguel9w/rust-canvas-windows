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

# Rodar
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
