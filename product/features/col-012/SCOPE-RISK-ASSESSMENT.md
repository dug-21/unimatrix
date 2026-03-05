# Scope Risk Assessment: col-012

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | Schema migration v6->v7 on existing production database may fail or corrupt data if observations table conflicts with future schema needs | Med | Low | Architect: design observations schema with extensibility in mind; migration must be idempotent |
| SR-02 | SQLite WAL mode write contention: high-frequency hook events (every tool call) writing to observations table may cause SQLITE_BUSY under burst conditions | Med | Med | Architect: consider batching writes or ensuring timeout/retry on SQLITE_BUSY; measure write latency |
| SR-03 | Observation table growth unbounded: no retention policy means disk usage grows indefinitely for long-running projects | Med | High | Architect: define retention strategy (e.g., 60-day cleanup matching current JSONL policy) |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | ADR-001 independence boundary: if trait abstraction is poorly designed, it may leak storage details into unimatrix-observe or create an over-engineered interface for a simple query | Med | Med | Architect: keep the abstraction minimal -- likely a single function signature returning Vec<ObservationRecord> |
| SR-05 | "Same results as JSONL" (AC-07) is hard to verify without dual-write: no mechanism to compare old vs new path on production data | Low | Med | Spec: define what "same results" means operationally -- same detection rules fire, same metric computation, not byte-identical output |
| SR-06 | Removing JSONL path removes the only offline/fallback observation mechanism: if UDS server is not running, hook events are silently lost | Med | Med | Architect: assess whether silent loss is acceptable or if hooks need a fallback buffer |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | Retrospective tool (context_retrospective) has two code paths: JSONL-based and structured-events-based. Migration must handle or remove both | Med | Med | Architect: audit all entry points into the retrospective pipeline and ensure single path after migration |
| SR-08 | context_status reports observation file stats (file_count, total_size, oldest_age). These must be replaced with observations table stats without breaking the status response schema | Low | Low | Spec: define replacement fields for observation stats in status response |

## Assumptions

1. **All RecordEvent payloads contain the fields needed by detection rules** (tool, input, response_size, response_snippet) -- per ASS-015 gap analysis. If any field is missing in the wire format, detection rules will produce different results. (References: SCOPE.md "Field Gap Analysis")
2. **SESSIONS table already has reliable feature_cycle values** for all sessions -- if feature_cycle is NULL for some sessions, attribution will be incomplete. (References: SCOPE.md "Phase 3")
3. **Single production database** -- the clean-break decision (no JSONL migration) assumes we accept losing historical observation data. (References: SCOPE.md "Resolved Questions #3")

## Design Recommendations

- **SR-02/SR-03**: Architect should design observation writes with the same fire-and-forget + spawn_blocking pattern used for injection log, and include a retention cleanup mechanism.
- **SR-04**: Keep the data source abstraction as thin as possible -- a trait with one or two methods, not a full repository pattern.
- **SR-06**: Decide explicitly whether silent event loss when server is down is acceptable. Document this as an ADR if the decision is non-obvious.
- **SR-07**: Architect should map all code paths that invoke retrospective analysis to ensure none are missed during migration.
