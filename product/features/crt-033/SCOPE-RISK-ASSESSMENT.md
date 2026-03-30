# Scope Risk Assessment: crt-033

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | `RetrospectiveReport` may not be fully `Serialize + Deserialize` — nested types in `unimatrix-observe` (e.g. `Hotspot`, `evidence` fields) could lack derived impls, causing a compile-time or runtime failure at JSON round-trip | High | Med | Architect must audit all `RetrospectiveReport` field types for serde impls before committing to direct serialization; plan a DTO shim if any field is non-serializable (AC-16) |
| SR-02 | `summary_json` blob size is unbounded at write time — SCOPE estimates "under 1MB" but full hotspot evidence for large cycles could exceed SQLite TEXT performance thresholds and inflate DB file size over time | Med | Low | Architect should define a hard byte-limit guard or document the accepted ceiling; consider whether a size-check assertion belongs in the store layer |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-03 | `SUMMARY_SCHEMA_VERSION` is scoped as a unified constant covering both detection-rules version and JSON structure, but these evolve at different rates — a detection-rule change in `unimatrix-observe` forces a version bump even if the serialization schema is stable, triggering spurious advisory messages to callers | Med | Med | Spec writer should clarify the bump policy and whether callers can distinguish rule-staleness from structural incompatibility; if not, document the trade-off explicitly |
| SR-04 | `pending_cycle_reviews` K-window boundary depends on a constant from GH #409 which is not yet merged — the scope defers coordination to delivery ("use a config default if #409 is not yet merged"), risking an arbitrary placeholder that diverges from the actual retention policy | Med | High | Spec writer must define the fallback default value and document that it must be reconciled with #409 at merge time; architect should ensure the constant is not inlined |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-05 | Schema v17→v18 migration requires updating `CURRENT_SCHEMA_VERSION`, the migration block, `create_tables_if_needed()` in `db.rs`, column-count structural tests, and SQLite parity tests — historically this cascade has caused gate failures when any touchpoint is missed (entry #3539) | High | Med | Architect must reference the schema cascade checklist (entry #3539) and spec writer must enumerate all five touchpoints as explicit ACs |
| SR-06 | Synchronous `store_cycle_review()` write on the handler return path (constraint: not fire-and-forget) adds latency to every first-call `context_cycle_review` response — the write-pool is shared with other handlers and `tools.rs` is already large | Med | Med | Architect should verify write-pool contention behavior under concurrent first-call scenarios and consider whether the store write should use a dedicated connection |
| SR-07 | The `force=true` + purged-signals path requires distinguishing between "empty attributed observations because signals were purged" vs. "empty because the cycle never had signals" — the three-path observation load cannot currently distinguish these cases, risking silent wrong-path selection | High | Med | Architect must design a reliable signal-absence discriminator; spec writer should add an AC covering the ambiguous-empty case |

## Assumptions

- **Section "Proposed Approach / pending_cycle_reviews"**: Assumes `query_log.feature_cycle` is reliably populated for all K-window cycles. If `query_log` rows lack `feature_cycle` (NULL or empty), the set-difference query silently under-reports pending reviews. The scope does not address NULL-handling in this query.
- **Section "summary_json content"**: Assumes `RetrospectiveReport` struct is stable enough for durable storage at v1. Any field addition to the struct after first writes creates a deserialization gap for stored records until `force=true` is used or schema version is bumped.
- **Section "Constraints / write_pool_server()"**: Assumes the synchronous write completes fast enough not to meaningfully degrade `context_cycle_review` latency. This has not been benchmarked.

## Design Recommendations

- **SR-01 + SR-07**: The two highest-severity risks both concern correctness of the memoization boundary — the architect should resolve serde completeness (SR-01) and signal-absence discrimination (SR-07) before specifying the handler flow in detail.
- **SR-04**: Pin the K-window fallback default in the architecture or specification; do not defer to delivery.
- **SR-05**: Explicitly list all schema cascade touchpoints in the spec ACs — the existing AC-13 covers only the migration test, not the `create_tables_if_needed()` mirror, column-count tests, or parity tests.
