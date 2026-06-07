#!/usr/bin/env bash
# vnc-026 AC-12 parity-corpus drift check (R-20: must FAIL, never skip).
#
# The Rust hook is the oracle (ADR-001). This job regenerates the committed
# goldens under packages/unimatrix/test/fixtures/parity/ from the additive Rust
# dev-test and asserts ZERO diff. Three non-vacuity guards make a silent
# no-op impossible (lesson #4452 vacuous-pass):
#
#   1. The generator test must actually RUN — we assert cargo reported
#      "1 passed" for the generator (0 matched / filtered-out => FAIL).
#   2. MANIFEST.json must report case_count > 0 AND its mtime must have advanced
#      during this run (proof the generator wrote, not a stale tree).
#   3. `git diff --exit-code` over the corpus tree must be clean (zero drift).
#
# A deliberate hook.rs behaviour change => run scripts/regen-parity.sh locally,
# review and commit the diff. This job then turns green again.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PARITY_DIR="$ROOT/packages/unimatrix/test/fixtures/parity"
MANIFEST="$PARITY_DIR/MANIFEST.json"
GENERATOR_TEST="generate_parity_corpus"

if [ ! -f "$MANIFEST" ]; then
  echo "drift-check FAIL: $MANIFEST missing — corpus never generated" >&2
  exit 1
fi

# Guard 2a: record MANIFEST mtime before regenerating (epoch seconds, portable).
mtime_before=$(node -e "process.stdout.write(String(require('fs').statSync(process.argv[1]).mtimeMs))" "$MANIFEST")

# Run the generator, capturing output so we can prove it executed (guard 1).
echo "drift-check: regenerating parity corpus from the Rust oracle…"
gen_out="$(UNIMATRIX_PARITY_DIR="$PARITY_DIR" \
  cargo test -p unimatrix-server --lib "$GENERATOR_TEST" -- --ignored 2>&1)"
echo "$gen_out"

# Guard 1: the generator test must have run and passed. The cargo summary line
# for the binary that contains the test reads "test result: ok. 1 passed; …".
# "0 passed" (filtered out) or absence of the test name => non-vacuity failure.
if ! grep -qE "test result: ok\. 1 passed" <<<"$gen_out"; then
  echo "drift-check FAIL (R-20 non-vacuity): generator '$GENERATOR_TEST' did not report '1 passed'." >&2
  echo "  The drift check must run the generator; 0 matched/filtered tests is a hard failure." >&2
  exit 1
fi
if ! grep -q "$GENERATOR_TEST" <<<"$gen_out"; then
  echo "drift-check FAIL (R-20 non-vacuity): generator '$GENERATOR_TEST' name not in cargo output." >&2
  exit 1
fi

# Guard 2b: MANIFEST mtime must have advanced (proof of a fresh write).
mtime_after=$(node -e "process.stdout.write(String(require('fs').statSync(process.argv[1]).mtimeMs))" "$MANIFEST")
case_count=$(node -e "process.stdout.write(String(require(process.argv[1]).case_count||0))" "$MANIFEST")
echo "drift-check: MANIFEST case_count=$case_count mtime_before=$mtime_before mtime_after=$mtime_after"
if [ "$case_count" -le 0 ]; then
  echo "drift-check FAIL: MANIFEST case_count is $case_count (must be > 0)." >&2
  exit 1
fi
if ! node -e "process.exit(Number(process.argv[1]) > Number(process.argv[2]) ? 0 : 1)" "$mtime_after" "$mtime_before"; then
  echo "drift-check FAIL: MANIFEST mtime did not advance — generator did not write." >&2
  exit 1
fi

# Guard 3: zero drift between regenerated goldens and the committed tree.
if ! git diff --exit-code "$PARITY_DIR"; then
  echo "" >&2
  echo "drift-check FAIL: parity corpus drifted from the Rust oracle." >&2
  echo "  A deliberate hook.rs change? Run scripts/regen-parity.sh, review and commit the diff." >&2
  exit 1
fi

echo "drift-check OK: zero drift; generator ran ($case_count cases); MANIFEST fresh."
