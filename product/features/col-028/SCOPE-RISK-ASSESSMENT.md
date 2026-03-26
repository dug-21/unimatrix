# Scope Risk Assessment: col-028

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | `row_to_query_log` uses positional column index; adding `phase` as index 9 requires both SELECT statements and the deserializer to stay in sync — a silent runtime error if any one diverges | High | Med | Architect must treat `analytics.rs` INSERT, both SELECT statements, and `row_to_query_log` as a single atomic change surface; a compile-time check or explicit column-name binding is preferred |
| SR-02 | Schema version bump v16→v17 triggers the cascade pattern (#2933): all older migration test files that assert `schema_version = 16` must be updated to 17 — 15+ test callers of `QueryLogRecord::new()` are already counted but the cascade across migration test files is a separate, easily-missed obligation | Med | High | Spec must enumerate every affected migration test file; delivery must update them all before gate |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-03 | UDS `insert_query_log` is explicitly out of scope, but its `QueryLogRecord::new()` call must still compile after the signature gains `phase: Option<String>`. The UDS call site passes `None`, but it is easy to miss if the implementer patches only the MCP call site | Med | Med | Spec must call out UDS call site as a required compile-fix (pass `None`) even though no phase semantics change there |
| SR-04 | `confirmed_entries` ships with no consumer (D-03). If the future Thompson Sampling feature changes the `confirmed_entries` contract (e.g. requires `context_lookup` multi-ID tracking), the in-flight data from deployed sessions will be silently wrong and unrecoverable — sessions are ephemeral and non-backfillable | Low | Low | ADR #3508 is in place; spec should document the exact semantic contract (explicit-fetch-only, request-side cardinality) so the future consumer cannot silently reinterpret it |
| SR-05 | `context_briefing` weight 1→0 is a behavioural regression for any downstream consumer that currently expects briefing to increment `access_count`. If any analytics query groups by access source and normalises on historical briefing weight=1, historical vs. post-deploy data will be inconsistent | Low | Low | Architect should confirm no existing analytics query assumes briefing contributes to `access_count` before finalising the weight change |

## Integration Risks

| Risk ID | Risk | Likelihood | Severity | Recommendation |
|---------|------|------------|----------|----------------|
| SR-06 | Part 1 (in-memory phase capture) and Part 2 (schema migration + query_log write) share the same phase snapshot taken before `await` in `context_search`. If implemented in separate PRs or by separate implementers, there is a risk that the shared snapshot variable is duplicated rather than reused, violating the single-`get_state` contract and introducing two lock acquisitions | Med | Low | Spec should describe Parts 1 and 2 as a single atomic implementation unit; the phase snapshot variable at `context_search` must be shared between UsageContext and QueryLogRecord |
| SR-07 | D-01 guard (`if ctx.access_weight == 0 { return; }`) is placed in `record_briefing_usage`. If `record_mcp_usage` is ever called with `AccessSource::Briefing` by a future refactor, the guard would be bypassed. The guard is complete for the current routing but not structurally enforced | Low | Med | Architect should consider whether the guard belongs at the `AccessSource` dispatch level (the router) rather than inside `record_briefing_usage`, making it impossible to route weight-0 through `filter_access` regardless of source |

## Assumptions

| Section | Assumption | Risk if Wrong |
|---------|------------|---------------|
| Non-Goals | UDS `insert_query_log` rows with `phase = NULL` are acceptable to all downstream analytics consumers | If the phase-conditioned frequency table (ass-032) treats NULL as a valid phase label rather than absent phase, UDS-sourced rows will pollute the frequency table |
| Background Research (D-05a) | The EC-04 contract ("weight 0 silently drops access increment") holds because the D-01 early-return guard enforces it, not because the flat_map arithmetic handles weight=0 correctly | If the D-01 guard is ever removed without a weight-0 arithmetic fix, access_count will silently increment for briefing events |
| Proposed Approach (Part 4) | `pragma_table_info` pre-check makes the migration idempotent (AC-15 relies on this) | SQLite `pragma_table_info` returns rows for all columns including those added by `ALTER TABLE` in an open transaction — confirmed by prior migrations; safe |
| SCOPE.md §Constraints | `mcp/tools.rs` file size is within the 500-line limit after all four call-site changes | If tools.rs exceeds 500 lines, a mid-feature split is required; architect should verify current line count before scoping |

## Design Recommendations

1. **(SR-01)** Treat `analytics.rs` INSERT positional params, both `scan_query_log_*` SELECT statements, and `row_to_query_log` as a single change unit in the spec. Add an explicit test that reads back the phase value end-to-end (AC-17 covers this but should be called out as the guard against positional drift).

2. **(SR-02)** Spec must include an explicit step to audit all migration test files for `schema_version` assertions at v16 and update them to v17. Pattern #2933 confirms this is a recurring miss.

3. **(SR-03)** Spec must list the UDS call site (`uds/listener.rs:1324`) as a required compile-fix with `phase: None` — not as a semantic change, but as a mandatory compilation update.

4. **(SR-06 + SR-07)** Spec should make explicit that the phase snapshot variable at `context_search` is shared between `UsageContext` and `QueryLogRecord::new()`, and that the D-01 guard location in `record_briefing_usage` is load-bearing for the EC-04 contract.
