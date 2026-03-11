# crt-018: Knowledge Effectiveness Analysis — Implementation Brief

## Source Documents

| Document | Path |
|----------|------|
| Scope | product/features/crt-018/SCOPE.md |
| Scope Risk Assessment | product/features/crt-018/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/crt-018/architecture/ARCHITECTURE.md |
| Specification | product/features/crt-018/specification/SPECIFICATION.md |
| Risk & Test Strategy | product/features/crt-018/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/crt-018/ALIGNMENT-REPORT.md |

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| effectiveness-engine | pseudocode/effectiveness-engine.md | test-plan/effectiveness-engine.md |
| effectiveness-store | pseudocode/effectiveness-store.md | test-plan/effectiveness-store.md |
| status-integration | pseudocode/status-integration.md | test-plan/status-integration.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

## Goal

Compute per-entry utility scores from injection_log joined with session outcomes, classify every active entry into one of five effectiveness categories (Effective, Settled, Unmatched, Ineffective, Noisy), validate confidence calibration against actual helpfulness rates, and surface all effectiveness data through the existing `context_status` MCP tool in all three output formats. This provides the first empirical answer to whether injected knowledge entries are actually helping agents succeed, with particular focus on validating auto-extracted entries.

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Store method design: 4 independent scans vs 1 consolidated method | Single `compute_effectiveness_aggregates()` returning `EffectivenessAggregates` struct, following StatusAggregates pattern (Unimatrix #704). One `lock_conn()`, 4 sequential SQL queries. | SR-01, SR-07 | architecture/ADR-001-consolidated-effectiveness-query.md |
| NULL topic and feature_cycle handling | Map NULL/empty topic to `"(unattributed)"` sentinel. Sessions with NULL feature_cycle excluded from active_topics but included in injection JOINs. | SR-06, Unimatrix #981 | architecture/ADR-002-null-topic-handling.md |
| GC-bounded data window visibility | Include `DataWindow` struct (session_count, earliest/latest timestamps) in every effectiveness report so consumers understand coverage bounds. | SR-02 | architecture/ADR-003-data-window-indicator.md |
| Noisy trust source filter scope | Array constant `NOISY_TRUST_SOURCES: &[&str] = &["auto"]` instead of hardcoded string comparison. Adding "neural" later is a one-line change. | SR-05 | architecture/ADR-004-configurable-noisy-trust-sources.md |
| Classification priority order | Noisy > Ineffective > Unmatched > Settled > Effective (mutually exclusive, first match wins) | Spec FR-01 | N/A (specification) |
| Rework outcome weighting | success=1.0, rework=0.5, abandoned=0.0 as named constants | SR-08, Spec FR-02 | N/A (specification) |

## Files to Create/Modify

### New Files

| Path | Summary |
|------|---------|
| `crates/unimatrix-engine/src/effectiveness.rs` | Pure computation module: classification logic, calibration bucketing, aggregation (~250-350 lines) |

### Modified Files

| Path | Summary |
|------|---------|
| `crates/unimatrix-engine/src/lib.rs` | Add `pub mod effectiveness;` |
| `crates/unimatrix-store/src/read.rs` | Add `compute_effectiveness_aggregates()` and `load_entry_classification_meta()` methods + supporting structs |
| `crates/unimatrix-server/src/mcp/response/status.rs` | Extend StatusReport with `effectiveness: Option<EffectivenessReport>`, add EffectivenessReportJson, update format_status_report for all three formats |
| `crates/unimatrix-server/src/services/status.rs` | Add Phase 8 to compute_report: spawn_blocking for effectiveness SQL + classification |

## Data Structures

### Engine Types (`unimatrix-engine::effectiveness`)

```rust
pub const INEFFECTIVE_MIN_INJECTIONS: u32 = 3;
pub const OUTCOME_WEIGHT_SUCCESS: f64 = 1.0;
pub const OUTCOME_WEIGHT_REWORK: f64 = 0.5;
pub const OUTCOME_WEIGHT_ABANDONED: f64 = 0.0;
pub const NOISY_TRUST_SOURCES: &[&str] = &["auto"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum EffectivenessCategory {
    Effective, Settled, Unmatched, Ineffective, Noisy,
}

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

#[derive(Debug, Clone, Serialize)]
pub struct CalibrationBucket {
    pub confidence_lower: f64,
    pub confidence_upper: f64,
    pub entry_count: u32,
    pub actual_success_rate: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DataWindow {
    pub session_count: u32,
    pub earliest_session_at: Option<u64>,
    pub latest_session_at: Option<u64>,
}

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
```

### Store Types (`unimatrix-store`)

```rust
pub struct EffectivenessAggregates {
    pub entry_stats: Vec<EntryInjectionStats>,
    pub active_topics: HashSet<String>,
    pub calibration_rows: Vec<(f64, bool)>,
    pub data_window: DataWindow,
}

pub struct EntryInjectionStats {
    pub entry_id: u64,
    pub injection_count: u32,
    pub success_count: u32,
    pub rework_count: u32,
    pub abandoned_count: u32,
}

pub struct EntryClassificationMeta {
    pub entry_id: u64,
    pub title: String,
    pub topic: String,
    pub trust_source: String,
    pub helpful_count: u32,
    pub unhelpful_count: u32,
}
```

## Function Signatures

### Engine (`effectiveness.rs`)

```rust
pub fn classify_entry(
    entry_id: u64, title: &str, topic: &str, trust_source: &str,
    helpful_count: u32, unhelpful_count: u32,
    injection_count: u32, success_count: u32, rework_count: u32, abandoned_count: u32,
    topic_has_sessions: bool, noisy_trust_sources: &[&str],
) -> EntryEffectiveness;

pub fn utility_score(success: u32, rework: u32, abandoned: u32) -> f64;

pub fn aggregate_by_source(entries: &[EntryEffectiveness]) -> Vec<SourceEffectiveness>;

pub fn build_calibration_buckets(rows: &[(f64, bool)]) -> Vec<CalibrationBucket>;

pub fn build_report(
    classifications: Vec<EntryEffectiveness>,
    calibration_rows: &[(f64, bool)],
    data_window: DataWindow,
) -> EffectivenessReport;
```

### Store (`read.rs`)

```rust
impl Store {
    pub fn compute_effectiveness_aggregates(&self) -> Result<EffectivenessAggregates>;
    pub fn load_entry_classification_meta(&self) -> Result<Vec<EntryClassificationMeta>>;
}
```

### Server (`status.rs` / `response/status.rs`)

```rust
// StatusReport extension
pub effectiveness: Option<EffectivenessReport>,

// StatusReportJson extension
#[serde(skip_serializing_if = "Option::is_none")]
pub effectiveness: Option<EffectivenessReportJson>,
```

## Constraints

1. **No schema migration** — Must work with existing tables (injection_log, sessions, entries). No new tables, columns, or schema version bump.
2. **Performance budget** — 500ms for 500 entries + 10,000 injection_log rows. SQL-side GROUP BY aggregation mandatory.
3. **Session GC interaction** — Sessions older than 30 days are deleted by gc_sessions along with injection_log rows (cascade). Effectiveness analysis operates on the retained window. No hardcoded time thresholds. "Settled" uses absence of sessions for a topic in available data, not a fixed cutoff.
4. **Signal queue transience** — Use persisted helpful_count/unhelpful_count on entries, not the transient signal_queue.
5. **StatusReport size** — Use `skip_serializing_if` for JSON format. Cap lists: top 10 ineffective, top 10 unmatched, all noisy.
6. **Test infrastructure** — Extend existing TestDb helper and patterns from injection_log.rs / sessions.rs. No isolated test scaffolding.
7. **NULL handling (ADR-002)** — Entries with NULL/empty topic mapped to "(unattributed)". Sessions with NULL feature_cycle excluded from active_topics but included in injection JOINs.
8. **Read-only** — context_status with effectiveness data performs no writes, no side effects. Classifications are transient, computed fresh each call.
9. **Async runtime safety** — All SQL queries run inside `spawn_blocking`. No blocking I/O on the tokio runtime.

## Dependencies

### Crate Dependencies (no new external crates)

- `unimatrix-engine` — New effectiveness.rs module (pure computation, uses serde for Serialize derive)
- `unimatrix-store` — New query methods on Store (uses existing rusqlite)
- `unimatrix-server` — StatusReport extension, Phase 8 integration (uses existing tokio, serde_json)

### Existing Components Referenced

- `injection_log` table + `InjectionLogRecord` (`crates/unimatrix-store/src/injection_log.rs`)
- `sessions` table + `SessionRecord` (`crates/unimatrix-store/src/sessions.rs`)
- `entries` table + `EntryRecord` (`crates/unimatrix-store/`, `crates/unimatrix-core/`)
- `StatusReport` struct + `format_status_report` (`crates/unimatrix-server/src/mcp/response/status.rs`)
- `StatusService::compute_report` (`crates/unimatrix-server/src/services/status.rs`)
- `confidence.rs` in `unimatrix-engine` (pattern reference)
- `TestDb` helper (`crates/unimatrix-store/src/test_helpers.rs`)

## SQL Queries (Store Layer)

**Query 1 — Entry injection stats** (uses `idx_injection_log_entry`):
```sql
SELECT il.entry_id,
    COUNT(DISTINCT il.session_id) as injection_count,
    COALESCE(SUM(CASE WHEN s.outcome = 'success' THEN 1 ELSE 0 END), 0) as success_count,
    COALESCE(SUM(CASE WHEN s.outcome = 'rework' THEN 1 ELSE 0 END), 0) as rework_count,
    COALESCE(SUM(CASE WHEN s.outcome = 'abandoned' THEN 1 ELSE 0 END), 0) as abandoned_count
FROM injection_log il
JOIN sessions s ON il.session_id = s.session_id
WHERE s.outcome IS NOT NULL
GROUP BY il.entry_id
```

**Query 2 — Active topics** (uses `idx_sessions_feature_cycle`):
```sql
SELECT DISTINCT feature_cycle FROM sessions
WHERE feature_cycle IS NOT NULL AND feature_cycle != ''
```

**Query 3 — Calibration rows**:
```sql
SELECT il.confidence, (s.outcome = 'success') as succeeded
FROM injection_log il
JOIN sessions s ON il.session_id = s.session_id
WHERE s.outcome IS NOT NULL
```

**Query 4 — Data window**:
```sql
SELECT COUNT(*), MIN(started_at), MAX(started_at)
FROM sessions WHERE outcome IS NOT NULL
```

## NOT in Scope

- Retrieval pipeline changes (effectiveness scores NOT wired into search re-ranking, briefing, or confidence recomputation)
- Automated quarantine or pruning (human reviews recommendations)
- New MCP tools (surfaces exclusively through existing context_status)
- Schema migration (no new tables, columns, or ALTER TABLE)
- UI/dashboard work
- Confidence formula recalibration (calibration data is observational only)
- Embedding tuning from effectiveness data
- Runtime-configurable weights (compile-time constants only)
- Historical trend tracking (point-in-time snapshots only)

## Alignment Status

**5 PASS, 1 WARN. No variances requiring approval.**

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Serves "trustworthy, correctable, auditable" lifecycle and confidence evolution pillars |
| Milestone Fit | PASS | Listed in Activity Intelligence milestone, Wave 3 |
| Scope Gaps | PASS | All 15 SCOPE ACs covered |
| Scope Additions | WARN | Spec adds AC-16 (NULL topic handling) and AC-17 (named constants) beyond SCOPE's 15 ACs; both trace to SCOPE-RISK-ASSESSMENT recommendations (SR-06, SR-08) |
| Architecture Consistency | PASS | Store method consolidation follows SR-07 recommendation |
| Risk Completeness | PASS | 13 risks with full traceability to all 8 scope risks |

The WARN on Scope Additions is justified: AC-16 prevents a known production bug pattern (Unimatrix #981) and AC-17 eliminates magic numbers per scope risk assessment recommendations. Both additions are risk-driven, not scope creep.

Additional architecture additions (DataWindow struct, NOISY_TRUST_SOURCES array) all trace to specific SCOPE-RISK-ASSESSMENT recommendations (SR-02, SR-05) and add minimal complexity.
