# col-010: Session Lifecycle Persistence & Structured Retrospective

## Problem Statement

col-009 closes the confidence feedback loop using in-memory session state — but in-memory is ephemeral. Every server restart loses all session history: what was injected, in which session, with what confidence, and with what outcome. Agents cannot query past sessions. The col-002 retrospective pipeline still relies on JSONL telemetry files written by the legacy observation hooks — a file-system-based system with content-based feature attribution that is imprecise, subject to file rotation gaps, and disconnected from the structured session data the server now accumulates.

col-010 makes session state durable. It adds two new redb tables — SESSIONS and INJECTION_LOG — that persistently record the lifecycle of every session and every injection event. With this foundation, the col-002 retrospective gains a second, more accurate entry point: `from_structured_events()` reads directly from the store instead of parsing JSONL files. Feature attribution becomes exact (from session registration) rather than inferred. Session outcomes recorded by col-009's signal pipeline are also surfaced as structured col-001 outcome entries, closing the loop between implicit hook signals and the queryable knowledge base.

## Goals

1. **SESSIONS table** (schema v5, 16th table): persist a `SessionRecord` for each session lifecycle — written on `SessionStart`, updated on `Stop`/`TaskCompleted`, GC'd on maintenance.
2. **INJECTION_LOG table** (schema v5, 17th table): persist an `InjectionLogRecord` for every injection event — written alongside the existing in-memory `record_injection()` call.
3. **Schema v4→v5 migration**: create SESSIONS and INJECTION_LOG tables, write `next_log_id = 0` to COUNTERS. No entry scan-and-rewrite required (new tables only).
4. **SESSIONS writes in UDS listener**: on `SessionRegister` dispatch, write Active SessionRecord. On `SessionClose` dispatch (after col-009 signal generation), write Completed/Abandoned SessionRecord with outcome, total_injections.
5. **INJECTION_LOG writes in UDS listener**: on every successful `ContextSearch` injection response, write one `InjectionLogRecord` per injected entry alongside the existing `record_injection()` call.
6. **Structured retrospective** (`from_structured_events()` in `unimatrix-observe`): reads SESSIONS + INJECTION_LOG from the store for a given `feature_cycle`, converts to `ObservationRecord` stream, runs the same 21-rule hotspot detection and metric computation. JSONL parser is retained as fallback.
7. **Capped evidence output** (issue #65): the `context_retrospective` tool gains an `evidence_limit: usize` parameter (default `3`). Each hotspot returns at most `evidence_limit` representative evidence items — enough to be actionable, not enough to flood the context window. `evidence_limit = 0` means unlimited (backward-compatible with current behavior for callers that need it). The `hotspots` field type is unchanged (`Vec<HotspotFinding>` with its existing `evidence` array); the array is simply truncated server-side before returning. Narrative synthesis (Goal 8) appears alongside the capped evidence — the combination of a synthesized pattern description + 3 concrete examples is more useful than 26 raw timestamps. The `from_structured_events()` path populates narrative fields; the JSONL fallback path returns capped evidence only.
8. **Evidence synthesis for Layer 2** (issue #65): per-hotspot narrative generation from structured data — timestamp clustering ("5 permission retries within 30s"), sequence extraction (sleep escalation patterns), top-N for file lists, entry-performance correlation using `InjectionLogRecord.confidence` + `SessionRecord.outcome`. This is the structural data advantage that SESSIONS + INJECTION_LOG provides over the JSONL path.
9. **Recommendation templates** (issue #65): map the 4 common hotspot types (`permission_retries`, `coordinator_respawns`, `sleep_workarounds`, `compile_cycles`) to actionable recommendations in `report.rs`. Zero infrastructure cost; included here because we are already modifying `unimatrix-observe`.
10. **Auto-generated session outcomes** (col-001 integration): on `SessionClose` with a determined outcome of `Success` or `Rework`, call the existing store write pathway to create a `category:outcome, type:session` entry. Requires adding `"session"` to `VALID_TYPES` in `outcome_tags.rs`.
11. **Retrospective auto-persist as knowledge entry** (lesson-learned): after `context_retrospective` computes a non-empty report (>0 hotspots or recommendations), automatically write the synthesized narrative as a `category:lesson-learned` entry in Unimatrix. Content is the Layer 2 narrative output (hotspot summaries + recommendations). The entry is embedded (full ONNX path) so it is queryable via `context_search` by future agents. De-duped by feature_cycle: if a lesson-learned entry for this cycle already exists, supersede it. Written with `trust_source = "system"` (cortical implant is a system process; scores 0.7 vs 0.5 for agent-written entries — correctness fix, not a hack).
12. **Query-time provenance boost for `lesson-learned` entries**: add a small constant boost (`PROVENANCE_BOOST = 0.02`) applied at search re-ranking time alongside the existing `co_access_affinity`. Applied when `entry.category == "lesson-learned"`. Final score: `0.85*sim + 0.15*conf + co_access_affinity + provenance_boost`. This is the "slight bump" that makes retrospective findings surface with a gentle priority signal over equal-confidence entries. The boost is constant at query time; natural crt-002 decay (freshness half-life 168h) and promotion (helpful_count accumulation) handle the rest.
13. **Session GC in maintenance sweep**: during `maintain=true` on `context_status`, mark sessions with `status:Active` and `started_at > 24h` as `TimedOut`; delete sessions older than 30 days.

## Non-Goals

- **`session_id` field on `EntryRecord`** — would require a full scan-and-rewrite migration (bincode is positional). The benefit (which session created a given entry) is low priority; SESSIONS + INJECTION_LOG cover the retrospective use case without it. Deferred to a future feature.
- **Replacing the JSONL parser** — `from_structured_events()` is additive. The JSONL path is retained as a fallback for historical data that predates col-010.
- **SubagentStart/SubagentStop persistent tracking** — subagent sessions are handled in-memory only (existing RecordEvent path). They are not written to the SESSIONS table. Deferred.
- **Cross-session dashboards or agent routing** — col-011's concern. col-010 provides the data infrastructure (SESSIONS, INJECTION_LOG); col-011 consumes it for routing quality scoring.
- **INJECTION_LOG secondary indexes** — the structured retrospective does a full scan of INJECTION_LOG and filters by session_id in-process. At expected volumes (< 5,000 records/day), this is acceptable. Index optimization is future work.
- **Sophisticated narrative ML** — evidence synthesis uses deterministic heuristics (timestamp clustering, sequence detection, top-N). No LLM or model inference for synthesizing retrospective narratives.
- **`helpful_count` seeding** — initializing `helpful_count = 1` at write time has no effect: `helpfulness_score` returns the neutral prior of 0.5 for total votes < `MINIMUM_SAMPLE_SIZE` (5). Not a viable mechanism.
- **Category-specific `MINIMUM_SAMPLE_SIZE` reduction** — lowering the Wilson guard for `lesson-learned` entries would let a single helpful vote deviate from neutral. Rejected: Wilson's minimum sample guard is a global safety property. Not appropriate to punch holes in it by category.
- **`detail_level: String` ("summary" | "narrative" | "full") parameter** — rejected in favour of `evidence_limit: usize`. Three-tier string enum introduces type-level changes to `hotspots` (replacing `Vec<HotspotFinding>` with `Vec<HotspotSummary>`) that break all existing callers. `evidence_limit` keeps the type unchanged, truncates evidence at the server, and makes the default (3 examples) immediately actionable without flooding the context window.

## Background Research

### What col-009 Delivers (Schema v4, in-memory)

col-009 adds SIGNAL_QUEUE (table 15, schema v4) and the in-memory signal generation pipeline. The `SessionRegistry` holds `SessionState` (injection_history, rework_events, agent_actions, signaled_entries, last_activity_at) — all lost on server restart. The `drain_and_signal_session()` method generates `SignalOutput` (helpful_entry_ids, flagged_entry_ids, final_outcome) and evicts the session from memory. col-010 intercepts this same `SessionClose` path to write a durable `SessionRecord`.

### What the UDS Listener Currently Does (post col-009)

`crates/unimatrix-server/src/uds_listener.rs` dispatches:
- `HookRequest::SessionRegister` → calls `registry.register_session()` (in-memory only)
- `HookRequest::SessionClose` → calls `drain_and_signal_session()`, writes signal records to SIGNAL_QUEUE, runs confidence consumer, accumulates `PendingEntriesAnalysis`
- `HookRequest::ContextSearch` → searches, formats, calls `record_injection()` (in-memory)
- `HookRequest::CompactPayload` → reads session state, formats compaction payload

col-010 adds persistent writes at the `SessionRegister` and `SessionClose` dispatch points, and a persistent write at every `ContextSearch` injection point.

### Hook Wiring (`.claude/settings.json`)

Already registered:
- `SessionStart` → `unimatrix-server hook SessionStart` → `HookRequest::SessionRegister`
- `Stop` → `unimatrix-server hook Stop` → `HookRequest::SessionClose { outcome: "success" }`
- `UserPromptSubmit` → `HookRequest::ContextSearch`
- `PostToolUse` → rework tracking + `HookRequest::RecordSignal`
- `PreToolUse`, `SubagentStart`, `SubagentStop` → `HookRequest::RecordEvent`

No hook changes needed for col-010.

### Existing col-002 Retrospective Pipeline

`crates/unimatrix-observe/src/`:
- `parser.rs`: `parse_jsonl_dir(path)` → `Vec<ObservationRecord>`
- `attribution.rs`: content-based feature_cycle inference (fallback; imprecise)
- `metrics.rs`: `compute_metrics(records)` → `MetricVector`
- `report.rs`: `build_report(store, feature_cycle, dir)` → `RetrospectiveReport`

The new `from_structured_events(store, feature_cycle)` entry point produces the same `RetrospectiveReport` by reading SESSIONS + INJECTION_LOG instead of parsing JSONL. The `types.rs` `ObservationRecord` struct is the shared intermediate format.

### Issue #65: Retrospective Output Size and Quality

The retrospective engine (col-002) returns ~87KB of raw JSON for a single feature cycle (col-006). Evidence arrays account for ~90% of payload but ~10% of value. Specific observations:

- 1,163 telemetry records parsed, 13 hotspots detected, **0 recommendations generated**
- `permission_retries` hotspot: 26+ individual `PreToolUse` timestamps instead of "5 retries within 30s at ts=X"
- `compile_cycles` hotspot: 18 cargo commands listed individually instead of a count + pattern
- `sleep_workarounds`: 12 commands that form a clear escalation pattern (30s→60s→90s→120s) listed as independent records
- `file_breadth`: 83 files listed individually; top-5 by mutation count is sufficient

**What SESSIONS + INJECTION_LOG enable for #65:**

The JSONL path can attempt narrative synthesis, but loses session context: it cannot say "entry #42 was injected in 14 sessions, 11 of which completed successfully." SESSIONS + INJECTION_LOG make this correlation exact:
- `InjectionLogRecord.confidence` per injected entry per session → entry-level performance trajectory
- `SessionRecord.outcome` → exact success/rework attribution (not inferred from content)
- Session grouping → "these 5 permission retries all occurred in session X which had outcome=rework"

The `from_structured_events()` path is the right home for Layer 2 narrative synthesis because it has access to this richer structured context. The JSONL fallback path produces Layer 1 only.

### col-001 Outcome Type Extension

`crates/unimatrix-server/src/outcome_tags.rs` defines `VALID_TYPES = &["feature", "bugfix", "incident", "process"]`. Adding `"session"` enables auto-generated outcomes from the hook pipeline. The auto-outcome entry uses tags: `["type:session", "result:pass|rework", "source:auto", "phase:implementation"]`.

`"source"` is not currently a recognized structured tag key. Three options:
1. Add `"source"` as a plain (unvalidated) tag — pass-through, no validation needed.
2. Add `"source"` as a new recognized key in `RECOGNIZED_KEYS` with allowed values `["auto", "agent"]`.
3. Omit `"source"` tag and rely on the entry's `source` field (already set to `"hook"` for hook-generated entries).

**Decision**: Use option 3 (omit `"source"` tag; use `entry.source = "hook"` instead). Keeps outcome_tags.rs stable. Auto-outcome entries are distinguishable by source field.

## Proposed Approach

### 7 Build Components

**1. Storage Layer: SESSIONS + INJECTION_LOG Tables (schema v5)**

Add to `crates/unimatrix-store/src/schema.rs`:
```rust
pub const SESSIONS: TableDefinition<&str, &[u8]> = TableDefinition::new("sessions");
pub const INJECTION_LOG: TableDefinition<u64, &[u8]> = TableDefinition::new("injection_log");
```

Add `crates/unimatrix-store/src/sessions.rs`:
```rust
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SessionRecord {
    pub session_id: String,
    pub feature_cycle: Option<String>,
    pub agent_role: Option<String>,
    pub started_at: u64,
    pub ended_at: Option<u64>,
    pub status: SessionLifecycleStatus,
    pub compaction_count: u32,
    pub outcome: Option<String>,   // "success" | "rework" | "abandoned"
    pub total_injections: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum SessionLifecycleStatus { Active, Completed, TimedOut }

// Store write: insert_session(record), update_session(session_id, updater)
// Store write: write to SESSIONS with bincode serde path (same as EntryRecord)
// Store read: get_session(session_id) -> Option<SessionRecord>
// Store read: scan_sessions_by_feature(feature_cycle) -> Vec<SessionRecord>
// Store write: gc_sessions(cutoff_active_secs, cutoff_delete_secs)
//   marks Active sessions older than cutoff_active_secs as TimedOut
//   deletes sessions older than cutoff_delete_secs
```

Add `crates/unimatrix-store/src/injection_log.rs`:
```rust
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct InjectionLogRecord {
    pub log_id: u64,
    pub session_id: String,
    pub entry_id: u64,
    pub confidence: f64,
    pub timestamp: u64,
}

// Store write: insert_injection_log(record) — allocates from next_log_id counter
// Store read: scan_injection_log_by_session(session_id) -> Vec<InjectionLogRecord>
//   (full scan + in-process filter — acceptable at current volumes)
```

Migration: `migrate_v4_to_v5()` — open SESSIONS and INJECTION_LOG tables (triggers creation), write `next_log_id = 0` to COUNTERS. Bump `CURRENT_SCHEMA_VERSION` to 5.

**2. UDS Listener Integration: Persistent Writes on Session Events**

Extend `uds_listener.rs` `dispatch_request()`:

On `HookRequest::SessionRegister { session_id, cwd, agent_role, feature }`:
- (existing) call `registry.register_session()`
- (new) write `SessionRecord { session_id, feature_cycle: feature, agent_role, started_at: now, ended_at: None, status: Active, compaction_count: 0, outcome: None, total_injections: 0 }` to SESSIONS via `spawn_blocking`

On `HookRequest::SessionClose { session_id, outcome, .. }`:
- (existing) call `drain_and_signal_session()`, process signals
- (new) resolve `final_outcome` from `SignalOutput.final_outcome`, update SessionRecord: `status = Completed, ended_at = now, outcome = final_outcome_str, total_injections = injection_count` via `spawn_blocking`
- (new) if `final_outcome != Abandoned` and `total_injections > 0`: write auto-outcome entry via existing `store_entry()` pathway

On every successful `HookRequest::ContextSearch` injection response:
- (existing) call `record_injection()`
- (new) for each injected entry: write `InjectionLogRecord` to INJECTION_LOG via `spawn_blocking`

Abandoned sessions (outcome = Abandoned): write SessionRecord with status=Completed, outcome="abandoned". No auto-outcome entry.

**3. Session GC in Maintenance Sweep**

In `crates/unimatrix-server/src/coherence.rs` or directly in `tools.rs` where `maintain=true` is handled:

Call `store.gc_sessions(TIMED_OUT_THRESHOLD_SECS, DELETE_THRESHOLD_SECS)` where:
- `TIMED_OUT_THRESHOLD_SECS = 24 * 3600` (24 hours)
- `DELETE_THRESHOLD_SECS = 30 * 24 * 3600` (30 days)

Named constants in `sessions.rs`. Not user-configurable in v1.

**4. Auto-Generated Session Outcomes (col-001 Integration)**

In `outcome_tags.rs`: add `"session"` to `VALID_TYPES`.

In UDS listener `SessionClose` handler:
- Build outcome entry content: `"Session {session_id} completed with outcome: {outcome_str}. Injected {n} entries."`
- Tags: `["type:session", "result:{pass|rework}"]` (result:pass for Success, result:rework for Rework)
- `source = "hook"`, `created_by = "cortical-implant"`, `feature_cycle = session.feature_cycle`
- Only written if `total_injections > 0` (non-trivial sessions only) and outcome is not Abandoned
- Written via `spawn_blocking` call to `store.insert_entry()` + `embed_handle.embed()` — same pathway as `context_store` MCP tool, but bypasses the MCP validation layer

Wait — embedding requires ONNX. This adds latency to SessionClose. Alternative: no embedding for auto-outcomes, or use a fire-and-forget task. **Decision**: write the entry to store without embedding (embedding_dim = 0, no VECTOR_MAP entry). The entry is searchable by category/tag/topic lookup but not by vector similarity. This is acceptable — auto-outcomes serve as structured metadata, not knowledge entries. The outcome is still queryable via `context_lookup(category: "outcome", tags: ["type:session"])` and surfaces in `context_retrospective`. Avoids the latency hit.

**5. Structured Retrospective (`from_structured_events()` in `unimatrix-observe`)**

Add `crates/unimatrix-observe/src/structured.rs`:

```rust
pub fn from_structured_events(
    store: &Store,
    feature_cycle: &str,
) -> Result<RetrospectiveReport, ObserveError> {
    // Step 1: Get all sessions for this feature_cycle
    let sessions = store.scan_sessions_by_feature(feature_cycle)?;
    if sessions.is_empty() {
        return Ok(RetrospectiveReport::empty(feature_cycle));
    }

    // Step 2: For each session, get injection log records
    let mut all_records: Vec<ObservationRecord> = Vec::new();
    for session in &sessions {
        let injections = store.scan_injection_log_by_session(&session.session_id)?;
        for inj in injections {
            all_records.push(ObservationRecord {
                session_id: session.session_id.clone(),
                feature_cycle: feature_cycle.to_string(),
                entry_id: inj.entry_id,
                confidence_at_injection: inj.confidence,
                timestamp: inj.timestamp,
                outcome: session.outcome.clone(),
                // ... other fields
            });
        }
    }

    // Step 3: Run same computation pipeline as JSONL path
    let metrics = compute_metrics(&all_records);
    let hotspots = detect_hotspots(&all_records);
    // ...
    Ok(build_report_from_records(feature_cycle, sessions.len(), all_records, metrics, hotspots))
}
```

The `ObservationRecord` struct may need minor extension to carry `confidence_at_injection` and `outcome` — fields that JSONL-based records don't have but structured records do. Add with `#[serde(default)]` for backward compat.

Update `context_retrospective` MCP tool handler: try `from_structured_events()` first; fall back to JSONL path if SESSIONS has no data for the feature_cycle. Log which path was used.

**6. Tiered Retrospective Output + Evidence Synthesis (unimatrix-observe, issue #65)**

Restructure `RetrospectiveReport` in `types.rs`:

```rust
pub struct RetrospectiveReport {
    // Layer 1: always populated (existing fields, renamed/reorganized)
    pub feature_cycle: String,
    pub session_count: usize,
    pub total_records: usize,
    pub metrics: MetricVector,
    pub hotspots: Vec<HotspotSummary>,          // claims only: type, severity, measured, threshold
    pub recommendations: Vec<Recommendation>,    // NEW: from templates (issue #65)
    pub is_cached: bool,
    pub baseline_comparison: Option<BaselineComparison>,
    pub entries_analysis: Option<Vec<EntryAnalysis>>,  // from col-009

    // Layer 2: populated by from_structured_events() only
    pub narratives: Option<Vec<HotspotNarrative>>,  // synthesized evidence summaries
}

pub struct HotspotSummary {     // Layer 1: claim only
    pub hotspot_type: String,
    pub severity: String,
    pub measured: f64,
    pub threshold: f64,
    pub evidence_count: usize,  // count only, not the evidence list
}

pub struct HotspotNarrative {   // Layer 2: synthesized
    pub hotspot_type: String,
    pub summary: String,        // human-readable synthesized description
    pub clusters: Vec<EvidenceCluster>,  // timestamp-clustered events
    pub top_files: Vec<(String, u32)>,   // top-N by mutation count (file_breadth only)
    pub sequence_pattern: Option<String>, // e.g., "30s→60s→90s→120s" (sleep_workarounds)
}

pub struct Recommendation {     // NEW (issue #65)
    pub hotspot_type: String,
    pub action: String,
    pub rationale: String,
}
```

`context_retrospective` tool gains a `detail_level` parameter (string, default `"summary"`):
- `"summary"`: return `RetrospectiveReport` with `narratives = None`, `hotspots` as `HotspotSummary` only. ~1-2KB. Default behavior replaces the current ~87KB default.
- `"narrative"`: return full `RetrospectiveReport` including `narratives`. ~5-10KB. Only meaningful when structured-events path was used.
- `"full"`: return existing raw evidence arrays per hotspot. ~87KB. Backward-compatible with callers that currently expect raw evidence.

Add `hotspots_summary` backward-compat field via `#[serde(skip_serializing_if = "Option::is_none")]` to avoid breaking existing callers that read the old `hotspots` array.

**Evidence synthesis logic** (in `structured.rs` `from_structured_events()`):

- **Timestamp clustering**: group events within a sliding 30-second window; report as "N events within 30s at ts=X"
- **Sequence extraction**: detect monotone-increasing sleep values; format as "Ns→Ns→..."
- **Top-N file lists**: sort by mutation count, return top 5 with "... and N more" suffix
- **Entry performance correlation**: for each entry in `entries_analysis`, add injection success rate: `successful_sessions / total_sessions` where both pull from the SESSIONS scan

**Recommendation templates** (in `report.rs`, pure logic, no infrastructure):

```rust
fn recommendations_for_hotspots(hotspots: &[HotspotSummary]) -> Vec<Recommendation> {
    hotspots.iter().filter_map(|h| match h.hotspot_type.as_str() {
        "permission_retries" => Some(Recommendation {
            hotspot_type: "permission_retries".into(),
            action: "Add common build/test commands to settings.json allowlist".into(),
            rationale: format!("{} permission retries detected", h.measured as u32),
        }),
        "coordinator_respawns" => Some(Recommendation { ... }),
        "sleep_workarounds" => Some(Recommendation {
            action: "Use run_in_background + TaskOutput instead of sleep polling".into(),
            ...
        }),
        "compile_cycles" if h.measured > 10.0 => Some(Recommendation { ... }),
        _ => None,
    }).collect()
}
```

**7. Auto-Generated Session Outcomes (col-001 Integration)**

_(Previously component 6 — renumbered. Content unchanged.)_

## Acceptance Criteria

- AC-01: Schema v5 migration runs on `Store::open()` when schema version is 4 — creates SESSIONS and INJECTION_LOG tables, writes `next_log_id = 0` to COUNTERS. Schema version increments to 5. All existing entries and signal records survive intact.
- AC-02: `SessionRegister` dispatch writes a `SessionRecord` with `status = Active` and `started_at` set to current unix timestamp. The record is readable via `get_session(session_id)`.
- AC-03: `SessionClose` dispatch with `final_outcome = Success` updates the SessionRecord to `status = Completed`, `ended_at = now`, `outcome = "success"`, `total_injections = N`.
- AC-04: `SessionClose` dispatch with `final_outcome = Rework` updates the SessionRecord to `status = Completed`, `outcome = "rework"`.
- AC-05: `SessionClose` dispatch with `final_outcome = Abandoned` writes `status = Completed`, `outcome = "abandoned"`. No auto-outcome entry is created.
- AC-06: Every injected entry in a `ContextSearch` response produces an `InjectionLogRecord` in INJECTION_LOG with correct `session_id`, `entry_id`, `confidence`, and `timestamp`.
- AC-07: `scan_injection_log_by_session(session_id)` returns exactly the records written for that session (full scan + filter).
- AC-08: Maintenance sweep marks `Active` sessions with `started_at > 24h` as `TimedOut` (without deleting them).
- AC-09: Maintenance sweep deletes sessions (any status) with `started_at > 30 days`.
- AC-10: `VALID_TYPES` in `outcome_tags.rs` includes `"session"`. `validate_outcome_tags(["type:session"])` returns `Ok(())`.
- AC-11: Auto-outcome entry is written to SESSIONS on `SessionClose` with outcome `Success` or `Rework` when `total_injections > 0`. The entry has `category = "outcome"`, `tags` contains `"type:session"` and `"result:pass"` or `"result:rework"`, `embedding_dim = 0` (no vector embedding).
- AC-12: `from_structured_events(store, feature_cycle)` returns a `RetrospectiveReport` with `session_count` equal to the number of SESSIONS records for that feature_cycle.
- AC-13: `context_retrospective` tool prefers the structured path when SESSIONS data exists for the feature_cycle; falls back to JSONL path otherwise.
- AC-14: A server restart (simulated by clearing in-memory SessionRegistry) does not lose SESSIONS or INJECTION_LOG data from prior sessions.
- AC-15: `context_retrospective` with default `evidence_limit = 3` returns at most 3 evidence items per hotspot. A hotspot with 26 raw evidence records returns exactly 3. Total response size for a 13-hotspot report is ≤ 10KB.
- AC-16: `context_retrospective` with `evidence_limit = 0` returns the complete evidence arrays for every hotspot — output is backward-compatible with pre-col-010 callers and existing integration tests pass when `evidence_limit = 0` is supplied.
- AC-17: `context_retrospective` with `evidence_limit = 3` returns `narratives` populated (not None) for each hotspot when the structured-events path was used. The combination of narrative summary + 3 evidence items is present in the same response.
- AC-18: For a `sleep_workarounds` hotspot with escalating sleep intervals (30→60→90→120), `HotspotNarrative.sequence_pattern` contains the sequence string.
- AC-19: Recommendation templates: a `RetrospectiveReport` with a `permission_retries` hotspot includes a `Recommendation` with non-empty `action`. A report with no recognized hotspot types includes an empty `recommendations` list.
- AC-20: After `context_retrospective` completes with ≥1 hotspot or recommendation, a `category:lesson-learned` entry exists in Unimatrix with `topic = "retrospective/{feature_cycle}"`, non-empty content (narrative summary), `trust_source = "system"`, and a valid embedding (`embedding_dim > 0`).
- AC-21: Calling `context_retrospective` twice for the same feature_cycle produces exactly one active `lesson-learned` entry — the second call supersedes the first (prior entry status = `Deprecated`, `superseded_by` set).
- AC-22: `context_search` with a query related to a prior retrospective finding (e.g., "permission retry patterns") returns the `lesson-learned` entry in results.
- AC-23: A `lesson-learned` entry and a generic `convention` entry with identical similarity and confidence scores: the `lesson-learned` entry ranks higher in search results by exactly `PROVENANCE_BOOST = 0.02`.
- AC-24: All existing tests pass without modification after schema v5 migration, new write paths, `RetrospectiveReport` type changes, and provenance boost addition.

## Constraints

### Hard Constraints

- **Schema migration pattern**: follow exactly the 3-step process from prior migrations (schema.rs constant bump + `migrate_v4_to_v5()` function + `migrate_if_needed()` call on open). The v4→v5 migration is table-creation-only — no entry scan-and-rewrite.
- **No embedding for auto-outcomes**: auto-generated session outcome entries are written without ONNX embedding (`embedding_dim = 0`). SessionClose must not block on embedding.
- **Backward compatibility**: `from_structured_events()` is additive. Existing `build_report()` JSONL path is unchanged.
- **Zero regression**: all existing tests pass. Existing MCP tools and hook handlers work identically.
- **Edition 2024, MSRV 1.89**: workspace constraints inherited.

### Soft Constraints

- **GC thresholds are named constants**: `TIMED_OUT_THRESHOLD_SECS` (24h) and `DELETE_THRESHOLD_SECS` (30 days) defined in `sessions.rs`. Not user-configurable in v1.
- **INJECTION_LOG scan performance**: full scan + in-process filter is acceptable at current volumes (< 5,000 records). A secondary session→log index is deferred.
- **Auto-outcome entries not embedded**: these entries surface in `context_lookup` by tag/category but not in `context_search` similarity results. Acceptable for structured metadata.
- **`spawn_blocking` for store writes**: all new persistent writes use `spawn_blocking` (consistent with existing server pattern). SessionClose P99 latency budget is soft (< 200ms, same as existing signal processing).

### Dependencies

- **col-009** (hard, must be complete): SIGNAL_QUEUE (schema v4), `drain_and_signal_session()`, `SignalOutput.final_outcome` — required so SessionClose handler knows the resolved outcome before writing SessionRecord
- **col-008** (hard, complete): `increment_compaction()` in SessionRegistry — `total_compactions` available in SessionState at session end
- **col-007** (hard, complete): injection recording pipeline — `record_injection()` call site is where INJECTION_LOG writes are added
- **col-001** (existing): OUTCOME_INDEX table — auto-outcome entries are indexed here if `feature_cycle` is non-empty (existing behavior)
- **col-002** (existing): `RetrospectiveReport`, `ObservationRecord`, metric/hotspot pipeline — extended with `from_structured_events()` entry point

### Downstream Dependents

| Feature | What It Needs from col-010 |
|---------|---------------------------|
| col-011 | SESSIONS table for outcome-correlation routing; INJECTION_LOG for entry performance history |

## Resolved Design Decisions

1. **No `session_id` on `EntryRecord`**: bincode positional encoding requires a full scan-and-rewrite migration. The benefit (tracking which session created an entry) is low priority. Deferred.

2. **Auto-outcome entries have `embedding_dim = 0`**: SessionClose must complete without waiting for ONNX embedding. Auto-outcomes serve as structured metadata (queryable by tag/category lookup), not semantic knowledge entries. The vector similarity pipeline is not needed for session outcome queries.

3. **INJECTION_LOG key is monotonic `u64`**: simplest design. Retrospective does full scan + in-process filter. No secondary index needed at current volumes.

4. **`from_structured_events()` is a new entry point, not a replacement**: the JSONL parser is preserved for historical data. `context_retrospective` tries structured path first, falls back to JSONL.

5. **Session GC lives in the existing `maintain=true` path**: consistent with how coherence gate maintenance runs (crt-005). No new background tasks.

6. **Auto-outcome entry skips MCP validation layer**: written directly via `store.insert_entry()`. The entry is pre-validated (category, type, tags all known at write time). No user input is involved — no security boundary to cross.

7. **`evidence_limit = 3` default** (issue #65): the default of 3 evidence items per hotspot is actionable (representative examples) without flooding the context window. The hotspot type is unchanged — `evidence` is still `Vec<Evidence>`, just truncated server-side. Callers needing full evidence pass `evidence_limit = 0`. This avoids the type-level breaking change that `detail_level: String` would have introduced.

8. **Evidence synthesis is deterministic heuristics only** (issue #65): timestamp clustering (30s window), sequence detection (monotone-increasing values), top-N sorting. No LLM or model inference. Synthesis runs synchronously in `from_structured_events()`.

9. **Recommendation templates cover 4 hotspot types** (issue #65): `permission_retries`, `coordinator_respawns`, `sleep_workarounds`, `compile_cycles`. Other hotspot types produce no recommendation. Template text is hardcoded in `report.rs` — not stored in Unimatrix, not configurable in v1.

10. **Retrospective entries use `lesson-learned` category**: the existing planned category (currently empty in the knowledge base). Topic: `"retrospective/{feature_cycle}"`. Enables future agents to query "what did previous retrospectives surface about X?" and receive semantically relevant findings. The entry is fully embedded (unlike auto-session outcomes) because retrospective narratives are genuine semantic knowledge, not just metadata.

11. **Supersede, not update**: when re-running retrospective for the same feature_cycle, the prior `lesson-learned` entry is marked `Deprecated` (via the existing `supersedes` chain) and a new entry is created. This preserves the correction audit trail and allows confidence evolution to apply to retrospective knowledge entries over time.

12. **Provenance boost is query-time, not stored**: `PROVENANCE_BOOST = 0.02` is applied at the call site in the UDS listener search dispatch — the same place `co_access_affinity` is currently added to the rerank score. It does not disturb the stored weight invariant (`W_BASE + W_USAGE + W_FRESH + W_HELP + W_CORR + W_TRUST = 0.92`). A named constant in `confidence.rs`; no formula logic change.

13. **`trust_source = "system"` is a correctness fix, not a boost mechanism**: entries written by the cortical implant without an explicit trust_source would fall into the `_ => 0.3` arm of `trust_score()` — below agent-written entries. Setting `trust_source = "system"` corrects this and gives a structurally appropriate trust level (0.7) for system-generated knowledge.

## Tracking

- GH Issue: https://github.com/dug-21/unimatrix/issues/76
