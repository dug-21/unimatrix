# Specification: crt-018 Knowledge Effectiveness Analysis

## Objective

Compute per-entry utility scores by joining injection_log with session outcomes, classify every active entry into one of five effectiveness categories (Effective, Settled, Unmatched, Ineffective, Noisy), validate confidence calibration against actual helpfulness rates, and surface all effectiveness data through the existing `context_status` MCP tool. This provides the first empirical answer to whether injected knowledge entries are actually helping agents succeed.

## Functional Requirements

### FR-01: Entry Effectiveness Classification

Every active entry in the knowledge base must be classified into exactly one of five categories based on injection history and session outcomes:

- **Effective**: Entry was injected into sessions that produced success outcomes, with a positive utility score (weighted success rate >= 30%).
- **Settled**: Entry's topic has no sessions in the available data window AND the entry has at least one historical injection with a success-outcome session. Represents knowledge that served its era.
- **Unmatched**: Entry has zero injection_log records AND the entry's topic has at least one session in the available data window. Indicates the entry is never retrieved despite its topic being active.
- **Ineffective**: Entry was injected into >= INEFFECTIVE_MIN_INJECTIONS distinct sessions AND weighted success rate < 30%. Indicates knowledge that is retrieved but does not correlate with session success.
- **Noisy**: Entry has trust_source="auto" AND zero helpful_count AND at least one injection_log record. Indicates auto-extracted entries that show no quality signal.

Classification is mutually exclusive. An entry matching multiple categories is assigned the first matching category in the priority order: Noisy > Ineffective > Unmatched > Settled > Effective. Entries not matching any specific category default to Effective (they have been injected into at least one success-outcome session or have insufficient data to classify negatively).

### FR-02: Weighted Success Rate Computation

Utility scoring uses weighted session outcomes with the following constants:

| Outcome | Weight |
|---------|--------|
| success | 1.0 |
| rework | 0.5 |
| abandoned | 0.0 |

These weights must be defined as named constants (`OUTCOME_WEIGHT_SUCCESS`, `OUTCOME_WEIGHT_REWORK`, `OUTCOME_WEIGHT_ABANDONED`) in the effectiveness module.

Weighted success rate for an entry = sum(weight * session_count per outcome) / total_sessions_where_injected.

Sessions with NULL outcome are excluded from the computation (neither numerator nor denominator).

### FR-03: Aggregate Effectiveness by Trust Source

Compute per-trust-source aggregate metrics covering all five known trust sources (auto, agent, system, human, neural). Each source aggregate includes:

- Total entry count for that source
- Count per effectiveness category (effective, settled, unmatched, ineffective, noisy)
- Aggregate utility ratio: weighted success rate across all entries of that source

### FR-04: Confidence Calibration Validation

Produce 10 calibration buckets of 0.1 width (0.0-0.1, 0.1-0.2, ..., 0.9-1.0). For each bucket:

- Count of injection_log records where confidence-at-injection-time falls in the bucket range (lower bound inclusive, upper bound exclusive, except the last bucket 0.9-1.0 which is inclusive on both ends)
- Actual session success rate for those injections (using weighted outcomes: success=1.0, rework=0.5, abandoned=0.0)
- Expected success rate (bucket midpoint, e.g., 0.75 for the 0.7-0.8 bucket)

This enables comparison of predicted (confidence) vs actual (outcome) helpfulness.

### FR-05: context_status Summary Format Output

Append a one-line effectiveness summary to context_status summary output:

```
Effectiveness: 42 effective, 15 settled, 3 unmatched, 2 ineffective, 1 noisy (N sessions analyzed)
```

The session count indicator communicates data coverage to the consumer. If no injection_log data exists, output: `Effectiveness: no injection data`.

### FR-06: context_status Markdown Format Output

Add an `### Effectiveness Analysis` section to context_status markdown output containing:

1. **Category table**: effectiveness category, count, percentage of total active entries
2. **Per-source table**: trust_source, effective, settled, unmatched, ineffective, noisy, utility ratio
3. **Calibration table**: confidence bucket, injection count, actual success rate, expected success rate
4. **Top ineffective entries**: up to 10 entries with highest injection count and lowest success rate, showing entry_id, title, injection_count, success_rate
5. **Noisy entries**: all entries classified as Noisy, showing entry_id and title
6. **Unmatched entries**: up to 10 entries classified as Unmatched, showing entry_id, title, and topic
7. **Data window indicator**: "Analysis covers N sessions over M days" derived from min/max started_at in available sessions

### FR-07: context_status JSON Format Output

Add an `effectiveness` object to the JSON output with the following structure:

- `by_category`: array of {category, count}
- `by_source`: array of {trust_source, effective, settled, unmatched, ineffective, noisy, utility_ratio}
- `calibration_buckets`: array of {range_low, range_high, injection_count, actual_success_rate, expected_success_rate}
- `ineffective_entries`: array of {entry_id, title, injection_count, success_rate} (top 10)
- `noisy_entries`: array of {entry_id, title} (all)
- `unmatched_entries`: array of {entry_id, title, topic} (top 10)
- `data_window`: {session_count, span_days}

Use `#[serde(skip_serializing_if = "Option::is_none")]` on the effectiveness field. The field is None when no injection_log data exists.

### FR-08: Pure Computation Module

All classification and aggregation logic resides in a new `crates/unimatrix-engine/src/effectiveness.rs` module containing pure functions (no I/O, no database access). The module receives pre-fetched data structs and returns classification results. This follows the established pattern of `confidence.rs`.

### FR-09: Store Layer Query Methods

New SQL aggregation methods on Store that perform the joins server-side (GROUP BY) rather than loading all rows into Rust:

- Injection stats per entry: entry_id, injection_count (distinct sessions), avg_confidence_at_injection
- Session outcomes per entry: entry_id, success_count, rework_count, abandoned_count (from injection_log JOIN sessions)
- Topic activity: topic (from entries.topic matched to sessions.feature_cycle), last_session_at, session_count
- Confidence calibration buckets: CASE expression bucketing confidence into 0.1 ranges, with per-bucket outcome counts

### FR-10: StatusService Integration

Effectiveness computation integrates as a new phase in `StatusService::compute_report`, using `spawn_blocking` for the SQL queries. The phase is independent of existing phases and populates new fields on StatusReport.

## Non-Functional Requirements

### NFR-01: Query Performance

Effectiveness queries must complete within 500ms for a knowledge base of 500 active entries and 10,000 injection_log rows. SQL-side aggregation (GROUP BY) is mandatory. The architect should verify that existing indexes on injection_log (idx_injection_log_session, idx_injection_log_entry) and sessions (idx_sessions_feature_cycle) provide adequate coverage for the join paths. If additional indexes are needed, they must be created in the existing schema initialization code (no migration).

### NFR-02: No Schema Migration

All required data exists in the current SQLite schema (entries, injection_log, sessions, signal_queue). No new tables, no new columns, no schema version bump.

### NFR-03: Read-Only Computation

context_status with effectiveness data remains strictly read-only. No writes to any table, no side effects on entry state, confidence scores, or any other persistent data. Classifications are transient, computed fresh on each call.

### NFR-04: Async Runtime Safety

All SQL queries run inside `spawn_blocking`. No blocking I/O on the async tokio runtime.

### NFR-05: Output Size Management

StatusReport already has ~45 fields and ~700 lines of formatting. The effectiveness section must use `skip_serializing_if` in JSON format to omit when no data exists. Lists (ineffective, noisy, unmatched) are capped at documented maximums (10, all, 10 respectively) to prevent output explosion.

### NFR-06: Graceful Degradation

When injection_log is empty or has insufficient data, the effectiveness section degrades gracefully:
- Summary format: "Effectiveness: no injection data"
- Markdown format: "### Effectiveness Analysis\n\nInsufficient injection data for analysis."
- JSON format: effectiveness field is None (omitted via skip_serializing_if)

No panics, no errors. Empty data is a valid state (fresh knowledge base).

## Acceptance Criteria

| AC-ID | Criterion | Verification Method |
|-------|-----------|-------------------|
| AC-01 | Every active entry is classified into exactly one of five categories: Effective, Settled, Unmatched, Ineffective, Noisy | Unit test: provide entries spanning all five categories, verify each gets exactly one classification |
| AC-02 | Classification uses injection_log + sessions.outcome join; entries with zero injections are classified as Unmatched (if topic active) or Settled (if topic inactive) | Unit test: create entries with zero injections for both active and inactive topics, verify correct classification |
| AC-03 | "Settled" classification requires no sessions for the entry's topic within the available data window AND at least one historical injection with success outcome | Unit test: entry with topic that has no sessions + has historical success injection -> Settled; entry with topic that has no sessions + no historical success injection -> NOT Settled |
| AC-04 | "Ineffective" classification requires >= INEFFECTIVE_MIN_INJECTIONS (default: 3) distinct injection sessions AND weighted success rate < 30% | Unit test: entry with 2 injection sessions + 0% success -> NOT Ineffective (below threshold); entry with 3 injection sessions + 0% success -> Ineffective; entry with 3 injection sessions + 35% success -> NOT Ineffective |
| AC-05 | "Noisy" classification requires trust_source="auto" AND zero helpful_count AND at least one injection | Unit test: auto entry with 0 helpful + injections -> Noisy; auto entry with 1 helpful -> NOT Noisy; agent entry with 0 helpful + injections -> NOT Noisy |
| AC-06 | Aggregate effectiveness metrics computed per trust_source (auto, agent, system, human, neural) showing counts per category and aggregate utility ratio | Unit test: entries from multiple trust sources, verify per-source aggregates match expected counts |
| AC-07 | Confidence calibration produces 10 buckets (0.0-0.1 through 0.9-1.0) comparing confidence-at-injection-time to actual session success rate | Unit test: injections with known confidence values and outcomes, verify bucket assignment and success rate computation |
| AC-08 | context_status summary format includes a one-line effectiveness summary with category counts and session count | Integration test: insert test data, call context_status with summary format, verify effectiveness line present with correct counts |
| AC-09 | context_status markdown format includes "### Effectiveness Analysis" section with category table, per-source table, calibration table, and top-10 ineffective entries | Integration test: insert test data, call context_status with markdown format, verify section headers and table content |
| AC-10 | context_status JSON format includes an `effectiveness` object with structured data matching the specified schema | Integration test: insert test data, call context_status with JSON format, deserialize effectiveness object, verify fields |
| AC-11 | Effectiveness computation runs in spawn_blocking and does not block the async runtime | Code review: verify spawn_blocking wrapping in StatusService |
| AC-12 | Top 10 ineffective entries and all noisy entries are listed with entry_id and title for human review | Unit test: create >10 ineffective entries, verify only top 10 returned sorted by injection_count descending; verify all noisy entries listed |
| AC-13 | Effectiveness analysis computed on every context_status call; no opt-in flag; context_status remains read-only with no writes or side effects; classifications are transient | Integration test: call context_status twice with same data, verify identical effectiveness results; verify no table modifications via row count checks before/after |
| AC-14 | Pure computation functions in effectiveness.rs have unit tests covering all five classification categories, boundary conditions (exactly INEFFECTIVE_MIN_INJECTIONS threshold, exactly 30% success rate), and empty-data graceful handling | Unit tests: explicit boundary tests at thresholds; empty input returns empty/default results |
| AC-15 | Integration tests verify end-to-end flow: insert entries + injection_log + sessions with known outcomes, call status, verify effectiveness section in output | Integration test using TestDb: insert controlled test data, invoke status computation, verify all three output formats |
| AC-16 | Entries with NULL topic are classified into a dedicated "(unattributed)" bucket rather than silently dropped; sessions with NULL feature_cycle are excluded from topic activity computation but their outcomes still count for injection-level effectiveness | Unit test: entry with NULL topic still gets classified; session with NULL feature_cycle still contributes to entry-level success rate via injection_log join |
| AC-17 | Rework outcome weights (success=1.0, rework=0.5, abandoned=0.0) are defined as named constants, not magic numbers | Code review: verify named constants exist and are used in all success rate computations |

## Domain Models

### Key Entities

**EffectivenessCategory** (enum): Effective, Settled, Unmatched, Ineffective, Noisy. Mutually exclusive classification for each active entry.

**EntryEffectiveness**: Per-entry classification result. Fields: entry_id, title, topic, trust_source, category (EffectivenessCategory), injection_count (distinct sessions), weighted_success_rate, helpful_count.

**SourceEffectiveness**: Aggregate metrics per trust_source. Fields: trust_source, total_entries, counts per category, aggregate_utility_ratio.

**CalibrationBucket**: One bucket in the 10-bucket calibration grid. Fields: range_low (f64), range_high (f64), injection_count (u64), actual_success_rate (f64), expected_success_rate (f64).

**EffectivenessReport**: Top-level container returned by the pure computation module. Fields: entries_by_category (Vec<EntryEffectiveness>), by_source (Vec<SourceEffectiveness>), calibration_buckets (Vec<CalibrationBucket>), data_window (session_count, span_days).

### Ubiquitous Language

- **Injection**: An entry served to an agent during ContextSearch, recorded in injection_log.
- **Session outcome**: The result of a session (success, rework, abandoned), recorded in sessions.outcome.
- **Weighted success rate**: Sum of (outcome_weight * count) / total_sessions. Uses success=1.0, rework=0.5, abandoned=0.0.
- **Topic activity**: Whether sessions exist for a given topic (entries.topic matched to sessions.feature_cycle) in the available data window.
- **Available data window**: The set of sessions currently retained after GC. Bounded by DELETE_THRESHOLD_SECS (30 days). No hardcoded time threshold in effectiveness logic.
- **Confidence-at-injection-time**: The injection_log.confidence column, capturing the reranked score at the moment of injection (not the entry's current stored confidence).
- **Unattributed**: Entries with NULL or empty topic field. Classified separately rather than silently dropped.

## User Workflows

### Workflow 1: Human Reviews Knowledge Effectiveness

1. Human calls `context_status` (any format)
2. System computes effectiveness analysis as part of the status report
3. Human reviews category distribution, identifies ineffective/noisy entries
4. Human manually reviews flagged entries and decides whether to quarantine, correct, or leave them
5. No automated action is taken by the system

### Workflow 2: Agent Assesses Knowledge Base Health

1. Agent calls `context_status` with JSON format
2. Agent parses the `effectiveness` object
3. Agent uses category counts and calibration data to assess knowledge base health
4. Agent may include findings in session outcome or retrospective notes
5. No automated action is taken by the system

### Workflow 3: Trust Source Validation

1. Human or agent calls `context_status` to compare effectiveness across trust sources
2. Reviews per-source aggregate utility ratios
3. Determines whether auto-extracted entries (trust_source="auto") provide value comparable to human-authored entries
4. Informs decisions about extraction pipeline tuning (separate feature)

## Constraints

1. **No schema migration**: Must work with existing tables (injection_log, sessions, entries). No new tables, no new columns, no schema version bump.

2. **Performance budget**: 500ms for 500 entries + 10,000 injection_log rows. SQL-side GROUP BY aggregation required.

3. **Session GC interaction**: Sessions older than 30 days are deleted by gc_sessions (DELETE_THRESHOLD_SECS = 30 * 24 * 3600), along with their injection_log rows via cascade delete. Effectiveness analysis operates on whatever data GC retains. No hardcoded time thresholds in effectiveness logic. "Settled" uses absence of sessions for a topic in the available window, not a fixed day count. If GC retention changes, analysis adapts automatically.

4. **Signal queue transience**: signal_queue is drained and deleted after processing. Effectiveness analysis uses the persisted helpful_count/unhelpful_count on entries, not the transient queue.

5. **StatusReport size**: Use skip_serializing_if for JSON format. Cap list outputs (top 10 ineffective, top 10 unmatched). Do not break existing output structure.

6. **Test infrastructure**: Extend existing TestDb helper and test patterns from injection_log.rs and sessions.rs. Do not create isolated test scaffolding.

7. **NULL topic handling (SR-06)**: Entries with NULL or empty topic must not be silently dropped from classification. They are assigned to an "(unattributed)" bucket for topic activity purposes. Sessions with NULL feature_cycle are excluded from topic activity computation but still contribute to entry-level effectiveness through the injection_log JOIN. This is a known failure mode (Unimatrix #981) and must have explicit test coverage.

8. **Noisy trust_source filter**: The Noisy classification currently applies only to trust_source="auto". The architect should consider making this configurable (e.g., a constant or small set) to accommodate future trust sources like "neural" (per SR-05), but the default for this feature is "auto" only.

9. **Outcome weight tunability (SR-08)**: The rework weights (1.0/0.5/0.0) are product decisions embedded as named constants. They must be defined in one place and documented with rationale. No runtime configuration is required for this feature.

10. **StatusAggregates consolidation (SR-07)**: The architect should evaluate whether effectiveness queries can be folded into the existing StatusAggregates single-method pattern (per ADR-004) rather than adding independent scan methods. This is a design decision for the architect; the specification requires only that the queries complete within the performance budget.

## Dependencies

### Crate Dependencies

- `unimatrix-engine`: New `effectiveness.rs` module (pure computation, no new external crate dependencies)
- `unimatrix-store`: New query methods on Store (uses existing rusqlite dependency)
- `unimatrix-server`: StatusReport extension, StatusService new phase, format_status_report additions

### Existing Components

- `injection_log` table and `InjectionLogRecord` type (col-010, `crates/unimatrix-store/src/injection_log.rs`)
- `sessions` table and `SessionRecord` type (col-010, `crates/unimatrix-store/src/sessions.rs`)
- `entries` table and `EntryRecord` type (`crates/unimatrix-store/`, `crates/unimatrix-core/`)
- `StatusReport` struct and `format_status_report` function (`crates/unimatrix-server/src/mcp/response/status.rs`)
- `StatusService::compute_report` (`crates/unimatrix-server/src/services/status.rs`)
- `confidence.rs` in `unimatrix-engine` (pattern reference for pure computation module)
- `TestDb` helper (`crates/unimatrix-store/src/test_helpers.rs`)

### External Dependencies

None. No new crates required.

## NOT in Scope

- **Retrieval pipeline changes**: Effectiveness scores are NOT wired into search re-ranking, briefing selection, or confidence recomputation. Measure-only. (Follow-on: crt-018b)
- **Automated quarantine or pruning**: The system reports ineffective/noisy entries but does not automatically quarantine them. Human reviews recommendations.
- **New MCP tools**: Effectiveness data surfaces exclusively through the existing `context_status` tool. No new tool registration.
- **Schema migration**: No new tables, no new columns, no ALTER TABLE, no schema version bump.
- **UI/dashboard work**: No visualization. Text and JSON output only.
- **topic_deliveries dependency**: Analysis uses entries.topic + sessions.feature_cycle. Does not depend on col-017 (topic attribution) or nxs-010 (topic_deliveries table).
- **Confidence formula recalibration**: Calibration data is produced for observation. Actual formula adjustments are a separate feature.
- **Embedding tuning from effectiveness data**: Using effectiveness labels for embedding model training is out of scope.
- **Runtime-configurable weights**: Outcome weights and INEFFECTIVE_MIN_INJECTIONS are compile-time constants, not runtime parameters.
- **Historical trend tracking**: Effectiveness is computed as a point-in-time snapshot. No persistence of historical effectiveness data across calls.
