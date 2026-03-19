# CI grep Enforcement — Verification Plan

**Component**: CI pipeline (`.github/workflows/` or `xtask`)
**Risks addressed**: R-04, R-06, R-09, R-11
**AC addressed**: AC-07, AC-01

The CI enforcement step is a static analysis gate that runs on every PR against main.
It is not a unit test — it is a shell script (or Makefile target / xtask) embedded
in the CI workflow. Stage 3c verifies that the step exists, runs, and passes.

---

## §spawn-blocking-grep — No spawn_blocking at Inference Sites (AC-07, R-06)

### What the CI Step Must Check

```bash
# Step: reject any spawn_blocking in the inference service files
INFERENCE_FILES=(
  "crates/unimatrix-server/src/services/search.rs"
  "crates/unimatrix-server/src/services/store_ops.rs"
  "crates/unimatrix-server/src/services/store_correct.rs"
  "crates/unimatrix-server/src/services/status.rs"
)

for f in "${INFERENCE_FILES[@]}"; do
  count=$(grep -c "spawn_blocking" "$f" 2>/dev/null || echo 0)
  if [ "$count" -gt 0 ]; then
    echo "FAIL: spawn_blocking found in $f ($count occurrence(s))"
    exit 1
  fi
done
```

`services/` files have no permitted `spawn_blocking` after migration (all were inference sites).

```bash
# Step: reject inference-path spawn_blocking in background.rs
# background.rs has PERMITTED spawn_blocking (run_extraction_rules ~1088, persist_shadow_evaluations ~1144)
# The grep must be targeted to the embedding call sites, not the whole file.
# Strategy: check for spawn_blocking immediately before or containing embed_entry/embed_entries

count=$(grep -n "spawn_blocking" crates/unimatrix-server/src/background.rs | \
  grep -v "run_extraction_rules\|persist_shadow_evaluations\|registry\|audit" | wc -l)
if [ "$count" -gt 0 ]; then
  echo "FAIL: unexpected spawn_blocking at inference site in background.rs"
  exit 1
fi
```

Note: the exact grep pattern for `background.rs` requires context-aware matching. The CI
step implementer should validate the pattern against the post-migration file to confirm
no false positives. If the pattern is too broad (catches permitted sites), adjust to
match by proximity to `embed_entry` instead of by file-level grep.

### Verification in Stage 3c

Run the CI grep step directly:
```bash
# From workspace root
grep -rn "spawn_blocking" \
  crates/unimatrix-server/src/services/search.rs \
  crates/unimatrix-server/src/services/store_ops.rs \
  crates/unimatrix-server/src/services/store_correct.rs \
  crates/unimatrix-server/src/services/status.rs
# Expected: zero results → AC-07 passed
```

```bash
# Additional: no spawn_blocking_with_timeout in services/ (replaced by spawn_with_timeout)
grep -rn "spawn_blocking_with_timeout" crates/unimatrix-server/src/services/
# Expected: zero results
```

```bash
# async_wrappers.rs no longer contains AsyncEmbedService spawn_blocking calls
grep -c "spawn_blocking" crates/unimatrix-core/src/async_wrappers.rs
# Expected: 7 (AsyncVectorStore methods only — see async_embed_removal.md §spawn-blocking-in-core)
# The exact count depends on the number of AsyncVectorStore methods
```

---

## §crate-boundary — Rayon Confined to unimatrix-server (AC-01, ADR-001)

```bash
# unimatrix-server must have rayon
cargo tree -p unimatrix-server 2>/dev/null | grep " rayon "
# Expected: at least 1 line containing "rayon"

# No other workspace crate must have rayon
for crate in unimatrix-core unimatrix-embed unimatrix-vector unimatrix-store; do
  result=$(cargo tree -p "$crate" 2>/dev/null | grep " rayon " | wc -l)
  if [ "$result" -gt 0 ]; then
    echo "FAIL: rayon found in $crate — violates ADR-001 (C-01)"
    exit 1
  fi
done
```

Stage 3c runs this as a shell command to confirm AC-01.

---

## §cargo-toml-check — Rayon Version Specifier (R-11)

```bash
grep "rayon" crates/unimatrix-server/Cargo.toml
# Expected: exactly one line: rayon = "1"
# Must NOT be: rayon = "*" or rayon = ">= 1" or rayon = "1.0" (ambiguous resolution)
```

`"1"` semver-pins to the rayon 1.x line. Cargo will not resolve to 2.x without an
explicit version bump. This is the correct specifier.

---

## §ci-step-existence — Workflow File Check (AC-07)

The CI step must exist in a workflow file that runs on PRs against main.

```bash
# Verify the enforcement step is present in CI
grep -rn "spawn_blocking" .github/workflows/
# Expected: at least 1 result — the CI step grep command

# Verify the step runs on PR events
grep -rn "pull_request" .github/workflows/
# Expected: the workflow containing the spawn_blocking check is triggered by pull_request
```

If the enforcement is implemented as an xtask, adapt the search accordingly:
```bash
grep -rn "spawn_blocking" xtask/ 2>/dev/null
```

---

## §no-wrap-in-macro — Macro Expansion Limitation

The CI grep step operates on source text, not expanded macros. A `spawn_blocking` call
inside a declarative macro expansion would not be caught. This is documented as a known
limitation in RISK-TEST-STRATEGY.md §Security Risks (CI grep step bypass).

Mitigation: code review of the implementation PR is the primary control for macro-hidden
calls. The grep step is defence-in-depth.

No test scenario is added for this limitation — it is a known gap, documented here for
transparency in the RISK-COVERAGE-REPORT.md.

---

## §step-pass-on-clean-code — Regression Prevention

In Stage 3c, verify the CI step produces a zero exit code on the post-migration codebase:

```bash
# Run the full CI grep check manually to confirm it passes
! grep -rn "spawn_blocking" \
    crates/unimatrix-server/src/services/search.rs \
    crates/unimatrix-server/src/services/store_ops.rs \
    crates/unimatrix-server/src/services/store_correct.rs \
    crates/unimatrix-server/src/services/status.rs
echo "AC-07 grep check: PASS (exit $?)"
```

A non-zero exit from the grep inverted with `!` means grep found no matches — which is
the desired outcome. Document this result explicitly in the RISK-COVERAGE-REPORT.md.

---

## Summary of CI Checks

| Check | Command | Expected | AC |
|-------|---------|----------|----|
| No spawn_blocking in services/ | grep | 0 results | AC-07 |
| No spawn_blocking_with_timeout in services/ | grep | 0 results | AC-06 |
| rayon in unimatrix-server Cargo.toml | grep | "rayon = \"1\"" | AC-01 |
| rayon NOT in other crates | cargo tree | 0 rayon lines | AC-01 |
| CI step exists in workflow | grep .github/ | ≥1 result | AC-07 |
| Workflow triggers on pull_request | grep .github/ | ≥1 result | AC-07 |
