#!/usr/bin/env bash
# widget.sh — cria janelas JSX no rust-canvas-windows sem sofrer com
# escaping de JSON. Lê o widget de um ARQUIVO ou do STDIN (heredoc).
#
# Uso:
#   widget.sh meu_widget.jsx                    # de um arquivo
#   widget.sh meu_widget.html                   # HTML cru
#   widget.sh - <<'EOF'                         # do stdin (heredoc)
#     function Widget() {
#       const [c, s] = React.useState(0);
#       return React.createElement('button', { onClick: () => s(c+1) }, 'Clicks: ' + c);
#     }
#   EOF
#   widget.sh - --title "Relogio" --width 320 --height 160 <<'EOF'
#     ...
#   EOF
#
# Opções:
#   --title T    título da janela (default: nome do arquivo ou "Widget")
#   --width N    largura (default: 600)
#   --height N   altura (default: 400)
#   --port N     porta do IPC (default: RUST_CANVAS_PORT ou 8081)
set -euo pipefail

PORT="${RUST_CANVAS_PORT:-8081}"
TITLE=""
WIDTH=600
HEIGHT=400
SRC=""

while [ $# -gt 0 ]; do
  case "$1" in
    --title) TITLE="$2"; shift 2 ;;
    --width) WIDTH="$2"; shift 2 ;;
    --height) HEIGHT="$2"; shift 2 ;;
    --port) PORT="$2"; shift 2 ;;
    -h|--help) grep '^#' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) SRC="$1"; shift ;;
  esac
done

if [ -z "$SRC" ]; then
  echo "uso: widget.sh <arquivo.jsx|-> [--title T] [--width N] [--height N]" >&2
  echo "     (use - para ler do stdin, ex: widget.sh - <<'EOF' ... EOF)" >&2
  exit 1
fi

# Lê o conteúdo: arquivo ou stdin
if [ "$SRC" = "-" ]; then
  [ -t 0 ] && { echo "stdin vazio? use: widget.sh - <<'EOF' ... EOF" >&2; exit 1; }
  JSX=$(cat)
  [ -z "$TITLE" ] && TITLE="Widget"
else
  [ ! -f "$SRC" ] && { echo "arquivo não encontrado: $SRC" >&2; exit 1; }
  JSX=$(cat "$SRC")
  [ -z "$TITLE" ] && TITLE=$(basename "$SRC")
fi

# JSON com escaping correto (python — zero dor de cabeça com aspas)
JSON=$(TITLE="$TITLE" WIDTH="$WIDTH" HEIGHT="$HEIGHT" python3 -c '
import json, os
json.dump({
  "action": "CREATE_WINDOW",
  "title": os.environ["TITLE"],
  "jsx": __import__("sys").stdin.read(),
  "width": int(os.environ["WIDTH"]),
  "height": int(os.environ["HEIGHT"]),
}, __import__("sys").stdout)
' <<< "$JSX")

RESP=$(curl -s -X POST "http://127.0.0.1:${PORT}" -H 'Content-Type: application/json' -d "$JSON")
echo "$RESP" | grep -q '"success":true' \
  && echo "✅ janela '${TITLE}' criada (${WIDTH}x${HEIGHT})" \
  || { echo "❌ falha: $RESP" >&2; exit 1; }
