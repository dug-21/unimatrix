# col-031 Researcher Report

**Agent ID**: col-031-researcher
**Feature**: col-031
**Date**: 2026-03-27

## Summary

Explored the full problem space for the phase-conditioned frequency table feature.
All seven research areas from the spawn prompt were investigated. SCOPE.md written to
`product/features/col-031/SCOPE.md`.

## Key Findings

### 1. w_phase_explicit placeholder — exact location confirmed

`crates/unimatrix-server/src/services/search.rs` line 873:
```rust
phase_explicit_norm: 0.0,  // hardcoded, W3-1 placeholder
```

`crates/unimatrix-server/src/infra/config.rs` line 441:
```rust
w_phase_explicit: 0.0,   // crt-026: W3-1 placeholder (ADR-003)
```

The field, weight, and formula term all exist. Only the signal source is missing.
Raising the default to 0.05 requires changing `default_w_phase_explicit()` and one
test assertion. The six-weight sum constraint is unaffected — `w_phase_explicit` is
an additive term outside it (ADR-004, crt-026, Unimatrix #3206).

### 2. TypedGraphStateHandle pattern — the exact template to replicate

`services/typed_graph.rs` is the canonical model:
- `TypedGraphState` struct with `use_fallback: bool`
- `TypedGraphStateHandle = Arc<RwLock<TypedGraphState>>`
- `new_handle()` creates cold-start state
- `rebuild(store: &Store)` called by background tick, returns `Result<Self, StoreError>`
- Tick swaps via `*guard = new_state` under write lock on `Ok`
- Hot path: short read lock → clone → release → use
- Poison recovery: `.unwrap_or_else(|e| e.into_inner())` throughout
- `EffectivenessState` adds `generation: u64` for clone-avoidance (defer unless profiling shows need)

### 3. query_log schema (col-028, schema v17) — confirmed shipped

`query_log` columns: `query_id, session_id, query_text, result_entry_ids (JSON),
top_similarity, timestamp, source, feature_cycle, phase TEXT` (nullable).
Index: `idx_query_log_phase`.
Col-028 Gate 3c PASS 2026-03-26. 3629 tests pass.
Pre-col-028 rows have `phase = NULL` — filtered by `WHERE phase IS NOT NULL`.

### 4. PPR (#398) personalization vector integration point

GH #398 specifies `personalization[v] = hnsw_score[v]`. col-031 provides:
```
personalization[v] = hnsw_score[v] * phase_affinity_score(v, current_phase)
```
The `phase_affinity_score` method is the integration contract. #398 is not yet
implemented. col-031 must not block on it; #398 must not block on col-031.
The method is published on `PhaseFreqTable` and the integration point is documented.

### 5. Retention framework (#409) alignment

GH #409 specifies `query_log_retention_cycles = 20` (K) governing both GC and
frequency table lookback. col-031 adds this field to `InferenceConfig`. GC belongs
to #409 — col-031 uses the value for the SQL lookback window only. Unimatrix #3414
documents the hard constraint: K must be the same for frequency table and GNN training
reconstruction window.

### 6. Background tick mechanism

`background.rs` shows the tick sequence. `TypedGraphState::rebuild` is called via
`tokio::time::timeout(TICK_TIMEOUT, tokio::spawn(...))`. The `PhaseFreqTable` rebuild
follows the same pattern, placed after `TypedGraphState` rebuild by convention
(structural state before analytical state).

### 7. Unimatrix ADR evidence

Key ADRs confirmed via Unimatrix briefing:
- #3163 (ADR-003 crt-026): w_phase_explicit=0.0 placeholder strategy, defer to W3-1
- #3175 (ADR-004 crt-026): w_phase_histogram=0.02 additive outside sum constraint
- #3206 (pattern): FusionWeights additive field dual exemption — confirmed w_phase_explicit safe to raise without touching sum-check
- #3519 (ADR-007 col-028): query_log.phase positional append pattern
- #3555 (pattern): Eval harness doesn't select query_log.phase — this is Open Question 5 in SCOPE.md
- #1560 (pattern): Background-tick state cache Arc<RwLock<T>> pattern — general, still valid

## Scope Proposed

15 acceptance criteria across:
- `PhaseFreqTable` struct and handle (AC-01 through AC-05)
- Scoring wire-up / `phase_explicit_norm` computation (AC-06)
- `phase_affinity_score` API for PPR (AC-07, AC-08)
- `InferenceConfig` changes (AC-09, AC-10)
- Cold-start invariant (AC-11)
- Eval regression gate (AC-12)
- Normalization contract (AC-13)
- Integration test (AC-14)
- File placement / size limit (AC-15)

## Open Questions for Human

1. **Normalization strategy**: min-max within bucket vs. rank-based? Proposed default:
   min-max with 0.5 floor so no bucket member is penalized below neutral. Decide during
   spec phase.

2. **json_each SQL form**: `result_entry_ids` is stored as a JSON array. The exact
   `json_each` syntax needs verification against a real row during implementation.

3. **Join shape for rebuild query**: The aggregation joins `query_log` + `json_each`
   + `entries` (for category). Confirm this is within `unimatrix-store` access patterns.

4. **Tick ordering**: `PhaseFreqTable` rebuild before or after `TypedGraphState`?
   No dependency. Convention suggests after.

5. **Eval harness phase gap** (Unimatrix #3555): `eval/scenarios/extract.rs` does not
   select `query_log.phase`. If eval scenarios all have `phase = NULL`,
   `phase_explicit_norm = 0.0` everywhere and the scoring change is invisible to eval.
   AC-12 (no regression) is still valid, but signal activation cannot be validated. Is
   fixing the eval harness phase gap in scope for col-031, or a separate issue?

6. **w_phase_explicit = 0.05 calibration**: Treat as a configurable default or
   validate against eval data before shipping?

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — 16 results; ADRs #3163, #3175, #3519
  directly relevant; pattern #1560 confirmed the tick-cache pattern; pattern #3555
  surfaced the eval harness phase gap as an open question.
- Stored: entry #3677 "PhaseFreqTable cold-start neutral score: absent entries return
  1.0, not 0.0" via `/uni-store-pattern` — generalizes to any future signal source
  gating on query_log data availability.
