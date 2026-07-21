#!/usr/bin/env bash
# Development environment setup for the `ved` project.
#
# Installs the Rust toolchain (via rustup) required to build and test this
# crate. The project pins `nightly` in rust-toolchain.toml (main.rs relies on
# the unstable `#![feature(test)]` attribute), so rustup will automatically
# pick up and install that channel once present.
#
# Usage:
#   ./tools/dev_setup.sh
#
# Safe to re-run: rustup/cargo are only installed if missing, and the
# nightly toolchain + components are installed idempotently.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# 1. Install a C toolchain (provides `cc`, needed as the linker by rustc).
if ! command -v cc >/dev/null 2>&1; then
    echo "==> Installing build-essential (C compiler/linker)..."
    if command -v sudo >/dev/null 2>&1; then
        sudo apt-get update -qq
        sudo apt-get install -y -qq build-essential
    else
        apt-get update -qq
        apt-get install -y -qq build-essential
    fi
fi

# 2. Install rustup (and the default toolchain) if not already present.
if ! command -v rustup >/dev/null 2>&1; then
    echo "==> Installing rustup..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain none
fi

# Make cargo/rustup available in this shell.
# shellcheck disable=SC1091
source "$HOME/.cargo/env"

# 3. Install the nightly toolchain pinned by rust-toolchain.toml.
#    Running `rustup show` from the repo root will also trigger this
#    automatically, but we do it explicitly here to make the step visible
#    and to ensure it happens even before the first cargo invocation.
echo "==> Ensuring nightly toolchain is installed..."
cd "$REPO_ROOT"
rustup toolchain install nightly --profile default
rustup show

echo "==> Verifying toolchain..."
cargo --version
rustc --version

echo "==> Setup complete."
