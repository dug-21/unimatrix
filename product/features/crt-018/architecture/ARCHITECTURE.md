# crt-018: Knowledge Effectiveness Analysis — Architecture

## System Overview

crt-018 adds a read-only effectiveness analysis pipeline to `context_status`. It joins injection_log (which entries were served to agents) with session outcomes (did those sessions succeed) to classify every active entry into one of five effectiveness categories. The analysis surfaces through all three `context_status` response formats (summary, markdown, JSON) and produces confidence calibration data.

This feature touches three crates:
- **unimatrix-engine** — New `effectiveness.rs` pure computation module (classification logic, calibration bucketing)
- **unimatrix-store** — New `compute_effectiveness_aggregates()` method consolidating all SQL joins into a single Store call
- **unimatrix-server** — StatusReport extension, StatusService Phase 8, formatting in all three response formats

No new MCP tools. No schema migration. No writes. Purely analytical.

## Component Breakdown

### Component 1: Effectiveness Engine (`unimatrix-engine::effectiveness`)

**Responsibility:** Pure classification and calibration computation. Takes pre-aggregated data from the store layer and produces effectiveness classifications. Zero I/O, fully deterministic, directly unit-testable.

**Pattern:** Follows `confidence.rs` — public pure functions, named constants for thresholds, exhaustive tests.

**Key types:**

```rust
/// Minimum distinct sessions with injection before entry can be classified Ineffective.
pub const INEFFECTIVE_MIN_INJECTIONS: u32 = 3;

/// Outcome weights for utility score computation.
pub const OUTCOME_WEIGHT_SUCCESS: f64 = 1.0;
pub const OUTCOME_WEIGHT_REWORK: f64 = 0.5;
pub const OUTCOME_WEIGHT_ABANDONED: f64 = 0.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum EffectivenessCategory {
    Effective,
    Settled,
    Unmatched,
    Ineffective,
    Noisy,
}

/// Per-entry effectiveness classification result.
#[derive(Debug, Clone, Serialize)]
pub struct EntryEffectiveness {
    pub entry_id: u64,
    pub title: String,
    pub topic: String,
    pub trust_source: String,
    pub category: EffectivenessCategory,
    pub injection_count: u32,
    pub success_rate: f64,
    pub helpfulness_ratio: f64,
}

/// Aggregated effectiveness per trust_source.
#[derive(Debug, Clone, Serialize)]
pub struct SourceEffectiveness {
    pub trust_source: String,
    pub total_entries: u32,
    pub effective_count: u32,
    pub settled_count: u32,
    pub unmatched_count: u32,
    pub ineffective_count: u32,
    pub noisy_count: u32,
    pub aggregate_utility: f64,
}

/// Confidence calibration bucket.
#[derive(Debug, Clone, Serialize)]
pub struct CalibrationBucket {
    pub confidence_lower: f64,
    pub confidence_upper: f64,
    pub entry_count: u32,
    pub actual_success_rate: f64,
}

/// Complete effectiveness analysis result.
#[derive(Debug, Clone, Serialize)]
pub struct EffectivenessReport {
    pub by_category: Vec<(EffectivenessCategory, u32)>,
    pub by_source: Vec<SourceEffectiveness>,
    pub calibration: Vec<CalibrationBucket>,
    pub top_ineffective: Vec<EntryEffectiveness>,
    pub noisy_entries: Vec<EntryEffectiveness>,
    pub unmatched_entries: Vec<EntryEffectiveness>,
    pub data_window: DataWindow,
}

/// Data coverage indicator (SR-02).
#[derive(Debug, Clone, Serialize)]
pub struct DataWindow {
    pub session_count: u32,
    pub earliest_session_at: Option<u64>,
    pub latest_session_at: Option<u64>,
}
```

**Key functions:**

```rust
/// Classify a single entry given its injection/outcome stats and topic activity.
pub fn classify_entry(
    entry_id: u64,
    title: &str,
    topic: &str,
    trust_source: &str,
    helpful_count: u32,
    unhelpful_count: u32,
    injection_count: u32,
    success_count: u32,
    rework_count: u32,
    abandoned_count: u32,
    topic_has_sessions: bool,
    noisy_trust_sources: &[&str],
) -> EntryEffectiveness;

/// Compute weighted success rate from outcome counts.
pub fn utility_score(success: u32, rework: u32, abandoned: u32) -> f64;

/// Aggregate per-entry classifications into source-level stats.
pub fn aggregate_by_source(entries: &[EntryEffectiveness]) -> Vec<SourceEffectiveness>;

/// Build calibration buckets from injection-time confidence and session outcomes.
pub fn build_calibration_buckets(
    rows: &[(f64, bool)],  // (confidence_at_injection, session_succeeded)
) -> Vec<CalibrationBucket>;

/// Assemble the full EffectivenessReport from raw components.
pub fn build_report(
    classifications: Vec<EntryEffectiveness>,
    calibration_rows: &[(f64, bool)],
    data_window: DataWindow,
) -> EffectivenessReport;
```

### Component 2: Effectiveness Aggregates (`unimatrix-store`)

**Responsibility:** SQL-side aggregation of injection_log + sessions + entries data. Returns pre-aggregated rows that the engine classifies. Follows the `compute_status_aggregates()` pattern (ADR-004, Unimatrix #704): one method, one struct, minimal SQL round-trips.

**Key type:**

```rust
/// Raw effectiveness data aggregated by SQL (crt-018: ADR-001).
#[derive(Debug, Clone)]
pub struct EffectivenessAggregates {
    /// Per-entry injection and outcome stats from injection_log JOIN sessions.
    pub entry_stats: Vec<EntryInjectionStats>,
    /// Topics that have at least one session in the retained window.
    pub active_topics: HashSet<String>,
    /// Per-injection confidence bucket and outcome for calibration.
    pub calibration_rows: Vec<(f64, bool)>,
    /// Data window metadata.
    pub data_window: DataWindow,
}

/// Per-entry aggregated injection + outcome data.
#[derive(Debug, Clone)]
pub struct EntryInjectionStats {
    pub entry_id: u64,
    pub injection_count: u32,
    pub success_count: u32,
    pub rework_count: u32,
    pub abandoned_count: u32,
}

/// Metadata about entries needed for classification (from entries table).
#[derive(Debug, Clone)]
pub struct EntryClassificationMeta {
    pub entry_id: u64,
    pub title: String,
    pub topic: String,
    pub trust_source: String,
    pub helpful_count: u32,
    pub unhelpful_count: u32,
}
```

**Store method:**

```rust
impl Store {
    /// Compute effectiveness aggregates via SQL joins.
    ///
    /// Three queries in one method:
    /// 1. Entry injection stats: injection_log JOIN sessions GROUP BY entry_id
    /// 2. Active topics: sessions GROUP BY feature_cycle (WHERE feature_cycle IS NOT NULL)
    /// 3. Calibration: injection_log JOIN sessions bucketed by confidence
    /// 4. Data window: MIN/MAX started_at, COUNT from sessions
    ///
    /// All queries use existing indexes (idx_injection_log_entry,
    /// idx_injection_log_session, idx_sessions_feature_cycle).
    pub fn compute_effectiveness_aggregates(&self) -> Result<EffectivenessAggregates>;
}
```

**SQL design (SR-01: consolidated queries):**

Query 1 — Entry injection stats (uses `idx_injection_log_entry`):
```sql
SELECT
    il.entry_id,
    COUNT(DISTINCT il.session_id) as injection_count,
    COALESCE(SUM(CASE WHEN s.outcome = 'success' THEN 1 ELSE 0 END), 0) as success_count,
    COALESCE(SUM(CASE WHEN s.outcome = 'rework' THEN 1 ELSE 0 END), 0) as rework_count,
    COALESCE(SUM(CASE WHEN s.outcome = 'abandoned' THEN 1 ELSE 0 END), 0) as abandoned_count
FROM injection_log il
JOIN sessions s ON il.session_id = s.session_id
WHERE s.outcome IS NOT NULL
GROUP BY il.entry_id
```

Query 2 — Active topics (uses `idx_sessions_feature_cycle`):
```sql
SELECT DISTINCT feature_cycle
FROM sessions
WHERE feature_cycle IS NOT NULL AND feature_cycle != ''
```

Query 3 — Calibration rows (full scan of injection_log JOIN sessions):
```sql
SELECT il.confidence, (s.outcome = 'success') as succeeded
FROM injection_log il
JOIN sessions s ON il.session_id = s.session_id
WHERE s.outcome IS NOT NULL
```

Query 4 — Data window:
```sql
SELECT COUNT(*), MIN(started_at), MAX(started_at)
FROM sessions
WHERE outcome IS NOT NULL
```

### Component 3: StatusReport Extension (`unimatrix-server`)

**Responsibility:** Add effectiveness section to StatusReport struct, integrate computation into StatusService Phase 8, format output in all three response formats.

**StatusReport new field:**

```rust
// In StatusReport struct:
/// Effectiveness analysis results (None if no injection data exists).
pub effectiveness: Option<EffectivenessReport>,
```

**StatusService integration (Phase 8 in `compute_report`):**

```rust
// Phase 8: Effectiveness analysis (crt-018)
let store_for_eff = Arc::clone(&self.store);
let active_entries_for_eff = active_entries.clone();
let effectiveness_result = tokio::task::spawn_blocking(move || {
    // 1. Get aggregates from store (SQL)
    let aggregates = store_for_eff.compute_effectiveness_aggregates()?;

    // 2. Get entry metadata for classification
    let entry_meta = store_for_eff.load_entry_classification_meta()?;

    // 3. Classify each entry (pure computation)
    // 4. Build report
    Ok::<_, StoreError>(/* EffectivenessReport */)
}).await;
```

**Formatting:**

- **Summary:** One line: `Effectiveness: 42 effective, 15 settled, 3 unmatched, 2 ineffective, 1 noisy (N sessions over M days)`
- **Markdown:** `### Effectiveness Analysis` section with category table, per-source table, calibration table, top-10 ineffective entries, data window indicator
- **JSON:** `effectiveness` object with `skip_serializing_if = "Option::is_none"` — omitted when no injection data

## Component Interactions

```
context_status call
       |
       v
StatusService::compute_report()
       |
       | Phase 8 (spawn_blocking)
       v
Store::compute_effectiveness_aggregates()  -->  SQL: injection_log JOIN sessions
       |                                              GROUP BY entry_id
       | Returns EffectivenessAggregates
       v
Store::load_entry_classification_meta()    -->  SQL: SELECT from entries (active only)
       |
       | Returns Vec<EntryClassificationMeta>
       v
effectiveness::classify_entry()            -->  Pure function, per-entry
       |
       v
effectiveness::build_report()              -->  Assembles EffectivenessReport
       |
       v
StatusReport.effectiveness = Some(report)
       |
       v
format_status_report()                     -->  Summary / Markdown / JSON
```

## Technology Decisions

| Decision | ADR | Rationale |
|----------|-----|-----------|
| Single consolidated Store method | ADR-001 | Follows StatusAggregates pattern (Unimatrix #704); minimizes SQL round-trips (SR-01, SR-07) |
| NULL topic/feature_cycle explicit handling | ADR-002 | Entries with empty/NULL topic assigned to "(unattributed)" bucket; sessions with NULL feature_cycle excluded from active_topics (SR-06) |
| Data window indicator in output | ADR-003 | Classifications are ephemeral snapshots bounded by GC retention; consumers need coverage context (SR-02) |
| Configurable noisy trust sources | ADR-004 | Array constant instead of hardcoded "auto"; allows "neural" inclusion (SR-05) |

## Integration Points

### Dependencies (existing, no changes)
- `unimatrix-store::Store` — SQLite connection, existing tables
- `unimatrix-store::sessions` — SessionRecord, GcStats, DELETE_THRESHOLD_SECS
- `unimatrix-store::injection_log` — InjectionLogRecord (type reference only; queries are new SQL)
- `unimatrix-engine::confidence` — Constants referenced in calibration analysis docs
- `unimatrix-server::services::status` — StatusService, compute_report
- `unimatrix-server::mcp::response::status` — StatusReport, format_status_report

### New code locations
- `crates/unimatrix-engine/src/effectiveness.rs` — Pure computation module (~250-350 lines)
- `crates/unimatrix-engine/src/lib.rs` — Add `pub mod effectiveness;`
- `crates/unimatrix-store/src/read.rs` — Add `compute_effectiveness_aggregates()` and `load_entry_classification_meta()` to Store impl, add `EffectivenessAggregates` and related structs
- `crates/unimatrix-server/src/mcp/response/status.rs` — Extend StatusReport, StatusReportJson, format_status_report
- `crates/unimatrix-server/src/services/status.rs` — Add Phase 8 to compute_report

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `Store::compute_effectiveness_aggregates()` | `&self -> Result<EffectivenessAggregates>` | `unimatrix-store/src/read.rs` (new) |
| `Store::load_entry_classification_meta()` | `&self -> Result<Vec<EntryClassificationMeta>>` | `unimatrix-store/src/read.rs` (new) |
| `effectiveness::classify_entry()` | `(entry_id, title, topic, trust_source, helpful, unhelpful, inj_count, success, rework, abandoned, topic_active, noisy_sources) -> EntryEffectiveness` | `unimatrix-engine/src/effectiveness.rs` (new) |
| `effectiveness::build_report()` | `(Vec<EntryEffectiveness>, &[(f64, bool)], DataWindow) -> EffectivenessReport` | `unimatrix-engine/src/effectiveness.rs` (new) |
| `effectiveness::build_calibration_buckets()` | `(&[(f64, bool)]) -> Vec<CalibrationBucket>` | `unimatrix-engine/src/effectiveness.rs` (new) |
| `effectiveness::aggregate_by_source()` | `(&[EntryEffectiveness]) -> Vec<SourceEffectiveness>` | `unimatrix-engine/src/effectiveness.rs` (new) |
| `effectiveness::utility_score()` | `(u32, u32, u32) -> f64` | `unimatrix-engine/src/effectiveness.rs` (new) |
| `StatusReport.effectiveness` | `Option<EffectivenessReport>` | `unimatrix-server/src/mcp/response/status.rs` (extended) |
| `StatusReportJson.effectiveness` | `#[serde(skip_serializing_if = "Option::is_none")] Option<EffectivenessReportJson>` | `unimatrix-server/src/mcp/response/status.rs` (extended) |
| `EffectivenessAggregates` | struct with `entry_stats`, `active_topics`, `calibration_rows`, `data_window` | `unimatrix-store/src/read.rs` (new) |
| `EntryInjectionStats` | struct with `entry_id`, `injection_count`, `success_count`, `rework_count`, `abandoned_count` | `unimatrix-store/src/read.rs` (new) |
| `EntryClassificationMeta` | struct with `entry_id`, `title`, `topic`, `trust_source`, `helpful_count`, `unhelpful_count` | `unimatrix-store/src/read.rs` (new) |
| `INEFFECTIVE_MIN_INJECTIONS` | `u32 = 3` | `unimatrix-engine/src/effectiveness.rs` (new) |
| `OUTCOME_WEIGHT_SUCCESS` | `f64 = 1.0` | `unimatrix-engine/src/effectiveness.rs` (new) |
| `OUTCOME_WEIGHT_REWORK` | `f64 = 0.5` | `unimatrix-engine/src/effectiveness.rs` (new) |
| `OUTCOME_WEIGHT_ABANDONED` | `f64 = 0.0` | `unimatrix-engine/src/effectiveness.rs` (new) |
| `NOISY_TRUST_SOURCES` | `&[&str] = &["auto"]` | `unimatrix-engine/src/effectiveness.rs` (new) |

## Error Boundaries

- **Store layer errors** (`StoreError::Sqlite`) propagate through `Result<EffectivenessAggregates>` to StatusService
- **StatusService** catches store errors via `spawn_blocking` join + result unwrap; on failure, `report.effectiveness = None` (graceful degradation, matching contradiction scan pattern)
- **Engine layer** is infallible — pure functions with no error paths; degenerate inputs produce valid but empty reports
- **Division by zero** guarded in `utility_score()` — returns 0.0 when total outcomes = 0
- **Empty injection_log** — `compute_effectiveness_aggregates()` returns empty vecs; engine produces report with all entries as Unmatched/Settled; `effectiveness = Some(report)` with empty data window

## Test Strategy

**Unit tests** (in `effectiveness.rs`):
- All five classification categories with boundary conditions
- `utility_score` edge cases (all zeros, mixed)
- `build_calibration_buckets` with known distributions
- `aggregate_by_source` with mixed trust sources
- Empty data produces valid empty report

**Store integration tests** (in `read.rs` tests or dedicated test):
- `compute_effectiveness_aggregates` with known injection_log + sessions data
- NULL feature_cycle sessions excluded from active_topics
- Empty tables return empty aggregates

**Server integration tests** (in existing status test infrastructure):
- End-to-end: insert entries + sessions + injection_log, call compute_report, verify effectiveness section
- All three format outputs contain effectiveness data
- Graceful degradation when no injection data exists

All tests extend existing `TestDb` helper. No isolated test scaffolding.
