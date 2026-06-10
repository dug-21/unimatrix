# crt-053 Architect Report — agent-1-architect

## Deliverables
- ARCHITECTURE.md: `product/features/crt-053/architecture/ARCHITECTURE.md`
- ADR-001: `product/features/crt-053/architecture/ADR-001-active-only-ppr-seeds.md` (Unimatrix #4917)

## Key decisions
- Single production change: filter `seed_ids` to `e.status == Status::Active` inside the `if self.ppr_expander_enabled` branch at search.rs:~915. `results_with_scores` itself is NOT filtered.
- Typed `Status::Active` predicate (enum, not string). Drops Deprecated/Proposed/Quarantined seeds; 6b terminal-active heads pass by construction (SR-02).
- Off-path identity (C-02) is structural — filter is lexically inside the expander block, touches only local `seed_ids`.
- Validation behavior-based only (SR-01): seed-exclusion fixture (deprecated A superseded_by active B) + off-path identity + unchanged ranking. No eval-harness gate, no soft-GT P@5.
- All five SCOPE Locked Decisions carried verbatim into ARCHITECTURE constraints (SR-03 / vnc-018 #4495 precedent). Excluded mechanisms explicitly listed as NOT designed.

## Confirmed against source
- `seed_ids` build at search.rs:915 from full pool; quarantine-only filter at :950.
- 6b terminal-active guard `terminal.status != Status::Active` at :814.
- `Status` enum repr(u8)+PartialEq, 4 variants, schema.rs:8-15; `EntryRecord.status` schema.rs:57.
- `graph_expand` signature unchanged.

## Open questions
1. Fixture host: nan-018 corpus (needs ass-073 positive-edge revision) vs Python integration suite — Delivery/Tester picks. Not an architecture blocker.
2. If the fixture reproduces #406, that signals fixture divergence from ass-073's eval graph — raise, do not patch retrieval (per SCOPE).

## Edges
None asserted. No relationship meets the HIGH traversal-necessity bar. Supersession of the prior crt-053 draft is a `context_correct` concern for the leader, not an edge.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- returned PPR/graph_expand/penalty-map prior decisions; most load-bearing: #4887 (frozen penalty/filter map leaks when later stages admit candidates — the exact structural basis for this fix), #481 (two-mode RetrievalMode, C-03), crt-042 Phase 0 decisions (#4049/#4050), #3768 (TypedRelationGraph active-only filter precedent). Applied all in ARCHITECTURE/ADR.
- Stored: entry #4917 "ADR-001: Active-Only PPR Expansion Seeds at Phase 0" via context_store (decision, crt-053). Nothing else novel — the recurring pattern (#4887) already exists; this feature is its surgical application, not a new generalizable pattern.
