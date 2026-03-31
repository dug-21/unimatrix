# Agent Report: crt-037-agent-7-tick

## Task
Implement Phase 4b (Informs candidate HNSW scan) and Phase 8b (Informs write loop) in `nli_detection_tick.rs` for feature crt-037.

## Files Modified

- `crates/unimatrix-server/src/services/nli_detection_tick.rs` (+1117/-31 lines)

## Changes Made

### New Types
- `NliCandidatePair` enum (tagged union per ADR-001): `SupportsContradict` and `Informs` variants
- `InformsCandidate` struct with 9 required non-Option fields (compiler enforces guard metadata presence)
- `PairOrigin` internal scaffolding enum (consumed by Phase 7 zip, not a discriminator)

### Phase Changes
- **Phase 2**: Added `query_existing_informs_pairs` call; graceful degradation to empty set on error
- **Phase 4**: Refactored to produce `PairOrigin::SupportsContradict` entries in `pair_origins`
- **Phase 4b** (new): HNSW scan at `nli_informs_cosine_floor` (0.45), builds `InformsCandidate` via `phase4b_candidate_passes_guards()` with cross-category, temporal, cross-feature, and dedup guards
- **Phase 5**: Combined sequential cap — Supports fills to `max_cap`, Informs fills remainder
- **Phase 6**: Extended to fetch text for all `PairOrigin` variants (both Supports and Informs)
- **Phase 7**: Zip `pair_origins` with scores to build `Vec<NliCandidatePair>`
- **Phase 8**: Refactored to `match NliCandidatePair::SupportsContradict`
- **Phase 8b** (new): `match NliCandidatePair::Informs`, calls `apply_informs_composite_guard()`, writes via `write_nli_edge("Informs", weight, ...)`

### New Helpers
- `phase4b_candidate_passes_guards()` — validates cosine floor, category pair, temporal direction, cross-feature, dedup
- `apply_informs_composite_guard()` — validates neutral > 0.5, entailment threshold, non-entailment, FR-11 mutual exclusion
- `format_nli_metadata_informs()` — serializes entailment, contradiction, AND neutral (unlike existing `format_nli_metadata`)

## Tests

**2580 passed, 0 failed** (full workspace)

New tests added (AC-13 through AC-23 per test plan):
- AC-13: `test_informs_candidate_passes_cosine_floor_guard`
- AC-14: `test_informs_candidate_fails_cosine_floor_guard`
- AC-15: `test_informs_candidate_fails_same_category`
- AC-16: `test_informs_candidate_fails_unconfigured_category_pair`
- AC-17: `test_informs_candidate_fails_same_feature_cycle`
- AC-18: `test_informs_composite_guard_passes_all_conditions`
- AC-19: `test_informs_composite_guard_fails_low_neutral`
- AC-20: `test_informs_composite_guard_fails_fr11_mutual_exclusion`
- AC-21: `test_no_tokio_runtime_handle_in_production_code`
- AC-22: `test_no_domain_vocab_literals_in_production_code`
- AC-23: `test_phase8b_writes_informs_edge_when_all_guards_pass`

Additional tests: `test_informs_cap_fills_remainder`, `test_informs_dedup_against_existing_pairs`

## Issues Encountered

1. **Borrow error on `source_feature_cycle`**: Moved into `InformsCandidate` in inner loop. Fixed by adding `.clone()` at struct construction site.

2. **Dead code warnings**: `cosine` in `SupportsContradict` variant and `source_category`/`target_category` in `InformsCandidate` not read in Phase 8/8b dispatch. Fixed with `#[allow(dead_code)]` with explanatory comments.

3. **`GraphEdgeRow.metadata` missing**: Test tried to access `.metadata` field that doesn't exist on `GraphEdgeRow`. Fixed by using raw `sqlx::query_as` to SELECT the metadata column directly.

4. **AC-21/22 self-referential `include_str!`**: Grep gate tests using `include_str!("nli_detection_tick.rs")` scanned their own source, finding forbidden strings in assertion/comment lines. Fixed by splitting forbidden strings across runtime-concatenated array elements and filtering comment lines.

5. **`phase4b_candidate_passes_guards` dead code warning**: Helper function defined but Phase 4b loop still used inlined guards. Fixed by refactoring Phase 4b to call the helper, removing duplication.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` -- returned 5 entries covering ADR-001 tagged union convention, ADR-002 sequential cap, ADR-003 directional dedup, W1-2 rayon sync-only contract, and C-14 no tokio handle in rayon closure. All applied.
- Stored: entry #3945 "EntryRecord.feature_cycle is String not Option<String> — empty string means absent" via `/uni-store-pattern` — this is a gotcha invisible in source code; pseudocode spec said `Option<String>` but actual schema uses empty-string sentinel, requiring `.is_empty()` checks instead of `is_none()` in production guards.

## Commit

`5c4ee9a impl(nli_detection_tick): add Phase 4b Informs HNSW scan, Phase 8b write loop, NliCandidatePair tagged union (#463)`
