# col-010 Architect Report

**Agent**: col-010-agent-1-architect
**Date**: 2026-03-02
**Feature**: col-010 — Session Lifecycle Persistence & Structured Retrospective

---

## Status: COMPLETE

All architecture artifacts produced and written.

---

## Artifacts Produced

| File | Description |
|------|-------------|
| `architecture/ARCHITECTURE.md` | Full architecture: 7 components, schema v5 design, migration pattern, UDS listener integration, GC cascade, retrospective tiers, lesson-learned write path, provenance boost |
| `architecture/ADR-001-abandoned-session-status-variant.md` | Distinct `Abandoned` variant in `SessionLifecycleStatus` (SR-06) |
| `architecture/ADR-002-injection-log-gc-cascade.md` | Cascading INJECTION_LOG delete on session GC (SR-04) |
| `architecture/ADR-003-batch-injection-log-writes.md` | One transaction per ContextSearch response for INJECTION_LOG (SR-12) |
| `architecture/ADR-004-lesson-learned-fire-and-forget-embedding.md` | ONNX embedding via fire-and-forget `tokio::spawn` (SR-07) |
| `architecture/ADR-005-provenance-boost-query-time-constant.md` | `PROVENANCE_BOOST = 0.02` applied at re-rank time only |
| `architecture/ADR-006-p0-p1-component-split.md` | Explicit P0/P1 delivery sequencing (SR-02) |

---

## Key Design Decisions

### Schema v5 Migration
- Two new tables: `SESSIONS` (`&str → &[u8]`) and `INJECTION_LOG` (`u64 → &[u8]`)
- `migrate_v4_to_v5()` follows the established 3-step process exactly
- `next_log_id = 0` guarded by `if_none` check — idempotent under restart (SR-05)
- Table-creation-only migration: no entry scan-and-rewrite

### SessionLifecycleStatus has 4 variants
`Active | Completed | TimedOut | Abandoned` — `Abandoned` is distinct (ADR-001, SR-06). Prevents retrospective metric contamination from cancelled sessions.

### GC Cascade is Atomic
`gc_sessions()` deletes INJECTION_LOG orphans in the same `WriteTransaction` as the SESSIONS delete. No orphaned records accumulate over time (ADR-002, SR-04).

### Batch INJECTION_LOG Writes
`insert_injection_log_batch()` is the only write API — one transaction per ContextSearch response, not one per entry. Reduces COUNTERS contention from N to 1 (ADR-003, SR-12).

### P0/P1 Split
Components 1–4 are P0 (col-011 blocking). Components 5–7 are P1 (issue #65, independent). Clear split in implementation brief protects col-011's timeline (ADR-006, SR-02).

### Confidence Formula Invariant Preserved
`PROVENANCE_BOOST = 0.02` is query-time only. `W_BASE + W_USAGE + W_FRESH + W_HELP + W_CORR + W_TRUST = 0.92` is unchanged (ADR-005).

### Lesson-Learned Embedding Fire-and-Forget
`context_retrospective` returns immediately. ONNX embedding of lesson-learned entry runs in a detached `tokio::spawn`. Embedding failure is tolerated gracefully (ADR-004, SR-07).

---

## Risks Not Fully Mitigated (Known Limitations)

### SR-09 — Concurrent Supersede Race
Two concurrent `context_retrospective` calls for the same feature_cycle can produce two active lesson-learned entries. The check-then-supersede sequence is not transactionally atomic. Accepted as a tolerated edge case — concurrent retrospective calls for the same cycle are rare in practice. Documented as a known limitation. Future fix: atomic compare-and-swap entry status in a single redb write transaction.

### SR-10 — PRODUCT-VISION.md Discrepancy
The vision document col-010 row references `session_id: Option<String>` on `EntryRecord` — explicitly a Non-Goal in SCOPE.md (bincode positional encoding requires a full scan-and-rewrite migration that is disproportionate to the benefit). This needs correction in PRODUCT-VISION.md to avoid confusing future agents.

---

## Pre-Implementation Gate Check Required

Per SR-01: **col-009 must be merged and all col-009 acceptance criteria must pass** before col-010 implementation begins. The `SessionClose` handler design depends on `SignalOutput.final_outcome` from col-009's `drain_and_signal_session()`. The SESSIONS table schema v5 migration depends on schema v4 (SIGNAL_QUEUE) being in place. Do not begin col-010 implementation on col-009 branches.

---

## Codebase References

- Migration pattern: `crates/unimatrix-store/src/migration.rs` — `migrate_v3_to_v4()` is the direct template for `migrate_v4_to_v5()`
- UDS dispatch: `crates/unimatrix-server/src/uds_listener.rs` — `dispatch_request()`, `handle_context_search()`, `process_session_close()`
- Retrospective types: `crates/unimatrix-observe/src/types.rs` — `RetrospectiveReport`, `HotspotFinding`, `EntryAnalysis`
- Retrospective assembly: `crates/unimatrix-observe/src/report.rs` — `build_report()` is the template for the tiered output refactor
- Co-access boost pattern: `crates/unimatrix-engine/src/confidence.rs` — `co_access_affinity()` and `rerank_score()` are the templates for `PROVENANCE_BOOST` application
