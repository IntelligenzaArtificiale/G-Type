#!/usr/bin/env bash
# install.sh — One-line installer for G-Type on Linux/macOS.
# Usage: curl -sSf https://raw.githubusercontent.com/IntelligenzaArtificiale/G-Type/main/install.sh | bash
set -euo pipefail

REPO="IntelligenzaArtificiale/G-Type"
BIN_NAME="g-type"
INSTALL_DIR="${HOME}/.local/bin"
CONFIG_DIR="${HOME}/.config/g-type"
CONFIG_FILE="${CONFIG_DIR}/config.toml"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

info()  { echo -e "${CYAN}[INFO]${NC}  $*"; }
ok()    { echo -e "${GREEN}[OK]${NC}    $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC}  $*"; }
fail()  { echo -e "${RED}[FAIL]${NC}  $*"; exit 1; }

# Detect OS and architecture
detect_platform() {
    local os arch

    case "$(uname -s)" in
        Linux*)  os="linux" ;;
        Darwin*) os="macos" ;;
        *)       fail "Unsupported OS: $(uname -s). Use Windows install.ps1 instead." ;;
    esac

    case "$(uname -m)" in
        x86_64|amd64)   arch="x86_64" ;;
        arm64|aarch64)  arch="aarch64" ;;
        *)              fail "Unsupported architecture: $(uname -m)" ;;
    esac

    echo "${os}-${arch}"
}

# Fetch the latest release tag from GitHub
get_latest_version() {
    local url="https://api.github.com/repos/${REPO}/releases/latest"
    local tag

    if command -v curl &>/dev/null; then
        tag=$(curl -sSf "$url" | grep '"tag_name"' | head -1 | sed -E 's/.*"tag_name": *"([^"]+)".*/\1/')
    elif command -v wget &>/dev/null; then
        tag=$(wget -qO- "$url" | grep '"tag_name"' | head -1 | sed -E 's/.*"tag_name": *"([^"]+)".*/\1/')
    else
        fail "Neither curl nor wget found. Install one and retry."
    fi

    if [[ -z "$tag" ]]; then
        fail "Could not determine latest release version. Check https://github.com/${REPO}/releases"
    fi

    echo "$tag"
}

# Download the binary
download_binary() {
    local version="$1"
    local platform="$2"
    local asset_name="${BIN_NAME}-${platform}"
    local url="https://github.com/${REPO}/releases/download/${version}/${asset_name}"

    info "Downloading ${BIN_NAME} ${version} for ${platform}..."
    mkdir -p "$INSTALL_DIR"
    rm -f "${INSTALL_DIR}/${BIN_NAME}"

    if command -v curl &>/dev/null; then
        curl -sSfL "$url" -o "${INSTALL_DIR}/${BIN_NAME}"
    elif command -v wget &>/dev/null; then
        wget -q "$url" -O "${INSTALL_DIR}/${BIN_NAME}"
    fi

    chmod +x "${INSTALL_DIR}/${BIN_NAME}"
    ok "Binary installed to ${INSTALL_DIR}/${BIN_NAME}"
}

# Ensure install dir is in PATH
check_path() {
    if [[ ":$PATH:" != *":${INSTALL_DIR}:"* ]]; then
        warn "${INSTALL_DIR} is not in your PATH."
        echo ""
        echo "  Add this to your shell profile (~/.bashrc, ~/.zshrc, etc.):"
        echo ""
        echo "    export PATH=\"\$HOME/.local/bin:\$PATH\""
        echo ""
        return 1
    fi

    return 0
}

# Persist PATH update in common shell profile files when missing
persist_path_update() {
    local export_line='export PATH="$HOME/.local/bin:$PATH"'
    local shell_name profile=""

    shell_name="$(basename "${SHELL:-}")"
    case "$shell_name" in
        zsh) profile="${HOME}/.zshrc" ;;
        bash)
            if [[ "$(uname -s)" == "Darwin" ]]; then
                profile="${HOME}/.bash_profile"
            else
                profile="${HOME}/.bashrc"
            fi
            ;;
        *) profile="${HOME}/.profile" ;;
    esac

    if [[ -f "$profile" ]] && grep -Fq "$export_line" "$profile"; then
        ok "PATH export already present in ${profile}"
        return
    fi

    {
        echo ""
        echo "# Added by G-Type installer"
        echo "$export_line"
    } >> "$profile"

    ok "Added ${INSTALL_DIR} to PATH in ${profile}"
    info "Open a new terminal (or run: source ${profile}) before using '${BIN_NAME}'."
}

setup_autostart() {
    if [[ "$(uname -s)" != "Linux" ]]; then
        return
    fi

    local AUTOSTART_DIR="${HOME}/.config/autostart"
    local DESKTOP_FILE="${AUTOSTART_DIR}/g-type.desktop"

    info "Setting up autostart..."
    mkdir -p "$AUTOSTART_DIR"

    cat > "$DESKTOP_FILE" << EOF
[Desktop Entry]
Type=Application
Name=G-Type
Comment=Voice Dictation Daemon
Exec=${INSTALL_DIR}/${BIN_NAME}
Icon=microphone
Terminal=false
Categories=Utility;
X-GNOME-Autostart-enabled=true
EOF

    ok "Autostart desktop entry created at ${DESKTOP_FILE}"
}

# Install system dependencies on Linux
install_linux_deps() {
    if [[ "$(uname -s)" != "Linux" ]]; then
        return
    fi

    info "Checking Linux audio/input dependencies..."
    local missing=()

    if ! pkg-config --exists alsa 2>/dev/null; then missing+=("libasound2-dev"); fi
    if ! pkg-config --exists x11 2>/dev/null; then missing+=("libx11-dev"); fi
    if ! pkg-config --exists xtst 2>/dev/null; then missing+=("libxtst-dev"); fi

    # UI Dependencies
    if ! pkg-config --exists gtk+-3.0 2>/dev/null; then missing+=("libgtk-3-dev"); fi
    if ! pkg-config --exists webkit2gtk-4.1 2>/dev/null; then missing+=("libwebkit2gtk-4.1-dev"); fi
    if ! pkg-config --exists appindicator3-0.1 2>/dev/null; then missing+=("libayatana-appindicator3-dev"); fi

    if [[ ${#missing[@]} -gt 0 ]]; then
        warn "Missing system packages: ${missing[*]}"
        if command -v apt-get &>/dev/null; then
            info "Installing via apt-get..."
            sudo apt-get update && sudo apt-get install -y "${missing[@]}"
        fi
    else
        ok "System dependencies satisfied"
    fi
}

start_daemon() {
    info "Starting G-Type daemon..."
    nohup "${INSTALL_DIR}/${BIN_NAME}" >/dev/null 2>&1 &
    ok "Daemon started in background"
}

# Main
main() {
    echo ""
    echo -e "${GREEN}╔══════════════════════════════════════╗${NC}"
    echo -e "${GREEN}║       G-Type Installer v2.0          ║${NC}"
    echo -e "${GREEN}║  Global Voice Dictation Daemon       ║${NC}"
    echo -e "${GREEN}╚══════════════════════════════════════╝${NC}"
    echo ""

    local platform
    platform=$(detect_platform)
    info "Detected platform: ${platform}"

    install_linux_deps

    local version
    version=$(get_latest_version)
    info "Latest version: ${version}"

    download_binary "$version" "$platform"
    if ! check_path; then
        persist_path_update
    fi
    
    setup_autostart
    start_daemon

    echo ""
    echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${GREEN}  Installation complete!${NC}"
    echo -e "${GREEN}  Look for the G-Type icon in your system tray.${NC}"
    echo -e "${GREEN}  A setup page should open in your browser shortly.${NC}"
    echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo ""
}

main "$@"
