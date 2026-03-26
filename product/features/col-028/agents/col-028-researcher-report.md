# col-028 Researcher Agent Report

**Agent ID**: col-028-researcher
**Feature**: col-028 — Unified Phase Signal Capture (Read-Side + query_log)
**GH Issues**: #394 (in-memory), #397 (query_log persistence)

---

## Summary

SCOPE.md written to `product/features/col-028/SCOPE.md`. The earlier partial SCOPE.md
(covering only #394) was replaced with a unified scope covering both issues as a single
coherent feature.

---

## Key Findings

### Confirmed Facts from Source

**1. UsageDedup dedup-slot collision (D-01 — confirmed, critical)**

`UsageDedup.access_counted` is a single `HashSet<(String, u64)>` shared across all
`AccessSource` variants. `filter_access` is called in both `record_mcp_usage` and
`record_briefing_usage`. A briefing call at weight=1 registers `(agent_id, entry_id)` in
`access_counted`. A subsequent `context_get` for the same entry returns an empty
`access_ids` vec — zero access count increment. The highest-value signal in the pipeline
is silenced. The D-01 guard (`if ctx.access_weight == 0 { return; }` before
`filter_access` in `record_briefing_usage`) is required and sufficient.

**2. access_weight=0 arithmetic (D-05a — confirmed)**

`usage.rs` lines 169–187: for `access_weight <= 1`, the code produces
`multiplied_all_ids = entry_ids.to_vec()` (one copy per entry). The flat_map path handles
weight > 1 only. EC-04 ("weight 0 silently drops the access increment") is a contract
claim, not enforced by the arithmetic. The D-01 early-return guard is the enforcement
mechanism — it prevents `record_usage_with_confidence` from being called at all.

**3. record_briefing_usage has no co-access generation (D-05b — confirmed)**

`record_briefing_usage` (usage.rs:313–349) contains only `record_usage_with_confidence`.
No `generate_pairs`, no `filter_co_access_pairs`. Briefing does not generate co-access
pairs today and will not at weight-0 because `record_usage_with_confidence` is not called.

**4. query_log write sites (confirmed)**

Two call sites:
- `tools.rs:397` — MCP `context_search`, in scope
- `uds/listener.rs:1324` — UDS injection, out of scope (no session registry reference)

**5. Current schema version is 16 (confirmed)**

`migration.rs:19`: `CURRENT_SCHEMA_VERSION: u64 = 16`. Feature introduces v16→v17.

**6. Migration pattern is pragma_table_info pre-check (confirmed)**

Seen in v14→v15, v15→v16, v13→v14, v7→v8. Required. No deviation.

**7. context_lookup already uses access_weight=2 (confirmed)**

`tools.rs:482`: `access_weight: 2` already. Only `current_phase` needs to be added.

**8. QueryLogRecord::new() has 15+ test call sites (confirmed)**

`eval/scenarios/tests.rs` uses a local `insert_query_log_row` helper that wraps
`QueryLogRecord::new`. Adding `phase` as a parameter requires updating the helper and
all 15+ call sites. Mechanical but non-trivial scope.

**9. scan_query_log read paths use positional column lists (confirmed)**

Both `scan_query_log_by_sessions` and `scan_query_log_by_session` use hardcoded column
position indices (0–8). Adding `phase` at index 9 requires updating both SELECT
statements and `row_to_query_log`.

**10. D-01 guard location is record_briefing_usage (resolved)**

The briefing call site continues to use `AccessSource::Briefing`, routing to
`record_briefing_usage`. The guard belongs there, not in `record_mcp_usage`.

---

## Scope Boundaries

**In scope**: All changes described in the SCOPE.md Goals section (1–10).

**Out of scope explicitly confirmed**:
- UDS query_log call site
- Any scoring pipeline changes
- Backfill of historical rows
- Phase-conditioned frequency table, Thompson Sampling, gap detection

---

## Risks

1. **Test file blast radius**: 15+ callers of `QueryLogRecord::new` in test code. High
   confidence it's mechanical, but spec agent must enumerate them all.
2. **AnalyticsWrite::QueryLog positional binding**: Adding `phase` as parameter 9 in the
   INSERT must match the column order in the ALTER TABLE. Order must be consistent.
3. **tools.rs line count**: The file is large. Adding the phase snapshot lines and
   `confirmed_entries` mutations at four call sites plus a new free function may push
   toward 500 lines. Delivery agent should check line count before implementation.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for "phase snapshot", "query_log schema migration",
  "UsageDedup SessionState field" — found patterns #3027 (context_store phase snapshot),
  #3180 (SessionState field additions), #3412 (in-memory counter pattern), #3210
  (SessionRegistry access patterns).
- Stored: entry #3510 "UsageDedup shared access_counted set across AccessSource variants
  — weight-0 dedup bypass required" via `context_store` (pattern, topic: unimatrix-server).
  This finding was absent from Unimatrix and is directly relevant to any future feature
  that modifies access_weight semantics.
