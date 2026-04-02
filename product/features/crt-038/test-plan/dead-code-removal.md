# Test Plan: Dead-Code Removal (Components 3, 4, 5)

**Components**:
- Component 3: `run_post_store_nli` removal — `nli_detection.rs` + `store_ops.rs` + `mod.rs`
- Component 4: `maybe_run_bootstrap_promotion` removal — `nli_detection.rs` + `background.rs`
- Component 5: NLI auto-quarantine removal — `background.rs`

**AC Coverage**: AC-03, AC-04, AC-05, AC-06, AC-07, AC-08, AC-09, AC-10, AC-11, AC-13, AC-14
**Risk Coverage**: R-04, R-05, R-06, R-07, R-08, R-11
**Wave**: Wave 2 (single agent, sequenced after Wave 1 and eval gate)

---

## Testing Strategy

Dead-code removal has no positive test coverage — there is nothing new to assert. The testing strategy is **absence verification**: confirm deleted symbols are gone, confirm compilation succeeds, confirm retained symbols are intact, confirm deleted tests are deleted.

Testing proceeds in three steps:
1. Symbol checklist grep-verification (absence)
2. Retained symbol verification (presence)
3. Build + test suite confirmation

---

## Step 1: Symbol Absence Checklist

Run each of these grep commands. Every command must return **zero matches** before marking the corresponding AC complete.

### Component 3: run_post_store_nli (AC-03, AC-04, AC-14)

```bash
# AC-03: run_post_store_nli deleted from nli_detection.rs
grep -r "run_post_store_nli" /workspaces/unimatrix/crates/
# Expected: zero output

# AC-04: spawn block and import removed from store_ops.rs
grep -n "tokio::spawn.*nli\|run_post_store_nli" \
  /workspaces/unimatrix/crates/unimatrix-server/src/services/store_ops.rs
# Expected: zero output

# AC-14: NliStoreConfig deleted entirely (struct, fields, import, construction)
grep -r "NliStoreConfig" /workspaces/unimatrix/crates/
# Expected: zero output

grep -r "nli_store_cfg" /workspaces/unimatrix/crates/
# Expected: zero output
```

### Component 4: maybe_run_bootstrap_promotion (AC-05, AC-06)

```bash
# AC-05: maybe_run_bootstrap_promotion and run_bootstrap_promotion deleted
grep -r "maybe_run_bootstrap_promotion\|run_bootstrap_promotion" /workspaces/unimatrix/crates/
# Expected: zero output

# AC-06: background.rs import and call site removed; stale comment removed
grep -n "maybe_run_bootstrap_promotion" \
  /workspaces/unimatrix/crates/unimatrix-server/src/background.rs
# Expected: zero output (covers import, call site, and any comment referencing it)
```

### Component 5: NLI auto-quarantine (AC-07, AC-08)

```bash
# AC-07: nli_auto_quarantine_allowed and NliQuarantineCheck deleted
grep -r "nli_auto_quarantine_allowed\|NliQuarantineCheck" /workspaces/unimatrix/crates/
# Expected: zero output

# AC-08: process_auto_quarantine signature updated (no nli_enabled or nli_auto_quarantine_threshold params)
grep -n "nli_enabled\|nli_auto_quarantine_threshold" \
  /workspaces/unimatrix/crates/unimatrix-server/src/background.rs
# Expected: zero output in the process_auto_quarantine signature context
# Note: nli_enabled may still appear in other contexts (InferenceConfig field access)
# Refine: grep for the parameter declarations specifically
grep -n "nli_enabled: bool\|nli_auto_quarantine_threshold: f32" \
  /workspaces/unimatrix/crates/unimatrix-server/src/background.rs
# Expected: zero output
```

### write_edges_with_cap (R-05, AC-11)

```bash
# write_edges_with_cap must be deleted (dead code after run_post_store_nli removal)
grep -r "write_edges_with_cap" /workspaces/unimatrix/crates/
# Expected: zero output
```

### R-11: Stale sequencing comment

```bash
# Comment "Must remain after maybe_run_bootstrap_promotion" must be removed
grep -n "Must remain after maybe_run_bootstrap_promotion\|maybe_run_bootstrap_promotion" \
  /workspaces/unimatrix/crates/unimatrix-server/src/background.rs
# Expected: zero output
```

---

## Step 2: Retained Symbol Verification (AC-13)

Three symbols in `nli_detection.rs` MUST remain. Grep must return exactly three matches (one definition per symbol):

```bash
grep -n "pub.crate.*fn write_nli_edge\|pub.crate.*fn format_nli_metadata\|pub.crate.*fn current_timestamp_secs" \
  /workspaces/unimatrix/crates/unimatrix-server/src/services/nli_detection.rs
# Expected: exactly 3 lines (one for each symbol)
```

Additionally verify the import in `nli_detection_tick.rs` is unchanged:

```bash
grep -n "write_nli_edge\|format_nli_metadata\|current_timestamp_secs" \
  /workspaces/unimatrix/crates/unimatrix-server/src/services/nli_detection_tick.rs
# Expected: one line at line 34 (the use statement)
```

### Visibility check

Confirm the three retained symbols are still `pub(crate)` — they must not have been accidentally made private during a cleanup pass:

```bash
grep -n "fn write_nli_edge\|fn format_nli_metadata\|fn current_timestamp_secs" \
  /workspaces/unimatrix/crates/unimatrix-server/src/services/nli_detection.rs
# Expected output lines must contain "pub(crate)" prefix
```

---

## Step 3: Build and Test Verification

### Incremental build verification (R-06, R-07, R-08)

Run after EACH component removal to catch partial-deletion compile errors early:

```bash
# After Component 3 (run_post_store_nli removal):
cargo build --workspace 2>&1 | tail -20

# After Component 4 (bootstrap promotion removal):
cargo build --workspace 2>&1 | tail -20

# After Component 5 (auto-quarantine removal):
cargo build --workspace 2>&1 | tail -20
```

**Why incremental**: `process_auto_quarantine` definition and call site span ~300 lines within `background.rs`. A partial edit (signature updated, call site not) produces a compile error that may be suppressed in incremental builds targeting a single file. A full workspace build catches cross-function inconsistency.

### Call site inspection for R-08

After removing `nli_enabled` and `nli_auto_quarantine_threshold` parameters from `process_auto_quarantine`, manually inspect the `maintenance_tick` call site:

- Before: `process_auto_quarantine(..., nli_enabled, nli_auto_quarantine_threshold).await`
- After: `process_auto_quarantine(...).await` — two fewer arguments

Assert the argument count at the call site matches the new function signature. This is verified implicitly by `cargo build --workspace` succeeding, but explicit inspection is documented as R-08 coverage.

### Full test suite (AC-09, AC-10)

```bash
cargo test --workspace 2>&1 | tail -30
```

**Expected**: All existing tests pass. Total test count should be reduced by exactly 17 from the pre-crt-038 baseline:
- 13 deleted tests from `nli_detection.rs`
- 4 deleted tests from `background.rs`

If the count reduction differs, a test was accidentally retained or accidentally deleted beyond the 17.

**Pre-crt-038 test count**: Confirm against the MEMORY.md note: 2169 unit + 16 migration + 185 infra-001 = 2370 total. After crt-038, unit test count should be 2169 - 17 = 2152. Verify this arithmetic against the actual count reported by `cargo test --workspace`.

### Clippy (AC-11, R-05)

```bash
cargo clippy --workspace -- -D warnings 2>&1 | tail -20
```

**Expected**: Zero warnings. The most likely failure cause is `write_edges_with_cap` retained as dead code. If clippy reports "function is never used" for any symbol, it is a dead-code residual — delete it and re-run.

---

## Test Symbols Deleted (AC-09)

The following test functions must be absent after removal. Grep-verify each before claiming AC-09 complete.

### nli_detection.rs (13 functions)

```bash
grep -n "fn test_empty_embedding_skips_nli\
\|fn test_nli_not_ready_exits_immediately\
\|fn test_circuit_breaker_stops_at_cap\
\|fn test_circuit_breaker_counts_all_edge_types\
\|fn test_bootstrap_promotion_zero_rows_sets_marker\
\|fn test_maybe_bootstrap_promotion_skips_if_marker_present\
\|fn test_maybe_bootstrap_promotion_defers_when_nli_not_ready\
\|fn test_bootstrap_promotion_confirms_above_threshold\
\|fn test_bootstrap_promotion_refutes_below_threshold\
\|fn test_bootstrap_promotion_idempotent_second_run_no_duplicates\
\|fn test_bootstrap_promotion_nli_inference_runs_on_rayon_thread" \
  /workspaces/unimatrix/crates/unimatrix-server/src/services/nli_detection.rs
# Expected: zero output
```

Note: The spec lists 11 names above plus "2 additional test functions covering run_post_store_nli — confirm all 13 present in spec deleted." Stage 3b must identify and include those 2 additional functions. Stage 3c must confirm all 13 are absent.

### background.rs (4 integration tests)

```bash
grep -n "fn test_nli_edges_below_auto_quarantine_threshold_no_quarantine\
\|fn test_nli_edges_above_threshold_allow_quarantine\
\|fn test_nli_auto_quarantine_mixed_penalty_allowed\
\|fn test_nli_auto_quarantine_no_edges_allowed" \
  /workspaces/unimatrix/crates/unimatrix-server/src/background.rs
# Expected: zero output
```

---

## Module Boundary Safety (R-04)

**Risk**: Deleting `nli_detection.rs` functions that share a `#[cfg(test)]` module with retained tests. A line-range deletion that accidentally removes a `#[cfg(test)]` or `mod tests {` boundary silently disables all remaining tests in the module — no compile error, just a reduced test count.

**Verification protocol**:
1. Before deletion: note the test count in `nli_detection.rs` by running `grep -c "^    fn test_" crates/unimatrix-server/src/services/nli_detection.rs`.
2. After deletion: run `grep -c "^    fn test_" crates/unimatrix-server/src/services/nli_detection.rs` again.
3. The count must be (pre-count - 13). If it is 0 and pre-count was not 13, a module boundary was accidentally removed.
4. Confirm `cargo test --workspace` includes tests from `nli_detection.rs` in its output (not just 0 tests from that module).

---

## Integration Test Behavior After Removal

The infra-001 suites exercise `context_store`, `context_search`, and `context_briefing` through MCP JSON-RPC. After dead-code removal:

- **context_store**: No longer spawns a background NLI task. The MCP response timing may be marginally faster. The response format is identical. Integration tests pass unchanged.
- **context_search / context_briefing**: No change to response format or tool behavior. Integration tests pass unchanged.
- **Background tick (maintenance_tick)**: `process_auto_quarantine` runs without NLI guard. No MCP tool exposes this directly. Integration tests are not affected.

The infra-001 suites that must pass after dead-code removal:
- `smoke` (mandatory gate)
- `tools` (confirms search/store/briefing tools work)
- `lifecycle` (confirms multi-step flows)
- `edge_cases` (confirms boundary behavior)

---

## Failure Modes

**Partial deletion of NliStoreConfig (R-07)**: `store_ops.rs` struct deleted but `mod.rs` import left behind → compile error "unresolved import". Fix: delete the import on line 26 of `mod.rs` and the `nli_store_cfg` construction block.

**write_edges_with_cap retained (R-05)**: `cargo clippy --workspace -- -D warnings` reports "function is never used". Fix: delete `write_edges_with_cap` from `nli_detection.rs` — it had one caller (`run_post_store_nli`) which is already deleted.

**process_auto_quarantine call site not updated (R-08)**: `cargo build --workspace` fails with "this function takes N arguments but M were supplied". Fix: remove the two dropped arguments (`nli_enabled` and `nli_auto_quarantine_threshold`) from the `maintenance_tick` call site.

**Retained symbol made private (R-04)**: `nli_detection_tick.rs` fails to compile with "function `write_nli_edge` is private". Fix: restore `pub(crate)` visibility on the affected function.

**13 nli_detection tests not fully removed**: `cargo test --workspace` test count is higher than expected. Scan for remaining test function names from the deleted list. The SPECIFICATION.md note about "2 additional functions" means Stage 3b must identify both from the live source before deleting.
