# nan-022 Architect Agent Report

**Agent:** nan-022-agent-1-architect
**Task:** Architecture for the cross-transport parity suite (C0 proof artifact, #837).

## Deliverables

- `product/features/nan-022/architecture/ARCHITECTURE.md`
- 7 ADR files, `architecture/ADR-001..007-*.md` (also stored in Unimatrix #5305–#5311).

## Key design

Generalize the nan-021 single-`MetricVector` parity gate into a **dimension-keyed parity
matrix** driven by ONE authoritative dimension registry. One workload / one identity / one
token / pytest-as-orchestrator / closed-justified-exclusion comparator are all preserved
verbatim; the only change is one output → N. The five net-new dimensions get comparators on a
single base-class framework; analytics consumes the nan-021 comparator unchanged (AC-04).

Risk dispositions encoded structurally:
- **SR-02 / OQ-2:** four-valued outcome model — INFRA-ERROR (transport-health preflight +
  bounded connect/idle deadlines, so a #839 half-open hang is never a parity verdict) and
  INTRA-TRANSPORT-NONDETERMINISM (double-capture-and-diff; GH#746 HNSW flips routed OUT of the
  red gate to a separate bug) are STRUCTURALLY distinct from PARITY-FAIL.
- **SR-03 / SR-05 / #5302:** ONE ranking tolerance policy (stable-prefix + tie-class) shared by
  retrieval + briefing; comparator discipline is a base class + ONE forbidden-seed set + a
  cross-dimension drift guard — convention replaced by a guard.
- **SR-07 / OQ-3:** PreCompact stays in scope; capture shape carries `measurable`/`host_side_gap`
  so any undrivable host-side component is a documented delivery-time call-out, never a silent
  drop.
- **SR-06 / SR-08:** augmented single workload (deterministic seed-corpus + query phase) for
  non-degenerate ranking; two-HTTPS-surface routing keyed by the registry with never-empty
  capture.

No production code; all output extends `product/test/infra-001/` cumulatively. No nan-021 ADR
superseded (extension, not invalidation).

## Open questions

- OQ-A (human): must all six dimensions pass to flip C0, or only the three `done_when` pillars?
  `Dimension.blocks_c0_proof` makes this a data change. Default all six True.
- OQ-B (delivery): PreCompact host-side measurability — decide at first live drive.
- OQ-C (spec): seed-corpus size + query set numbers.
- OQ-D (delivery): `Informs`-edge + phase-signal determinism for analytics — first live run.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_get/lookup — surfaced C0 (#5191), the full
  nan-021 ADR set (#5286 substrate, #5294 bridge-in-path, #5293 comparator contract, #5291
  durability barrier, #5289 derived-attribution, #5290 release-gate lane), the #5298 RecordEvent
  sequence, and the #5302 single-source-the-contract lesson. All applied.
- Stored: ADRs #5305–#5311 via context_store (category decision, topic nan-022). #5307 (ADR-003)
  carries a `Supports` edge to #5302 (it is the structural implementation of that lesson;
  traversal-necessary for any future agent editing the comparators). No general architect-topic
  pattern stored yet — the multi-output parity-matrix generalization is a candidate to store on
  completion if it proves out, not at design time (per scope guidance).
