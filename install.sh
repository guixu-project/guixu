#!/usr/bin/env bash
# =============================================================================
# Guixu Installer — One-line install:
#   curl -fsSL https://raw.githubusercontent.com/data-protocols/guixu/main/install.sh | bash
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

REPO="https://github.com/data-protocols/guixu.git"
INSTALL_DIR="$HOME/.guixu"
BIN_DIR="$INSTALL_DIR/bin"
DATA_DIR="$HOME/shared-datasets"

# --- Pre-flight checks ---
step "Checking prerequisites..."

# Rust
if ! command -v cargo &>/dev/null; then
    warn "Rust not found. Installing via rustup..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
    source "$HOME/.cargo/env"
fi
info "Rust $(rustc --version | awk '{print $2}')"

# Git
command -v git &>/dev/null || fail "Git is required. Install it first."
info "Git $(git --version | awk '{print $3}')"

# --- Clone or update ---
step "Installing Guixu..."

if [ -d "$INSTALL_DIR/src" ]; then
    info "Updating existing installation..."
    cd "$INSTALL_DIR/src"
    git pull --quiet
else
    mkdir -p "$INSTALL_DIR"
    git clone --quiet --depth 1 "$REPO" "$INSTALL_DIR/src"
    cd "$INSTALL_DIR/src"
fi

# --- Build ---
step "Building (this takes ~2 minutes on first install)..."
cargo build --release --quiet 2>&1 | tail -1 || cargo build --release

mkdir -p "$BIN_DIR"
cp target/release/data-node "$BIN_DIR/guixu"
info "Binary installed to $BIN_DIR/guixu"

# --- Add to PATH ---
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

# --- Initialize node ---
step "Initializing node..."

if [ ! -f "$HOME/.data-node/config.toml" ]; then
    guixu init --data-dir "$DATA_DIR"
else
    info "Node already initialized, skipping"
fi

# --- Done ---
echo ""
echo -e "${GREEN}═══════════════════════════════════════════════════════${NC}"
echo -e "${GREEN}  ✅ Guixu installed successfully!${NC}"
echo -e "${GREEN}═══════════════════════════════════════════════════════${NC}"
echo ""
echo "  Quick start:"
echo ""
echo "    guixu start              # Start node + open Web UI"
echo "    open http://localhost:3927  # Drag & drop to publish datasets"
echo ""
echo "  Or publish from CLI:"
echo ""
echo "    cp my_data.csv ~/shared-datasets/   # Auto-published!"
echo ""
echo "  For AI agent integration:"
echo ""
echo "    guixu mcp                # stdio MCP server"
echo "    guixu mcp --mode http    # HTTP MCP server on :3927"
echo ""
echo -e "  ${YELLOW}Restart your shell or run: source $SHELL_RC${NC}"
echo ""
