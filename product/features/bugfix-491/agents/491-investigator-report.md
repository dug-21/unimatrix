# 491-investigator — Diagnosis Report

## Root Cause

`compute_graph_cohesion_metrics()` in `crates/unimatrix-store/src/read.rs:1020` uses an inclusive
`CASE WHEN source = 'nli' THEN 1 ELSE 0 END` filter to count `inferred_edge_count`. This was correct
when NLI was the only inference source, but silently excludes all newer sources added by crt-040
(`cosine_supports`) and crt-041 (`S1`, `S2`, `S8`). The result is that `inferred_edge_count` in
`context_status` reports a small and shrinking fraction of all inferred edges.

## Affected Files

- `crates/unimatrix-store/src/read.rs` — SQL (line 1020), struct doc (line 1762), tests
- `crates/unimatrix-server/src/mcp/response/status.rs` — field doc (line 85), label string (line 562)
- `crates/unimatrix-server/src/services/nli_detection_tick.rs` — TC-15 (line 3227)

## Proposed Fix

Option B: replace inclusive `source = 'nli'` with exclusive `source NOT IN ('co_access', '')` filter.
Counts all inference sources automatically without requiring future updates when new sources are added.
Use `EDGE_SOURCE_CO_ACCESS` Rust constant via `format!()` — not a bare string literal.

## Risk Assessment

Low blast radius. `inferred_edge_count` is a monitoring-only field in `context_status`. A subtle bug
produces a wrong count in the status report but does not affect search ranking, confidence scoring,
or any write path.

## Missing Test

TC-15 asserted NLI-only semantics. Should be rewritten as a table-driven test covering all inference
sources and verifying `co_access` is excluded.

## Knowledge Stewardship

Queried:
- `context_briefing` (task-scoped) — returned entries #3591, #4027, #4056, #4063, #4046; confirmed
  crt-041 ADR-004 explicitly deferred per-source counts to a follow-up issue.
- `context_search` ("per-source edge count breakdown") — no additional signal beyond briefing results.

Stored:
- Entry #4167: "Inclusive single-source SQL filter silently undercounts when new EDGE_SOURCE_* constants
  are added" — stored via `/uni-store-lesson` to surface for future investigators hitting the same pattern.
