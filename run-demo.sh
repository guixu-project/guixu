#!/usr/bin/env bash
set -e
cd "$(dirname "$0")"

PORT="${1:-3927}"

# Ensure cargo/rustc shims from rustup are visible in this shell.
ensure_rust_env() {
  if [ -f "$HOME/.cargo/env" ]; then
    # shellcheck disable=SC1090
    . "$HOME/.cargo/env"
  elif [ -d "$HOME/.cargo/bin" ]; then
    export PATH="$HOME/.cargo/bin:$PATH"
  fi
}

has_rustup_stable_cargo() {
  rustup_usable && rustup run stable cargo --version >/dev/null 2>&1
}

rustup_usable() {
  command -v rustup &>/dev/null && rustup --version >/dev/null 2>&1
}

install_rustup() {
  echo "Installing Rust..."
  if curl -s --connect-timeout 3 https://sh.rustup.rs >/dev/null 2>&1; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable --no-modify-path
  else
    # China mirror
    curl --proto '=https' --tlsv1.2 -sSf https://rsproxy.cn/rustup-init.sh | sh -s -- -y --default-toolchain stable --no-modify-path
  fi
}

cargo_cmd() {
  if has_rustup_stable_cargo; then
    # `rustup run stable cargo` can still pick `/usr/bin/rustc` if the shell PATH
    # does not include rustup proxies. Resolve the stable toolchain bin dir and
    # pin cargo/rustc/rustdoc to that directory for this build.
    local sysroot toolchain_bin
    sysroot=$(rustup run stable rustc --print sysroot)
    toolchain_bin="$sysroot/bin"
    env \
      -u RUSTC_WRAPPER \
      -u RUSTC_WORKSPACE_WRAPPER \
      PATH="$toolchain_bin:$PATH" \
      RUSTC="$toolchain_bin/rustc" \
      CARGO_BUILD_RUSTC="$toolchain_bin/rustc" \
      RUSTDOC="$toolchain_bin/rustdoc" \
      "$toolchain_bin/cargo" "$@"
  elif rustup_usable; then
    echo "Error: rustup is installed but stable cargo is unavailable." >&2
    echo "Run: rustup toolchain install stable --profile minimal -c cargo -c rustc" >&2
    return 1
  elif command -v cargo &>/dev/null; then
    env -u RUSTC -u CARGO_BUILD_RUSTC -u RUSTC_WRAPPER -u RUSTC_WORKSPACE_WRAPPER cargo "$@"
  else
    return 127
  fi
}

rustc_version() {
  local raw
  if has_rustup_stable_cargo; then
    raw=$(rustup run stable rustc --version 2>/dev/null || true)
    [ -n "$raw" ] && { echo "$raw" | awk '{print $2}'; return 0; }
  fi
  if rustup_usable; then
    # rustup exists but stable cargo/rustc are not both ready; treat as not OK.
    return 1
  fi
  if command -v rustc &>/dev/null; then
    raw=$(rustc --version 2>/dev/null || true)
    [ -n "$raw" ] && { echo "$raw" | awk '{print $2}'; return 0; }
  fi
  return 1
}

# Install Rust if missing or too old (need >= 1.82)
install_rust() {
  ensure_rust_env
  if rustup_usable; then
    echo "Updating Rust..."
    if ! rustup update stable; then
      echo "Existing rustup is unusable, reinstalling via installer..."
      install_rustup
    fi
    rustup toolchain install stable --profile minimal -c cargo -c rustc >/dev/null 2>&1 || true
  else
    install_rustup
  fi
  ensure_rust_env
}

rust_ok() {
  command -v cargo &>/dev/null || has_rustup_stable_cargo || return 1
  local ver
  ver=$(rustc_version) || return 1
  [ "$(printf '%s\n' "1.82.0" "$ver" | sort -V | head -1)" = "1.82.0" ]
}

# Setup crates.io mirror if needed
setup_cargo_mirror() {
  local cfg="$HOME/.cargo/config.toml"
  [ -f "$cfg" ] && grep -q 'rsproxy' "$cfg" && return
  if ! curl -s --connect-timeout 3 https://crates.io >/dev/null 2>&1; then
    echo "Setting up crates.io China mirror..."
    mkdir -p "$HOME/.cargo"
    cat >> "$cfg" <<'EOF'
[source.crates-io]
replace-with = "rsproxy-sparse"
[source.rsproxy-sparse]
registry = "sparse+https://rsproxy.cn/crates.io-index/"
[registries.rsproxy]
index = "https://rsproxy.cn/crates.io-index"
EOF
  fi
}

# Install system deps
install_deps() {
  if command -v apt-get &>/dev/null; then
    local pkgs=()
    command -v clang &>/dev/null || pkgs+=(libclang-dev)
    command -v pkg-config &>/dev/null || pkgs+=(pkg-config)
    if [ ${#pkgs[@]} -gt 0 ]; then
      sudo apt-get update
      sudo apt-get install -y "${pkgs[@]}"
    fi
  fi
  return 0
}

ensure_rust_env
rust_ok || install_rust
rust_ok || { echo "Error: Rust install failed." >&2; exit 1; }

setup_cargo_mirror
install_deps

# Build
if [ ! -f target/release/data-node ] || [ "$(find crates -name '*.rs' -newer target/release/data-node 2>/dev/null | head -1)" ]; then
  echo "Building..."
  cargo_cmd build --release
fi

# Init if first run
[ -d ~/.data-node ] || ./target/release/data-node init

echo "Web UI  → http://localhost:$PORT"
echo "Demo UI → http://localhost:$PORT/demo"
exec ./target/release/data-node start