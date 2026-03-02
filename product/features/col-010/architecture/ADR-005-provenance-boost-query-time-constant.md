# ADR-005: Provenance Boost as Query-Time Constant

**Feature**: col-010
**Status**: Accepted
**Date**: 2026-03-02

## Context

AC-23 requires that a `lesson-learned` entry ranks higher than a generic `convention` entry with identical similarity and confidence scores. The mechanism needs to provide a "slight bump" without disturbing the stored confidence formula invariant.

The stored confidence formula uses 6 additive weighted factors summing to 0.92:

```
W_BASE + W_USAGE + W_FRESH + W_HELP + W_CORR + W_TRUST = 0.92
```

Co-access affinity (W_COAC = 0.08) is already applied at query time without being stored. The question is where to apply a provenance signal for `lesson-learned` entries.

Two options were considered:
1. **Stored boost**: add a `lesson-learned` weight to the confidence formula, changing the stored `EntryRecord.confidence`.
2. **Query-time constant**: apply a constant `PROVENANCE_BOOST` at search re-ranking time, alongside co-access affinity.

## Decision

Apply `PROVENANCE_BOOST = 0.02` at **query time only** as a named constant in `unimatrix-engine/src/confidence.rs`.

Applied in both `uds_listener.rs` and `tools.rs` search re-ranking, alongside the existing co-access boost:

```rust
// Final score formula (search re-ranking):
// base = 0.85 * similarity + 0.15 * confidence
// co_access = compute_search_boost(...)    [max 0.03]
// provenance = PROVENANCE_BOOST if entry.category == "lesson-learned" else 0.0
// final = base + co_access + provenance
let prov = if entry.category == "lesson-learned" { PROVENANCE_BOOST } else { 0.0 };
let final_score = base_score + co_access + prov;
```

`PROVENANCE_BOOST = 0.02` — smaller than co-access max (0.03), so it acts as a tiebreaker rather than a dominant signal.

## Rationale

**Why query-time, not stored:**
- The stored confidence formula invariant (`sum = 0.92`) is a well-tested system property (crt-002, crt-005). Adding a category-specific stored weight would require touching `compute_confidence()` with a category parameter it currently doesn't receive, and would change how `lesson-learned` entries decay via the freshness component.
- Query-time application is already established for co-access affinity. The mechanism is proven, the location is well-understood, and the impact is bounded.

**Why 0.02:**
- Must be smaller than co-access max (0.03) to avoid overriding strong co-access signals.
- Must be large enough to break ties reliably (0.02 > float noise at f64 precision).
- Natural crt-002 decay (freshness half-life 168h) and promotion (helpful_count accumulation) handle the long-term evolution; the boost only needs to provide initial tiebreaking priority.

**Why not a per-category configurable weight:**
- Single constant is simpler and sufficient. Multiple category weights would require a data structure and parsing; not justified for v1.

## Consequences

- `W_BASE + W_USAGE + W_FRESH + W_HELP + W_CORR + W_TRUST = 0.92` stored confidence invariant unchanged.
- `PROVENANCE_BOOST` is a named constant (not magic number) — easy to tune in a follow-on.
- Applied in both UDS listener and MCP tools search path (two application sites, consistent with co-access boost pattern).
- Does not affect `context_lookup` (metadata-only, no re-ranking).
- Does not affect `context_briefing` search path — briefing uses the same search infrastructure, so lesson-learned entries get a natural boost in briefing results too.
