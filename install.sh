#!/usr/bin/env bash
set -euo pipefail

echo "Building replay..."
cargo install --path "$(dirname "$0")"

echo ""
echo "Installing hooks and /replay command..."
replay install
