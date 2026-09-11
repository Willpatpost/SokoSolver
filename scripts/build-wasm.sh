#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd -- "$SCRIPT_DIR/.." && pwd)"

cd "$PROJECT_DIR"

echo "Building sokomind-wasm..."
wasm-pack build crates/sokomind-wasm \
  --target web \
  --out-dir "$PROJECT_DIR/packages/sokomind-ui/src/solver/wasm" \
  --out-name sokomind_solver

echo "WASM build complete."
ls -la packages/sokomind-ui/src/solver/wasm/ 2>/dev/null || echo "(output dir not yet created)"
