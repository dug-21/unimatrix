# Gate 3b Report: crt-058

> Gate: 3b (Code Review)
> Date: 2026-07-05
> Result: PASS (2 WARN)

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Pseudocode fidelity | PASS | Helper, handler step 6.5, formatter, audit-emit all match validated pseudocode line-for-line |
| 2. Architecture compliance | PASS | Tick production code UNCHANGED; chokepoint-only; ADR-001/002/003/004 followed |
| 3. Interface implementation | PASS | All signatures match Integration Surface; `Option<u64>` threaded; LOCKED predicate exact |
| 4. Test case alignment | PASS | Every test-plan scenario + risk-strategy risk (R-01…R-11) has a corresponding test |
| 5. Code quality | PASS (WARN) | Builds clean, clippy clean, 4398 lib tests pass; no stubs/`todo!`; no non-test `.unwrap()`. WARN: `edge_write.rs` = 588 lines |
| 6. Security | PASS | Parameterized SQL (`?1`/`?2`=const); metadata via serde encoder; no new deps; no path/injection/secret surface |
| 7. Knowledge stewardship | PASS | Agents 3/4/5 reports each carry `## Knowledge Stewardship` with Queried + Stored/"nothing novel" |

**Load-bearing checks (spawn prompt):** 1–4 PASS; 5 PASS (subset) with one carry-forward WARN.

## Detailed Findings

### Load-bearing check 1 — LOCKED eager predicate
**Status**: PASS
**Evidence**: `edge_write.rs:346-355`. Single statement:
`DELETE FROM graph_edges WHERE (source_id = ?1 OR target_id = ?1) AND source = ?2 RETURNING source_id, target_id, relation_type`, `?1 = entry_id as i64`, `?2 = EDGE_SOURCE_AGENT`, on `store.write_pool_server()`, one `fetch_all`. Count is `Some(tuples.len() as u64)` in the handler (`tools.rs`), never `rows_affected()`. No relation_type widening, no runtime `superseded_by` clause. Pinned behaviorally by `test_helper_predicate_and_pool_are_locked` (include_str! of the verbatim WHERE + RETURNING + the `AND source = ?2 \` terminator guard).

### Load-bearing check 2 — Step 6.5 placement
**Status**: PASS
**Evidence**: `tools.rs` diff. Step-5 idempotency early-return now passes `None` and returns before 6.5. Identity clones (`agent_id_for_cleanup`, `session_id_for_cleanup`, `attribution_for_cleanup`) captured BEFORE the step-6 flip `AuditEvent` construction that moves `ctx.agent_id`. 6.5 sits after the flip (`deprecate_with_audit`). Non-fatal: `Err(e) => tracing::warn!(entry, error, ...)` (warn, not debug) + `None`; `Ok(tuples) => Some(tuples.len())`, `emit_edge_cleanup_audit` only under `if !tuples.is_empty()`. Step 8 threads `edges_removed`.

### Load-bearing check 3 — `emit_edge_cleanup_audit`
**Status**: PASS
**Evidence**: `server.rs:659-724`. `operation: "context_deprecate.edge_cleanup"` (distinct from flip). `metadata = serde_json::to_string(removed)`; on serialize `Err` → `warn!` + early `return` (event skipped, never a `"{}"` sentinel on non-empty). `target_ids: vec![entry_id]`, defensive `if removed.is_empty() { return; }`. Verified by `test_edge_cleanup_audit_metadata_not_sentinel_on_nonempty`, `..._tuple_set_equality`, `test_flip_and_cleanup_are_two_distinct_records`.

### Load-bearing check 4 — `edges_removed` plumbing
**Status**: PASS
**Evidence**: `mutations.rs:16-112`. `format_status_change` gains `edges_removed: Option<u64>` before `format`; `format_deprecate_success` forwards it; `format_quarantine_success`/`format_restore_success` hardcode `None`. Summary/Markdown append advisory only on `Some(n)` (incl. `Some(0)`); Json inserts key on a mutable `Value` only for `Some` (absent, not null, for `None`). Behavioral per-format matrix present for `Some(3)`, `Some(0)`, `None`, plus `Some(0)`≠`None` discriminator and quarantine/restore byte-identity tests.

### Load-bearing check 5 — AC-10 subset test
**Status**: PASS (subset) with carry-forward WARN on the chokepoint half
**Evidence**: `background.rs` `test_deprecate_eager_subset_of_tick_and_exactly_agent_edges` seeds fixtures A/B from the ONE shared `seed_all_source_edges` helper, asserts pre-deprecation fixture identity (14 edges each), runs the REAL `delete_agent_edges_for_entry` → R and the REAL `run_orphaned_edge_compaction` → T, asserts `R == exactly the two agent edges` AND `R ⊆ T` AND `T.len() == 14`. Any eager widening or tick narrowing breaks it.
`test_successor_bearing_edge_repointed_by_tick_but_eager_would_destroy` covers the negative-mutation / R-06 hazard (tick Phase 1 repoints the inbound agent edge; the unguarded helper would destroy it).
**Gap (WARN, not FAIL)**: the chokepoint-exclusion "via the REAL `context_correct` handler" (drive correction, assert NO `edge_cleanup` audit, inbound survives) is deferred to the Stage-3c Python suite — documented in-code, because the `#[tool] context_correct`/`context_deprecate` methods are not unit-constructible (no `RequestContext`). The unit layer proves the hazard via a seeded successor fixture, not the production correction route. **Gate 3c MUST enforce the Python chokepoint-exclusion.**

### Check 5 — Code quality (WARN)
**Status**: PASS with WARN
**Evidence**: `cargo build -p unimatrix-server` clean; `cargo clippy -p unimatrix-server --all-targets` zero warnings; `cargo test -p unimatrix-server --lib` = 4398 passed / 0 failed (34 crt-058 tests confirmed present and green). No `todo!`/`unimplemented!`/`FIXME`; no `.unwrap()` in non-test code (formatter uses `to_string_pretty(...).unwrap_or_default()`, the established pattern).
**WARN — 500-line rule**: `mcp/edge_write.rs` is **588 lines** (was 493 pre-feature). Production code is ~505 lines; the overage is the inline `#[cfg(test)] mod tests` (Display unit tests, ~73 lines) — the DB-integration tests were already extracted to `edge_write_delete_agent_tests.rs` (338 lines) to manage size. Classed WARN not FAIL because: (a) every sibling file this feature touches is far larger and grandfathered (`tools.rs` 13240, `background.rs` 5075, `server.rs` 4436, `response/mod.rs` 2164) so the codebase does not enforce 500 literally; (b) the overage is a test module the spirit of the rule excludes; (c) fix is a mechanical `#[path]` extraction of the inline Display tests. Recommend extracting them to bring the file back under 500.

### Check 6 — Security
**Status**: PASS
**Evidence**: only external input reaching the delete is `entry_id`, bound as `?1`; `source` is bound as `?2` to the `EDGE_SOURCE_AGENT` const — no injection surface. Audit metadata built via `serde_json::to_string` (encoder-escaped), proven by `test_..._wellformed_with_unusual_relation_type` (embedded quotes/backslashes/newlines/commas round-trip intact). No `Cargo.toml`/`Cargo.lock` change → `cargo audit` CVE surface unchanged by this feature. No hardcoded secrets, no path/file/deserialization surface.

### Check 2 — Tick unchanged
**Status**: PASS
**Evidence**: `background.rs` diff touches only (a) hoisting the existing test-only `insert_graph_edge_with_source` helper out of `mod tests` to `pub(crate)` (fixture-identity sharing — R-02) and (b) appending the crt-058 test module. `run_orphaned_edge_compaction` and its Phase 1/Phase 2 predicates are untouched (grep of the diff for the fn/predicate returns nothing). FR-08 / AC-08 hold.

## Wave B deferrals — assessment

Both deferrals flagged by the Wave B agent are **legitimate** for Gate 3b:

1. **R-02 literal predicate-string const not extracted** — covered behaviorally by two independent guards: the AC-10 exact-set assertion (`R == the two agent edges`) over both real functions, and `test_helper_predicate_and_pool_are_locked`'s `include_str!` pin of the verbatim WHERE/RETURNING text plus the `AND source = ?2 \` line-terminator (blocks any appended clause). A named const would add no coverage the pin doesn't already give.

2. **AC-06 fault-injection + AC-07/AC-10 handler halves deferred to Stage 3c Python** — the `#[tool]` methods are not unit-constructible (no `RequestContext`), an established constraint. The unit layer covers everything reachable at the seam: helper atomicity/predicate/per-source/self-loop/high-degree/zero-row; audit record content/tuple-equality/sentinel/distinctness/empty-guard; formatter `Some(n)`/`Some(0)`/`None` per-format matrix + backward-compat. What genuinely needs the handler (end-to-end route, warn+success on injected delete failure, no `edge_cleanup` audit on the correction path, synchronous absence on return through the real tool) is correctly routed to 3c.

## Rework Required

None (gate PASSES). Two carry-forwards for tracking:

| Item | Owner | Action |
|------|-------|--------|
| WARN: `edge_write.rs` 588 lines | uni-rust-dev (optional/mechanical) | Extract inline `mod tests` (Display units) to a `#[path]` file to return under 500 |
| WARN: AC-10 chokepoint-exclusion real-handler half | Stage 3c (uni-tester, Python) | Drive real `context_correct`, assert NO `context_deprecate.edge_cleanup` audit + inbound edge survives; plus AC-06 injected-failure + AC-07 handler halves. Gate 3c must verify these exist and pass. |

## Knowledge Stewardship
- Stored: nothing novel to store -- no recurring cross-feature gate-failure pattern surfaced; this gate passed with only a mechanical file-size WARN and legitimate unit-constructibility deferrals, both already-known classes (500-line rule; `#[tool]` RequestContext non-constructibility). Feature-specific results live in this gate report.
