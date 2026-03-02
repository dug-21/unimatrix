# col-010 Implementation Brief: Session Lifecycle Persistence & Structured Retrospective

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/col-010/SCOPE.md |
| Architecture | product/features/col-010/architecture/ARCHITECTURE.md |
| Specification | product/features/col-010/specification/SPECIFICATION.md |
| Risk Strategy | product/features/col-010/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/col-010/ALIGNMENT-REPORT.md |

## ADR Files

| ADR | File |
|-----|------|
| ADR-001: Abandoned Session as Distinct Status Variant | product/features/col-010/architecture/ADR-001-abandoned-session-status-variant.md |
| ADR-002: INJECTION_LOG GC Cascade in gc_sessions | product/features/col-010/architecture/ADR-002-injection-log-gc-cascade.md |
| ADR-003: Batch INJECTION_LOG Writes Per ContextSearch Response | product/features/col-010/architecture/ADR-003-batch-injection-log-writes.md |
| ADR-004: Lesson-Learned Entry Embedding via Fire-and-Forget | product/features/col-010/architecture/ADR-004-lesson-learned-fire-and-forget-embedding.md |
| ADR-005: Provenance Boost as Query-Time Constant | product/features/col-010/architecture/ADR-005-provenance-boost-query-time-constant.md |
| ADR-006: P0/P1 Component Split | product/features/col-010/architecture/ADR-006-p0-p1-component-split.md |

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| storage-layer | product/features/col-010/pseudocode/storage-layer.md | product/features/col-010/test-plan/storage-layer.md |
| uds-listener | product/features/col-010/pseudocode/uds-listener.md | product/features/col-010/test-plan/uds-listener.md |
| session-gc | product/features/col-010/pseudocode/session-gc.md | product/features/col-010/test-plan/session-gc.md |
| auto-outcomes | product/features/col-010/pseudocode/auto-outcomes.md | product/features/col-010/test-plan/auto-outcomes.md |
| structured-retrospective | product/features/col-010/pseudocode/structured-retrospective.md | product/features/col-010/test-plan/structured-retrospective.md |
| tiered-output | product/features/col-010/pseudocode/tiered-output.md | product/features/col-010/test-plan/tiered-output.md |
| lesson-learned | product/features/col-010/pseudocode/lesson-learned.md | product/features/col-010/test-plan/lesson-learned.md |

### Cross-Cutting Artifacts

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | product/features/col-010/pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | product/features/col-010/test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

---

## Goal

col-010 makes session state durable by adding two new redb tables — SESSIONS (16th) and INJECTION_LOG (17th) — that persistently record the lifecycle and injection events of every agent session. It upgrades the col-002 retrospective pipeline with a structured data entry point that reads directly from the store instead of JSONL files, adds an `evidence_limit` parameter to cap per-hotspot evidence arrays and reduce the ~87KB default payload, and auto-generates session outcome entries and `lesson-learned` knowledge entries from retrospective findings.

---

## P0 / P1 Delivery Split (ADR-006)

This is the most important structural constraint for implementation. ADR-006 makes the split explicit.

### P0 — Required Before col-011 (AC-01 through AC-11 + AC-24)

| # | Component | ACs Covered |
|---|-----------|-------------|
| 1 | Storage Layer: SESSIONS + INJECTION_LOG tables, schema v5 migration | AC-01, AC-06, AC-07 |
| 2 | UDS Listener Integration: SessionRegister, SessionClose, ContextSearch writes | AC-02, AC-03, AC-04, AC-05, AC-06 |
| 3 | Session GC with INJECTION_LOG cascade | AC-08, AC-09 |
| 4 | Auto-Generated Session Outcomes (col-001 integration) | AC-10, AC-11 |
| 5 | Structured Retrospective (`from_structured_events()`) | AC-12, AC-13, AC-14 |

P0 ships independently. If the timeline is constrained, P0 merges first, col-011 gates on P0 ACs only, and P1 follows as a continuation (col-010b or same PR after P0 stabilizes).

### P1 — Independent (Resolves Issue #65, AC-12 through AC-23)

| # | Component | ACs Covered |
|---|-----------|-------------|
| 6 | Evidence-Limited Retrospective Output + Evidence Synthesis | AC-15, AC-16, AC-17, AC-18, AC-19 |
| 7 | Lesson-Learned Auto-Persistence + Provenance Boost | AC-20, AC-21, AC-22, AC-23 |

P1 begins only after P0 acceptance criteria pass. P1 has no schema migration (application logic changes only). P1 is col-011 independent.

**BLOCKING gate for P1**: Before implementing the evidence_limit change (Component 6), audit all existing integration tests for `context_retrospective` that assert on exact `hotspots[].evidence` array lengths. Update those tests to either pass `evidence_limit = 0` (restore full arrays) or update their expected count to ≤ 3 items. This is R-09, the highest-likelihood regression in the feature (FR-10.8). Do not start P1 Component 6 without completing this audit.

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|------------|--------|----------|
| Abandoned session status encoding | Add `Abandoned` as a distinct 4th `SessionLifecycleStatus` variant. Abandoned sessions are excluded from `from_structured_events()` metrics; no auto-outcome written. | SR-06 | architecture/ADR-001-abandoned-session-status-variant.md |
| INJECTION_LOG GC cascade | `gc_sessions()` deletes orphaned INJECTION_LOG records in the same 5-phase `WriteTransaction` as SESSIONS deletion. Returns `GcStats` with `log_entries_deleted` count. | SR-04 | architecture/ADR-002-injection-log-gc-cascade.md |
| Batch INJECTION_LOG writes | `insert_injection_log_batch()` is the only public write API — allocates a contiguous ID range and writes all records in one transaction. One transaction per ContextSearch response (not per entry). | SR-12 | architecture/ADR-003-batch-injection-log-writes.md |
| Lesson-learned ONNX embedding | Fire-and-forget via `tokio::spawn`. `context_retrospective` returns before embedding completes. On embed failure: entry written with `embedding_dim = 0`, logged at `warn`. | SR-07 | architecture/ADR-004-lesson-learned-fire-and-forget-embedding.md |
| Provenance boost for lesson-learned | `PROVENANCE_BOOST = 0.02` as named constant in `confidence.rs`, applied at query time in both `uds_listener.rs` and `tools.rs` search re-ranking. Does not touch stored confidence or the 0.92 invariant. | SR-03 | architecture/ADR-005-provenance-boost-query-time-constant.md |
| P0/P1 component sequencing | 4 P0 + 1 P1-adjacent (structured retro) components gate col-011. 2 P1 components resolve issue #65 independently. P1 cannot merge before P0 ACs pass. | SR-02 | architecture/ADR-006-p0-p1-component-split.md |
| `total_injections` source of truth | Use in-memory `signal_output.injection_count` at SessionClose. Accept the accepted discrepancy that fire-and-forget INJECTION_LOG writes may still be in-flight. Document in tests (OQ-01). | OQ-01 | SPECIFICATION.md §OQ-01 |
| JSONL fallback trigger | Use JSONL fallback only when `scan_sessions_by_feature()` returns an empty list AND the JSONL observation directory has files for the feature_cycle. Structured path is authoritative post-deployment. | OQ-03 | SPECIFICATION.md §OQ-03 |
| Auto-outcome entries skip embedding | `embedding_dim = 0` on auto-outcome entries. SessionClose must not block on ONNX. Entries queryable via `context_lookup` by tag/category only. | SCOPE.md §Proposed Approach | SPECIFICATION.md FR-08 |
| `trust_source = "system"` on all hook-written entries | All auto-generated entries (auto-outcomes, lesson-learned) set `trust_source = "system"` for correct 0.7 trust score. Correctness fix, not a boost mechanism. | SR-13 | SPECIFICATION.md SEC-03 |
| Supersede race condition tolerated | Concurrent `context_retrospective` calls for the same feature_cycle may briefly produce two active lesson-learned entries. Accepted known limitation — next retrospective call reduces to one. | SR-09 | SPECIFICATION.md FR-11.6 |

---

## Files to Create

| File | Summary |
|------|---------|
| `crates/unimatrix-store/src/sessions.rs` | `SessionRecord`, `SessionLifecycleStatus` (4 variants), all store ops: `insert_session`, `update_session`, `get_session`, `scan_sessions_by_feature`, `scan_sessions_by_feature_with_status`, `gc_sessions`. GC constants: `TIMED_OUT_THRESHOLD_SECS`, `DELETE_THRESHOLD_SECS`. |
| `crates/unimatrix-store/src/injection_log.rs` | `InjectionLogRecord`, `insert_injection_log_batch` (only public write API), `scan_injection_log_by_session`. |
| `crates/unimatrix-observe/src/structured.rs` | `from_structured_events(store, feature_cycle)` — reads SESSIONS + INJECTION_LOG, excludes Abandoned/TimedOut sessions, runs existing metrics + hotspot pipeline, synthesizes Layer 2 narratives. |

## Files to Modify

| File | Change Summary |
|------|---------------|
| `crates/unimatrix-store/src/schema.rs` | Add `SESSIONS: TableDefinition<&str, &[u8]>` and `INJECTION_LOG: TableDefinition<u64, &[u8]>` constants. Bump `CURRENT_SCHEMA_VERSION` to 5. |
| `crates/unimatrix-store/src/migration.rs` | Add `migrate_v4_to_v5()` implementing check-then-write for `next_log_id`. Chain in `migrate_if_needed()`. |
| `crates/unimatrix-store/src/lib.rs` | Re-export `sessions` and `injection_log` modules. |
| `crates/unimatrix-server/src/uds_listener.rs` | Add persistent writes on `SessionRegister` (insert_session), `SessionClose` (update_session + auto-outcome), `ContextSearch` (insert_injection_log_batch). All via `spawn_blocking`. Add `session_id` input sanitization (`[a-zA-Z0-9-_]`, max 128 chars). |
| `crates/unimatrix-server/src/tools.rs` | Add `gc_sessions()` call in `maintain=true` path. Add `context_retrospective` path selection (structured first, JSONL fallback). Add `evidence_limit` parameter + server-side evidence truncation. Add lesson-learned auto-persist fire-and-forget (P1). |
| `crates/unimatrix-server/src/outcome_tags.rs` | Add `"session"` to `VALID_TYPES`. |
| `crates/unimatrix-observe/src/types.rs` | Add `HotspotNarrative`, `EvidenceCluster`, `Recommendation` types (additive). Add `narratives: Option<Vec<HotspotNarrative>>` to `RetrospectiveReport` with `#[serde(default, skip_serializing_if)]`. Extend `ObservationRecord` with `confidence_at_injection: Option<f64>` and `session_outcome: Option<String>` (`#[serde(default)]`). |
| `crates/unimatrix-observe/src/report.rs` | Add `recommendations_for_hotspots()` covering 4 hotspot types. |
| `crates/unimatrix-engine/src/confidence.rs` | Add `pub const PROVENANCE_BOOST: f64 = 0.02`. |
| `crates/unimatrix-server/src/wire.rs` | Add `evidence_limit: Option<usize>` to `context_retrospective` request type. |

---

## Data Structures

### SessionRecord (new, `sessions.rs`)

```rust
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SessionRecord {
    pub session_id: String,
    pub feature_cycle: Option<String>,
    pub agent_role: Option<String>,
    pub started_at: u64,               // unix epoch seconds
    pub ended_at: Option<u64>,
    pub status: SessionLifecycleStatus,
    pub compaction_count: u32,
    pub outcome: Option<String>,       // "success" | "rework" | "abandoned"
    pub total_injections: u32,         // in-memory count at SessionClose (OQ-01)
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum SessionLifecycleStatus {
    Active,
    Completed,   // Success or Rework outcome
    TimedOut,    // GC marked; was Active > 24h
    Abandoned,   // ADR-001: distinct variant for precise retrospective filtering
}

pub struct GcStats {
    pub timed_out_count: u32,
    pub deleted_session_count: u32,
    pub deleted_injection_log_count: u32,
}

pub const TIMED_OUT_THRESHOLD_SECS: u64 = 24 * 3600;
pub const DELETE_THRESHOLD_SECS: u64 = 30 * 24 * 3600;
```

### InjectionLogRecord (new, `injection_log.rs`)

```rust
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct InjectionLogRecord {
    pub log_id: u64,          // monotonic, allocated by insert_injection_log_batch
    pub session_id: String,
    pub entry_id: u64,
    pub confidence: f64,      // reranked score at injection time
    pub timestamp: u64,       // unix epoch seconds
}
```

### RetrospectiveReport (additive change, `types.rs`)

```rust
pub struct RetrospectiveReport {
    // Layer 1: always populated (unchanged from pre-col-010)
    pub feature_cycle: String,
    pub session_count: usize,
    pub total_records: usize,
    pub metrics: MetricVector,
    pub hotspots: Vec<HotspotFinding>,          // unchanged type; evidence truncated server-side by evidence_limit
    pub recommendations: Vec<Recommendation>,   // new in col-010
    pub is_cached: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline_comparison: Option<Vec<BaselineComparison>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entries_analysis: Option<Vec<EntryAnalysis>>,

    // Layer 2: structured-events path only (additive)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub narratives: Option<Vec<HotspotNarrative>>,
}

pub struct HotspotNarrative {
    pub hotspot_type: String,
    pub summary: String,                   // non-empty, human-readable
    pub clusters: Vec<EvidenceCluster>,
    pub top_files: Vec<(String, u32)>,     // top-5 by mutation count
    pub sequence_pattern: Option<String>,  // e.g. "30s->60s->90s->120s"
}

pub struct EvidenceCluster {
    pub window_start: u64,
    pub event_count: u32,
    pub description: String,
}

pub struct Recommendation {
    pub hotspot_type: String,
    pub action: String,
    pub rationale: String,
}
```

---

## Function Signatures

### Store layer (`sessions.rs`, `injection_log.rs`)

```rust
// Sessions
impl Store {
    pub fn insert_session(&self, record: &SessionRecord) -> Result<()>;
    pub fn update_session(&self, session_id: &str, updater: impl FnOnce(&mut SessionRecord)) -> Result<()>;
    pub fn get_session(&self, session_id: &str) -> Result<Option<SessionRecord>>;
    pub fn scan_sessions_by_feature(&self, feature_cycle: &str) -> Result<Vec<SessionRecord>>;
    pub fn scan_sessions_by_feature_with_status(
        &self,
        feature_cycle: &str,
        status_filter: Option<SessionLifecycleStatus>,
    ) -> Result<Vec<SessionRecord>>;
    pub fn gc_sessions(
        &self,
        timed_out_threshold_secs: u64,
        delete_threshold_secs: u64,
    ) -> Result<GcStats>;
}

// Injection log
impl Store {
    pub fn insert_injection_log_batch(&self, records: &[InjectionLogRecord]) -> Result<()>;
    pub fn scan_injection_log_by_session(&self, session_id: &str) -> Result<Vec<InjectionLogRecord>>;
}
```

### Migration (`migration.rs`)

```rust
fn migrate_v4_to_v5(txn: &redb::WriteTransaction) -> Result<()>;
// Idempotency: opens tables (noop if exist), writes next_log_id=0 only if key is absent
```

### Structured retrospective (`structured.rs`)

```rust
pub fn from_structured_events(
    store: &Store,
    feature_cycle: &str,
) -> Result<RetrospectiveReport, ObserveError>;
// Excludes Abandoned and TimedOut sessions from metric computation.
// Returns RetrospectiveReport::empty(feature_cycle) if no sessions found.

pub const CLUSTER_WINDOW_SECS: u64 = 30;  // named constant for future tuning
```

### Recommendation templates (`report.rs`)

```rust
fn recommendations_for_hotspots(hotspots: &[HotspotFinding]) -> Vec<Recommendation>;
// Covers: permission_retries, coordinator_respawns, sleep_workarounds,
//         compile_cycles (only if measured > 10.0). Others: no recommendation.
```

### Provenance boost (`confidence.rs`)

```rust
pub const PROVENANCE_BOOST: f64 = 0.02;
// Applied at both uds_listener.rs and tools.rs search re-ranking:
// final_score = 0.85*sim + 0.15*conf + co_access_affinity + provenance_boost
// where provenance_boost = PROVENANCE_BOOST if category == "lesson-learned" else 0.0
```

---

## Constraints

### Hard Constraints

- **Schema migration pattern**: follow the established 3-step process (schema.rs constant bump + `migrate_v4_to_v5()` + `migrate_if_needed()` chain). v4→v5 is table-creation-only — no entry scan-and-rewrite.
- **Migration idempotency**: write `next_log_id = 0` only if the key does not already exist in COUNTERS (check-then-write per R-01).
- **Batch injection writes**: `insert_injection_log_batch` is the only public write API for INJECTION_LOG. No single-record insert. One transaction per ContextSearch response.
- **No embedding for auto-outcomes**: auto-outcome entries have `embedding_dim = 0`. SessionClose must not block on ONNX.
- **Fire-and-forget for lesson-learned embedding**: `context_retrospective` returns report before embedding completes.
- **`session_id` sanitization**: enforce `[a-zA-Z0-9-_]`, max 128 chars before any SESSIONS write. Reject invalid session_ids with error response.
- **Abandoned/TimedOut filter in `from_structured_events()`**: exclude both from metric computation.
- **P0 before P1**: P1 implementation begins only after P0 ACs (AC-01 through AC-11 + AC-24) pass.
- **R-09 test audit before P1 Component 6**: audit existing `context_retrospective` tests for `hotspots[].evidence` assertions before adding tiered output.
- **Backward compatibility**: `build_report()` JSONL path remains unchanged. `evidence_limit = 0` output is structurally identical to pre-col-010 output.
- **Zero regression**: all existing tests pass (AC-24). `RetrospectiveReport` struct gains only the additive `narratives` field; `hotspots` type is unchanged. Tests asserting exact evidence array lengths must be updated to use `evidence_limit = 0` or expect ≤ 3 items.
- **Edition 2024, MSRV 1.89**: workspace constraints inherited.
- **`trust_source = "system"` on all hook-generated entries**: both auto-outcomes and lesson-learned.

### Soft Constraints

- **GC thresholds are named constants** (`TIMED_OUT_THRESHOLD_SECS`, `DELETE_THRESHOLD_SECS`): not user-configurable in v1.
- **INJECTION_LOG full-scan acceptable**: at <5,000 records/day volume, full scan + in-process filter in `scan_injection_log_by_session` is acceptable. Secondary index deferred.
- **SessionClose P99 latency**: soft budget of <200ms. All new writes are fire-and-forget.
- **CLUSTER_WINDOW_SECS = 30**: named constant, tunable in a follow-on.

---

## Dependencies

### Crate / Feature Dependencies

| Dependency | Type | Needed For |
|------------|------|-----------|
| col-009 (hard, must be merged first) | Feature | `drain_and_signal_session()`, `SignalOutput.final_outcome`, schema v4 SIGNAL_QUEUE |
| col-008 (hard, complete) | Feature | `SessionState.compaction_count` at session end |
| col-007 (hard, complete) | Feature | `record_injection()` call site in `uds_listener.rs` for INJECTION_LOG writes |
| col-001 (existing) | Feature | OUTCOME_INDEX table for auto-outcome `feature_cycle` indexing |
| col-002 (existing) | Feature | `RetrospectiveReport`, `ObservationRecord`, metric/hotspot pipeline |
| redb 3.1.x | Crate | SESSIONS and INJECTION_LOG table operations |
| bincode v2 serde | Crate | `SessionRecord` and `InjectionLogRecord` serialization |
| tokio | Crate | `spawn_blocking` for all new store writes; `tokio::spawn` for lesson-learned fire-and-forget |

### Downstream Consumers

| Feature | Dependency |
|---------|-----------|
| col-011 | SESSIONS table (`scan_sessions_by_feature()`) for routing quality scoring; INJECTION_LOG for per-entry performance history |

---

## NOT in Scope

- `session_id: Option<String>` field on `EntryRecord` — bincode positional encoding requires scan-and-rewrite. Deferred. **PRODUCT-VISION.md must be corrected to remove this reference (VARIANCE-01 — required pre-implementation action).**
- Replacing the JSONL parser — `from_structured_events()` is additive; JSONL is preserved as fallback.
- SubagentStart/SubagentStop persistent tracking — in-memory only, not written to SESSIONS.
- Cross-session dashboards or agent routing — col-011.
- INJECTION_LOG secondary indexes — full scan acceptable at current volumes.
- Sophisticated narrative ML — synthesis is deterministic heuristics only.
- `helpful_count` seeding on auto-written entries.
- Category-specific `MINIMUM_SAMPLE_SIZE` reduction for lesson-learned.

---

## Security Notes

- **SR-SEC-02 (open gap)**: The specification sanitizes `session_id` (SEC-01) but `agent_role` and `feature_cycle` are interpolated into auto-outcome content without explicit sanitization. The implementer must resolve: either apply the same `[a-zA-Z0-9-_]` sanitization to these fields at the `SessionRegister` write point, or explicitly omit them from auto-outcome content and use `entry.source = "hook"` alone for attribution. Do not leave this unresolved. See ALIGNMENT-REPORT.md pre-implementation actions #2.

---

## Alignment Status

**Overall**: 1 VARIANCE (VARIANCE-01), 1 WARN, 5 PASS. Feature is well-aligned with M5 vision goals.

**VARIANCE-01 (Required pre-implementation action)**: `product/PRODUCT-VISION.md` col-010 row references `session_id: Option<String>` on `EntryRecord`. This is explicitly a Non-Goal in SCOPE.md and all source documents. PRODUCT-VISION.md must be corrected before implementation begins to prevent downstream agent confusion. Correction: remove the `session_id` field reference; replace with "Adds SESSIONS table (16th) and INJECTION_LOG table (17th). No `session_id` field on EntryRecord — deferred."

**WARN (Scope additions, accepted)**: Architecture adds `Abandoned` as a distinct 4th `SessionLifecycleStatus` variant and extends the retrospective filter to also exclude `TimedOut` sessions (not in SCOPE.md, but well-justified by SR-06). Also adds `scan_sessions_by_feature_with_status()` filter variant. All additions are conservative scope expansions with clear correctness rationale.

**Pre-implementation checklist (from ALIGNMENT-REPORT.md)**:
1. [REQUIRED] Update PRODUCT-VISION.md to remove `session_id` reference (VARIANCE-01).
2. [RECOMMENDED] Resolve `agent_role`/`feature_cycle` sanitization gap (SR-SEC-02).
3. [GATE] Confirm col-009 PR is merged and all col-009 ACs pass before beginning implementation.

**Open Questions Resolved by Synthesis**:
- OQ-01: `total_injections` uses in-memory `signal_output.injection_count` at SessionClose. Accepted discrepancy documented in tests.
- OQ-03: JSONL fallback triggers only when `scan_sessions_by_feature()` returns empty AND JSONL directory has files for the feature_cycle.

**Known Limitations**:
- SR-09: Concurrent `context_retrospective` calls for the same feature_cycle may briefly produce two active lesson-learned entries. Tolerated edge case.
- R-03: `total_injections` may under-count if in-flight INJECTION_LOG batch writes fail. Accepted discrepancy.
