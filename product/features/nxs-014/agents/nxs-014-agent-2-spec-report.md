# Agent Report — nxs-014-agent-2-spec

**Task:** Author SPECIFICATION.md for nxs-014 (weak-mode cross-version hash chain in `context_correct` + transport-agnostic chain-verify core via CLI).

**Produced:** `product/features/nxs-014/specification/SPECIFICATION.md`

## Summary of decisions
- Split the two-literal fix into FR-01 (struct site) + FR-02 (INSERT-bind site) as independent, non-negotiable requirements; AC-01/AC-02 require DB read-back (false-green guard, SR-06).
- Added AC-08 (reject empty predecessor `content_hash` at correction time) to make assumption A-1 testable.
- Added AC-10 (frozen-hash tripwire via `hash.rs` known-value vectors) for SR-01, AC-11 (no MCP tool) for D-4, AC-12 (no migration) for NFR-02.
- Kept the crate-home of the shared verify core as an explicit open architect decision (SR-03), not resolved in spec.

## Self-check
- [x] All SCOPE AC-01..AC-07 carried forward (plus AC-08..AC-12).
- [x] Every FR testable; ACs give verification methods.
- [x] NFRs have measurable targets (frozen vectors, schema v30, O(entries), 500-line limit).
- [x] Domain Models defines key terms.
- [x] NOT-in-scope explicit (strong cascade, anchor, DB-write adversary, backfill migration, background tick, MCP tool, supersession semantics).
- [x] No TBD/placeholder — unknowns raised as open questions.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- #5478 (KI-CHAIN-XV done_when = weak mode), #5475, ADR-004 #74 (frozen hash), lesson #3611 (multi-doc single-site fix trap → reinforces SR-06 two-site FR), pattern #4617. No storable generalizable pattern (read-only tier).
