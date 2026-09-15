#!/usr/bin/env bash
# Instalador de tcode para Linux y macOS (PLAN.md §10): descarga el
# binario de la última release de GitHub (o la que indique
# TCODE_VERSION, p. ej. TCODE_VERSION=v0.1.0) y lo instala en
# ~/.local/bin sin sudo, añadiéndolo al PATH del shell activo si hace
# falta.
#
# Uso:
#   curl -fsSL https://raw.githubusercontent.com/1franky/tcode/main/install/linux.sh | bash
set -euo pipefail

REPO="1franky/tcode"
DESTINO_DIR="$HOME/.local/bin"
DESTINO_BIN="$DESTINO_DIR/tcode"
VERSION="${TCODE_VERSION:-latest}"

log() { printf '\033[1;34m==>\033[0m %s\n' "$1"; }
error() {
    printf '\033[1;31merror:\033[0m %s\n' "$1" >&2
    exit 1
}

detectar_plataforma() {
    local so arquitectura
    so="$(uname -s)"
    arquitectura="$(uname -m)"

    case "$so" in
        Linux)
            case "$arquitectura" in
                x86_64) echo "linux-x86_64" ;;
                # `uname -m` reporta "aarch64" en todos los Linux ARM de
                # 64 bits que se probaron (VPS ARM tipo AWS Graviton/
                # Oracle Ampere, Raspberry Pi de 64 bits) — "arm64" queda
                # como alias por si algún sistema lo reporta distinto.
                aarch64 | arm64) echo "linux-arm64" ;;
                *) error "Linux en '$arquitectura' no tiene binario pre-compilado todavía. Compila desde el código fuente con 'cargo build --release'." ;;
            esac
            ;;
        Darwin)
            case "$arquitectura" in
                arm64) echo "macos-arm64" ;;
                x86_64) echo "macos-x86_64" ;;
                *) error "macOS en '$arquitectura' no tiene binario pre-compilado todavía." ;;
            esac
            ;;
        *)
            error "'$so' no es Linux ni macOS. Para Windows usa install/windows.ps1."
            ;;
    esac
}

main() {
    command -v curl >/dev/null 2>&1 || error "hace falta 'curl' para descargar tcode."
    command -v tar >/dev/null 2>&1 || error "hace falta 'tar' para extraer el binario."

    # dir_tmp NO es local: el trap de limpieza se dispara al salir del
    # script completo (después de que main() retorna), momento en el que
    # una variable local a esta función ya no existiría.
    local plataforma url_descarga archivo_tmp
    dir_tmp=""
    plataforma="$(detectar_plataforma)"

    if [ "$VERSION" = "latest" ]; then
        url_descarga="https://github.com/$REPO/releases/latest/download/tcode-$plataforma.tar.gz"
    else
        url_descarga="https://github.com/$REPO/releases/download/$VERSION/tcode-$plataforma.tar.gz"
    fi

    log "Descargando tcode ($plataforma, $VERSION)..."
    dir_tmp="$(mktemp -d)"
    trap 'rm -rf "$dir_tmp"' EXIT
    archivo_tmp="$dir_tmp/tcode.tar.gz"

    if ! curl -fsSL "$url_descarga" -o "$archivo_tmp"; then
        error "no se pudo descargar $url_descarga — ¿ya existe una release publicada? (github.com/$REPO/releases)"
    fi

    log "Instalando en $DESTINO_BIN..."
    mkdir -p "$DESTINO_DIR"
    tar xzf "$archivo_tmp" -C "$dir_tmp" tcode
    mv "$dir_tmp/tcode" "$DESTINO_BIN"
    chmod +x "$DESTINO_BIN"

    # El binario no está firmado/notarizado: sin esto, Gatekeeper lo
    # bloquea con "no se puede abrir porque su desarrollador no se pudo
    # verificar" en el primer arranque.
    if [ "$(uname -s)" = "Darwin" ] && command -v xattr >/dev/null 2>&1; then
        xattr -d com.apple.quarantine "$DESTINO_BIN" 2>/dev/null || true
    fi

    agregar_al_path

    log "tcode instalado correctamente. Prueba con: tcode"
}

agregar_al_path() {
    case ":$PATH:" in
        *":$DESTINO_DIR:"*)
            return 0
            ;;
    esac

    local archivo_perfil linea
    linea="export PATH=\"\$HOME/.local/bin:\$PATH\""

    case "${SHELL:-}" in
        */zsh) archivo_perfil="$HOME/.zshrc" ;;
        */bash) archivo_perfil="$HOME/.bashrc" ;;
        */fish)
            archivo_perfil="$HOME/.config/fish/config.fish"
            linea="fish_add_path \$HOME/.local/bin"
            ;;
        *) archivo_perfil="$HOME/.profile" ;;
    esac

    log "Agregando $DESTINO_DIR al PATH en $archivo_perfil..."
    mkdir -p "$(dirname "$archivo_perfil")"
    {
        echo ""
        echo "# Añadido por el instalador de tcode"
        echo "$linea"
    } >>"$archivo_perfil"

    log "Abre una terminal nueva (o ejecuta: source $archivo_perfil) para que 'tcode' quede disponible."
}

main "$@"
