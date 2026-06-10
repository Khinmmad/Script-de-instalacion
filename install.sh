#!/usr/bin/env bash
# Instalador rapido de arch-postinstall.
#
# Copia el binario precompilado (estatico, sin dependencias) a tu PATH para
# que puedas ejecutarlo escribiendo solo: arch-postinstall
#
# Uso:
#   ./install.sh            # instala en ~/.local/bin (sin sudo)
#   ./install.sh --system   # instala en /usr/local/bin (requiere sudo)

set -euo pipefail

BIN_NAME="arch-postinstall"
SRC_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/dist"
SRC="$SRC_DIR/${BIN_NAME}-x86_64-linux"

if [[ ! -f "$SRC" ]]; then
    echo "Error: no se encontro el binario en $SRC" >&2
    echo "Compilalo con: cargo build --release" >&2
    exit 1
fi

if [[ "${1:-}" == "--system" ]]; then
    DEST_DIR="/usr/local/bin"
    echo "Instalando en $DEST_DIR (requiere sudo)..."
    sudo install -Dm755 "$SRC" "$DEST_DIR/$BIN_NAME"
else
    DEST_DIR="$HOME/.local/bin"
    echo "Instalando en $DEST_DIR ..."
    install -Dm755 "$SRC" "$DEST_DIR/$BIN_NAME"
fi

echo "Listo. Binario en $DEST_DIR/$BIN_NAME"

if ! command -v "$BIN_NAME" >/dev/null 2>&1; then
    echo
    echo "Nota: $DEST_DIR no esta en tu PATH. Agregalo con:"
    echo "  echo 'export PATH=\"\$HOME/.local/bin:\$PATH\"' >> ~/.bashrc && source ~/.bashrc"
else
    echo "Ahora puedes ejecutarlo escribiendo: $BIN_NAME"
fi
