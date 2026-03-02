# col-010 Architecture: Session Lifecycle Persistence & Structured Retrospective

**Schema version**: v4 → v5
**Feature cycle**: col-010
**Architect**: col-010-agent-1-architect
**Date**: 2026-03-02

---

## Overview

col-010 makes session state durable by adding two new redb tables (SESSIONS, INJECTION_LOG) and upgrading the col-002 retrospective pipeline with a structured data path. The feature is internally split into **P0** (required for col-011) and **P1** (retrospective quality improvements resolving issue #65).

### P0 Components (col-011 blocking)

1. **Storage Layer** — SESSIONS + INJECTION_LOG tables, schema v5 migration
2. **UDS Listener Integration** — persistent writes on SessionRegister, SessionClose, ContextSearch
3. **Session GC** — TimeOut/delete in `maintain=true` sweep with INJECTION_LOG cascade
4. **Auto-Generated Session Outcomes** — col-001 integration, no embedding

### P1 Components (issue #65, independent)

5. **Structured Retrospective** — `from_structured_events()` in `unimatrix-observe`
6. **Tiered Retrospective Output** — Layer 1/2/3, `detail_level` parameter
7. **Lesson-Learned Auto-Persistence + Provenance Boost** — fire-and-forget embedding, query-time constant

P1 can ship independently without breaking the dependency graph for col-011.

---

## 1. Storage Layer (schema v5)

### 1.1 New Tables

Two new `TableDefinition` constants added to `crates/unimatrix-store/src/schema.rs`:

```rust
/// Session lifecycle records: session_id -> bincode bytes.
/// Added in schema v5 (col-010).
pub const SESSIONS: TableDefinition<&str, &[u8]> = TableDefinition::new("sessions");

/// Injection event log: log_id (monotonic u64) -> bincode bytes.
/// Added in schema v5 (col-010).
pub const INJECTION_LOG: TableDefinition<u64, &[u8]> = TableDefinition::new("injection_log");
```

### 1.2 SessionRecord

New file: `crates/unimatrix-store/src/sessions.rs`

```rust
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SessionRecord {
    pub session_id: String,
    pub feature_cycle: Option<String>,
    pub agent_role: Option<String>,
    pub started_at: u64,              // unix epoch seconds
    pub ended_at: Option<u64>,        // set on SessionClose
    pub status: SessionLifecycleStatus,
    pub compaction_count: u32,        // from SessionState at close
    pub outcome: Option<String>,      // "success" | "rework" | "abandoned"
    pub total_injections: u32,        // count at session end
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum SessionLifecycleStatus {
    Active,
    Completed,
    TimedOut,
    Abandoned,    // ADR-001: distinct variant for precise filtering
}
```

**Store operations** (all synchronous; callers use `spawn_blocking`):

- `insert_session(record: &SessionRecord) -> Result<()>` — write to SESSIONS
- `update_session(session_id: &str, updater: impl FnOnce(&mut SessionRecord)) -> Result<()>` — read-modify-write
- `get_session(session_id: &str) -> Result<Option<SessionRecord>>`
- `scan_sessions_by_feature(feature_cycle: &str) -> Result<Vec<SessionRecord>>`
- `gc_sessions(timed_out_threshold_secs: u64, delete_threshold_secs: u64) -> Result<GcStats>` — see §3

**GC constants** (named, not user-configurable in v1):

```rust
pub const TIMED_OUT_THRESHOLD_SECS: u64 = 24 * 3600;      // 24 hours
pub const DELETE_THRESHOLD_SECS: u64 = 30 * 24 * 3600;    // 30 days
```

### 1.3 InjectionLogRecord

New file: `crates/unimatrix-store/src/injection_log.rs`

```rust
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct InjectionLogRecord {
    pub log_id: u64,
    pub session_id: String,
    pub entry_id: u64,
    pub confidence: f64,
    pub timestamp: u64,    // unix epoch seconds
}
```

**Store operations**:

- `insert_injection_log_batch(records: &[InjectionLogRecord]) -> Result<()>` — allocates a range of IDs from `next_log_id` in a single transaction; writes all N records in that same transaction. Prevents both counter contention (SR-12) and N-transaction overhead.
- `scan_injection_log_by_session(session_id: &str) -> Result<Vec<InjectionLogRecord>>` — full scan + in-process filter (acceptable at <5K records/day volume)

**ID allocation** — `next_log_id` counter in COUNTERS table; allocated atomically in a batch write transaction.

### 1.4 Schema v5 Migration

Migration pattern follows the established 3-step process (mirrors `migrate_v3_to_v4`):

**Step 1** — `crates/unimatrix-store/src/migration.rs`: bump `CURRENT_SCHEMA_VERSION` to 5.

**Step 2** — Add `migrate_v4_to_v5(txn: &WriteTransaction) -> Result<()>`:

```rust
fn migrate_v4_to_v5(txn: &redb::WriteTransaction) -> Result<()> {
    // Open tables to trigger creation (redb creates on first open in write txn)
    txn.open_table(SESSIONS)?;
    txn.open_table(INJECTION_LOG)?;

    // Write next_log_id = 0 only if key does not already exist (SR-05 idempotency)
    {
        let mut counters = txn.open_table(COUNTERS)?;
        if counters.get("next_log_id")?.is_none() {
            counters.insert("next_log_id", 0u64)?;
        }
    }

    Ok(())
}
```

**Step 3** — Chain in `migrate_if_needed()` after the entry-rewriting step:

```rust
// Non-entry-rewriting step: chain from any starting version <= 4
if current_version <= 4 {
    migrate_v4_to_v5(&txn)?;
}
```

**Idempotency guarantee** (SR-05): The `next_log_id = 0` write is guarded by `if counters.get(...).is_none()`. Table creation is idempotent (redb no-ops on already-existing tables). If the server restarts mid-migration before the schema version is written, the migration re-runs safely.

---

## 2. UDS Listener Integration

All new persistent writes use `spawn_blocking` (consistent with the established server pattern; see `process_session_close` and signal write paths). SessionClose P99 latency budget remains <200ms.

### 2.1 SessionRegister Handler

Location: `dispatch_request` → `HookRequest::SessionRegister` arm in `uds_listener.rs`.

**New (after existing registry call)**:
```rust
// Persist SessionRecord to SESSIONS (col-010)
let record = SessionRecord {
    session_id: session_id.clone(),
    feature_cycle: feature.clone(),
    agent_role: agent_role.clone(),
    started_at: unix_now_secs(),
    ended_at: None,
    status: SessionLifecycleStatus::Active,
    compaction_count: 0,
    outcome: None,
    total_injections: 0,
};
let store_clone = Arc::clone(store);
tokio::task::spawn_blocking(move || store_clone.insert_session(&record))
    .await
    .ok(); // fire-and-forget: log error but don't fail SessionRegister
```

### 2.2 SessionClose Handler

Location: `process_session_close` in `uds_listener.rs`.

**New (after existing `drain_and_signal_session` and signal processing)**:

1. Resolve `final_status` and `final_outcome`:

```rust
let (final_status, outcome_str) = match signal_output.final_outcome {
    SessionOutcome::Success  => (SessionLifecycleStatus::Completed, "success"),
    SessionOutcome::Rework   => (SessionLifecycleStatus::Completed, "rework"),
    SessionOutcome::Abandoned => (SessionLifecycleStatus::Abandoned, "abandoned"),
};
let injection_count = session_registry
    .get_injection_count(&session_id)
    .unwrap_or(0);
```

2. Write updated `SessionRecord` via `spawn_blocking`.

3. If `final_status != Abandoned && injection_count > 0`: write auto-outcome entry (§4).

### 2.3 ContextSearch Handler — Batch INJECTION_LOG Write

Location: `handle_context_search` in `uds_listener.rs`, after step 10 (injection tracking).

**New (immediately after `session_registry.record_injection`)**:

```rust
// Persist injection log batch (col-010, ADR-003: one transaction per response)
if let Some(ref sid) = session_id {
    if !sid.is_empty() && !filtered.is_empty() {
        let now = unix_now_secs();
        let records: Vec<InjectionLogRecord> = filtered
            .iter()
            .map(|(entry, _sim)| InjectionLogRecord {
                log_id: 0, // allocated by insert_injection_log_batch
                session_id: sid.clone(),
                entry_id: entry.id,
                confidence: entry.confidence,
                timestamp: now,
            })
            .collect();
        let store_clone = Arc::clone(store);
        tokio::task::spawn_blocking(move || store_clone.insert_injection_log_batch(&records))
            .await
            .ok(); // fire-and-forget: log error but don't block response
    }
}
```

**Critical**: one transaction for all N entries in the response (not N transactions). The batch reduces COUNTERS write contention from N to 1 per `ContextSearch` response.

---

## 3. Session GC in Maintenance Sweep

Location: `maintain=true` handling in `tools.rs` (same path as coherence gate maintenance in crt-005).

```rust
// GC sessions: mark old Active as TimedOut, delete expired (col-010)
let store_gc = Arc::clone(&store);
tokio::task::spawn_blocking(move || {
    store_gc.gc_sessions(TIMED_OUT_THRESHOLD_SECS, DELETE_THRESHOLD_SECS)
})
.await
.ok();
```

### 3.1 GC Logic with INJECTION_LOG Cascade (SR-04)

`gc_sessions` in `sessions.rs` runs in a single write transaction:

```
Phase 1: collect session_ids to delete (started_at + DELETE_THRESHOLD_SECS < now)
Phase 2: full scan INJECTION_LOG; collect log_ids whose session_id is in deletion set
Phase 3: delete all collected INJECTION_LOG entries
Phase 4: delete all collected SESSIONS entries
Phase 5: mark Active sessions with started_at + TIMED_OUT_THRESHOLD_SECS < now as TimedOut
```

All five phases in one `WriteTransaction`. Returns `GcStats { timed_out: u32, deleted: u32, log_entries_deleted: u32 }`.

---

## 4. Auto-Generated Session Outcomes (col-001 Integration)

### 4.1 VALID_TYPES Extension

`crates/unimatrix-server/src/outcome_tags.rs`: add `"session"` to `VALID_TYPES`.

### 4.2 Auto-Outcome Entry Write

Written in `process_session_close` after the `SessionRecord` update, only when:
- `final_status != Abandoned`
- `injection_count > 0` (non-trivial session)

Content: `"Session {session_id} completed with outcome: {outcome}. Injected {n} entries."`

Tags: `["type:session", "result:pass"]` (Success) or `["type:session", "result:rework"]` (Rework)

Fields:
- `source = "hook"` (distinguishes from MCP-written outcomes; no `"source"` tag needed — uses `entry.source` field per SCOPE.md decision)
- `created_by = "cortical-implant"`
- `trust_source = "system"` (scores 0.7, not the `_ => 0.3` arm — correctness, not boost)
- `feature_cycle = session.feature_cycle.unwrap_or_default()`
- `embedding_dim = 0` (no ONNX embedding; SR-07 does not apply here — entry is structured metadata only)

Written directly via `store.insert_entry()` — bypasses MCP validation layer per SCOPE.md decision. Minimum validation applied at write point: category allowlist check + tag key validation.

---

## 5. Structured Retrospective (`from_structured_events()`)

New file: `crates/unimatrix-observe/src/structured.rs`

```rust
pub fn from_structured_events(
    store: &Store,
    feature_cycle: &str,
) -> Result<RetrospectiveReport, ObserveError> {
    let sessions = store.scan_sessions_by_feature(feature_cycle)?;
    if sessions.is_empty() {
        return Ok(RetrospectiveReport::empty(feature_cycle));
    }
    // Exclude Abandoned sessions from metric computation (ADR-001 benefit)
    let active_sessions: Vec<&SessionRecord> = sessions
        .iter()
        .filter(|s| s.status != SessionLifecycleStatus::Abandoned)
        .collect();
    // ...build ObservationRecord stream from INJECTION_LOG...
    // ...run same metrics + hotspot pipeline as JSONL path...
    // Layer 2 narrative synthesis for non-Abandoned sessions (§6)
}
```

**Retrospective path selection** in `context_retrospective` MCP tool handler:

```
1. Try from_structured_events(store, feature_cycle)
   → if sessions found: use structured path (Layer 1 + optional Layer 2)
   → if sessions empty: fall back to JSONL parser path (Layer 1 only)
2. Apply detail_level filter to output
3. Return RetrospectiveReport
```

**`ObservationRecord` extension** — add fields with `#[serde(default)]` (no migration needed):
```rust
pub struct ObservationRecord {
    // existing fields...
    #[serde(default)]
    pub confidence_at_injection: Option<f64>,  // from InjectionLogRecord
    #[serde(default)]
    pub session_outcome: Option<String>,        // from SessionRecord
}
```

---

## 6. Tiered Retrospective Output + Evidence Synthesis

### 6.1 RetrospectiveReport Type Changes

`crates/unimatrix-observe/src/types.rs` — restructure `RetrospectiveReport`:

```rust
pub struct RetrospectiveReport {
    // Layer 1: always populated
    pub feature_cycle: String,
    pub session_count: usize,
    pub total_records: usize,
    pub metrics: MetricVector,
    pub hotspots: Vec<HotspotSummary>,           // was Vec<HotspotFinding>
    pub recommendations: Vec<Recommendation>,    // NEW: from templates
    pub is_cached: bool,
    #[serde(default)]
    pub baseline_comparison: Option<Vec<BaselineComparison>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entries_analysis: Option<Vec<EntryAnalysis>>,

    // Layer 2: structured-events path only
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub narratives: Option<Vec<HotspotNarrative>>,

    // Layer 3 backward-compat: raw evidence (detail_level="full" only)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hotspot_details: Option<Vec<HotspotFinding>>,  // original HotspotFinding with evidence
}
```

**`HotspotSummary`** (Layer 1 — claim only, no raw evidence):
```rust
pub struct HotspotSummary {
    pub category: HotspotCategory,
    pub severity: Severity,
    pub rule_name: String,
    pub claim: String,
    pub measured: f64,
    pub threshold: f64,
    pub evidence_count: usize,    // count only
}
```

**`HotspotNarrative`** (Layer 2 — synthesized):
```rust
pub struct HotspotNarrative {
    pub rule_name: String,
    pub summary: String,           // e.g. "5 permission retries within 30s at ts=X"
    pub clusters: Vec<EvidenceCluster>,
    pub top_files: Vec<(String, u32)>,           // top-5 by mutation count
    pub sequence_pattern: Option<String>,         // e.g. "30s→60s→90s→120s"
}

pub struct EvidenceCluster {
    pub window_start: u64,         // unix epoch ms
    pub event_count: u32,
    pub description: String,
}
```

**`Recommendation`** (NEW):
```rust
pub struct Recommendation {
    pub hotspot_type: String,
    pub action: String,
    pub rationale: String,
}
```

### 6.2 detail_level Parameter

`context_retrospective` tool handler gains a `detail_level: Option<String>` parameter (default `"summary"`):

| Value | What's populated | Size |
|-------|-----------------|------|
| `"summary"` | `hotspots` (HotspotSummary), `recommendations`. `narratives = None`, `hotspot_details = None` | ~1-2KB |
| `"narrative"` | All of summary + `narratives` (structured-events path only) | ~5-10KB |
| `"full"` | All of narrative + `hotspot_details` (original HotspotFinding with evidence arrays) | ~87KB (backward-compat) |

**SR-03 mitigation**: existing callers that relied on evidence arrays must pass `detail_level = "full"`. Document this prominently in implementation brief. Tests that assert on `hotspots[].evidence` must be updated to use `hotspot_details[].evidence`.

### 6.3 Evidence Synthesis Logic

In `structured.rs`, `synthesize_narratives(sessions, injection_log)`:

- **Timestamp clustering**: 30-second sliding window; group events by session; report as "N events within 30s at ts=X". Window size is a named constant (`CLUSTER_WINDOW_MS = 30_000u64`) for future tuning.
- **Sequence extraction**: detect monotone-increasing sleep values in `sleep_workarounds` hotspot; format as "Ns→Ns→...". Returns `None` if pattern is non-monotone or <2 values.
- **Top-N files**: sort by mutation count, truncate to top 5, append "... and N more" if truncated.
- **Entry performance correlation**: for each entry in `entries_analysis`, compute `successful_sessions / total_sessions_that_injected_this_entry` using SESSIONS + INJECTION_LOG cross-reference.

All synthesis is deterministic/heuristic only (no LLM). Best-effort: empty `sequence_pattern = None` and empty `clusters` are handled gracefully.

### 6.4 Recommendation Templates

In `report.rs`, `recommendations_for_hotspots(hotspots: &[HotspotSummary]) -> Vec<Recommendation>`:

Covers 4 hotspot types: `permission_retries`, `coordinator_respawns`, `sleep_workarounds`, `compile_cycles` (only when `measured > 10.0`). Hardcoded in `report.rs`. Not configurable in v1.

---

## 7. Lesson-Learned Auto-Persistence + Provenance Boost

### 7.1 Auto-Persist Logic

After `context_retrospective` computes a report with `hotspots.len() > 0 || recommendations.len() > 0`:

1. Build `lesson-learned` entry content from Layer 2 narrative (hotspot summaries + recommendations).
2. Topic: `"retrospective/{feature_cycle}"`.
3. Check for existing active `lesson-learned` entry with matching topic via `context_lookup`.
4. If exists: deprecate via the existing `context_correct` supersede path (ADR-002 chain).
5. Write new entry: `trust_source = "system"`, `category = "lesson-learned"`.
6. Embed and index via the ONNX embedding pipeline.

**SR-07 mitigation** (fire-and-forget): embedding is launched via `tokio::task::spawn_blocking` detached from the response future. The `context_retrospective` tool returns its `RetrospectiveReport` to the caller immediately. The `lesson-learned` entry appears in vector search after embedding completes (typically <500ms on warm ONNX).

```rust
// Fire-and-forget lesson-learned write (col-010)
tokio::spawn(async move {
    let embed_result = tokio::task::spawn_blocking(move || {
        adapter.embed_entry(&title, &content)
    }).await;
    // write entry to store + VECTOR_MAP regardless
    // log embed failure but don't fail the retrospective
});
```

### 7.2 Supersede De-duplication (SR-09)

The check-then-supersede sequence is **not** wrapped in a distributed lock. Concurrent calls for the same feature_cycle may produce two active entries in an edge case. This is accepted as a known tolerated race (concurrent retrospective calls for the same cycle are rare; the crt-003 contradiction detection will surface any duplicates). Document as known limitation.

### 7.3 Provenance Boost

**Location**: `unimatrix-engine/src/confidence.rs` (alongside `co_access_affinity`).

```rust
/// Query-time boost for `lesson-learned` category entries.
/// Applied in search re-ranking alongside co_access_affinity.
/// Does NOT change the stored confidence formula invariant.
pub const PROVENANCE_BOOST: f64 = 0.02;
```

**Application site** — both `uds_listener.rs` and `tools.rs` search re-ranking, co-access boost sort:

```rust
let prov_a = if entry_a.category == "lesson-learned" { PROVENANCE_BOOST } else { 0.0 };
let final_a = base_a + boost_a + prov_a;
```

**Invariant preserved**: `W_BASE + W_USAGE + W_FRESH + W_HELP + W_CORR + W_TRUST = 0.92` is unchanged. Provenance boost is query-time only, never stored in `EntryRecord.confidence`.

---

## Risk Mitigations Summary

| Risk | Severity | Resolution |
|------|----------|-----------|
| SR-01 col-009 dependency | Critical | Architecture assumes col-009 merged. Gate check required before impl. |
| SR-02 bundle delivery | High | Explicit P0/P1 split. P1 independent of col-011. |
| SR-03 default behavior change | High | `detail_level="full"` preserves backward compat. Test audit required. |
| SR-04 INJECTION_LOG GC cascade | Medium | `gc_sessions` deletes orphan log entries in same transaction (§3.1). |
| SR-05 migration idempotency | Medium | `next_log_id = 0` guarded by `if_none` check (§1.4). |
| SR-06 Abandoned variant | Medium | `Abandoned` is a distinct `SessionLifecycleStatus` variant (ADR-001). |
| SR-07 ONNX latency | Medium | lesson-learned embedding fire-and-forget via `tokio::spawn` (§7.1). |
| SR-08 heuristic fragility | Medium | Best-effort synthesis; `CLUSTER_WINDOW_MS` named constant for future tuning. |
| SR-09 concurrent supersede | Medium | Accepted tolerated race; documented known limitation (§7.2). |
| SR-10 vision doc discrepancy | Low | `session_id` on `EntryRecord` is a Non-Goal; PRODUCT-VISION.md needs correction. |
| SR-11 auto-outcome validation | Low | Category allowlist + tag validation applied before write (§4.2). |
| SR-12 counter contention | Low | Batch writes: one transaction per ContextSearch response (§2.3, ADR-003). |

---

## Component-to-File Mapping

| Component | Files |
|-----------|-------|
| P0: Storage Layer | `crates/unimatrix-store/src/schema.rs`, `sessions.rs` (new), `injection_log.rs` (new), `migration.rs` |
| P0: UDS Listener | `crates/unimatrix-server/src/uds_listener.rs` |
| P0: Session GC | `crates/unimatrix-server/src/tools.rs` (maintain path), `crates/unimatrix-store/src/sessions.rs` |
| P0: Auto-Outcomes | `crates/unimatrix-server/src/outcome_tags.rs`, `uds_listener.rs` |
| P1: Structured Retro | `crates/unimatrix-observe/src/structured.rs` (new), `report.rs`, `types.rs` |
| P1: Tiered Output | `crates/unimatrix-observe/src/types.rs`, `report.rs`, `crates/unimatrix-server/src/tools.rs` |
| P1: Lesson-Learned | `crates/unimatrix-server/src/tools.rs`, `crates/unimatrix-engine/src/confidence.rs` |

---

## Downstream Impact (col-011)

col-011 (Semantic Agent Routing) consumes:
- `SESSIONS` table: `scan_sessions_by_feature()` for outcome-correlation routing quality scoring
- `INJECTION_LOG` table: `scan_injection_log_by_session()` for per-entry performance history

col-011 has no dependency on the P1 components.

---

## Open Questions

1. **`session_id` field on `EntryRecord`** — deferred as Non-Goal per SCOPE.md. PRODUCT-VISION.md col-010 row references this field erroneously; needs correction before agent confusion arises.
2. **INJECTION_LOG secondary index** — full scan + in-process filter is acceptable at current volumes (<5K records/day). The threshold where a secondary index becomes necessary is approximately 500K+ total records. No action needed in col-010.
3. **`lesson-learned` category allowlist** — MEMORY.md confirms `lesson-learned` is in the allowlist. Implementer should verify via `context_status` before writing the first entry.
4. **`detail_level` backward-compat test audit** — SR-03 requires auditing existing `context_retrospective` integration tests for `hotspots[].evidence` assertions before implementing the tiered output change. This is an implementation task, not an architecture question.
