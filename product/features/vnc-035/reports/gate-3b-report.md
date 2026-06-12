# Gate 3b Report: vnc-035

> Gate: 3b (Code Review)
> Date: 2026-06-12
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Pseudocode fidelity | PASS | `query_outgoing_edges`, `run_carry_forward_loop`, `CarrySummary`, `carry_write_edge`, handler 8b′ + ack all match the validated pseudocode (incl. Note A shape (b): `created_at` param). |
| 2. Architecture compliance | PASS | Pipeline order 8→8b→8b′→8c honored; ADR-001..005 followed exactly. |
| 3. Interface implementation | PASS | Signatures match the Integration Surface table: `query_outgoing_edges(&self, source_id: u64) -> Result<Vec<OutgoingEdgeRow>>`; `run_carry_forward_loop(store, original_id, new_entry_id, created_at) -> CarrySummary`; `CarrySummary{found,carried,failed}`; `edges_carried` ack count-only, omitted when zero. |
| 4. Test case alignment | PASS (with scope note) | All loop/store-unit tests from the test plan present and passing (15 server carry tests, 4 store tests, 1 formatter test). Handler-integration tests (Layer 3) + infra-001 Python tests are Stage 3c deliverables per the test plan's own layering — not Gate 3b code-review scope. |
| 5. Code quality | PASS | `cargo build --workspace` clean. No `todo!`/`unimplemented!`/`TODO`/`FIXME`/`unsafe`. No `.unwrap()`/`.expect()` in non-test carry code. New module `read_outgoing.rs` = 289 lines (< 500). `tools.rs` is pre-existing large — not a new module. |
| 6. Security | PASS | Parameterized `WHERE source_id = ?1` bind; static `NOT IN` literal predicate — no injection surface. Carry introduces no new external input (reads validated rows, re-writes onto B). No hardcoded secrets. |
| 7. Knowledge stewardship | PASS | Both impl agent reports (agent-3, agent-4) contain `## Knowledge Stewardship` with `Queried:` + `Stored:`/`nothing novel -- {reason}` entries. |

**CRITICAL by-name check — AC-07:** `test_carry_forward_continues_on_edge_copy_failure` is PRESENT by name and PASSES. It uses a genuine **per-edge Nth-call** fault seam (`carry_fault::arm_fail_on_nth(2)`), seeds 2 eligible edges, and asserts exactly **one** carried edge persists on B (the write before the injected failure) — a true "edges before the failure persist" assertion, NOT vnc-017's all-writes-fail table-rename-to-view view. Lesson #4473 satisfied.

## Detailed Findings

### 1. Pseudocode fidelity — PASS
`crates/unimatrix-store/src/read_outgoing.rs` mirrors `query_incoming_edges`: same SQL shape on `source_id`, same `i64`-bind / `try_get` casts, `read_pool()` accessor, `StoreError::Database` mapping. The `OutgoingEdgeRow` DTO matches (`target_id`, `relation_type`, `created_at`; `created_at` doc'd as read-for-observability, not written onto B).

`run_carry_forward_loop` (tools.rs:5021) matches the pseudocode body line-for-line: query → empty-guard → per-row `RelationType::from_str` defensive skip → forward `carry_write_edge` (counted) → `Contradicts` reverse write (not counted) → completion `info!` log → return `CarrySummary` by value. The implementation chose pseudocode option (a) for `failed` (test-seam-driven `SqlError`, exact `carried`) and Note A shape (b) for `now` (param threaded from the handler so 8b and 8b′ share one timestamp).

### 2. Architecture compliance — PASS
Handler (tools.rs:1147) inserts 8b′ between 8b (`validate_and_write_edges`, :1133) and 8c (`run_redirect_loop`, :1167). `now` is hoisted (:1129) above the 8b `if !empty` block and passed to both 8b and 8b′ (ADR-001 / ADR-004 — one correction timestamp). Carry runs after the commit (step 8) and cannot roll it back (returns `CarrySummary` by value, never `Err`).

- **ADR-002 (single-source predicate / posture):** exclusion list `('Supersedes','CoAccess','Informs')` appears in exactly one SQL clause; no parallel Rust-side filter. Superset-vs-incoming rationale documented inline. Warn-and-continue posture: `query_outgoing_edges` Err → `CarrySummary::default()` + warn; per-edge SQL error → `failed++` + warn + continue.
- **ADR-003 (count contract):** carry loop OWNS its write loop, calls `write_graph_edge` (via `carry_write_edge`), counts `Inserted` (true) ONLY. Does NOT delegate to bool-discarding `validate_and_write_edges` (the only mention is a structural comment). R-08 satisfied.
- **ADR-004 (additive-on-triple / no preservation):** `created_at = now`, `weight = 1.0`, `source`/`created_by = "agent"`, `metadata = ""`, `bootstrap_only = 0` — byte-indistinguishable from a fresh declaration. R-11 satisfied and asserted by `test_carried_edge_metadata_is_fresh_agent`.
- **ADR-005 (Contradicts bidirectional / disjointness):** forward write counted, reverse write inline + not counted; one logical edge = one `carried`. No source-validation guard on carry side (source is Active B). `test_carry_contradicts_counts_one`, `test_carry_contradicts_both_directions_exactly_once`, `test_carry_redirect_contradicts_converge` all pass.

### 3. Interface implementation — PASS
All signatures match the architecture Integration Surface table. `OutgoingEdgeRow` re-exported from `unimatrix-store/src/lib.rs`. `format_edges_carried` added to `response/entries.rs` and re-exported via `response/mod.rs`. `CarrySummary` / `run_carry_forward_loop` / `carry_fault` are `pub(super)` for test visibility (mirrors `RedirectSummary`/`run_redirect_loop`).

### 4. Test case alignment — PASS (with Stage-3c scope note)
**Implemented and passing:**
- Store-unit (read_outgoing.rs tests): `test_query_outgoing_excludes_derived_classes`, `test_query_outgoing_returns_eligible_with_fields`, `test_query_outgoing_empty_when_no_edges`, `test_query_outgoing_only_ineligible_returns_empty` (4/4 pass).
- Carry-loop-unit (tools.rs carry_forward_loop_tests): all 15 named tests from the test plan — incl. the mandatory `test_carry_forward_continues_on_edge_copy_failure`, `test_carry_query_err_returns_empty_summary`, `test_correction_committed_before_carry`, `test_carry_count_idempotent_repass`, `test_carry_count_keys_off_true_only`, `test_carried_edge_metadata_is_fresh_agent`, the three Contradicts tests, the two self-loop tests, `test_carry_no_ceiling_all_carry_above_50`, `test_carry_eligible_attach_to_new_id_not_original`, `test_carry_excludes_derived_classes`, `test_carry_empty_when_no_eligible_edges` (15/15 pass).
- Formatter: `test_format_edges_carried_count_only` (pass — pins AC-11c count-only/no-content).

**Deferred to Stage 3c (per the test plan's own three-layer model, OVERVIEW.md):** the handler-integration tests assigned to `context_correct_handler.md` — pipeline-order assertion (R-04), tick-window depth-1/BFS (R-07), shed via new id + deprecated-rejection (R-10), AC-03 `Advances→vision_root` regression, AC-08 additive/changed-target, AC-11 ack present/omitted end-to-end — plus the three infra-001 Python suites (`tools`, `lifecycle`). These are the Layer-3 / infra-001 deliverables the plan explicitly schedules for Stage 3c, and Gate 3c is where risk-coverage completeness is enforced. Gate 3b (code-matches-pseudocode) is satisfied: every load-bearing risk (R-01, R-02, R-03, R-05, R-08, R-11, AC-09) has passing code-level coverage now.

### 5. Code quality — PASS
- `cargo build --workspace`: clean (only pre-existing warnings in unrelated code).
- No anti-stub tokens, no `unsafe`, no non-test `.unwrap()`/`.expect()` in vnc-035 code.
- `read_outgoing.rs` = 289 lines. No new module exceeds 500 lines. `tools.rs` (11,236) is pre-existing large; the carry additions are cohesive and adjacent to `run_redirect_loop`.
- `cargo clippy -p unimatrix-store -p unimatrix-server`: warnings present are all pre-existing (collapsible_if, unused imports, type_complexity) — NONE originate in vnc-035 new code (verified by grep over `read_outgoing`/`run_carry_forward`/`carry_write_edge`/`CarrySummary`/`carry_fault`/`format_edges_carried`).
- The `#[cfg_attr(not(test), allow(dead_code))]` on `CarrySummary` fields and `CarryWriteOutcome::SqlError` is justified: `found`/`failed` and the `SqlError` variant are part of the ADR-003 contract and consumed only under `#[cfg(test)]`; the attribute silences non-test dead-code analysis without dropping contract fields. Acceptable.

### 6. Security — PASS
`query_outgoing_edges` uses `WHERE source_id = ?1` parameterized bind; the predicate is a static `NOT IN` literal — no string interpolation, no injection surface. Carry reads `source_id`-keyed rows already in `graph_edges` (written by prior validated calls) and re-writes onto B — no new external input. The agent-declared-only filter blocks tick-generated high-fan-out classes from being laundered through correction (the R-03 security-adjacent guard). No hardcoded secrets. `index idx_graph_edges_source_id` confirmed present (db.rs:969, migration.rs:367) — R-09 resolved, latency-only risk eliminated.

### 7. Knowledge stewardship — PASS
- agent-3 (query_outgoing_edges): `Queried:` #4417/#3884/#2451 + ADR-002; `Stored:` entry #4993 (novel module-split test-accessor trap).
- agent-4 (carry_forward_handler): `Queried:` context_briefing (#4985/#4041/#4983/#4987/#4462); `Stored:` "nothing novel to store -- {reason}" (followed settled ADRs; the Nth-call seam is already recorded in the pseudocode/brief).

## Build / Test Verification
- `cargo build --workspace` — clean.
- `cargo test -p unimatrix-server --lib carry_forward` — 15 passed, 0 failed.
- `cargo test -p unimatrix-store --lib query_outgoing` — 4 passed, 0 failed.
- `cargo test -p unimatrix-server --lib format_edges_carried` — 1 passed, 0 failed.

Note: a `cargo test -p unimatrix-server carry_forward` (full target set, incl. integration test binaries) was killed by the linker OOM (`ld terminated with signal 9`) in this environment — a resource limit when linking all integration binaries simultaneously, NOT a code defect. Re-running against `--lib` (the relevant target) passes cleanly. The pre-existing `eval::runner::sweep_tests::test_ac14_correlated_sweep_non_vacuous` flake is unrelated to this feature.

## Rework Required
None.
