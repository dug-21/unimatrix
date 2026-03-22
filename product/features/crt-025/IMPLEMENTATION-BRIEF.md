# crt-025 Implementation Brief
## WA-1: Phase Signal + FEATURE_ENTRIES Tagging

GH #330 | Schema v14 → v15

---

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/crt-025/SCOPE.md |
| Architecture | product/features/crt-025/architecture/ARCHITECTURE.md |
| Specification | product/features/crt-025/specification/SPECIFICATION.md |
| Risk Strategy | product/features/crt-025/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/crt-025/ALIGNMENT-REPORT.md |

---

## Goal

Add explicit workflow phase awareness to the Unimatrix engine by extending `context_cycle` with a `phase-end` event type and phase-transition parameters, recording every lifecycle event in a new append-only `CYCLE_EVENTS` table, propagating the active phase into `SessionState`, tagging each `feature_entries` row with the phase at store time, and enriching `context_cycle_review` with an explicit phase narrative and cross-cycle category distribution comparison. This produces clean supervised training labels for the W3-1 GNN pipeline and gives the retrospective tool an explicit record of phase sequences and rework events — a capability that was previously absent because only `start` and `stop` events existed.

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| Validation Layer | pseudocode/validation-layer.md | test-plan/validation-layer.md |
| MCP Tool Handler | pseudocode/mcp-tool-handler.md | test-plan/mcp-tool-handler.md |
| Hook Path | pseudocode/hook-path.md | test-plan/hook-path.md |
| SessionState | pseudocode/session-state.md | test-plan/session-state.md |
| UDS Listener | pseudocode/uds-listener.md | test-plan/uds-listener.md |
| Store Layer | pseudocode/store-layer.md | test-plan/store-layer.md |
| Schema Migration | pseudocode/schema-migration.md | test-plan/schema-migration.md |
| Context Store Phase Capture | pseudocode/context-store-phase-capture.md | test-plan/context-store-phase-capture.md |
| Phase Narrative (cycle_review) | pseudocode/phase-narrative.md | test-plan/phase-narrative.md |
| CategoryAllowlist | pseudocode/category-allowlist.md | test-plan/category-allowlist.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Phase snapshot timing for `FeatureEntry` drain path | Snapshot `current_phase` at enqueue time into `AnalyticsWrite::FeatureEntry.phase` field; drain task never re-reads `SessionState` | SR-07 / NFR-03 | architecture/ADR-001-phase-snapshot-at-enqueue.md |
| `seq` monotonicity enforcement | Advisory `seq` via `COALESCE(MAX(seq), -1) + 1`; true ordering at query time uses `ORDER BY timestamp ASC, seq ASC` | SR-02 / NFR-04 | architecture/ADR-002-seq-advisory-timestamp-ordering.md |
| `CYCLE_EVENTS` write path | Direct write pool (`insert_cycle_event` method), not analytics drain — prevents silent queue shedding of audit-trail rows | ADR-003 | architecture/ADR-003-cycle-events-direct-write-pool.md |
| Phase narrative data model | `phase_narrative: Option<PhaseNarrative>` optional field on `RetrospectiveReport`; `#[serde(default, skip_serializing_if = "Option::is_none")]`; construction extracted to `phase_narrative.rs` pure function | ADR-004 | architecture/ADR-004-phase-narrative-report-type.md |
| `outcome` category retirement | Remove from `INITIAL_CATEGORIES` only (block new ingest); no data deletion; `outcome_tags.rs` retained; cleanup tracked in GH #338 | SR-03 / ADR-005 | architecture/ADR-005-outcome-category-retirement.md |
| Cross-cycle comparison scope | Explicitly in scope as FR-10; included in `PhaseNarrative`; resolves SR-05 scope/vision disagreement | SR-05 resolved | architecture/ARCHITECTURE.md |
| `current_phase` mutation ordering | Synchronous mutation inside UDS listener handler before any `spawn_blocking` DB write; never queued behind analytics drain | SR-01 / NFR-02 | architecture/ARCHITECTURE.md Component 5 |
| Behavioral corroboration | Explicitly out of scope; existing observation-pipeline rework signals are sufficient; no corroboration layer added | ALIGNMENT-REPORT.md WARN resolved | product/features/crt-025/SCOPE.md Non-Goals |

---

## Files to Create / Modify

### `unimatrix-store`

| File | Change |
|------|--------|
| `crates/unimatrix-store/src/analytics.rs` | Add `phase: Option<String>` field to `AnalyticsWrite::FeatureEntry` variant; update drain handler INSERT to write `phase` column |
| `crates/unimatrix-store/src/write_ext.rs` | Change `record_feature_entries` signature to `(feature_cycle: &str, entry_ids: &[u64], phase: Option<&str>)`; update INSERT |
| `crates/unimatrix-store/src/db.rs` | Add `SqlxStore::insert_cycle_event(...)` method; add `CYCLE_EVENTS` DDL and `feature_entries.phase` column to `create_tables_if_needed` |
| `crates/unimatrix-store/src/migration.rs` | Bump `CURRENT_SCHEMA_VERSION` to 15; add v14→v15 migration block |

### `unimatrix-server`

| File | Change |
|------|--------|
| `crates/unimatrix-server/src/infra/validation.rs` | Add `CycleType::PhaseEnd`; add `phase`, `outcome`, `next_phase` to `ValidatedCycleParams`; remove `keywords`; extend `validate_cycle_params` with phase format validation; add `CYCLE_PHASE_END_EVENT` constant |
| `crates/unimatrix-server/src/infra/session.rs` | Add `current_phase: Option<String>` to `SessionState`; add `SessionRegistry::set_current_phase` method |
| `crates/unimatrix-server/src/infra/categories.rs` | Remove `"outcome"` from `INITIAL_CATEGORIES` |
| `crates/unimatrix-server/src/mcp/tools.rs` | Replace `keywords` field with `phase`, `outcome`, `next_phase` on `CycleParams`; update `context_cycle` handler; update `context_cycle_review` handler with three new SQL queries and phase narrative assembly |
| `crates/unimatrix-server/src/uds/hook.rs` | Extract `phase`, `outcome`, `next_phase` from `tool_input`; handle `phase-end` mapping to `CYCLE_PHASE_END_EVENT`; remove keywords extraction |
| `crates/unimatrix-server/src/uds/listener.rs` | Handle `cycle_phase_end` and `cycle_stop` in dispatch; synchronous `set_current_phase` call before `spawn_blocking` DB write |
| `crates/unimatrix-server/src/services/usage.rs` | Add `current_phase: Option<String>` to `UsageContext`; propagate to `record_feature_entries` call site |
| `crates/unimatrix-server/src/server.rs` | Snapshot `session_state.current_phase` at call time in `context_store` handler; pass to both write paths |
| `crates/unimatrix-server/src/format.rs` | Update formatting for `context_cycle_review` response to render phase narrative section |

### `unimatrix-observe`

| File | Change |
|------|--------|
| `crates/unimatrix-observe/src/types.rs` | Add `CycleEventRecord`, `PhaseNarrative`, `PhaseCategoryComparison` types; add `phase_narrative` field to `RetrospectiveReport` |
| `crates/unimatrix-observe/src/phase_narrative.rs` | New file: `build_phase_narrative(events, current_dist, cross_dist) -> PhaseNarrative` pure function |
| `crates/unimatrix-observe/src/lib.rs` | Declare `phase_narrative` module |

---

## Data Structures

### `CycleParams` (updated MCP wire schema, `mcp/tools.rs`)

```rust
pub struct CycleParams {
    pub r#type:     String,
    pub topic:      String,
    pub phase:      Option<String>,
    pub outcome:    Option<String>,
    pub next_phase: Option<String>,
    pub agent_id:   Option<String>,
    pub format:     Option<String>,
    // keywords removed; unknown fields silently discarded (no deny_unknown_fields)
}
```

### `ValidatedCycleParams` (updated, `infra/validation.rs`)

```rust
pub struct ValidatedCycleParams {
    pub cycle_type: CycleType,
    pub topic:      String,
    pub phase:      Option<String>,   // normalized: lowercase, trimmed
    pub outcome:    Option<String>,   // max 512 chars
    pub next_phase: Option<String>,   // normalized: lowercase, trimmed
    // keywords removed
}

pub enum CycleType { Start, PhaseEnd, Stop }
```

### `CYCLE_EVENTS` table (new, schema v15)

```sql
CREATE TABLE IF NOT EXISTS cycle_events (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    cycle_id   TEXT    NOT NULL,
    seq        INTEGER NOT NULL,
    event_type TEXT    NOT NULL,
    phase      TEXT,
    outcome    TEXT,
    next_phase TEXT,
    timestamp  INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_cycle_events_cycle_id ON cycle_events (cycle_id);
```

### `feature_entries.phase` column (added, schema v15)

```sql
ALTER TABLE feature_entries ADD COLUMN phase TEXT;
```

### `AnalyticsWrite::FeatureEntry` variant (updated, `analytics.rs`)

```rust
AnalyticsWrite::FeatureEntry {
    feature_id: String,
    entry_id:   u64,
    phase:      Option<String>,  // new; snapshot at enqueue time
}
```

### `SessionState` (updated, `infra/session.rs`)

Gains `current_phase: Option<String>` field, initialized `None` on session registration.

### Phase narrative types (new, `unimatrix-observe/types.rs`)

```rust
pub struct CycleEventRecord {
    pub seq:        i64,
    pub event_type: String,
    pub phase:      Option<String>,
    pub outcome:    Option<String>,
    pub next_phase: Option<String>,
    pub timestamp:  i64,
}

pub struct PhaseNarrative {
    pub phase_sequence:        Vec<String>,
    pub rework_phases:         Vec<String>,
    pub per_phase_categories:  HashMap<String, HashMap<String, u64>>,
    pub cross_cycle_comparison: Option<Vec<PhaseCategoryComparison>>,
}

pub struct PhaseCategoryComparison {
    pub phase:              String,
    pub category:           String,
    pub this_feature_count: u64,
    pub cross_cycle_mean:   f64,
    pub sample_features:    usize,
}
```

---

## Function Signatures

```rust
// infra/validation.rs
pub fn validate_cycle_params(
    type_str:   &str,
    topic:      &str,
    phase:      Option<&str>,
    outcome:    Option<&str>,
    next_phase: Option<&str>,
) -> Result<ValidatedCycleParams, String>;

// infra/session.rs
impl SessionRegistry {
    pub fn set_current_phase(&self, session_id: &str, phase: Option<String>);
}

// unimatrix-store/write_ext.rs
pub async fn record_feature_entries(
    feature_cycle: &str,
    entry_ids:     &[u64],
    phase:         Option<&str>,
) -> Result<()>;

// unimatrix-store/db.rs
impl SqlxStore {
    pub async fn insert_cycle_event(
        &self,
        cycle_id:   &str,
        seq:        i64,
        event_type: &str,
        phase:      Option<&str>,
        outcome:    Option<&str>,
        next_phase: Option<&str>,
        timestamp:  i64,
    ) -> Result<()>;
}

// unimatrix-observe/phase_narrative.rs
pub fn build_phase_narrative(
    events:       &[CycleEventRecord],
    current_dist: &PhaseCategoryDist,
    cross_dist:   &PhaseCategoryDist,
) -> PhaseNarrative;
```

### Three new SQL queries in `context_cycle_review` handler

```sql
-- 1. Cycle event log
SELECT seq, event_type, phase, outcome, next_phase, timestamp
  FROM cycle_events
 WHERE cycle_id = ?
 ORDER BY timestamp ASC, seq ASC;

-- 2. Current feature phase/category distribution
SELECT fe.phase, e.category, COUNT(*) AS cnt
  FROM feature_entries fe
  JOIN entries e ON e.id = fe.entry_id
 WHERE fe.feature_id = ? AND fe.phase IS NOT NULL
 GROUP BY fe.phase, e.category;

-- 3. Cross-feature baseline (excludes current feature)
SELECT fe.phase, e.category, COUNT(*) AS cnt
  FROM feature_entries fe
  JOIN entries e ON e.id = fe.entry_id
 WHERE fe.feature_id IN (
       SELECT DISTINCT feature_id FROM feature_entries WHERE phase IS NOT NULL
   )
   AND fe.feature_id != ?
   AND fe.phase IS NOT NULL
 GROUP BY fe.phase, e.category;
```

---

## Constraints

| ID | Constraint |
|----|-----------|
| C-01 | Phase string: no spaces, lowercase after normalization, max 64 chars. Hard GNN label requirement. |
| C-02 | `validate_cycle_params` returns `Result<ValidatedCycleParams, String>` — not `ServerError`. Hook path cannot use `ServerError`. |
| C-03 | `ImplantEvent` wire protocol unchanged. New fields travel as payload map keys. No struct changes in `unimatrix-engine`. |
| C-04 | `sessions.keywords` column left in place; stop populating. Removal deferred (more invasive migration). |
| C-05 | No backfill of existing `feature_entries` rows. Pre-existing rows get `phase = NULL`. |
| C-06 | No changes to `context_store` wire protocol. Phase tagging is automatic from `SessionState`. |
| C-07 | No changes to `context_cycle_review` behavioral telemetry pipeline (observation metrics, hotspot detection, baseline comparison). |
| C-08 | Schema migration must use `pragma_table_info` pre-check before `ALTER TABLE ADD COLUMN` on `feature_entries`. |
| C-09 | `seq` computed as `COALESCE(MAX(seq), -1) + 1` scoped to `cycle_id`. Advisory only; query ordering uses `(timestamp ASC, seq ASC)`. |
| C-10 | Hook latency budget: 40ms total transport timeout. `CYCLE_EVENTS` INSERT is fire-and-forget. |
| C-11 | `CategoryAllowlist` removal of `"outcome"` does not delete existing entries. Only new ingest is blocked. |
| C-12 | `AnalyticsWrite::FeatureEntry` is `#[non_exhaustive]`. All internal match arms must destructure `phase` explicitly — no `..` shortcut. |

---

## Dependencies

### Upstream (complete)

| Dependency | Status |
|------------|--------|
| WA-0 (`crt-024`) | Complete — PR #336 |
| col-023 (W1-5) | Complete — PR #332 |

### Downstream (blocked on this feature)

| Feature | Dependency |
|---------|-----------|
| WA-2 | Consumes `SessionState.current_phase` for phase-conditioned category affinity boosting |
| W3-1 | Consumes `FEATURE_ENTRIES.phase` as supervised GNN training labels |
| WA-4 | Phase-conditioned proactive injection uses `SessionState.current_phase` for cache rebuild at phase transitions |

### Crates

| Crate | Reason |
|-------|--------|
| `unimatrix-store` | Schema migration, `CYCLE_EVENTS` write, `feature_entries.phase` write, `AnalyticsWrite::FeatureEntry` variant update |
| `unimatrix-server` | `CycleParams`, `ValidatedCycleParams`, `CycleType`, `CategoryAllowlist`, `SessionState`, UDS listener, `context_cycle_review` handler |
| `unimatrix-observe` | `RetrospectiveReport` extension, new `PhaseNarrative` types, new `phase_narrative.rs` module |
| `rusqlite 0.34 (bundled)` | Schema v15 migration |

---

## NOT in Scope

- `context_store` wire protocol changes — phase comes from `SessionState`, not from the caller.
- WA-2 category histogram boosting — separate feature dependent on this one.
- Semantic interpretation of phase strings — engine stores opaque labels; protocol enforces consistency.
- Backfill of `feature_entries.phase` for pre-WA-1 rows — `NULL` is correct historical data.
- Removal of `sessions.keywords` column — left in place; stop populating.
- Removal of `outcome_tags.rs` file — retained; removal tracked in GH #338.
- Deletion of existing `outcome`-category entries — only new ingest is blocked.
- Changes to behavioral telemetry pipeline (`SqlObservationSource`, detection rules, hotspot pipeline, baseline comparison).
- Behavioral corroboration (cross-referencing edit-pattern rework with explicit `CYCLE_EVENTS` rework signal) — existing observation-pipeline rework signals are sufficient as an independent narrative; no corroboration layer added.
- W3-1 GNN implementation — this feature only produces the training data.
- `ImplantEvent` struct changes in `unimatrix-engine` — new fields travel through existing payload map.

---

## Alignment Status

Source: product/features/crt-025/ALIGNMENT-REPORT.md (reviewed 2026-03-22)

| Check | Status |
|-------|--------|
| Vision Alignment | PASS (WARN resolved) |
| Milestone Fit | PASS |
| Scope Gaps | PASS (WARN resolved) |
| Scope Additions | PASS |
| Architecture Consistency | PASS |
| Risk Completeness | PASS |

### Resolved WARNs

**WARN 1 — "Behavioral corroboration" not in scope**: PRODUCT-VISION.md WA-1 listed "Behavioral corroboration: edit-pattern rework signal cross-referenced with explicit phase rework" as a `context_cycle_review` enrichment. This is explicitly excluded from scope: existing observation-pipeline rework signals are sufficient as an independent narrative; no corroboration layer is added. SCOPE.md Non-Goals states "No changes to `context_cycle_review` behavioral telemetry pipeline." Vision bullet is accepted as deferred.

**WARN 2 — `outcome` field max length unresolved in spec**: RISK-TEST-STRATEGY.md flagged no length limit on the `outcome` free-form field. Resolved: SPECIFICATION.md FR-02.6 adds a 512-character maximum for `outcome` values, consistent with the pattern of other free-form fields.

### Minor document inconsistency (non-blocking)

RISK-TEST-STRATEGY.md Coverage Summary states "6 High-priority risks" but the Risk Register enumerates 8 items at High priority (R-03, R-04, R-05, R-06, R-08, R-10, R-11, R-14). All 14 risks have defined test scenarios regardless of the count discrepancy.

---

## Canonical Phase Vocabulary

The engine stores phase strings as opaque labels but GNN training requires consistent discrete class labels. All Unimatrix protocols MUST use these exact tokens:

| Token | When Used |
|-------|-----------|
| `scope` | Scope definition, problem statement, risk assessment |
| `design` | Architecture, pseudocode, specification authoring |
| `implementation` | Code writing and wiring |
| `testing` | Test authoring, test runs, coverage review |
| `gate-review` | Gate passage or rejection, PR review |

The engine normalizes to lowercase and rejects strings containing spaces or exceeding 64 characters, but does not enforce vocabulary membership.
