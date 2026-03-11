# Knowledge Effectiveness Analysis

## Problem Statement

Unimatrix's hook pipeline injects knowledge entries into every agent prompt via ContextSearch. The system currently tracks injection events (injection_log) and receives implicit helpfulness signals (signal_queue), but never connects these data streams to answer the fundamental question: **are the injected entries actually helping agents succeed?**

This matters most for the 384 auto-extracted entries (trust_source="auto") created by the neural extraction pipeline. An unused entry sitting in the database costs ~1KB of storage (irrelevant). An entry that gets injected into context windows but never contributes to agent success costs real tokens and displaces potentially better knowledge. Without effectiveness measurement, the system cannot distinguish between entries that are genuinely useful and entries that are noise.

The confidence formula (six-factor additive composite) includes a trust_source weight (auto=0.35) and a helpfulness Wilson score, but has never been validated against actual injection-to-outcome data. We do not know whether higher-confidence entries actually correlate with higher utility when injected.

## Goals

1. Compute per-entry utility scores from injection_log joined with session outcomes, classifying entries into effectiveness categories (Effective, Settled, Unmatched, Ineffective, Noisy)
2. Produce aggregate effectiveness metrics by trust_source (auto vs agent vs system vs human vs neural) to prove or disprove the value of auto-extracted knowledge
3. Validate confidence calibration by binning entries by confidence-at-injection-time and comparing to actual helpfulness rates
4. Surface all effectiveness data in `context_status` output (all three formats: summary, markdown, JSON)
5. Provide topic-aware lifecycle classification that distinguishes settled knowledge (topic inactive, historically useful) from genuinely ineffective knowledge

## Non-Goals

- **No retrieval pipeline changes**: Effectiveness scores are NOT wired into search re-ranking, briefing selection, or confidence recomputation. Measure-only. (Follow-on: crt-018b)
- **No automated quarantine or pruning**: The system reports ineffective/noisy entries but does not automatically quarantine them. Human reviews recommendations.
- **No new MCP tools**: Effectiveness data surfaces exclusively through the existing `context_status` tool.
- **No schema migration**: All required data already exists in injection_log, signal_queue (consumed into helpful_count/unhelpful_count on entries), sessions, and entries tables. Analysis is computed at query time.
- **No UI/dashboard work**: Raw data and formatted text only; visualization is a separate concern.
- **No topic_deliveries dependency**: While topic_deliveries could provide "topic active/inactive" signal, sessions.feature_cycle + entries.topic provide sufficient data without requiring col-017 (topic attribution) to be complete first.

## Background Research

### Existing Data Infrastructure

All raw data needed for effectiveness analysis already exists in SQLite:

**injection_log** table (col-010): Records every entry injected into an agent prompt.
- Columns: `log_id`, `session_id`, `entry_id`, `confidence` (at injection time), `timestamp`
- Indexes: `idx_injection_log_session` (session_id), `idx_injection_log_entry` (entry_id)
- Key: The `confidence` column captures the reranked score at the moment of injection, enabling calibration analysis.

**sessions** table (col-010): Records session lifecycle with outcome.
- Columns: `session_id`, `feature_cycle`, `outcome` ("success"/"rework"/"abandoned"), `total_injections`, `status`, `started_at`, `ended_at`
- Index: `idx_sessions_feature_cycle`
- Key: The `outcome` column is the primary signal for whether injected knowledge helped. `feature_cycle` links sessions to topics.

**signal_queue** table (col-010): Transient queue for helpfulness signals, consumed by confidence pipeline.
- Columns: `signal_id`, `session_id`, `entry_ids` (JSON), `signal_type` (Helpful/Flagged), `signal_source` (ImplicitOutcome/ImplicitRework)
- Key: Signals are drained and consumed into entries.helpful_count/unhelpful_count. The queue itself is transient, but the aggregated counts on entries persist.

**entries** table: Stores helpful_count, unhelpful_count, trust_source, confidence, access_count, topic, category, feature_cycle, status.

**query_log** table (nxs-010): Captures search queries with result entry IDs and similarity scores.
- Useful for cross-referencing which entries were retrieved (query_log) vs actually injected (injection_log).

### Key Join Paths for Effectiveness Analysis

The core analysis requires these SQL joins:

1. **Entry utility**: `injection_log JOIN sessions ON session_id` -- which entries were injected into sessions that succeeded vs failed
2. **Entry classification**: `entries LEFT JOIN injection_log ON entry_id` -- entries that were never injected (Unmatched) vs frequently injected
3. **Topic activity**: `sessions GROUP BY feature_cycle` -- is the topic still active (recent sessions) or settled (no recent sessions)
4. **Confidence calibration**: `injection_log` grouped by confidence bucket, joined with session outcomes

### Confidence Formula (crates/unimatrix-engine/src/confidence.rs)

Six stored factors summing to 0.92:
- W_BASE=0.18, W_USAGE=0.14, W_FRESH=0.18, W_HELP=0.14, W_CORR=0.14, W_TRUST=0.14
- Trust scores: human=1.0, system=0.7, agent=0.5, neural=0.40, auto=0.35
- Helpfulness uses Wilson score lower bound with min-5-votes guard

The calibration validation will test whether entries with confidence 0.7-0.8 at injection time actually have ~70-80% helpfulness rate. If the formula is miscalibrated, this data informs recalibration (separate feature).

### StatusReport Structure (crates/unimatrix-server/src/mcp/response/status.rs)

StatusReport is a plain struct with ~45 fields across categories (entry counts, coherence, co-access, outcomes, observations, extraction). Adding an effectiveness section follows the established pattern:
- Add fields to StatusReport struct
- Add formatting to all three ResponseFormat arms (Summary, Markdown, Json)
- Add a serializable JSON sub-struct
- Compute in StatusService::compute_report via spawn_blocking

### Entry Classification Logic

Using injection_log + sessions + entries, classify each active entry:

| Category | Condition | Action |
|----------|-----------|--------|
| **Effective** | Injected into sessions with success outcome, positive helpfulness ratio | None (working as intended) |
| **Settled** | Topic has no recent sessions (>30 days), entry has historical helpfulness | None (served its era) |
| **Unmatched** | Never appears in injection_log despite topic having active sessions | Review title/tags/embedding |
| **Ineffective** | Frequently injected (>5 sessions), but session outcomes are predominantly rework/abandoned | Confidence decay candidate |
| **Noisy** | trust_source="auto", low quality signals (no helpful votes, low access), injected but never helpful | Quarantine candidate |

### Existing Patterns in StatusService

StatusService::compute_report (crates/unimatrix-server/src/services/status.rs) follows a phased pattern:
- Phase 1: SQL queries via spawn_blocking (counters, aggregations)
- Phase 2-7: Additional computations (contradictions, co-access, coherence, observations)
- Each phase is independent and computes into mutable `report` fields

Effectiveness analysis fits as a new phase (Phase 8) in compute_report, using a dedicated spawn_blocking call for the SQL joins.

## Proposed Approach

### New Module: `crates/unimatrix-engine/src/effectiveness.rs`

Pure computation module (no I/O) that takes pre-fetched data and produces effectiveness classifications and metrics. Follows the pattern of `confidence.rs` (pure functions, testable without DB).

Key types:
- `EntryEffectiveness { entry_id, category: EffectivenessCategory, injection_count, success_rate, helpfulness_ratio }`
- `EffectivenessCategory` enum: `Effective`, `Settled`, `Unmatched`, `Ineffective`, `Noisy`
- `SourceEffectiveness { trust_source, total_entries, effective_count, settled_count, unmatched_count, ineffective_count, noisy_count, aggregate_utility }`
- `CalibrationBucket { confidence_range: (f64, f64), entry_count, actual_helpfulness_rate }`
- `EffectivenessReport { entries_by_category, by_source, calibration_buckets, summary_stats }`

### Store Layer: New query methods on Store

- `scan_injection_stats_by_entry() -> Vec<(entry_id, injection_count, distinct_sessions, avg_confidence)>` -- SQL aggregation on injection_log GROUP BY entry_id
- `scan_session_outcomes_by_entry() -> Vec<(entry_id, success_count, rework_count, abandoned_count)>` -- SQL JOIN injection_log + sessions GROUP BY entry_id
- `scan_topic_activity() -> Vec<(topic, last_session_at, session_count)>` -- SQL aggregation on sessions GROUP BY feature_cycle
- `scan_injection_confidence_buckets() -> Vec<(bucket, total, success_count)>` -- SQL CASE expression bucketing confidence into 0.1 ranges

### StatusService Integration

Add Phase 8 to `compute_report`:
- Single spawn_blocking call fetches all effectiveness data via the new Store methods
- Passes raw data to `effectiveness.rs` pure functions for classification
- Populates new fields on StatusReport

### StatusReport Extensions

New fields grouped in an `EffectivenessSection`:
- `effectiveness_by_category: Vec<(EffectivenessCategory, count)>`
- `effectiveness_by_source: Vec<SourceEffectiveness>`
- `calibration_buckets: Vec<CalibrationBucket>`
- `ineffective_entries: Vec<(entry_id, title, injection_count, success_rate)>` -- top 10 worst performers
- `noisy_entries: Vec<(entry_id, title)>` -- auto-extracted entries flagged for review
- `unmatched_entries: Vec<(entry_id, title, topic)>` -- entries never injected despite active topic

## Acceptance Criteria

- AC-01: Every active entry is classified into exactly one of five categories: Effective, Settled, Unmatched, Ineffective, Noisy
- AC-02: Classification uses injection_log + sessions.outcome join; entries with zero injections are classified as Unmatched (if topic active) or Settled (if topic inactive)
- AC-03: "Settled" classification requires no sessions for the entry's topic within the available data window (bounded by session GC retention) AND at least one historical injection with success outcome
- AC-04: "Ineffective" classification requires >= INEFFECTIVE_MIN_INJECTIONS (default: 3, tunable) injection sessions AND success rate < 30%
- AC-05: "Noisy" classification requires trust_source="auto" AND zero helpful_count AND at least one injection
- AC-06: Aggregate effectiveness metrics are computed per trust_source (auto, agent, system, human, neural), showing counts per category and aggregate utility ratio
- AC-07: Confidence calibration produces 10 buckets (0.0-0.1, 0.1-0.2, ..., 0.9-1.0) comparing confidence-at-injection-time to actual session success rate
- AC-08: context_status summary format includes a one-line effectiveness summary (e.g., "Effectiveness: 42 effective, 15 settled, 3 unmatched, 2 ineffective, 1 noisy")
- AC-09: context_status markdown format includes an "### Effectiveness Analysis" section with category table, per-source table, calibration table, and top-10 ineffective entries
- AC-10: context_status JSON format includes an `effectiveness` object with structured data matching the EffectivenessReport type
- AC-11: Effectiveness computation runs in spawn_blocking and does not block the async runtime
- AC-12: Top 10 ineffective entries and all noisy entries are listed with entry_id and title for human review
- AC-13: Effectiveness analysis is computed on every context_status call (no opt-in flag; data is always available). context_status remains read-only — no writes, no side effects. Classifications are transient, computed fresh each call.
- AC-14: Pure computation functions in effectiveness.rs have unit tests covering all five classification categories, boundary conditions (exactly 5 injections, exactly 30-day cutoff), and empty-data graceful handling
- AC-15: Integration tests verify end-to-end flow: insert entries + injection_log + sessions with known outcomes, call status, verify effectiveness section in output

## Constraints

1. **No schema migration**: Must work with existing tables (injection_log, sessions, entries, query_log). No new tables or columns.
2. **Performance budget**: Effectiveness queries run on every context_status call. Must complete within 500ms for a knowledge base of 500 entries and 10,000 injection_log rows. Use SQL aggregation (GROUP BY) rather than loading all rows into Rust.
3. **Session GC interaction**: Sessions older than 30 days are deleted by gc_sessions (DELETE_THRESHOLD_SECS = 30 days), along with their injection_log rows. Effectiveness analysis operates on whatever data GC retains — no hardcoded time thresholds. "Settled" classification uses absence of sessions for a topic in the available data, not a fixed cutoff. If GC retention changes, analysis adapts automatically.
4. **Signal queue transience**: signal_queue is drained and deleted after processing. Effectiveness analysis uses the persisted helpful_count/unhelpful_count on entries, not the transient queue.
5. **Existing StatusReport is already large** (~45 fields, ~700 lines of formatting). New fields must be added without breaking existing output. Use `skip_serializing_if` for JSON format to keep output clean when no effectiveness data exists.
6. **Test infrastructure**: Extend existing TestDb helper and test patterns from injection_log.rs and sessions.rs. Do not create isolated test scaffolding.

## Open Questions

No open questions — all resolved during scope review:

- **Settled threshold**: No hardcoded cutoff. Uses available data window (bounded by session GC retention).
- **Rework weighting**: success = 1.0, rework = 0.5, abandoned = 0.0.
- **Calibration buckets**: 10 buckets (0.1 width). Keeps output digestible.
- **Minimum injection threshold**: 3 (tunable via INEFFECTIVE_MIN_INJECTIONS constant).
- **context_status remains read-only**: No writes, no side effects.

## Out of Scope (Follow-on Opportunities)

- **crt-018b: Effectiveness-Weighted Retrieval** -- Wire effectiveness scores into search re-ranking (boost proven-effective entries, penalize ineffective ones)
- **Effectiveness-weighted briefing** -- Rank briefing results by proven utility rather than just confidence
- **Dead knowledge automation** -- Automatically quarantine entries below a utility threshold after N days of ineffectiveness
- **Knowledge-that-helped-this-topic** -- Surface in context_retrospective which entries contributed to topic success
- **Embedding tuning from effectiveness data** -- Use (entry, effective/ineffective) labels to fine-tune embedding model

## Tracking

https://github.com/dug-21/unimatrix/issues/205
