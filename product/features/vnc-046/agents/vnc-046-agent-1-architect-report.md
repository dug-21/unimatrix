# Agent Report: vnc-046-agent-1-architect

**Task:** Architecture + ADRs for vnc-046 — per-slug state isolation for the cloud (HTTPS)
observe path. Design artifacts only, no code.

## Deliverables
- `architecture/ARCHITECTURE.md` — system overview, component breakdown, data flow, full
  Integration Surface table (existing + new signatures), open questions.
- `architecture/ADR-001-per-slug-resolution-funnel.md` — Unimatrix #5630
- `architecture/ADR-002-per-slug-construction-parity.md` — Unimatrix #5631
- `architecture/ADR-003-boot-assertion-class-guard.md` — Unimatrix #5632
- `architecture/ADR-004-bidirectional-behavioral-isolation-seam.md` — Unimatrix #5633
- `architecture/ADR-005-issue-925-reconciliation.md` — Unimatrix #5634

## Key decisions
1. **Resolution funnel (ADR-001):** add `registry_for`/`pending_for`/`services_for` to
   `StoreResolver` beside `resolve_store`/`adapter_for`; resolve from the same
   `slug→ProjectEntry` map, O(1) Arc::clone (SR-01). `ObserveContext` reshaped to
   `{resolver, embed_service, server_version}`. No side-map (#4974 guard).
2. **Construction parity (ADR-002):** `build_project_server` sets P1 (registry+hold PAIR +
   pending), P2 (services already per-slug), P3 (5 config-snapshots via 3 new params-at-end);
   delete 2 vestigial `ObserveContext` fields. P3 in-scope.
3. **Class guard (ADR-003):** real `assert_per_slug_isolation` returning `ServerError` per built
   slug (not a debug_assert, SR-06) + compile-time exhaustive field census (no `..`) forcing
   classification of any future field (SR-02).
4. **Behavioral seam (ADR-004):** bidirectional N≥2 tests through `/v1/{slug}/...` as the primary
   gate; white-box guards complement; reuse the #800 multi-slug HTTP fixture (SR-08).
5. **#925 reconciliation verdict — NOT SUBSUMED (KEEP #925).** Different plane (metrics SQL vs
   in-memory transcript registry), different granularity (cross-feature-within-slug vs
   cross-slug), disjoint mechanisms. #925 stays open and independent; human owns the call.

## Open questions (for other agents / human)
- INV-C observability (OQ-3): confirm public surfaces for `retention_config`/`inference_config`;
  any field lacking one uses the ADR-003 boot assertion + wiring-pin as a documented AC-06
  exception (not deferred).
- #800 fixture ownership (SR-08): tester/architect confirm the fixture owner before building
  INV-C fixtures.
- P3 remains in-scope per uni-zero + researcher; if cut for speed, P1+P2 floor + human files the
  ADR-007-seam follow-up.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_get -- pattern #5629 (governing
  construction-parity + funnel-completeness invariant), vnc-038 ADR-003 #5082 (the store funnel
  this completes), ADR-007 #5135 (per-slug isolation seam), goal #5519 (OSS per-project
  isolation), #4974 (ceremonial-funnel guard), #5348/#5347/#5172 (bidirectional N≥2 test shape),
  crt-054 assert_wave_b_precondition. Read all 5 GH #930 comments (investigator + PER-SLUG-
  ROUTING-VIABLE addendum + 9-item architect inventory) and issue #925.
- Stored: entries #5630–#5634 "ADR-001…005 vnc-046" via context_store (category decision, topic
  vnc-046). Governing pattern #5629 already captured by the #930 audit — not re-stored.
- Edges: none asserted at authoring — the intra-feature Prerequisite spine (ADR-003 depends on
  ADR-001/002; ADR-001↔ADR-002) is left for retro per the typed-edge convention (assert only
  when a future agent must follow the link; siblings share the vnc-046 topic already).
- Declined: no bug-lesson for #930 (bugs are GH issues, not lessons — standing rule).
