# Agent Report — vnc-048-agent-2-spec

## Task
Produce SPECIFICATION.md for vnc-048 (add `--slug` to export/import for per-slug backup/restore) from approved SCOPE.md and SCOPE-RISK-ASSESSMENT.md.

## Deliverable
`product/features/vnc-048/specification/SPECIFICATION.md`

## Summary
- 16 functional requirements (slug surface, existence-check-before-open, export/import specifics, help+README).
- 9 non-functional requirements (fallthrough parity, fail-loud with resolved path, four-shape coverage, no-unwrap, sync pre-tokio, no new base mechanism, blast radius, structural traversal closure, no filter change).
- All 13 ACs mapped with an explicit verification method each; AC-09 (seam test — resolvers must disagree) and AC-10 (full round-trip all tables + hash/chain) called out as highest weight.
- Domain models define both resolvers, base, slug store, ProjectSlug, registered-vs-store-dir, and the four deploy shapes.
- Constraints C-1..C-10 carried; SR-01..SR-11 woven into ACs and NFRs.
- OQ-1..OQ-4 honored as resolved; OQ-5 explicitly deferred to vision session.

## Key Decisions
- Treated the four deploy shapes as a coverage axis (NFR-3) per risk recommendation, requiring resolve-or-fail-loud in each rather than one representative shape.
- Made "fail loud with fully-resolved absolute path" a cross-cutting NFR (NFR-2) covering all four accept-but-inert paths (missing store, non-empty audit, host base miss, live daemon).
- Export stderr summary (FR-8) specified for both slug and no-slug modes since it reports pre-existing behavior; no-slug behavior parity (NFR-1) still holds because it is additive stderr, not a path change — flagged for architect confirmation implicitly via AC-05.

## Open Questions
- None blocking. One design-time item for the architect: the exact `None`-fallback idiom for `data_dir.parent()` (SCOPE cites `main.rs:1287` / `projects.rs:181-185`); spec requires only no-unwrap + fail-loud-with-resolved-path.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing (vnc-048) — surfaced personal-cloud capability cluster (#5591, #5565, #5533) and per-slug sibling-dir pattern #4972; confirmed slug resolver placement and single-`.unimatrix`-base model. No conflicting conventions.
- Declined: no storage — spec decisions are feature-specific interpretations of resolved scope, not generalizable patterns (read-only tier per role). Promotable at retro if any generalizes.
