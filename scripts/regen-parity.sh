#!/usr/bin/env bash
# Regenerate the vnc-026 parity-corpus goldens from the Rust oracle (ADR-001).
#
# A deliberate hook.rs behavior change = run this script, review the diff,
# commit it. CI regenerates and diffs (zero-diff gate, R-20).
set -euo pipefail

cd "$(dirname "$0")/.."

UNIMATRIX_PARITY_DIR="$PWD/packages/unimatrix/test/fixtures/parity" \
  cargo test -p unimatrix-server --lib generate_parity_corpus -- --ignored

echo "parity corpus regenerated under packages/unimatrix/test/fixtures/parity/"
git status --short packages/unimatrix/test/fixtures/parity || true
