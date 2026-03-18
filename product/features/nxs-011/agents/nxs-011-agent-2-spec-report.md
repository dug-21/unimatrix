# Agent Report: nxs-011-agent-2-spec

**Agent**: Specification Writer
**Feature**: nxs-011 — sqlx Migration: Connection Pools + Async-Native Storage
**Output**: `product/features/nxs-011/specification/SPECIFICATION.md`

---

## Summary

SPECIFICATION.md written. 20 acceptance criteria, 17 functional requirements, 7
non-functional requirements, 10 explicit exclusions.

Key decisions reflected:
- Native `async fn` in EntryStore trait (RPITIT, Rust 1.89) — FR-09, AC-20
- Store owns drain task (started in Store::open(), shutdown via oneshot) — FR-04, AC-19
- Pool acquire timeout values deferred to architect ADR — FR-02, OQ-01
- AsyncVectorStore/AsyncEmbedService untouched — C-06, NOT IN SCOPE

---

## Acceptance Criteria Count: 20

AC-01 through AC-16 from SCOPE.md, plus:
- AC-17: migration regression harness (all 12 schema versions, SR-04)
- AC-18: shed counter in context_status output (SR-08)
- AC-19: Store::close() awaits drain task (SR-09)
- AC-20: impl-completeness tests replace object-safety tests (SR-02)

---

## Key Constraints

- write_pool max_connections hard cap ≤ 2, startup rejection (C-01, NF-01)
- async fn in traits via RPITIT only — no async_trait crate (C-02)
- Migration connection opened and closed before pool construction (C-03)
- Integrity writes never shed (C-04)
- Zero spawn_blocking in store crate post-migration (C-05)
- AsyncVectorStore/AsyncEmbedService frozen (C-06)
- Schema version stays at 12 — no DDL (C-07)
- AnalyticsWrite #[non_exhaustive] (C-08)
- unimatrix-observe migrates in same wave (C-09)
- sqlx-data.json regeneration discipline (C-10)

---

## Open Questions for Architect

- OQ-01: Pool acquire timeout values (suggested 2s read / 5s write — needs ADR before AC-10 is fully testable)
- OQ-02: SqliteWriteTransaction replacement API shape — typed wrapper vs. direct pool.begin().await
- OQ-03: AnalyticsWrite variant field sets — need cross-reference against schema v12 DDL
- OQ-04: sqlx-data.json placement — workspace-level vs. per-crate
- OQ-05: Drain task grace period configurability for test contexts

---

## Knowledge Stewardship

- Queried: /uni-query-patterns for async storage pool connection sqlx spawn_blocking — entries #735, #731, #1367, #1758, #1915 found; established batching and spawn_blocking patterns confirmed.
- Queried: /uni-query-patterns for analytics write queue drain task background writes shed policy — entry #2057 (store-owned background task shutdown protocol) directly shaped FR-04, AC-19, and test infrastructure constraints TC-02 and TC-05.
