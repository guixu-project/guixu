#!/usr/bin/env bash
# =============================================================================
# Guixu Installer — One-line install:
#   curl -fsSL https://raw.githubusercontent.com/guixu-project/guixu/main/install.sh | bash
#
# Downloads binary → inits node → detects AI clients → registers MCP. Done.
# =============================================================================
set -euo pipefail

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
CYAN='\033[0;36m'
NC='\033[0m'

info()  { echo -e "${GREEN}[✓]${NC} $1"; }
warn()  { echo -e "${YELLOW}[!]${NC} $1"; }
fail()  { echo -e "${RED}[✗]${NC} $1"; exit 1; }
step()  { echo -e "\n${CYAN}▶ $1${NC}"; }

REPO_OWNER="guixu-project"
REPO_NAME="guixu"
REPO_URL="https://github.com/${REPO_OWNER}/${REPO_NAME}"
INSTALL_DIR="$HOME/.guixu"
BIN_DIR="$INSTALL_DIR/bin"
DATA_DIR="$HOME/shared-datasets"

# --- Detect platform ---
detect_platform() {
    local os arch
    os="$(uname -s)"
    arch="$(uname -m)"

    case "$os" in
        Linux)  os="linux" ;;
        Darwin) os="darwin" ;;
        *)      fail "Unsupported OS: $os" ;;
    esac

    case "$arch" in
        x86_64|amd64)  arch="amd64" ;;
        aarch64|arm64) arch="arm64" ;;
        *)             fail "Unsupported architecture: $arch" ;;
    esac

    echo "guixu-${os}-${arch}"
}

# --- Try downloading prebuilt binary from latest GitHub Release ---
try_download_release() {
    local artifact="$1"
    step "Checking for prebuilt binary..."

    local release_url="https://api.github.com/repos/${REPO_OWNER}/${REPO_NAME}/releases/latest"
    local tag
    tag=$(curl -fsSL "$release_url" 2>/dev/null | grep '"tag_name"' | head -1 | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/') || true

    if [ -z "$tag" ]; then
        warn "No releases found, will build from source"
        return 1
    fi

    local download_url="${REPO_URL}/releases/download/${tag}/${artifact}.tar.gz"
    info "Found release $tag"

    local tmp_dir
    tmp_dir=$(mktemp -d)
    trap "rm -rf $tmp_dir" EXIT

    step "Downloading ${artifact}.tar.gz..."
    if curl -fsSL -o "$tmp_dir/guixu.tar.gz" "$download_url" 2>/dev/null; then
        cd "$tmp_dir"
        tar xzf guixu.tar.gz
        mkdir -p "$BIN_DIR"
        mv "$artifact" "$BIN_DIR/guixu"
        chmod +x "$BIN_DIR/guixu"
        info "Binary installed to $BIN_DIR/guixu ($tag)"
        return 0
    else
        warn "Download failed, will build from source"
        return 1
    fi
}

# --- Build from source ---
build_from_source() {
    step "Building from source..."

    if ! command -v cargo &>/dev/null; then
        warn "Rust not found. Installing via rustup..."
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
        source "$HOME/.cargo/env"
    fi
    info "Rust $(rustc --version | awk '{print $2}')"

    command -v git &>/dev/null || fail "Git is required. Install it first."

    if [ -d "$INSTALL_DIR/src" ]; then
        info "Updating existing source..."
        cd "$INSTALL_DIR/src"
        git pull --quiet
    else
        mkdir -p "$INSTALL_DIR"
        git clone --quiet --depth 1 "${REPO_URL}.git" "$INSTALL_DIR/src"
        cd "$INSTALL_DIR/src"
    fi

    step "Compiling (this takes ~2 minutes on first build)..."
    cargo build --release --quiet 2>&1 | tail -1 || cargo build --release

    mkdir -p "$BIN_DIR"
    cp target/release/data-node "$BIN_DIR/guixu"
    chmod +x "$BIN_DIR/guixu"
    info "Binary installed to $BIN_DIR/guixu (built from source)"
}

# =============================================================================
# Main
# =============================================================================

echo -e "${CYAN}"
echo "  ╔═══════════════════════════════════════╗"
echo "  ║   Guixu — Data Discovery Protocol     ║"
echo "  ╚═══════════════════════════════════════╝"
echo -e "${NC}"

ARTIFACT=$(detect_platform)
info "Platform: $ARTIFACT"

# ① Install binary
if ! try_download_release "$ARTIFACT"; then
    build_from_source
fi

# ② Add to PATH
SHELL_RC=""
if [ -f "$HOME/.zshrc" ]; then
    SHELL_RC="$HOME/.zshrc"
elif [ -f "$HOME/.bashrc" ]; then
    SHELL_RC="$HOME/.bashrc"
elif [ -f "$HOME/.bash_profile" ]; then
    SHELL_RC="$HOME/.bash_profile"
fi

if [ -n "$SHELL_RC" ]; then
    if ! grep -q "$BIN_DIR" "$SHELL_RC" 2>/dev/null; then
        echo "" >> "$SHELL_RC"
        echo "# Guixu" >> "$SHELL_RC"
        echo "export PATH=\"$BIN_DIR:\$PATH\"" >> "$SHELL_RC"
        info "Added $BIN_DIR to PATH in $SHELL_RC"
    fi
fi
export PATH="$BIN_DIR:$PATH"

# ③ Initialize node (silent — user doesn't need to know)
if [ ! -f "$HOME/.data-node/config.toml" ]; then
    "$BIN_DIR/guixu" init --data-dir "$DATA_DIR" >/dev/null 2>&1 && \
        info "Node initialized" || warn "Node init skipped"
else
    info "Node already initialized"
fi

# ④ Auto-detect AI clients and register MCP
step "Detecting AI clients..."

DETECTED=()
[ -d "$HOME/.codex" ]                          && DETECTED+=("codex")
[ -d "$HOME/.cursor" ]                         && DETECTED+=("cursor")
command -v claude &>/dev/null                   && DETECTED+=("claude-code")
[ -d "$HOME/.config/opencode" ]                && DETECTED+=("opencode")

if [ ${#DETECTED[@]} -gt 0 ]; then
    for client in "${DETECTED[@]}"; do
        if "$BIN_DIR/guixu" mcp install "$client" 2>/dev/null; then
            info "Registered MCP → $client"
        else
            warn "Failed to register MCP → $client"
        fi
    done
else
    info "No AI clients detected. Register later with: guixu mcp install <client>"
fi

# ⑤ Done
echo ""
echo -e "${GREEN}═══════════════════════════════════════════════════════${NC}"
echo -e "${GREEN}  ✅ Guixu installed!${NC}"
echo -e "${GREEN}═══════════════════════════════════════════════════════${NC}"
echo ""
echo "  Quick start:"
echo ""
echo "    guixu start                # Start node + Web UI"
echo "    open http://localhost:3927  # Drag & drop to publish datasets"
echo ""
echo "  Register MCP with more clients:"
echo ""
echo "    guixu mcp install codex       # Codex"
echo "    guixu mcp install cursor      # Cursor"
echo "    guixu mcp install claude-code # Claude Code"
echo "    guixu mcp install opencode    # OpenCode"
echo ""
if [ -n "$SHELL_RC" ]; then
    echo -e "  ${YELLOW}Restart your shell or run: source $SHELL_RC${NC}"
    echo ""
fi
