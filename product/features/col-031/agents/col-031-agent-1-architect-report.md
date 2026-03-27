# Agent Report: col-031-agent-1-architect

## Task
Design architecture for col-031: Phase-Conditioned Frequency Table (GH #414).

## Outputs

### ARCHITECTURE.md
`/workspaces/unimatrix/product/features/col-031/architecture/ARCHITECTURE.md`

### ADR Files

| ADR | File | Unimatrix ID |
|-----|------|--------------|
| ADR-001 | `architecture/ADR-001-rank-based-normalization.md` | #3679 |
| ADR-002 | `architecture/ADR-002-store-layer-sql-placement.md` | #3680 |
| ADR-003 | `architecture/ADR-003-json-each-integer-cast.md` | #3681 |
| ADR-004 | `architecture/ADR-004-lock-ordering-three-handles.md` | #3682 |
| ADR-005 | `architecture/ADR-005-w-phase-explicit-default-and-eval-gate.md` | #3683 (supersedes crt-026 ADR-003 #3163) |

## Key Decisions Summary

1. **Rank-based normalization** (ADR-001, #3679): `score = 1.0 - (rank/N)`, 0-indexed. Absent entries return 1.0 neutral (not 0.0). Chosen over min-max due to power-law distribution robustness and richer PPR gradient.

2. **SQL in unimatrix-store** (ADR-002, #3680): `Store::query_phase_freq_table` in `query_log.rs` with new `PhaseFreqRow` struct. Follows existing crate boundary conventions; enables TestDb-based AC-08 integration test.

3. **`CAST(json_each.value AS INTEGER)`** (ADR-003, #3681): Pinned SQL form for expanding `result_entry_ids` JSON integer array. Both SELECT projection and JOIN condition use explicit cast. AC-08 must verify against a live row.

4. **Lock acquisition order: Effectiveness → TypedGraph → PhaseFreqTable** (ADR-004, #3682): Structurally non-nested sequential scope blocks. PhaseFreqTable lock released before scoring loop; only the specific `(phase, category)` Vec clone is carried into the loop, not the full table.

5. **w_phase_explicit default 0.0 → 0.05; eval harness fix non-separable** (ADR-005, #3683, supersedes #3163): Activates the crt-026 placeholder. AC-16 (extract.rs fix) must complete before AC-12 (eval regression gate) can be declared. Accepted calibration risk — cold-start safety net applies.

## Supersession

- crt-026 ADR-003 (#3163) superseded by ADR-005 (#3683). The `w_phase_explicit = 0.0` placeholder strategy is replaced by the non-parametric frequency table signal.

## Open Questions for Delivery

1. **json_each form** (SR-01): Implementer must verify `CAST(je.value AS INTEGER)` against a live `query_log` row. AC-08 is the CI gate.
2. **Tick wall time** (SR-07): Add `tracing::debug!` timing log for rebuild step to establish empirical baseline.
3. **PPR status check** (SR-05): Confirm at delivery start whether #398 has shipped — determines if AC-07/AC-08 wire-up ACs are immediately applicable.
4. **w_phase_explicit = 0.05 calibration** (SR-02): Risk accepted. AC-12 is the safety gate; no additional spike required.
