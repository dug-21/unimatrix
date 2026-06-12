# Agent Report — vnc-035-agent-4-carry_forward_handler

## Scope
`run_carry_forward_loop` + `CarrySummary` (carry orchestrator), step 8b′ insertion into the
`context_correct` handler, and the `edges_carried` ack — all in `crates/unimatrix-server`.

## Files Modified
- `crates/unimatrix-server/src/mcp/tools.rs`
  - New `CarrySummary { found, carried, failed }` (sibling of `RedirectSummary`).
  - New `CarryWriteOutcome { Inserted, UniqueConflict, SqlError }` + `carry_write_edge`
    indirection hosting the AC-07 fault seam.
  - New `#[cfg(test)] mod carry_fault` (thread-local Nth-call counter seam:
    `arm_fail_on_nth` / `disarm` / `should_fail_next`).
  - New `run_carry_forward_loop(store, original_id, new_entry_id, created_at) -> CarrySummary`.
  - New free helper `append_to_first_text` (de-nests both ack appends).
  - Handler: hoisted shared correction `now`; inserted step 8b′ between 8b and 8c; threaded
    `edges_carried` into the ack (count-only, omitted when zero).
  - New `#[cfg(test)] mod carry_forward_loop_tests` (15 tests).
- `crates/unimatrix-server/src/mcp/response/entries.rs` — `format_edges_carried(carried)`.
- `crates/unimatrix-server/src/mcp/response/mod.rs` — re-export `format_edges_carried` +
  `test_format_edges_carried_count_only`.

## Tests
- `carry_forward_loop_tests`: 15 passed / 0 failed, including the MANDATORY
  `test_carry_forward_continues_on_edge_copy_failure` (all 4 assertions: correction success,
  B Active + A Deprecated, pre-failure edge persists, `failed >= 1` + `tracing::warn!` fired).
- `test_format_edges_carried_count_only`: pass.
- `redirect_loop_tests` (vnc-017): 12 passed / 0 — no regression.
- Full `mcp::` module: 842 passed / 0.
- `cargo build -p unimatrix-server`: clean. `cargo clippy`: no new warnings on the added code
  (dead-code on `found`/`failed`/`SqlError` silenced via `#[cfg_attr(not(test), allow(dead_code))]`
  — these are ADR-003-option-(a) contract artifacts exercised only under `#[cfg(test)]`).

### Note on full-workspace flake
`eval::runner::sweep_tests::test_ac14_correlated_sweep_non_vacuous` failed once under full-suite
parallelism but passes in isolation and on a clean baseline — unrelated to carry-forward (eval
sweep ordering flake). The only carry-forward touch near it was a stray `cargo fmt` import
reorder in `sweep_tests.rs`, which I reverted to keep the diff scoped.

## Design adherence
- Owns its write loop; counts `write_graph_edge` `true` only (ADR-003 / pattern #4041 / R-08).
- No ceiling — all eligible edges carry (AC-09).
- Contradicts: forward + reverse via `write_graph_edge`, counted once (ADR-005).
- `created_at = now`, `source`/`created_by = "agent"`, `weight = 1.0`, `bootstrap_only = 0`,
  `metadata = ""` — no preservation, no provenance marker (ADR-004 / R-11).
- Warn-and-continue: returns `CarrySummary` by value, never `Err`, never rolls back (ADR-002 /
  NFR-01); `query_outgoing_edges` Err → `CarrySummary::default()` + warn.
- Pipeline order 8 → 8b → 8b′ → 8c → 9 → 10; shared correction `now` across 8b and 8b′ (ADR-001).
- Fault seam is `#[cfg(test)]`-gated, zero production branch.

## Issues / Blockers
None.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — surfaced exactly the ADRs/patterns the brief already
  encoded (#4985 count contract, #4041 rows-affected bool, #4983 placement, #4987 Contradicts,
  #4462 vnc-017 warn-and-continue posture). No gap between briefing and brief.
- Stored: nothing novel to store — the implementation followed settled ADRs and existing patterns
  (#4041 three-case bool, #4459 Contradicts disjointness, vnc-017 seam precedent). The one mildly
  novel artifact (an Nth-call `#[cfg(test)]` thread-local fault seam vs. vnc-017's all-writes-fail
  table-rename-to-view) is already documented in the pseudocode and brief as the required AC-07
  seam, so it is not a new reusable pattern beyond what the feature already records.
