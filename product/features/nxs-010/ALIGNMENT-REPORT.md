# Alignment Report: nxs-010

> Reviewed: 2026-03-10
> Artifacts reviewed:
>   - product/features/nxs-010/architecture/ARCHITECTURE.md
>   - product/features/nxs-010/specification/SPECIFICATION.md
>   - product/features/nxs-010/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Directly enables Activity Intelligence milestone goals |
| Milestone Fit | PASS | Wave 2 feature correctly positioned; dependencies explicit |
| Scope Gaps | PASS | All SCOPE.md items addressed in source documents |
| Scope Additions | WARN | Two open questions resolved in spec without explicit scope approval; both are reasonable |
| Architecture Consistency | PASS | Follows established patterns (fire-and-forget, migration, module structure) |
| Risk Completeness | PASS | 14 risks mapped to scope risk assessment; traceability matrix complete |

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Simplification | `total_tool_calls` backfill | SCOPE.md open question #1 resolved: left at 0, col-020 recomputes. Documented in FR-03.7 and spec NOT-in-scope. Reasonable. |
| Simplification | Query embedding hash | SCOPE.md open question #2 resolved: deferred. Spec NOT-in-scope lists it. Reasonable. |
| Simplification | Foreign key enforcement | SCOPE.md open question #3 resolved: no FK, application-level. Spec C-05 and domain model section document rationale. Reasonable. |
| Simplification | Integration test strategy | SCOPE.md open question #4 resolved: follow full pipeline pattern matching injection_log tests. Risk strategy R-04/R-05 scenarios confirm. Reasonable. |
| Addition | FR-08 shared QueryLogRecord constructor | Not explicitly in SCOPE.md but directly responds to SCOPE-RISK-ASSESSMENT SR-07. Proportionate addition. |
| Addition | NFR-03 capacity sizing at 30K rows/year | Not in SCOPE.md; responds to SR-06 recommendation. Proportionate addition. |
| Addition | Architecture open questions on session_id handling | Architecture adds two open questions (UDS skip-if-None, MCP write-with-empty-string). Not in SCOPE.md but operationally necessary. No scope creep. |

## Variances Requiring Approval

None. All deviations from SCOPE.md are proportionate responses to scope risk assessment recommendations or resolutions of SCOPE.md open questions. No feature scope has been expanded beyond what the milestone requires.

## Detailed Findings

### Vision Alignment

The product vision identifies the Activity Intelligence milestone as the current priority, stating: "Connect the observation pipeline to make activity data queryable, attributable, and analyzable." nxs-010 directly supports this by providing the storage layer for topic aggregation (`topic_deliveries`) and search telemetry (`query_log`).

The vision's core value proposition -- "auditable knowledge lifecycle" -- is extended by `query_log`, which captures the retrieval side of the lifecycle (what was searched, what was found, how similar the results were). This data feeds crt-019 (search quality) and col-021 (embedding tuning export), both named in the roadmap.

The vision's emphasis on "invisible delivery" via hooks is respected: the UDS path captures hook-triggered searches and the MCP path captures tool-invoked searches, maintaining full observability across both delivery channels.

No vision principles are contradicted. The feature is purely additive schema infrastructure.

### Milestone Fit

The vision document places nxs-010 in "Wave 2 -- Connect & capture (depends on Wave 1)." The SCOPE.md, architecture, and specification all correctly identify col-017 as a prerequisite and name col-020, crt-018, crt-019, and col-021 as downstream consumers.

No future-milestone capabilities are being built prematurely. The feature provides storage and API only -- no analysis, no reporting, no export. Each downstream capability is explicitly deferred to its own feature.

Schema version sequencing (v10 from col-017, v11 from nxs-010) is correctly tracked across all three documents and the scope risk assessment.

### Architecture Review

The architecture follows established patterns:
- **Migration pattern**: Matches existing v5->v6 through v9->v10 migrations. Init sequence (migrate then create_tables) is explicitly verified against Unimatrix #375/#376.
- **Module structure**: Two new modules in unimatrix-store with pub mod + pub use re-exports. Consistent with existing modules (e.g., signal_queue, observations).
- **Fire-and-forget pattern**: Matches injection_log and usage recording precedent. ADR-002 documents failure semantics.
- **AUTOINCREMENT decision**: ADR-001 documents the boundary between counter-based and AUTOINCREMENT ID allocation. The observations table precedent is cited.

The architecture's component interaction diagrams clearly show data flow through both search paths. The batching consideration (Unimatrix #731, #735) demonstrates awareness of prior performance issues without over-engineering a solution.

One open question in the architecture (UDS session_id = None handling: skip write vs sentinel) is not resolved in the specification. The risk strategy covers it (R-04 scenario 4: "Invoke UDS search with session_id = None. Verify query_log write is skipped.") The architecture recommends "skip" and the risk strategy tests for "skip," so the implicit resolution is consistent, but the specification does not contain an explicit FR for this guard condition. This is a minor documentation gap, not a functional gap -- the test will enforce the behavior regardless.

### Specification Review

The specification is thorough:
- 8 functional requirements covering all SCOPE.md goals and acceptance criteria
- 7 non-functional requirements addressing performance, capacity, idempotency, backward compatibility, and JSON consistency
- Full acceptance criteria table mapping 1:1 to SCOPE.md AC-01 through AC-20
- Domain model with ubiquitous language definitions
- 4 user workflows covering all operational paths
- 7 constraints mapping to scope risk assessment items
- Explicit NOT-in-scope section matching SCOPE.md non-goals

The specification correctly resolves all 4 SCOPE.md open questions with documented rationale. FR-08.1 (shared QueryLogRecord constructor) was added in response to SR-07 and is a proportionate quality measure, not scope expansion.

The `result_count` field type differs between SCOPE.md (implicitly integer), architecture (`u32`), and specification FR-05.1 (`i64`). The specification's `i64` matches the rusqlite default integer mapping and is the safest choice for SQLite compatibility. This is not a meaningful variance.

### Risk Strategy Review

The risk strategy identifies 14 risks with severity/likelihood assessments and maps them to 27+ test scenarios. All 8 scope risks (SR-01 through SR-08) have explicit traceability to architecture risks and resolution strategies.

Risk coverage is proportionate:
- 3 Critical risks (R-02 backfill correctness, R-04 panic in spawn_blocking, R-10 INSERT OR REPLACE race) have 9 test scenarios
- 5 High risks have 13 scenarios
- 5 Medium risks have 5 scenarios
- 1 Low risk (R-13 whitespace variants) is explicitly accepted without test coverage

Security risks are addressed: SQL injection via query_text is mitigated by parameterized queries (the existing codebase pattern). The assessment correctly identifies that result_entry_ids and similarity_scores are internally generated and not a user-controlled injection vector.

The R-10 risk (INSERT OR REPLACE destroying concurrent counter updates) is correctly identified as Critical and the test strategy documents the expected behavior rather than trying to prevent it. The architecture confirms no concurrent upsert+update workflow exists in the documented feature set. This is an honest assessment of a real SQLite limitation with appropriate mitigation.
