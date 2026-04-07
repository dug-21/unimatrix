# Agent Report: crt-050-agent-1-architect

## Status: COMPLETE

## Outputs

- ARCHITECTURE.md: `product/features/crt-050/architecture/ARCHITECTURE.md`
- ADR-001: `product/features/crt-050/architecture/ADR-001-two-query-rebuild-strategy.md` — Unimatrix #4223
- ADR-002: `product/features/crt-050/architecture/ADR-002-query-a-sql-structure.md` — Unimatrix #4224
- ADR-003: `product/features/crt-050/architecture/ADR-003-outcome-weight-function-placement.md` — Unimatrix #4225
- ADR-004: `product/features/crt-050/architecture/ADR-004-config-field-rename.md` — Unimatrix #4226
- ADR-005: `product/features/crt-050/architecture/ADR-005-observations-input-storage-contract.md` — Unimatrix #4227
- ADR-006: `product/features/crt-050/architecture/ADR-006-ts-millis-unit-contract.md` — Unimatrix #4228
- ADR-007: `product/features/crt-050/architecture/ADR-007-hook-column-name.md` — Unimatrix #4229
- ADR-008: `product/features/crt-050/architecture/ADR-008-phase-category-weights-formula.md` — Unimatrix #4230

## Key Findings from Source Verification

### SR-01 RESOLVED (ADR-005)
The double-encoding risk is NOT present. `listener.rs extract_observation_fields()` uses
`serde_json::to_string(v)` on `Value::Object` directly (line 2696) — stores `{"id":42}`,
not `"\"{\\"id\\":42}\""`. Pure-SQL approach is valid.

### SR-02 RESOLVED (ADR-006)
`MILLIS_PER_DAY: i64 = 86_400 * 1_000` constant approach chosen. Pre-compute
`cutoff_millis` in Rust, bind as `?2`. Eliminates SQL arithmetic ambiguity.

### SR-06 RESOLVED (ADR-003)
`infer_gate_result()` NOT called from rebuild path. Private `outcome_weight(outcome: &str) -> f32`
defined inline in `phase_freq_table.rs`. Avoids mcp/→services/ layering violation and
incompatible `pass_count` parameter.

### SCOPE.md SQL BUG FOUND (ADR-007)
SCOPE.md draft SQL uses `o.hook_event = 'PreToolUse'`. The actual column is `o.hook`.
`hook_event` does not exist — using it would cause a runtime SQL error. All ADRs and
ARCHITECTURE.md use `o.hook`.

### SR-04 SURFACE AREA (ADR-004)
`query_log_lookback_days` rename: serde alias covers TOML deserialization. Rust struct
literal sites in tests must be updated manually (compiler enforces — no silent regression).
Implementer must audit all `query_log_lookback_days:` in test struct constructions.

## Open Questions for Implementer

1. **OQ-1 (per-phase weight aggregation):** ADR-001 specifies mean-weight across cycles
   for a phase. Implementer should verify this produces the expected behavior with the
   synthetic test cases in AC-13.

2. **OQ-2 (PhaseOutcomeRow visibility):** Keep `pub(crate)` or internal to unimatrix-store.
   Not re-exported from crate root.

3. **OQ-3 (min_phase_session_pairs default):** Default of 5 is pragmatic. Advisory-only
   diagnostic — low risk from wrong default.

## Integration Surface Summary

New functions implementers must create:
- `SqlxStore::query_phase_freq_observations(lookback_days: u32) -> Result<Vec<PhaseFreqRow>>`
- `SqlxStore::query_phase_outcome_map() -> Result<Vec<PhaseOutcomeRow>>`
- `PhaseFreqTable::phase_category_weights(&self) -> HashMap<(String, String), f32>`
- `fn outcome_weight(outcome: &str) -> f32` (private)
- `fn apply_outcome_weights(rows: Vec<PhaseFreqRow>, outcomes: Vec<PhaseOutcomeRow>) -> Vec<PhaseFreqRow>` (private or inline)

Deleted:
- `SqlxStore::query_phase_freq_table()` — one call site confirmed, safe to delete
