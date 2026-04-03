# Security Review: crt-041-security-reviewer

## Risk Level: low

## Summary

The crt-041 graph enrichment implementation (S1/S2/S8 edge sources) is well-constructed from a security standpoint. All five critical security areas from the spawn prompt were verified and found to be correctly implemented. One informational finding is noted: the quarantine guard uses `status = 0` (Active-only) rather than `status != 3` (not-Quarantined), which is consistent with the read.rs and graph cohesion metrics patterns but diverges from co_access_promotion_tick.rs. This is correct behavior and not a defect. No blocking findings.

---

## Findings

### F-01: S2 SQL injection — VERIFIED CLEAN
- **Severity**: n/a (mitigated)
- **Location**: `crates/unimatrix-server/src/services/graph_enrichment_tick.rs:168-203`
- **Description**: The S2 vocabulary matching path uses `sqlx::QueryBuilder::push_bind()` for every vocabulary term in both the e1 and e2 CASE WHEN expressions. A SECURITY comment is present at the construction site (lines 155-160). No vocabulary term is ever interpolated via `format!()` or string concatenation into the SQL body. The `LIMIT` value is also bound via `push_bind`. Two injection tests (`test_s2_sql_injection_single_quote`, `test_s2_sql_injection_double_dash`) both pass and assert graph_edges table survives.
- **Recommendation**: No action required. SECURITY comment is adequate and correctly placed.
- **Blocking**: No

### F-02: S8 JSON deserialization of target_ids — VERIFIED CLEAN
- **Severity**: n/a (mitigated)
- **Location**: `crates/unimatrix-server/src/services/graph_enrichment_tick.rs:311-324`
- **Description**: `serde_json::from_str::<Vec<u64>>()` is used to deserialize `target_ids` from audit_log. Malformed JSON is caught via `Err(e)` match arm. On error: (1) `tracing::warn!` is emitted with the raw `target_ids` content and error, (2) `new_watermark = event_id` advances past the malformed row, (3) processing continues to next rows via `continue 'rows`. This satisfies the R-05/SR-08 requirement: no infinite re-scan, no panic, no silent data loss for adjacent valid rows. Test `test_s8_watermark_advances_past_malformed_json_row` verifies this path with a malformed row sandwiched between two valid rows.
- **Note**: The deserialized type is `Vec<u64>` (unsigned integers only), eliminating negative ID injection. The subsequent quarantine filter (Phase 4) provides a second line of defense: any ID not present as an Active entry is silently dropped before edge writing.
- **Recommendation**: No action required.
- **Blocking**: No

### F-03: SQLite 999-parameter limit for S8 bulk IN clause — VERIFIED CLEAN
- **Severity**: n/a (mitigated)
- **Location**: `crates/unimatrix-server/src/services/graph_enrichment_tick.rs:381-403`
- **Description**: `SQLITE_MAX_VARIABLE_NUMBER = 900` is defined as a module constant (with reference to entry #3442). The quarantine filter uses `all_ids.chunks(SQLITE_MAX_VARIABLE_NUMBER)` to split the ID set before building the `IN (...)` clause. The `qb.separated(", ")` API with `push_bind` is used for each chunk. If any chunk query fails, the entire batch is skipped (not silently partially processed) with a `warn!` log.
- **Recommendation**: No action required. Chunking constant is conservative (900 vs 999 limit) which is correct defensive practice.
- **Blocking**: No

### F-04: Dual-endpoint quarantine guard — VERIFIED CLEAN
- **Severity**: n/a (mitigated)
- **Location**: S1: lines 93-94; S2: lines 196-198; S8: line 382 + lines 411-413
- **Description**: All three sources correctly filter quarantined entries on BOTH endpoints. S1 uses `JOIN entries e1 ON e1.id = t1.entry_id AND e1.status = 0` and `JOIN entries e2 ON e2.id = t2.entry_id AND e2.status = 0`. S2 uses `AND e1.status = 0 AND e2.status = 0` on the JOIN condition. S8 uses a bulk pre-fetch `WHERE status = 0 AND id IN (...)` and then checks `!valid_ids.contains(a) || !valid_ids.contains(b)` before each write. Tests cover both source and target positions for all three sources.
- **Note (informational)**: S1 and S2 use `status = 0` (Active-only) while co_access_promotion_tick.rs uses `status != 3` (not Quarantined, allowing Deprecated=1 and Proposed=2 entries through). The `status = 0` pattern used by crt-041 is consistent with `read.rs:compute_graph_cohesion_metrics()`, the NLI detection tick, and all graph cohesion metric queries. The two approaches produce different results for Deprecated (status=1) and Proposed (status=2) entries — crt-041 excludes them; co_access_promotion_tick includes them. The ARCHITECTURE.md and RISK-TEST-STRATEGY.md both document the intended behavior as Active-only (`status = 0`). This is not a security defect; it is a design choice that is correctly and consistently implemented as specified. The divergence from co_access_promotion_tick.rs is pre-existing and not introduced by crt-041.
- **Recommendation**: No action required for this PR. The Active-only filter is correct per spec.
- **Blocking**: No

### F-05: InferenceConfig validate() zero-value protection — VERIFIED CLEAN
- **Severity**: n/a (mitigated)
- **Location**: `crates/unimatrix-server/src/infra/config.rs:1128-1163`
- **Description**: All four numeric fields (`max_s1_edges_per_tick`, `max_s2_edges_per_tick`, `s8_batch_interval_ticks`, `max_s8_pairs_per_batch`) have range checks with lower bound 1 in `validate()`. The `s8_batch_interval_ticks = 0` case that would cause `current_tick % 0` integer division panic at runtime is explicitly blocked with a `ConfigError::NliFieldOutOfRange`. The run_s8_tick code contains the comment "Gate: s8_batch_interval_ticks >= 1 guaranteed by validate() — no % 0 risk" confirming the design intent. Six config tests (T-CFG-03a through T-CFG-04b) all pass verifying boundary behavior.
- **Recommendation**: No action required.
- **Blocking**: No

### F-06: Hardcoded secrets — VERIFIED CLEAN
- **Severity**: n/a
- **Location**: All changed files
- **Description**: No hardcoded credentials, API keys, tokens, or secrets found in any changed file. The `S8_WATERMARK_KEY = "s8_audit_log_watermark"` constant is a table key name, not a secret. The `SQLITE_MAX_VARIABLE_NUMBER = 900` is a safety constant. The `s2_vocabulary` recommended 9-term example list in a doc comment is not a secret.
- **Blocking**: No

### F-07: No new dependencies — VERIFIED CLEAN
- **Severity**: n/a
- **Description**: The ARCHITECTURE.md explicitly states "No new dependencies. All implementation uses sqlx, serde, and tracing — already in the workspace manifest." Verified: only `use` imports from existing workspace crates (`sqlx`, `serde_json`, `tracing`, `unimatrix_core`, `unimatrix_store`) are present in the new module. No Cargo.toml changes in the diff.
- **Blocking**: No

### F-08: No panic paths in production code — VERIFIED CLEAN
- **Severity**: n/a
- **Location**: `crates/unimatrix-server/src/services/graph_enrichment_tick.rs`
- **Description**: No `panic!`, `unwrap()`, `todo!()`, or `unimplemented!()` macros are present in production code (only in test helper functions where `unwrap()` is acceptable). All fallible operations use explicit `match` or `if let Err(e)` with `warn!` + early return. The module is fully infallible as specified.
- **Blocking**: No

### F-09: write_graph_edge ?6 positional parameter reuse — PRE-EXISTING, NOT INTRODUCED BY crt-041
- **Severity**: informational
- **Location**: `crates/unimatrix-server/src/services/nli_detection.rs:92`
- **Description**: `write_graph_edge` uses `?6` twice in the VALUES clause — for both `created_by` and `source` columns. This is a positional parameter reuse that is valid SQLite syntax (a SQLite `?N` parameter can be referenced multiple times with a single `.bind()` call). The comment on line 99 documents this intent. Six `write_graph_edge` tests all pass confirming the behavior. This pattern was introduced in crt-040, not crt-041, and is pre-existing. Not a security issue.
- **Recommendation**: No action required for this PR.
- **Blocking**: No

---

## Blast Radius Assessment

**Worst case scenario**: If S2 had a bug where vocabulary terms were interpolated rather than bound (the primary injection surface), a malicious term in config.toml could corrupt or drop the `graph_edges` table. However: (1) config.toml requires filesystem write access — an attacker with write access to the config already has full system access, (2) the parameterized binding is structurally verified and test-covered, (3) the `INSERT OR IGNORE` on a UNIQUE constraint limits blast radius to data written in graph_edges only.

**S1/S2 failure mode**: Any SQL error in S1 or S2 causes that source to log `warn!` and return 0. S2 and S8 (respectively) continue unaffected. No tick loop halt. No data loss from prior ticks.

**S8 failure mode**: If the bulk quarantine filter fails, the entire batch is skipped (not partially applied). The watermark does not advance. The same batch is retried next S8 run with INSERT OR IGNORE providing idempotency.

**S8 watermark write-after-commit ordering**: Phase 5 (all edge writes) is guaranteed to complete before Phase 6 (watermark update). A crash between Phase 5 and Phase 6 causes at-most re-processing of the same batch on next run, with INSERT OR IGNORE preventing duplicate edges. No silent data loss.

**Maximum edge count per tick**: S1 and S2 are capped at `max_s1_edges_per_tick` (default 200) and `max_s2_edges_per_tick` (default 200). S8 is capped at `max_s8_pairs_per_batch` (default 500). Maximum total per tick: 900 edge write attempts. Each is an INSERT OR IGNORE — existing edges are silent no-ops. The throughput is bounded and cannot be amplified by malformed input.

---

## Regression Risk

**Low.** The feature touches four files and adds one new module:

1. `background.rs`: Minimal change — adds one import and one function call at the end of `run_single_tick`. The tick ordering invariant is preserved. All existing tick behavior runs before the new code.
2. `infra/config.rs`: Additive — five new fields with defaults. Existing fields are unchanged. `merge_configs()` entries follow identical patterns to prior fields. The dual-site default test prevents divergence regression.
3. `unimatrix-store/src/read.rs`: Additive — three new `pub const` declarations. No modification to existing code.
4. `unimatrix-store/src/lib.rs`: Additive — re-export of three new constants. No modification to existing exports.
5. `services/graph_enrichment_tick.rs` + `graph_enrichment_tick_tests.rs`: New files only.

No modification to existing graph query paths, no change to GraphCohesionMetrics struct (per ADR-004), no schema migration. The UNIQUE(source_id, target_id, relation_type) constraint ensures INSERT OR IGNORE is safe for all existing edges.

**Potential regression**: S1 and S2 write `Informs` edges with a new `source` value. The UNIQUE constraint is on `(source_id, target_id, relation_type)` — not on source. This means an S1 Informs edge and an NLI Informs edge for the same pair are the **same row** (INSERT OR IGNORE, first writer wins). If NLI already wrote an Informs edge for a pair, S1 will silently not overwrite it (and vice versa). The source column on that existing row will retain whichever source wrote it first. This is documented behavior (read.rs EDGE_SOURCE_S1 doc comment: "first writer wins — INSERT OR IGNORE semantics") and is not a defect, but it means `inferred_edge_count` (which counts `source='nli'` edges) may include edges that S1/S2 would have written if NLI hadn't run first. The test AC-30 verifies `inferred_edge_count` reflects only NLI-source rows.

---

## PR Comments

Posted comment on PR #493 (see below).
Blocking findings: No.

---

## Knowledge Stewardship

- Queried: `context_search` for "quarantine dual endpoint graph edges security" — found #3978, #3980 (confirmed production bug pattern that crt-041 correctly guards against).
- Queried: `context_search` for "S8 watermark audit_log JSON deserialization malformed" — found #4026, #4033 (confirmed S8 watermark pattern is correctly implemented per ADR-003).
- Queried: `context_search` for "SQL injection vocabulary term parameterized binding sqlx QueryBuilder" — found #4032 (ADR-002, confirmed S2 injection mitigation is per design).
- Stored: nothing novel to store — the `status = 0` vs `status != 3` divergence between enrichment ticks and co_access_promotion_tick is a design-choice observation, not a cross-feature anti-pattern that warrants a lesson-learned entry (it's a pre-existing divergence, not a new failure). All security patterns from this review are already captured in #3978, #3980, #3981, #4026, #4032.
