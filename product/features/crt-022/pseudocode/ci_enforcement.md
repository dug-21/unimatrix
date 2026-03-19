# Component Pseudocode: CI Grep Enforcement

**Files to create / modify**: CI pipeline configuration

---

## Purpose

Add a CI grep step that enforces AC-07: no `spawn_blocking` or
`spawn_blocking_with_timeout` at ONNX embedding inference sites in
`unimatrix-server/src/services/` or `unimatrix-server/src/background.rs`. The step
runs on every PR against main and fails the build if any inference site remains on
`spawn_blocking`.

This is a shell script or CI pipeline step — no Rust code changes.

---

## Context: CI Pipeline Shape

The existing CI is a release workflow (`.github/workflows/release.yml`). No separate
PR CI workflow file was found in `.github/workflows/`. The step must be added to
the appropriate CI location:

- If a separate PR CI workflow exists (e.g., `ci.yml`, `test.yml`): add the grep step there.
- If only the release workflow exists: add a new `ci.yml` workflow file for PR-triggered checks,
  or add the step to the release workflow's pre-build stage.
- Alternatively: implement as a `cargo xtask check-inference-sites` command that the CI invokes.

The implementer must inspect the actual CI setup and choose the correct location.
The pseudocode below documents the logic regardless of wrapper format.

---

## Step Logic

### Step name

```
"Enforce: no spawn_blocking at ONNX inference sites (AC-07)"
```

### Trigger

Runs on every PR against `main`. Must not be skipped for any PR type.

### Algorithm

```
STEP: enforce_no_spawn_blocking_at_inference_sites

INPUT:
  - TARGET_DIRS = [
      "crates/unimatrix-server/src/services",
      "crates/unimatrix-server/src/background.rs",
    ]
  - SEARCH_PATTERNS = [
      "spawn_blocking_with_timeout",
      "spawn_blocking",       -- also catches spawn_blocking_with_timeout as substring
    ]

PROCEDURE:

1. For each directory/file in TARGET_DIRS:
   a. Run: grep -rn "spawn_blocking" <target>
   b. Collect all matching lines.

2. From the matching lines, EXCLUDE lines that are:
   a. In a comment (line starts with optional whitespace then `//`)
   b. Not at an inference call site — specifically, permitted non-inference sites:
      - services/search.rs: the co-access boost `compute_search_boost` call
        (does not contain `embed_entry` or `embed_entries` in context)
      - Any other non-inference spawn_blocking in services/ or background.rs
        that was present before this feature and is on the permitted list

3. After exclusion: if any matches REMAIN:
   a. Print each remaining match with file, line number, and matched text
   b. Print a diagnostic:
      "ERROR: spawn_blocking found at ONNX inference site(s). These sites must use
       rayon_pool.spawn_with_timeout (MCP handler paths) or rayon_pool.spawn
       (background paths). See crt-022 for the migration pattern."
   c. Exit with non-zero status (fail the CI step)

4. If no matches remain after exclusion:
   a. Print: "OK: no spawn_blocking at inference sites."
   b. Exit 0 (pass)
```

### Simpler alternative: allow-list approach

Because the permitted non-inference `spawn_blocking` calls in `services/` are
few and well-defined, a simpler implementation is:

```
STEP:

1. Check that spawn_blocking does NOT appear in services/*.rs at embedding lines:
   grep -rn "spawn_blocking" crates/unimatrix-server/src/services/ \
     | grep -v "co_access\|compute_search_boost\|// " \
     | grep "embed"

   If this returns any lines → FAIL

2. Check that spawn_blocking does NOT appear in background.rs at embed_entry lines:
   grep -n "spawn_blocking" crates/unimatrix-server/src/background.rs \
     | grep "embed"

   If this returns any lines → FAIL

3. Check that spawn_blocking_with_timeout does NOT appear anywhere in services/:
   grep -rn "spawn_blocking_with_timeout" crates/unimatrix-server/src/services/

   If this returns any lines → FAIL (the wrapper should be fully replaced)

4. Exit 0 if all checks pass.
```

This simpler grep-and-filter approach is less fragile than line-number-based allow-lists
and does not require updating the CI step when new non-inference `spawn_blocking` calls
are added to `background.rs`.

### Additional check: `async_wrappers.rs` is clean

```
5. Check that spawn_blocking does NOT appear in async_wrappers.rs:
   grep -n "spawn_blocking" crates/unimatrix-core/src/async_wrappers.rs

   If this returns any lines → FAIL
   (AsyncEmbedService's spawn_blocking calls have been removed)
```

### Additional check: `embed_handle.rs` retains exactly one `spawn_blocking`

```
6. Check that exactly one spawn_blocking remains in embed_handle.rs:
   COUNT=$(grep -c "spawn_blocking" crates/unimatrix-server/src/infra/embed_handle.rs)
   if [ "$COUNT" -ne 1 ]; then
       echo "ERROR: embed_handle.rs must have exactly 1 spawn_blocking (OnnxProvider::new)."
       echo "       Found: $COUNT"
       exit 1
   fi
```

This check prevents accidental migration of `OnnxProvider::new` to rayon (R-10) AND
prevents accidental addition of new inference-site `spawn_blocking` calls to `embed_handle.rs`.

---

## Shell Script Format (if implemented as a standalone script)

```bash
#!/usr/bin/env bash
set -euo pipefail

FAIL=0

# Check 1: no spawn_blocking for embedding in services/
echo "Checking services/ for spawn_blocking at embedding sites..."
MATCHES=$(grep -rn "spawn_blocking" crates/unimatrix-server/src/services/ \
  | grep "embed" \
  | grep -v "//") || true
if [ -n "$MATCHES" ]; then
    echo "ERROR: spawn_blocking at embedding inference site(s) in services/:"
    echo "$MATCHES"
    FAIL=1
fi

# Check 2: no spawn_blocking_with_timeout in services/ (fully replaced by rayon)
echo "Checking services/ for spawn_blocking_with_timeout..."
MATCHES=$(grep -rn "spawn_blocking_with_timeout" crates/unimatrix-server/src/services/) || true
if [ -n "$MATCHES" ]; then
    echo "ERROR: spawn_blocking_with_timeout must not appear in services/ after crt-022:"
    echo "$MATCHES"
    FAIL=1
fi

# Check 3: no spawn_blocking for embedding in background.rs
echo "Checking background.rs for spawn_blocking at embedding sites..."
MATCHES=$(grep -n "spawn_blocking" crates/unimatrix-server/src/background.rs \
  | grep "embed" \
  | grep -v "//") || true
if [ -n "$MATCHES" ]; then
    echo "ERROR: spawn_blocking at embedding inference site(s) in background.rs:"
    echo "$MATCHES"
    FAIL=1
fi

# Check 4: async_wrappers.rs has no spawn_blocking (AsyncEmbedService removed)
echo "Checking async_wrappers.rs for spawn_blocking..."
MATCHES=$(grep -n "spawn_blocking" crates/unimatrix-core/src/async_wrappers.rs) || true
if [ -n "$MATCHES" ]; then
    echo "ERROR: spawn_blocking found in async_wrappers.rs (AsyncEmbedService should be removed):"
    echo "$MATCHES"
    FAIL=1
fi

# Check 5: embed_handle.rs has exactly 1 spawn_blocking (OnnxProvider::new)
echo "Checking embed_handle.rs for exactly 1 spawn_blocking..."
COUNT=$(grep -c "spawn_blocking" crates/unimatrix-server/src/infra/embed_handle.rs || echo 0)
if [ "$COUNT" -ne 1 ]; then
    echo "ERROR: embed_handle.rs must have exactly 1 spawn_blocking (OnnxProvider::new), found: $COUNT"
    FAIL=1
fi

if [ "$FAIL" -eq 0 ]; then
    echo "OK: all spawn_blocking enforcement checks passed."
else
    echo ""
    echo "See crt-022 for the migration pattern."
    echo "MCP handler paths: rayon_pool.spawn_with_timeout(MCP_HANDLER_TIMEOUT, ...)"
    echo "Background paths:  rayon_pool.spawn(...)"
    exit 1
fi
```

This script can be saved as `scripts/check-inference-sites.sh` and invoked from CI.

---

## CI Workflow Integration

If added as a step in a GitHub Actions workflow:

```yaml
- name: Enforce no spawn_blocking at ONNX inference sites (AC-07 / crt-022)
  run: bash scripts/check-inference-sites.sh
```

If integrated as a `cargo xtask` command:

```yaml
- name: Enforce no spawn_blocking at ONNX inference sites (AC-07 / crt-022)
  run: cargo xtask check-inference-sites
```

The xtask approach wraps the same shell logic in Rust for portability across platforms,
but the grep logic is identical.

---

## Scope Notes

- The step checks `services/` (all `.rs` files) and `background.rs` as specified in AC-07 and C-09.
- The step does NOT check `uds/listener.rs` (warmup) against `spawn_blocking` — the warmup
  `spawn_blocking` was replaced with `spawn_with_timeout`, and no other `spawn_blocking` calls
  in `uds/listener.rs` are inference sites.
- The step is intentionally narrow: it checks for `embed` as a substring to identify inference
  sites, which is specific to ONNX embedding calls. DB calls and rule evaluation calls do not
  use `embed`.
- Macro-expansion hiding of `spawn_blocking` is noted in RISK-TEST-STRATEGY.md as a known
  limitation. Code review of the implementation PR is the primary control for that case.

---

## Error Handling

This is a CI enforcement step, not runtime code. "Error handling" here means:

| Scenario | Behaviour |
|----------|-----------|
| grep finds a match at an inference site | Script exits non-zero; CI step fails; PR is blocked |
| grep returns no matches | Script exits 0; CI step passes |
| Script is not present in CI | AC-07 invariant is not enforced — this is a gap to close |
| grep itself fails (file not found) | Script exits non-zero via `set -e`; CI alerts immediately |

---

## Key Test Scenarios (AC-07, C-09, R-06)

1. **Pre-migration: step fails** (R-06 scenario 1): on the branch before crt-022 is merged,
   run the script; assert it exits non-zero and identifies the 7 inference sites.
   (This validates the script is sensitive enough to detect the problem.)

2. **Post-migration: step passes** (AC-07): after crt-022 is merged, run the script;
   assert it exits 0.

3. **Single regression: step catches it** (R-06 scenario 1): on a branch that re-introduces
   one `spawn_blocking` at an embedding site in `services/`, run the script; assert it
   exits non-zero and names the offending file and line.

4. **Non-inference spawn_blocking ignored** (AC-08 complementary): a branch that adds a new
   non-embedding `spawn_blocking` call in `services/` (e.g., a DB read) does not trigger the
   step (grep filters on `embed`).

5. **`async_wrappers.rs` clean** (R-06 scenario 2): after `AsyncEmbedService` removal, the
   step check on `async_wrappers.rs` passes.

6. **`embed_handle.rs` count check** (R-10, AC-08): the step asserts exactly 1
   `spawn_blocking` in `embed_handle.rs`. If `OnnxProvider::new` is accidentally moved to
   rayon, the count drops to 0 and the step fails.
