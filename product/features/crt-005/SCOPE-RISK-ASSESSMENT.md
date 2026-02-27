# Scope Risk Assessment: crt-005

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | Schema migration v2->v3: `EntryRecord.confidence` f32->f64 changes bincode serialization (4->8 bytes). If migration reads an entry but fails mid-write, partially migrated database has mixed f32/f64 entries, causing deserialization panics on subsequent reads. | High | Med | Architect must ensure migration is atomic (all-or-nothing). Test: corrupt mid-migration, verify rollback or crash-safe resume. |
| SR-02 | f64 scoring upgrade touches every crate: EntryRecord (store), SearchResult (core), confidence weights (server), rerank_score (server), co_access_affinity (server), update_confidence trait method (core). A type mismatch anywhere causes compile failure, but subtle f32 literal constants missed in the sweep cause silent truncation. | High | High | Architect should produce an exhaustive list of every f32 scoring constant and function signature across all 5 crates. Grep for `f32` in scoring paths. |
| SR-03 | hnsw_rs has no point deletion API. Graph compaction requires full HNSW rebuild from scratch. If rebuild fails (OOM, embed service timeout, panic in hnsw_rs), the old index is destroyed and search is broken until server restart. | High | Low | Architect must design compaction as build-new-then-swap, never in-place mutation. Old index retained until new index is fully constructed and validated. |
| SR-04 | Graph compaction requires embed service for re-embedding. The embed service is lazily loaded (ONNX runtime). If unavailable (model file missing, first call before init), compaction silently skips or panics. | Med | Med | Architect should gate compaction on embed service readiness check. Report "compaction skipped: embed service unavailable" in maintenance recommendations. |
| SR-05 | Lambda threshold default 0.8 and staleness default 24h are arbitrary. If 0.8 is too aggressive, every `context_status` call emits noisy recommendations. If 24h is too short, every call triggers mass confidence refresh writes. | Med | Med | Architect should make all thresholds named constants with clear documentation. Consider whether initial defaults should be conservative (high lambda threshold, long staleness window) to avoid surprises. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-06 | Scope has 9 goals and 32 acceptance criteria -- the largest crt-phase feature by far. Risk of partial delivery where some goals ship and others are deferred, leaving the feature in an inconsistent state (e.g., lambda computed but compaction not wired). | High | Med | Architect should define a minimal viable subset (f64 upgrade + lambda computation) vs. full scope (+ compaction + refresh). Ensure each subset is independently coherent. |
| SR-07 | `context_status` transitions from diagnostic (read-only) to maintenance (read+write). Callers (agents, monitoring) that call `context_status` frequently may trigger unintended bulk writes (confidence refresh, compaction). Behavioral contract change is implicit. | Med | High | Spec writer should document the behavioral change. Consider a parameter to opt out of maintenance writes (e.g., `maintenance: false` to get scores without triggering refresh/compaction). |
| SR-08 | Embedding consistency dimension defaults to 1.0 (healthy) when checks are not performed (AC-05). This inflates lambda for callers who never opt in to embedding checks, masking potential degradation. | Med | Med | Architect should consider whether unavailable dimensions should be excluded from the weighted average rather than defaulted to 1.0. Document the trade-off either way. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-09 | 811 existing tests, many with hardcoded f32 confidence values and f32 comparison assertions. The f32->f64 promotion will cause widespread test failures requiring manual updates to expected values and comparison tolerances. | Med | High | Architect should estimate the blast radius (number of tests touching confidence/similarity types). Plan for a bulk update pass, not incremental fixes. |
| SR-10 | `update_confidence` Store trait signature changes from f32 to f64. This is a breaking change for the trait and all implementations (RedbStore, test mocks). Object safety must be preserved. | Med | High | Architect should verify trait object safety is maintained. All test mocks and fakes must be updated in the same commit as the trait change. |
| SR-11 | Confidence refresh writes during `context_status` create write contention with concurrent tool calls. If an agent calls `context_store` while `context_status` is refreshing confidence for the same entry, the write transactions serialize on redb, but the confidence value may be immediately stale again. | Low | Low | Accept at current scale (single-agent stdio). Document as a known limitation for future multi-agent scenarios. |

## Assumptions

1. **bincode f32->f64 migration is lossless** (SCOPE: "f32 as f64 is exact"). True for IEEE 754, but assumes bincode v2 deserializes f32 bytes correctly when the target struct field is f64. The migration must read-as-f32-then-cast, not read-as-f64-directly. (SCOPE section: Constraints, schema migration v2->v3)
2. **hnsw_rs rebuild produces equivalent search results** (SCOPE: AC-20). Assumes HNSW construction is deterministic enough that the same entries produce functionally equivalent (not bit-identical) search rankings. Non-deterministic construction order could shift borderline results. (SCOPE section: Acceptance Criteria)
3. **`max(updated_at, last_accessed_at)` is a reliable staleness proxy** (SCOPE section: Staleness Detection). Assumes fire-and-forget confidence updates on retrieval consistently update `last_accessed_at`. If any retrieval path skips this update, entries appear staler than they are, triggering unnecessary refresh. (SCOPE section: Background Research)
4. **Current scale (<1000 entries) makes full HNSW rebuild acceptable** (SCOPE section: VectorIndex Compaction Design). At 10K entries the scope estimates 5-10 seconds. If entry count grows faster than expected, compaction blocks `context_status` for an unacceptable duration. (SCOPE section: Background Research)

## Design Recommendations

1. **(SR-01, SR-03)** Design all destructive operations (schema migration, graph compaction) as build-new-then-swap with rollback on failure. Never mutate in place. Migration should write to a new table/column, validate, then swap.
2. **(SR-02, SR-09, SR-10)** Produce an exhaustive inventory of every f32 scoring site before implementation. Treat the f64 upgrade as a cross-crate refactor with a dedicated test update pass. Consider doing the f64 upgrade as the first implementation step so all subsequent work builds on the new types.
3. **(SR-06)** Define two delivery tiers: Tier 1 (f64 upgrade + schema migration + lambda computation + dimension scores) and Tier 2 (confidence refresh + graph compaction + maintenance recommendations). Tier 1 is read-only and low-risk. Tier 2 adds write behavior and is higher-risk.
4. **(SR-07)** Add a `maintenance: bool` parameter (default true) to `context_status` so callers can opt out of write-side effects when they only need diagnostic scores.
