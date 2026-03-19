#!/usr/bin/env bash
# check-inference-sites.sh — AC-07 / crt-022 enforcement
#
# Verifies that no ONNX embedding inference call site in unimatrix-server uses
# spawn_blocking or spawn_blocking_with_timeout. All inference sites must use
# ml_inference_pool.spawn_with_timeout (MCP handler paths) or
# ml_inference_pool.spawn (background paths).
#
# Run from the workspace root.
set -euo pipefail

FAIL=0

# Check 1: no spawn_blocking at embedding sites in services/
# Filter on "embed" to distinguish inference sites from permitted DB/co-access calls.
echo "Checking services/ for spawn_blocking at embedding inference sites..."
MATCHES=$(grep -rn "spawn_blocking" crates/unimatrix-server/src/services/ \
  | grep "embed" \
  | grep -v "//") || true
if [ -n "$MATCHES" ]; then
    echo "ERROR: spawn_blocking at embedding inference site(s) in services/:"
    echo "$MATCHES"
    FAIL=1
fi

# Check 2: no spawn_blocking_with_timeout at embedding sites in services/
# All inference-path spawn_blocking_with_timeout calls must be replaced by rayon.
echo "Checking services/ for spawn_blocking_with_timeout at embedding inference sites..."
MATCHES=$(grep -rn "spawn_blocking_with_timeout" crates/unimatrix-server/src/services/ \
  | grep "embed" \
  | grep -v "//") || true
if [ -n "$MATCHES" ]; then
    echo "ERROR: spawn_blocking_with_timeout at embedding inference site(s) in services/:"
    echo "$MATCHES"
    FAIL=1
fi

# Check 3: no spawn_blocking at embedding sites in background.rs
# background.rs retains permitted spawn_blocking for run_extraction_rules and
# persist_shadow_evaluations — the embed filter identifies inference sites only.
echo "Checking background.rs for spawn_blocking at embedding inference sites..."
MATCHES=$(grep -n "spawn_blocking" crates/unimatrix-server/src/background.rs \
  | grep "embed" \
  | grep -v "//") || true
if [ -n "$MATCHES" ]; then
    echo "ERROR: spawn_blocking at embedding inference site(s) in background.rs:"
    echo "$MATCHES"
    FAIL=1
fi

# Check 4: AsyncEmbedService must not exist in async_wrappers.rs (crt-022 removal)
echo "Checking async_wrappers.rs for AsyncEmbedService..."
MATCHES=$(grep -n "AsyncEmbedService" crates/unimatrix-core/src/async_wrappers.rs) || true
if [ -n "$MATCHES" ]; then
    echo "ERROR: AsyncEmbedService found in async_wrappers.rs (must be removed in crt-022):"
    echo "$MATCHES"
    FAIL=1
fi

# Check 5: embed_handle.rs must retain exactly 1 spawn_blocking (OnnxProvider::new)
# This prevents accidental migration of model-load I/O to rayon (C-03).
echo "Checking embed_handle.rs for exactly 1 spawn_blocking (OnnxProvider::new)..."
COUNT=$(grep -c "spawn_blocking" crates/unimatrix-server/src/infra/embed_handle.rs 2>/dev/null || echo 0)
if [ "$COUNT" -ne 1 ]; then
    echo "ERROR: embed_handle.rs must have exactly 1 spawn_blocking (OnnxProvider::new), found: $COUNT"
    echo "       Do not move OnnxProvider::new to rayon (C-03) and do not add new inference spawn_blocking calls here."
    FAIL=1
fi

if [ "$FAIL" -eq 0 ]; then
    echo "OK: all spawn_blocking enforcement checks passed (AC-07 / crt-022)."
else
    echo ""
    echo "See crt-022 for the migration pattern:"
    echo "  MCP handler paths: ml_inference_pool.spawn_with_timeout(MCP_HANDLER_TIMEOUT, ...)"
    echo "  Background paths:  ml_inference_pool.spawn(...)"
    exit 1
fi
