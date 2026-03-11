## ADR-001: Consolidated Effectiveness Query via Single Store Method

### Context

The SCOPE proposes four separate Store scan methods (`scan_injection_stats_by_entry`, `scan_session_outcomes_by_entry`, `scan_topic_activity`, `scan_injection_confidence_buckets`). SR-01 warns that multi-table SQL JOINs on every `context_status` call may exceed the 500ms performance budget. SR-07 notes that ADR-004 (Unimatrix #704, crt-013) consolidated status queries into a single `compute_status_aggregates()` method returning a `StatusAggregates` struct, and adding 4 independent scan methods would break this consolidation pattern.

Two approaches:
- A) Four separate scan methods as proposed in SCOPE (granular, reusable, but 4 round-trips and 4 connection locks)
- B) Single `compute_effectiveness_aggregates()` method returning one struct (matches ADR-004 pattern, one connection lock, 4 SQL queries within one method)

### Decision

**Option B: Single `compute_effectiveness_aggregates()` method returning `EffectivenessAggregates` struct.**

Implementation:
- One Store method acquires `lock_conn()` once
- Four SQL queries run sequentially on the same connection: (1) entry injection stats via GROUP BY, (2) active topics via DISTINCT, (3) calibration rows, (4) data window metadata
- Returns `EffectivenessAggregates` struct with all pre-aggregated data
- A separate `load_entry_classification_meta()` method returns entry metadata (title, topic, trust_source, helpful/unhelpful counts) for active entries — this could also be derived from the `active_entries` Vec already loaded in Phase 1, but a dedicated lightweight query avoids cloning the full EntryRecord set

All queries use existing indexes:
- `idx_injection_log_entry` covers GROUP BY entry_id
- `idx_injection_log_session` covers JOIN on session_id
- `idx_sessions_feature_cycle` covers active topics query

No new indexes needed. The JOIN between injection_log and sessions uses `injection_log.session_id = sessions.session_id` which hits `idx_injection_log_session` for the scan and sessions PK for the lookup.

### Consequences

- Single connection lock per effectiveness computation (same as StatusAggregates pattern)
- 4 SQL queries but within one method — clear what data is fetched and why
- Follows established crt-013 pattern; future status extensions should continue this pattern
- Entry classification metadata is a separate lightweight query because active_entries from Phase 1 does not include helpful/unhelpful counts in the right format for the classifier
- If injection_log grows beyond 100K rows, the GROUP BY query may need a composite index on `(entry_id, session_id)` — monitor in production
