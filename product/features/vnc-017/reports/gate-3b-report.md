# Gate 3b Report: vnc-017

> Gate: 3b (Code Review)
> Date: 2026-05-18
> Result: REWORKABLE FAIL

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Pseudocode fidelity | PASS | All three components match validated pseudocode exactly |
| Architecture compliance | PASS | REDIRECT_CEILING, run_redirect_loop extracted per ADR-004; NFR-09 intact |
| Interface implementation | PASS | Signatures, types, error handling all match architecture contracts |
| Test case alignment | FAIL | AC-04 stub test (redirect_graph_edge Err path) missing; R-04 deferred (accepted at 3a) |
| Code quality | PASS | Builds clean; no stubs, no .unwrap() in non-test code; entries.rs at 476 lines |
| Security | PASS | Parameterized SQL, no secrets, no path traversal vectors |
| Knowledge stewardship | FAIL | No agent report for the store-layer rust-dev agent (query_incoming_edges / IncomingEdgeRow implementation) |

## Detailed Findings

### Pseudocode Fidelity

**Status**: PASS

**Evidence**:

`query_incoming_edges` (`read.rs` lines 1694–1730): Matches pseudocode exactly — `read_pool()` used, `target_id` cast to `i64` for binding, rows deserialized via `try_get::<i64, _>` with cast back to `u64`, `StoreError::Database` error mapping, SQL excludes Supersedes at query level with required explanatory comment. C-07 comment present at call site (`db.rs:294` reference). `IncomingEdgeRow` struct matches: `{ source_id: u64, relation_type: String, created_at: u64 }`.

`run_redirect_loop` (`tools.rs` lines 4436–4579): Matches pseudocode exactly. Ceiling check at `REDIRECT_CEILING=50` with `tracing::warn!`. Source-validation guard using `store.get()` with Quarantined/Deprecated match arm (skipped++, not failed++). Source lookup Err also treated as skipped. `redirect_graph_edge` `Ok(())` → redirected++, `Err` → warn+failed++. `tracing::info!` summary after loop.

`response_format` (`tools.rs` lines 1108–1125, `entries.rs` lines 265–298): `format_redirect_summary` extracted to `entries.rs` as a free function (preferred minimal-impact approach). The function returns `Option<String>` gated on `found > 0`. All four FR-10 variants implemented correctly including em-dash U+2014 in skipped variant. Response append uses `rmcp::model::RawContent::Text` match to mutate `t.text` — confirmed valid against actual rmcp API.

Step 8c insertion in `context_correct` (lines 1085–1094): `run_redirect_loop` called after Phase B `validate_and_write_edges` block closes, before confidence recompute at step 9. No `find_terminal_active`, no `TypedGraphState` access.

---

### Architecture Compliance

**Status**: PASS

**Evidence**:

- `REDIRECT_CEILING: usize = 50` declared at module level with ADR-004 doc comment.
- `run_redirect_loop` is `pub(super)` — accessible from sibling test module, not exported to crate boundary.
- `RedirectSummary` is `pub(super)` with all six fields matching pseudocode OVERVIEW.md exactly.
- `format_redirect_summary` exported from `response/entries.rs` via `mod.rs` re-export — matches architecture §Component 4 preferred approach.
- `edge_write.rs` unmodified (git log confirms last change was vnc-015). NFR-09 satisfied.
- No `tokio::spawn` in redirect loop — inline synchronous execution per NFR-01.
- No lock on `TypedGraphState` anywhere in the redirect path — NFR-05 satisfied.
- ADR-003 partial-write posture: correction never returns Err due to redirect failures.
- ADR-001: `correct_result.corrected_entry.id` used directly as redirect target; no chain traversal.
- C-07 comment present in both `query_incoming_edges` doc block and at step 8c call site.
- `context_correct` handler: lines 956–1127 = 171 lines. NFR-07 satisfied (well under 500).

---

### Interface Implementation

**Status**: PASS

**Evidence**:

- `query_incoming_edges` signature: `pub async fn query_incoming_edges(&self, target_id: u64) -> Result<Vec<IncomingEdgeRow>>` — exact match.
- `IncomingEdgeRow` is `pub` and exported from `unimatrix-store/src/lib.rs` — cross-crate accessible.
- `run_redirect_loop` calls `redirect_graph_edge(store, source_id, original_id, new_entry_id, &edge.relation_type, edge.created_at)` — all six arguments match the architecture contract.
- `format_redirect_summary` function-level parameters match exactly: `(found, skipped, redirected, failed, truncated, total_raw): (usize, usize, usize, usize, bool, usize) -> Option<String>`.
- All error handling paths follow project patterns (`StoreError::Database`, `tracing::warn!`, `rmcp::ErrorData::invalid_params` not used in redirect path per NFR-01).

---

### Test Case Alignment

**Status**: FAIL

**Evidence**:

The redirect_loop_tests module (`tools.rs` lines 8973–9531) has 11 tests covering:
- AC-11 (zero edges), AC-14 (end-to-end), AC-08 (quarantined source), R-06 (deprecated source), AC-09/R-09 (UNIQUE conflict), R-06 mixed (Active+Quarantined fan-in), R-05 (55 edges ceiling), R-05 variant (exactly 50), R-10 (Phase B double-write), R-01 structural (REDIRECT_CEILING constant), AC-03 (no chain traversal).

`entries.rs` has 8 tests covering all FR-10 format variants: AC-11 (found=0), AC-12 (normal), AC-13 (partial failure), AC-17 (all-skipped), mixed skipped+failed, truncation, all-failed, singular form.

`read.rs` has 6 tests covering: AC-05 (basic return contract), R-02 (Supersedes SQL exclusion), R-03 (high-cardinality), R-07 (Supersedes-only), empty target, mixed exclusion.

**FAIL — AC-04 (correction succeeds when redirect_graph_edge returns Err) is missing.** The test plan (`test-plan/redirect_loop.md` §AC-04) requires a test that:
- Seeds entry A and one incoming edge `C → A`
- Causes `redirect_graph_edge` to return `Err(EdgeRedirectError::TransactionError(...))`
- Asserts: handler returns well-formed success response, `deprecated_original` and `corrected_entry` fields present, `failed == 1`, `tracing::warn!` emitted

The risk strategy (line 273) marks this "Yes — AC-04 stub test" and the spec FR-08 requires that redirect failures do not abort the correction. No such test exists in the redirect_loop_tests module. The implementation logic is correct (warn+continue) but the test for this critical failure path is absent.

**WARN — R-04 (store.get call count assertion) not implemented.** This was acknowledged at Gate 3a as "Partial coverage, mock pattern TBD." No mock/call-count test was added. Behavioral correctness of per-edge source validation is covered by the existing tests; the strict call-count assertion remains absent. Not blocking — R-04 is a "High" risk but its behavioral outcome (correct skip behavior) is tested.

**WARN — FR-09 Supersedes exclusion count not in tracing::info! summary.** The spec FR-09 lists "edges skipped (Supersedes exclusion)" as a required log field. The implementation logs `found`, `redirected`, `skipped`, `failed`, `truncated`, `total_raw` but not a `supersedes_excluded` count (which would always be zero since the SQL excludes them). This was accepted at Gate 3a without flagging. The information is zero by construction and carrying it would require tracking it at the query layer. Minor spec literal vs. design-intent discrepancy.

---

### Code Quality

**Status**: PASS

**Evidence**:

- `cargo build --workspace` completed with no errors. 19 warnings in unimatrix-server are pre-existing (not introduced by vnc-017).
- All test suites pass: 3006 unimatrix-server tests passed (including 11 new redirect_loop_tests and 8 new entries.rs tests), 73 unimatrix-store tests passed (including 6 new query_incoming_edges tests).
- No `todo!()`, `unimplemented!()`, TODO, or FIXME in the new code sections.
- No `.unwrap()` in non-test code paths for query_incoming_edges or run_redirect_loop.
- `entries.rs`: 476 lines (under 500). `tools.rs`: 9531 lines (pre-existing large file — additions are ~150 lines of implementation + 11 tests; rule applies to new modules, not additions). `read.rs`: 3765 lines (pre-existing large file; addition is ~95 lines including tests).
- `context_correct` handler: 171 lines total. NFR-07 satisfied.

---

### Security

**Status**: PASS

**Evidence**:

- `query_incoming_edges` uses `sqlx` parameterized query with `?1` placeholder — no SQL injection vector. The `relation_type` value returned from the database (trusted source) is passed to `redirect_graph_edge` without modification.
- No hardcoded secrets, API keys, or credentials in any new code.
- No user-controlled data flows into the SQL query — `target_id` comes from `params.original_id` which is validated by `validate_correct_params` before reaching the redirect logic.
- Fan-in ceiling (REDIRECT_CEILING=50) prevents crafted hub entries from causing unbounded inline latency.
- No path traversal vectors (no file system operations in vnc-017 scope).
- `cargo audit` not installed in this environment; no new dependencies were added by this feature (confirmed by architecture: no new crates, only existing `sqlx` and `tracing`).

---

### Knowledge Stewardship

**Status**: FAIL

**Evidence**:

Two rust-dev agent reports exist: `vnc-017-agent-4-response-format-report.md` (covers `entries.rs` and `mod.rs` changes) and `vnc-017-agent-5-redirect-loop-report.md` (covers `tools.rs` changes). Both contain proper `## Knowledge Stewardship` sections with `Queried:` and `Stored:` entries.

**FAIL — No agent report exists for the store-layer rust-dev agent** that implemented `query_incoming_edges` and `IncomingEdgeRow` in `crates/unimatrix-store/src/read.rs`. This implementation was delivered as a separate git commit (`41168600`) with its own PR commit message documenting 6 unit tests, but no corresponding agent report was written to `product/features/vnc-017/agents/`. The gate 3b rule requires each rust-dev agent report to contain a Knowledge Stewardship section. A report exists for the implementation but was not filed.

The design-phase agents (agents 1–3) all have `Queried:` entries. Agent 2-spec has "No new patterns stored (specification decisions are feature-specific)" as its `Stored:` justification. Agent 2-testplan has "nothing novel to store" with explicit reason. Agent 3-risk has "Stored: nothing novel to store" with reason. Agent 1-pseudocode has `Queried:` entries but no `Stored:` or "nothing novel" declaration — this is a WARN per the gate spec ("present but no reason after 'nothing novel' = WARN").

---

## Rework Required

| Issue | Which Agent | What to Fix |
|-------|-------------|-------------|
| Missing AC-04 test (redirect_graph_edge Err path) | rust-dev (store/server) | Add `test_redirect_loop_correction_succeeds_when_redirect_fails` to `redirect_loop_tests`: seed entry A + one incoming edge C→A; drop the `graph_edges` table or use a non-existent source_id that causes SQL error in redirect_graph_edge; assert `failed==1`, `redirected==0`, `run_redirect_loop` returns `Some(rs)` with those counts, no panic or Err propagation |
| Missing store-layer agent report | rust-dev (store) | Create `product/features/vnc-017/agents/vnc-017-agent-3-store-report.md` (or renumber to avoid collision) documenting the `query_incoming_edges` + `IncomingEdgeRow` implementation with `## Knowledge Stewardship` section including `Queried:` and `Stored:` (or "nothing novel to store — {reason}") entries |

## Knowledge Stewardship

- Stored: nothing novel to store — the missing AC-04 test and missing agent report are feature-specific delivery gaps, not cross-feature patterns. The finding that stub-injection tests for `redirect_graph_edge` failure paths are easily forgotten (because the warn+continue posture means the feature works without them) may be worth storing after it recurs. Deferring.
