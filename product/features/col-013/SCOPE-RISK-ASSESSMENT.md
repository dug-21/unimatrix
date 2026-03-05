# Scope Risk Assessment: col-013

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | Background `tokio::spawn` + interval timer may not execute if all runtime threads are blocked by `spawn_blocking` maintenance/extraction work, causing tick starvation | Med | Low | Architect: ensure the interval timer runs on the tokio async runtime, not in a blocking thread. Only the actual work (Store access, embedding) should use `spawn_blocking`. Size the blocking thread pool or use a dedicated runtime if starvation is observed. |
| SR-02 | Extraction rules query the `observations` table across all features, which may grow to millions of rows over time. Full-table scans for cross-feature pattern detection could degrade performance. | Med | Med | Architect: design extraction queries with SQL indexes and bounded time windows (e.g., last 90 days). Use LIMIT/OFFSET for large result sets. Consider a last-processed watermark to avoid re-scanning old data. |
| SR-03 | The quality gate pipeline runs 6 sequential checks per proposed entry, including embedding generation (for near-duplicate check) and contradiction scanning. Under high extraction volumes this could block the maintenance tick for extended periods. | Med | Low | Architect: enforce rate limiting early in the pipeline (before expensive checks). Cap total quality gate time per tick. Consider yielding between entries to avoid blocking other maintenance tasks. |
| SR-04 | Moving observation types (`ObservationRecord`, `HookType`, `ParsedSession`, `ObservationStats`) from unimatrix-observe to unimatrix-core changes the public API of both crates. Any external consumer (if any) would break. | Low | Low | Spec: this is a mechanical import refactor. Re-export the moved types from unimatrix-observe for backward compatibility. Ensure all 14 affected files compile and tests pass. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-05 | Adding `unimatrix-store` as a dependency of `unimatrix-observe` creates a new crate coupling direction. Currently unimatrix-observe is storage-independent (ADR-001 from col-012). This may complicate future crate boundary changes. | Med | Med | Architect: evaluate whether extraction rules should live in unimatrix-observe at all, or whether a thin `extraction` module in unimatrix-server (where Store is already available) is cleaner. If unimatrix-observe gains store dependency, document the architectural rationale as an ADR. |
| SR-06 | The CRT refactors (crt-002, crt-003, crt-005) are scoped as minor changes (~75 lines) but touch the confidence computation hot path, contradiction scanning, and status reporting. Regressions here affect the entire knowledge lifecycle. | High | Low | Spec: each CRT refactor must have dedicated tests. The crt-002 trust_score change needs a unit test for the "auto" value. The crt-003 extraction needs a test proving the extracted function produces identical results to the batch scan. |
| SR-07 | Silently ignoring `maintain=true` means existing agents that depend on maintenance running will see no error but also no maintenance. If the background tick fails to start (e.g., server startup race condition), maintenance stops entirely with no diagnostic signal. | Med | Med | Architect: ensure the background tick starts reliably during server initialization. Add a `last_maintenance_run` field to `context_status` output so agents/humans can detect if maintenance is not running. Log a warning if tick has not run in 2x the configured interval. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-08 | The extraction pipeline both reads from `observations` table and writes to `entries` table via Store API. If extraction runs concurrently with MCP tool calls that also write entries, SQLite write contention (SQLITE_BUSY) may occur. | Med | Med | Architect: extraction writes should use the same `spawn_blocking` + store locking pattern as existing writes. Consider batching extraction writes to minimize lock duration. |
| SR-09 | Auto-extracted entries with `trust_source: "auto"` will appear in `context_search` and `context_briefing` results alongside human/agent-authored entries. If extraction rules produce low-quality entries, they could degrade the overall search experience. | High | Med | Spec: the confidence floor (0.2) and cross-feature validation (2+ features) provide baseline quality. Additionally, the trust_score weight (0.35 for "auto") means auto-extracted entries will naturally rank lower in search results via the confidence re-ranking formula. Monitor entry quality in the first few feature cycles. |
| SR-10 | The `ExtractionRule` trait signature takes `&Store` directly, coupling extraction rules to the concrete store implementation. If the store interface changes, all extraction rules must be updated. | Low | Low | Architect: consider whether extraction rules should use a trait object or a subset of store operations instead of `&Store`. However, pragmatism suggests starting with `&Store` and abstracting later if needed. |

## Assumptions

1. **col-012 observations table is populated with data** -- extraction rules require observation data across multiple features to produce meaningful results. If the observations table is empty or has data from only 1 feature, no entries will be extracted (by design: cross-feature validation gate). First meaningful extractions expected after 3-5 features with observation data.
2. **The tokio runtime has sufficient capacity** for a background timer + periodic spawn_blocking work alongside MCP request handling. The server is single-user (per-session) so concurrent load is inherently bounded.
3. **Auto-extracted entries do not require human review before activation** -- they are stored as Active with appropriate confidence scores. The quality gate pipeline is the sole quality barrier. A future `context_review` tool (crt-009) will add human-in-the-loop review for Proposed entries.
4. **The observations table schema (v7) is stable** -- col-013 does not modify the observations table schema. If future features add columns, extraction rules may need updates but existing rules will continue to work on the original columns.

## Design Recommendations

- **SR-01/SR-03**: Architect should design the background tick as a lightweight async coordinator that dispatches work to `spawn_blocking` tasks, with timeouts on each maintenance phase.
- **SR-02**: Use a high-watermark pattern for extraction: track `last_processed_observation_id` to avoid re-scanning the entire table each tick.
- **SR-05**: Document the decision to add Store dependency to unimatrix-observe as an ADR, explicitly acknowledging the trade-off between crate purity and code colocation.
- **SR-06**: Each CRT refactor should be independently testable with existing test infrastructure.
- **SR-07**: The background tick should log its start, completion, and any errors at INFO level for observability.
- **SR-09**: Consider adding a `trust_source` filter to `context_search` to allow agents to exclude auto-extracted entries if desired (future enhancement, not in scope).
