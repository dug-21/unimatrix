# crt-040 Researcher Agent Report

**Agent ID:** crt-040-researcher
**Feature:** crt-040
**Date:** 2026-04-02

## Summary

Completed problem space exploration and SCOPE.md for crt-040 (Cosine Supports Edge Detection).

## Key Findings

### What Was Deleted

**crt-038** deleted the NLI post-store path:
- `run_post_store_nli` (primary Supports detection — called after `context_store`)
- `NliStoreConfig` struct
- `parse_nli_contradiction_from_metadata` + 5 tests
- `nli_auto_quarantine_allowed`, `NliQuarantineCheck`
- `maybe_run_bootstrap_promotion`, `run_bootstrap_promotion`

**crt-039** deleted NLI from the Informs path:
- `NliCandidatePair::Informs` variant
- `PairOrigin::Informs` variant
- NLI neutral guard from `apply_informs_composite_guard`
- Outer `if nli_enabled` gate in `background.rs`

### Current State of `structural_graph_tick`

`run_graph_inference_tick` in `crates/unimatrix-server/src/services/nli_detection_tick.rs`:
- **Path A (unconditional):** Structural Informs — cosine ≥ 0.50, `informs_category_pairs`
  filter, temporal + cross-feature guards. Writes source='nli'. Budget: `MAX_INFORMS_PER_TICK=25`.
- **Path B (NLI-gated):** NLI Supports — `get_provider()` gate. Dead in production
  (`nli_enabled=false`).

### Schema: `signal_origin`

No `signal_origin` column exists. The `graph_edges.source` TEXT column is the equivalent.
Current values: `'nli'` (Path A Informs + Path B Supports), `'co_access'` (promotion tick),
`''` (bootstrap). No migration needed — write `source = 'cosine_supports'` for new edges.

### Critical Constraint: `write_nli_edge` Hardcodes 'nli'

`write_nli_edge()` in `nli_detection.rs` hardcodes `'nli'` as the source value in the
INSERT. Cannot be reused for cosine Supports without adding a `source` parameter or writing
a new `write_graph_edge` helper. Changing the hardcoded literal would silently retag all
existing Informs and NLI Supports edges — not acceptable.

### Named Constant Pattern (ADR from col-029 + crt-034)

`EDGE_SOURCE_NLI` and `EDGE_SOURCE_CO_ACCESS` are defined in `read.rs` and re-exported
from `lib.rs`. Must add `EDGE_SOURCE_COSINE_SUPPORTS = "cosine_supports"` following the
same pattern.

### ASS-035 Validation

Cosine ≥ 0.65 on production embeddings: 6/8 true pairs detected, 0/10 false positives
(including 5 compatible-category cross-feature negatives). `same_feature_cycle` filter not
required for correctness. Two missed pairs (P02=0.523, P05=0.557) have structural
explanations (semantic distance is real).

## SCOPE.md

Written to: `/workspaces/unimatrix/product/features/crt-040/SCOPE.md`

15 ACs defined (AC-01 through AC-15).

## Open Questions for Human

1. Budget: constant (`MAX_COSINE_SUPPORTS_PER_TICK`) or config field?
2. Edge direction: symmetric (both A→B and B→A) or canonical (lower id as source)?
3. Edge weight: `cosine` or `cosine * multiplier`? (ASS-035 proposed 0.9x)
4. Metadata field name: `{"cosine": f32}` or `{"cosine_similarity": f32}`?
5. `inferred_edge_count` metric: broaden, add new field, or leave as-is?
6. `nli_post_store_k` dead field: clean up in this cycle or defer?

## Knowledge Stewardship

- Queried: `context_briefing` — 17 entries surfaced; entries #3713 (threshold calibration
  lesson) and #4024 (file path navigation lesson) directly relevant. Entries #3591 (ADR
  EDGE_SOURCE_NLI) and #3656/#3957 (graph inference tick design) also relevant.
- Stored: entry #4025 "write_nli_edge hardcodes source='nli' — new edge signal origins require write_graph_edge helper" via /uni-store-pattern
