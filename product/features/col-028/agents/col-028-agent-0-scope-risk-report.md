# Agent Report: col-028-agent-0-scope-risk

## Output

`product/features/col-028/SCOPE-RISK-ASSESSMENT.md` — 42 lines (limit: 100)

## Risk Summary

| Severity | Count |
|----------|-------|
| High     | 1 (SR-01) |
| Medium   | 3 (SR-02, SR-03, SR-06) |
| Low      | 3 (SR-04, SR-05, SR-07) |

Total: 7 risks across Technology, Scope Boundary, and Integration categories.

## Top 3 Risks for Architect/Spec Writer Attention

**SR-01 (High / Med)** — Positional column index fragility in `row_to_query_log`. The `analytics.rs` INSERT, both `scan_query_log_*` SELECT statements, and `row_to_query_log` are three independent surfaces that must stay synchronized when `phase` is added as column index 9. A mismatch is a silent runtime error, not a compile error. Recommendation: treat these as a single atomic change and ensure AC-17 (end-to-end read-back of phase value) is the guard.

**SR-02 (Med / High)** — Schema version cascade. Every v16→v17 bump triggers pattern #2933: all older migration test files with `schema_version = 16` assertions must be updated. With 15+ `QueryLogRecord::new()` call sites already counted, it is easy to overlook the migration test file cascade as a separate obligation. Spec must enumerate the affected migration test files explicitly.

**SR-03 (Med / Med)** — UDS call site compile fix. `uds/listener.rs:1324` is out of scope for phase semantics but its `QueryLogRecord::new()` call must still compile after the signature gains `phase: Option<String>`. This is a required `None`-pass with no behaviour change — easy to miss if the implementer patches only the MCP call site.

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "lesson-learned failures gate rejection" — no directly relevant results for this domain
- Queried: `/uni-knowledge-search` for "outcome rework schema migration query_log" — found patterns #2933 (schema version cascade) and #681 (create-new-then-swap migration), both applied to SR-02
- Queried: `/uni-knowledge-search` for "risk pattern" (category: pattern) — found #3426 (formatter regression risk), #3180 (SessionState test helper obligation), both applied to SR-02 and SR-04
- Queried: `/uni-knowledge-search` for "SQLite migration positional column index row_to" — found #374, #681, #370 confirming positional SELECT risk, applied to SR-01
- Queried: `/uni-knowledge-search` for "schema version cascade migration test files" — found #2933, #2937 confirming cascade pattern is recurring
- Stored: nothing novel to store — positional column risk is feature-specific; schema version cascade already documented as #2933; SessionState helper obligation already documented as #3180
