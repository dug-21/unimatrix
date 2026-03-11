# Agent Report: crt-018-researcher

## Task
Research the problem space for crt-018 (Knowledge Effectiveness Analysis) and produce SCOPE.md.

## Key Findings

### Data Infrastructure is Complete
All required data already exists in SQLite tables: injection_log (entry_id, session_id, confidence at injection time), sessions (outcome, feature_cycle), entries (trust_source, helpful_count, unhelpful_count), and query_log. No schema migration needed. The key join path is `injection_log JOIN sessions ON session_id`, grouped by entry_id.

### StatusService Pattern is Well-Established
StatusService::compute_report runs in 7 phases, each independent and using spawn_blocking. Adding effectiveness as Phase 8 follows the exact same pattern. StatusReport has ~45 fields with three output formats (Summary, Markdown, JSON). The StatusReportJson struct uses skip_serializing_if for optional sections.

### Session GC Creates a 30-Day Window
gc_sessions deletes sessions AND their injection_log rows after 30 days (DELETE_THRESHOLD_SECS). This means effectiveness analysis has at most 30 days of injection history. This is a hard constraint -- extending retention would require changing GC behavior (out of scope).

### Confidence Formula Validation is Straightforward
injection_log stores confidence at injection time. Joining with session outcomes gives us (predicted confidence, actual helpfulness) pairs for calibration analysis. The six-factor formula (0.92 stored weight sum) has never been empirically validated.

### Auto-Extracted Entries Have Lowest Trust
trust_score("auto") = 0.35, below neural (0.40) and agent (0.50). The 384 auto-extracted entries are the primary concern -- they have the lowest trust baseline and the highest risk of being noise.

## Scope Boundaries

- **In scope**: Measure and report effectiveness via context_status. Five-category classification. Per-source aggregates. Confidence calibration.
- **Out of scope**: Retrieval pipeline changes, automated pruning, new tools, schema changes.
- **Rationale**: Measurement must precede action. Wiring effectiveness into retrieval without validated data risks degrading search quality.

## Risks

1. **30-day data window**: Session GC limits historical analysis. Early features (nxs-001, etc.) will have zero injection history. The "Settled" classification handles this gracefully.
2. **StatusReport bloat**: Adding ~15 new fields to an already-large struct. Mitigated by using a sub-struct and skip_serializing_if.
3. **Performance on every status call**: SQL joins across injection_log (potentially thousands of rows) on every context_status invocation. Mitigated by using SQL GROUP BY aggregation rather than loading all rows.

## Output
- `/workspaces/unimatrix/product/features/crt-018/SCOPE.md`
