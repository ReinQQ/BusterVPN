#!/usr/bin/env bash
set -euo pipefail

echo "=================================================="
echo "          BusterVPN Linux Build Script            "
echo "=================================================="

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$PROJECT_ROOT"

# 1. Install prerequisites on Debian/Ubuntu systems
if command -v apt-get >/dev/null 2>&1; then
    echo "[1/5] Checking Debian/Ubuntu build dependencies..."
    DEBIAN_PACKAGES=(
        build-essential
        curl
        libssl-dev
        libgtk-3-dev
        libwebkit2gtk-4.1-dev
        libayatana-appindicator3-dev
        librsvg2-dev
        libjavascriptcoregtk-4.1-dev
    )
    if [ "${EUID:-$(id -u)}" -eq 0 ]; then
        apt-get update -y
        apt-get install -y "${DEBIAN_PACKAGES[@]}"
    else
        echo "Note: Running as non-root. If packages are missing, run:"
        echo "  sudo apt-get update && sudo apt-get install -y ${DEBIAN_PACKAGES[*]}"
    fi
elif command -v pacman >/dev/null 2>&1; then
    echo "[1/5] Checking Arch Linux build dependencies..."
    echo "Ensure base-devel, openssl, webkit2gtk-4.1, libappindicator-gtk3, librsvg are installed."
fi

# 2. Permissions on core binaries if present
echo "[2/5] Checking core engine binaries..."
if [ -d "cores" ]; then
    find cores/ -type f -exec chmod +x {} + 2>/dev/null || true
fi

# 3. Build Svelte 5 frontend
echo "[3/5] Installing frontend dependencies & building Svelte 5 assets..."
cd crates/buster-app/frontend
npm install
npm run build
cd "$PROJECT_ROOT"

# 4. Run full test suite
echo "[4/5] Running Cargo workspace unit and integration tests..."
cargo test --workspace

# 5. Build Tauri v2 application binary and release bundles
echo "[5/5] Compiling BusterVPN release binary with Tauri CLI..."
if command -v cargo-tauri >/dev/null 2>&1; then
    cargo tauri build
else
    echo "cargo-tauri CLI not found, installing or using cargo run..."
    cargo run --manifest-path crates/buster-app/Cargo.toml --release
fi

echo "=================================================="
echo "  BusterVPN build completed successfully!         "
echo "  Release binary: target/release/buster-app       "
echo "=================================================="
