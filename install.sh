#!/usr/bin/env bash
# G-Type one-command installer for Linux and macOS.
# Usage: curl -fsSL https://raw.githubusercontent.com/IntelligenzaArtificiale/G-Type/main/install.sh | bash
set -euo pipefail

REPO="IntelligenzaArtificiale/G-Type"
BIN_NAME="g-type"
INSTALL_DIR="${HOME}/.local/bin"
BIN_PATH="${INSTALL_DIR}/${BIN_NAME}"
MIN_BINARY_BYTES=1000000

RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; CYAN='\033[0;36m'; NC='\033[0m'
info(){ echo -e "${CYAN}[INFO]${NC}  $*"; }
ok(){ echo -e "${GREEN}[OK]${NC}    $*"; }
warn(){ echo -e "${YELLOW}[WARN]${NC}  $*"; }
fail(){ echo -e "${RED}[FAIL]${NC}  $*" >&2; exit 1; }

http_get(){
    local url="$1" out="${2:-}"
    if command -v curl >/dev/null 2>&1; then
        if [[ -n "$out" ]]; then
            curl -fsSL --retry 2 --connect-timeout 10 "$url" -o "$out"
        else
            curl -fsSL --retry 2 --connect-timeout 10 "$url"
        fi
    elif command -v wget >/dev/null 2>&1; then
        if [[ -n "$out" ]]; then
            wget -q --timeout=20 --tries=3 "$url" -O "$out"
        else
            wget -qO- --timeout=20 --tries=3 "$url"
        fi
    else
        fail "Serve curl oppure wget. Installane uno e riprova."
    fi
}

detect_platform(){
    local os arch
    case "$(uname -s)" in
        Linux*) os="linux" ;;
        Darwin*) os="macos" ;;
        *) fail "Sistema non supportato: $(uname -s). Su Windows usa install.ps1." ;;
    esac
    case "$(uname -m)" in
        x86_64|amd64) arch="x86_64" ;;
        arm64|aarch64) arch="aarch64" ;;
        *) fail "Architettura non supportata: $(uname -m)" ;;
    esac
    if [[ "$os" == "linux" && "$arch" != "x86_64" ]]; then
        fail "La release Linux precompilata è disponibile al momento solo per x86_64."
    fi
    echo "${os}-${arch}"
}

latest_version(){
    local tag
    tag="$(http_get "https://api.github.com/repos/${REPO}/releases/latest" | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -1)"
    [[ -n "$tag" ]] || fail "Impossibile determinare l'ultima release di G-Type."
    echo "$tag"
}

install_linux_deps(){
    [[ "$(uname -s)" == "Linux" ]] || return 0
    command -v pkg-config >/dev/null 2>&1 || { warn "pkg-config non trovato; salto il controllo automatico delle librerie Linux."; return 0; }

    local missing=()
    pkg-config --exists alsa 2>/dev/null || missing+=("libasound2-dev")
    pkg-config --exists x11 2>/dev/null || missing+=("libx11-dev")
    pkg-config --exists xtst 2>/dev/null || missing+=("libxtst-dev")
    pkg-config --exists gtk+-3.0 2>/dev/null || missing+=("libgtk-3-dev")
    pkg-config --exists webkit2gtk-4.1 2>/dev/null || missing+=("libwebkit2gtk-4.1-dev")
    pkg-config --exists appindicator3-0.1 2>/dev/null || missing+=("libayatana-appindicator3-dev")

    [[ ${#missing[@]} -eq 0 ]] && { ok "Dipendenze Linux presenti"; return 0; }
    if command -v apt-get >/dev/null 2>&1; then
        info "Installazione dipendenze Linux mancanti: ${missing[*]}"
        sudo apt-get update -qq
        sudo apt-get install -y "${missing[@]}"
    else
        warn "Librerie mancanti: ${missing[*]}. Installale con il package manager della tua distribuzione se G-Type non parte."
    fi
}

install_binary(){
    local version="$1" platform="$2" asset="${BIN_NAME}-${platform}"
    local url="https://github.com/${REPO}/releases/download/${version}/${asset}"
    local tmp size

    mkdir -p "$INSTALL_DIR"
    tmp="$(mktemp "${INSTALL_DIR}/.g-type-install.XXXXXX")"
    trap 'rm -f "${tmp:-}"' EXIT

    info "Download G-Type ${version} per ${platform}..."
    http_get "$url" "$tmp"
    size="$(wc -c < "$tmp" | tr -d ' ')"
    [[ "$size" -ge "$MIN_BINARY_BYTES" ]] || fail "Download incompleto (${size} byte). Il binario esistente non è stato toccato."

    chmod +x "$tmp"
    mv -f "$tmp" "$BIN_PATH"
    trap - EXIT
    ok "Installato in ${BIN_PATH}"
}

persist_path(){
    [[ ":${PATH}:" == *":${INSTALL_DIR}:"* ]] && return 0

    local shell_name profile line='export PATH="$HOME/.local/bin:$PATH"'
    shell_name="$(basename "${SHELL:-sh}")"
    case "$shell_name" in
        zsh) profile="${HOME}/.zshrc" ;;
        bash)
            [[ "$(uname -s)" == "Darwin" ]] && profile="${HOME}/.bash_profile" || profile="${HOME}/.bashrc"
            ;;
        *) profile="${HOME}/.profile" ;;
    esac

    touch "$profile"
    if ! grep -Fq "$line" "$profile"; then
        printf '\n# Added by G-Type installer\n%s\n' "$line" >> "$profile"
        ok "Aggiunto ${INSTALL_DIR} al PATH in ${profile}"
    fi
    export PATH="${INSTALL_DIR}:${PATH}"
}

setup_linux_autostart(){
    [[ "$(uname -s)" == "Linux" ]] || return 0
    local autostart_dir="${HOME}/.config/autostart"
    local desktop_file="${autostart_dir}/g-type.desktop"

    mkdir -p "$autostart_dir"
    cat > "$desktop_file" <<EOF
[Desktop Entry]
Type=Application
Name=G-Type
Comment=Global voice dictation
Exec=${BIN_PATH}
Terminal=false
X-GNOME-Autostart-enabled=true
EOF
    ok "Autoavvio Linux configurato"
}

start_gtype(){
    if command -v curl >/dev/null 2>&1 && curl -fsS --connect-timeout 1 http://127.0.0.1:9741/api/state >/dev/null 2>&1; then
        info "G-Type è già in esecuzione; non avvio una seconda istanza."
        return 0
    fi

    info "Avvio G-Type..."
    nohup "$BIN_PATH" >/dev/null 2>&1 &
    disown 2>/dev/null || true
    sleep 1
    ok "G-Type avviato. Al primo utilizzo si apre la configurazione nel browser."
}

main(){
    echo
    echo -e "${GREEN}G-Type · installer${NC}"

    local platform version
    platform="$(detect_platform)"
    info "Piattaforma: ${platform}"
    install_linux_deps

    version="$(latest_version)"
    info "Release: ${version}"
    install_binary "$version" "$platform"
    persist_path
    setup_linux_autostart
    start_gtype

    echo
    echo -e "${GREEN}Installazione completata.${NC}"
    echo "Dashboard: http://127.0.0.1:9741/"
    echo "Aggiornamenti futuri: g-type upgrade"
    echo
}

main "$@"
