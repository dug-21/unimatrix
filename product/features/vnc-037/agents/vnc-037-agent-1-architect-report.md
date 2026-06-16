# vnc-037 — Architect Agent Report (REVISION under next-hop reframe)

Agent: vnc-037-agent-1-architect

## Summary

Revised the vnc-037 architecture and ADRs for the human-directed reframe: `context_get`
edges are a **next-hop navigation affordance** with **ranking as the core** (cap 3), plus a
**symmetric-edge canonicalization** blocker. Updated the existing ARCHITECTURE.md + 4 of 5
ADRs in place; ADR-003 confirmed unchanged; added 2 new ADRs.

## Artifacts

- `product/features/vnc-037/architecture/ARCHITECTURE.md` (rewritten for the reframe)
- `product/features/vnc-037/architecture/ADR-001-reuse-direct-neighbors-read-path.md` (CHANGED)
- `product/features/vnc-037/architecture/ADR-002-get-edge-shape-projection-discovery-list.md` (MINOR UPDATE)
- `product/features/vnc-037/architecture/ADR-003-serializer-seam-none-key-absent.md` (UNCHANGED)
- `product/features/vnc-037/architecture/ADR-004-additive-source-rawedgerow.md` (EXTENDED)
- `product/features/vnc-037/architecture/ADR-005-json-totals-and-empty-subgroup-rendering.md` (UPDATED)
- `product/features/vnc-037/architecture/ADR-006-ranking-rule-authored-first-target-confidence.md` (NEW)
- `product/features/vnc-037/architecture/ADR-007-symmetric-edge-canonicalization.md` (NEW)

## Unimatrix ADR IDs

| ADR | File | Unimatrix | Action |
|-----|------|-----------|--------|
| ADR-001 | reuse-direct-neighbors-read-path | #5009 → **#5014** | context_correct |
| ADR-002 | get-edge-shape-projection | #5010 → **#5015** | context_correct |
| ADR-003 | serializer-seam-none-key-absent | **#5011** | unchanged (confirmed) |
| ADR-004 | additive-source-rawedgerow | #5012 → **#5016** | context_correct |
| ADR-005 | json-totals-and-rendering | #5013 → **#5017** | context_correct |
| ADR-006 | ranking-rule | **#5018** | context_store (Prerequisite → #5016) |
| ADR-007 | symmetric-canonicalization | **#5019** | context_store |

Typed edge: ADR-006 (#5018) `Prerequisite` → ADR-004 (#5016) — an agent implementing the
ranking must read ADR-004 first to know `entries.confidence` is JOINed (LEFT) as the rank
key; following the link avoids re-inventing the rank source or using `graph_edges.weight`.
Other intra-feature `Prerequisite` spine links left for retro (sibling IDs now exist but the
guardrail prefers completing the spine at retro).

## Key decisions

- **Rank-and-limit in SQL (ADR-001).** Two bounded queries: a ranked `LIMIT 3` select with a
  confidence JOIN + canonicalization, and a separate split `COUNT(*)`. Bounds hub fan-out
  (SR-14). Opt-out skips both.
- **Ranking rule (ADR-006).** `ORDER BY (source='agent') DESC, t.confidence DESC NULLS LAST,
  target_id ASC LIMIT 3`. Authored-first; inferred by **target-entry confidence**, NOT
  `graph_edges.weight` (frozen per ass-079). Carried-forward/`context_edge` classify authored
  via `source='agent'` (#4984) — SR-10 locked by named test.
- **Symmetric canonicalization (ADR-007, the SR-08 blocker).** Collapse
  Contradicts/CoAccess/Informs to one `↔` edge in SQL **before** `ORDER BY…LIMIT` AND
  `COUNT(*)`; display and totals both dedup; tested on both surfaces independently.
- **Rendering (ADR-005).** Markdown author/inferred sub-split DROPPED (ranking front-loads
  authored) → flat ranked list; `↔` glyph + `direction:"both"`; totals count symmetric once;
  `…N more — use context_graph` pointer. JSON `edge_totals` nested-object shape unchanged.
- **Shared-path firewall (ADR-004).** Plain `query_direct_neighbors`/`neighbors_sql` gains
  only `source`; the JOIN/canonicalization/LIMIT live on a separate ranked variant — keeps
  `context_graph` neighbors byte-stable (SR-02/SR-06). Still additive, no migration.

## Risk coverage

- SR-08 (blocker): ADR-007 — canonicalize in SQL before ORDER BY/LIMIT and before COUNT(*);
  both surfaces deduped; test independently.
- SR-09/SR-10: ADR-006 — exact ORDER BY locked; carried-forward/`context_edge` classify
  authored via `source='agent'` (#4984), named test.
- SR-11: ADR-006/004 — LEFT JOIN + `NULLS LAST`, dangling targets retained and ranked
  deterministically last.
- SR-12: AC-12 — flagged OQ-C (measured edge-free baseline incl. high-degree node before
  number lock); design makes opt-out skip all edge work.
- SR-14: ADR-001/007 — LIMIT + COUNT in SQL, never full fan-out in memory.

## OQ-03 resolution

**Default OFF for internal/programmatic callers; default ON only at the agent-facing MCP
tool boundary.** The hook path, briefing pipeline by-ID fetches, and by-ID loop fetches pay
the cost but never consume the affordance — they pass `include_edges: Some(false)` (opt-out
skips both queries). The `context_get` MCP tool stays default-on (`None`). Spec should
enumerate the exact internal call sites and make each `Some(false)` an asserted test.
Advisory to human/spec; D-01 keeps the field additive and default-on at the type level.

## Open questions (downstream)

- OQ-A: degrade-vs-fail on edge-query error (architecture leans fail; spec/human call).
- OQ-B: pre-authorize sibling modules (`graph_queries_ranked.rs`, `mcp/get_edges.rs`,
  `response/edges.rs`) for the 500-line limit.
- OQ-C: AC-12 numbers need a measured edge-free baseline (incl. high-degree node) before lock.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- returned prior vnc-037 ADRs (#5009-#5013), context_graph EdgeRecord/neighbors ADRs (#4478/#4479/#4461), vnc-035 carry-forward ADRs (#4983/#4984 confirm carry-forward + context_edge stamp source='agent', locking SR-10). Applied all.
- Stored: corrected #5009→#5014, #5010→#5015, #5012→#5016, #5013→#5017 via context_correct (provenance preserved); stored #5018 "ADR-006 ranking rule" and #5019 "ADR-007 symmetric canonicalization" via context_store. One typed edge: #5018 Prerequisite → #5016.
