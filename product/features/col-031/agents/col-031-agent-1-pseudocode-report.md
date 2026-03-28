# Agent Report: col-031-agent-1-pseudocode

Feature: col-031 — Phase-Conditioned Frequency Table
Stage: 3a (Pseudocode)
Date: 2026-03-28

---

## Deliverables

All pseudocode files written to `product/features/col-031/pseudocode/`:

| File | Component |
|------|-----------|
| `OVERVIEW.md` | Component interactions, data flow, shared types, sequencing |
| `phase_freq_table.md` | PhaseFreqTable module (new) |
| `query_log_store_method.md` | PhaseFreqRow + query_phase_freq_table (store) |
| `search_scoring.md` | Fused scoring wire-up (search.rs) |
| `background_tick.md` | Tick integration (background.rs) |
| `service_layer.md` | ServiceLayer wiring (services/mod.rs) |
| `inference_config.md` | InferenceConfig additions (infra/config.rs) |
| `replay_fix.md` | AC-16 one-line fix (eval/runner/replay.rs) |

---

## Components Covered

1. `phase_freq_table.rs` — new module with `PhaseFreqTable`, `PhaseFreqTableHandle`,
   `new()`, `new_handle()`, `rebuild()`, `phase_affinity_score()`, `Default` impl,
   and 9 test scenarios.

2. `query_log.rs` store method — `PhaseFreqRow` struct, `query_phase_freq_table`
   method, `row_to_phase_freq_row` helper, and export guidance.

3. `search.rs` scoring — `ServiceSearchParams.current_phase` field, new
   `phase_freq_table` field and constructor parameter, pre-loop snapshot extraction
   block, per-entry `phase_explicit_norm` assignment, and wiring site checklist.

4. `background.rs` tick — `spawn_background_tick`, `background_tick_loop`, and
   `run_single_tick` signature additions; `PhaseFreqTable::rebuild` call with
   retain-on-error semantics and lock-ordering comment.

5. `services/mod.rs` — module declaration, re-exports, struct field, handle
   creation in `with_rate_config`, `SearchService::new` argument, struct literal
   field, and `phase_freq_table_handle()` accessor.

6. `infra/config.rs` — `default_w_phase_explicit` raised to 0.05,
   `query_log_lookback_days` field with default 30, `validate()` range check
   `[1, 3650]`, `Default` impl update, `merge_configs` entry, and sum-check
   doc-comment update.

7. `eval/runner/replay.rs` — AC-16: exactly one line added:
   `current_phase: record.context.phase.clone()`.

---

## Open Questions / Ambiguities Found

### OQ-A: ConfigError variant for query_log_lookback_days range check

The pseudocode in `inference_config.md` (Change 5) uses
`ConfigError::InferenceFieldOutOfRange`. A grep of the config.rs content confirms
several error variants exist but the exact variant for per-field numeric range errors
was not confirmed. The implementation agent must:
1. Check the existing `ConfigError` enum.
2. Use an existing variant if one covers "field name + invalid value + range", or
3. Add a new variant if none is suitable.

The error must be machine-readable (field name + value + range). If the existing
variant is named differently, use that name.

### OQ-B: `InferenceConfig::Default` hardcoded vs. function call for w_phase_explicit

The existing `Default` impl has `w_phase_explicit: 0.0` as a literal. The
`inference_config.md` pseudocode shows two options for updating this. The
implementation agent should prefer using `default_w_phase_explicit()` in the
`Default` impl for consistency with how other defaulted fields are handled in
the same struct.

### OQ-C: `test_inference_config_default_phase_weights` existing test

This test (referenced in AC-09) currently asserts `w_phase_explicit == 0.0`.
After raising the default to 0.05, the test will fail. The implementation agent
must update the assertion to `0.05`. This is an intentional AC-09 requirement,
not a regression.

### OQ-D: MCP tool handler for context_search — current_phase forwarding

The `search_scoring.md` wiring site checklist lists "MCP handler" as a site that
must add `current_phase`. The exact file and call site in the MCP tool handler
was not read during this session (context budget). The implementation agent must:
1. Grep for `ServiceSearchParams {` in `crates/unimatrix-server/src/` (MCP tool file).
2. Add `current_phase: params.current_phase` (forwarded from the tool call parameter).
This is required for the feature to work in live queries, not just eval.

### OQ-E: `PhaseFreqRow` export path

`query_log_store_method.md` states `PhaseFreqRow` must be pub-exported from
`unimatrix-store/src/lib.rs`. The implementation agent must verify whether the
`lib.rs` already re-exports query_log items and add the export only if absent.

---

## Self-Check Results

- [x] Architecture output was read before writing any pseudocode
- [x] No invented interface names — every name traced to ARCHITECTURE.md, IMPLEMENTATION-BRIEF.md, or codebase
- [x] Output is per-component (OVERVIEW.md + one file per component), not monolithic
- [x] Each component file includes function signatures, error handling, and test scenarios
- [x] No TODO, placeholder functions, or TBD sections — gaps flagged as open questions above
- [x] Shared types defined in OVERVIEW.md match usage in component files
- [x] All output files within `product/features/col-031/pseudocode/`
- [x] Knowledge Stewardship report block included (below)

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned 15 entries. Key results:
  #3688 (ADR-004: w_phase_explicit activation and AC-16 non-separability),
  #3689 (ADR-005: required handle threading, lesson #3216 bypass failure mode),
  #3687 (ADR-003: two cold-start contracts for phase_affinity_score),
  #3686 (ADR-002: time-based retention window),
  #3685 (ADR-001: rank-based normalization),
  #3677 (pattern: absent entries return 1.0),
  #3555 (eval harness extract.rs gap — confirmed already resolved in architecture).
  All findings consistent with ARCHITECTURE.md and IMPLEMENTATION-BRIEF.md.

- Queried: `mcp__unimatrix__context_search` category=pattern for "phase frequency
  table scoring patterns" — returned #3677 (PhaseFreqTable cold-start neutral score
  pattern) and #3576 (compute_phase_stats grouping pattern). Both incorporated.

- Queried: `mcp__unimatrix__context_search` category=decision topic=col-031 —
  returned all 5 col-031 ADRs (#3685-#3690). All incorporated.

- Deviations from established patterns: none. All pseudocode follows the
  `TypedGraphState` template exactly (struct shape, handle type alias, new_handle(),
  rebuild() -> Result<Self>, poison recovery, retain-on-error). Lock acquisition order
  pattern (EffectivenessStateHandle -> TypedGraphStateHandle -> new handle) matches
  the documented convention from crt-021.
