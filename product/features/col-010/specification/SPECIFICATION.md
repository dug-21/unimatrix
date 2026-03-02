# Specification: col-010 Session Lifecycle Persistence & Structured Retrospective

Feature: col-010
Status: Draft
Author: col-010-agent-2-spec
Date: 2026-03-02
Schema: v4 → v5

---

## Objective

col-010 makes session state durable. It introduces two new redb tables (SESSIONS and INJECTION_LOG, schema v5) that persist the lifecycle and injection events of every session. It upgrades the col-002 retrospective pipeline with a structured data entry point (`from_structured_events()`) that reads directly from the store instead of parsing JSONL files. It adds an `evidence_limit` parameter to `context_retrospective` to cap per-hotspot evidence arrays server-side, reducing the ~87KB default payload while keeping the response immediately actionable. Auto-generated session outcomes close the loop between the hook signal pipeline and the queryable knowledge base. Retrospective findings are written as `lesson-learned` entries with full embeddings, and a provenance boost ensures they surface appropriately in search results.

**Component Priority Split (from SR-02):**
- **P0** (required before col-011): Components 1–5 — storage layer, UDS writes, GC, auto-outcomes, structured retrospective
- **P1** (resolves issue #65, separate PR acceptable): Components 6–7 — evidence_limit + evidence synthesis, lesson-learned persistence + provenance boost

---

## Functional Requirements

### FR-01: Schema v5 Migration

**FR-01.1**: On `Store::open()`, if `schema_version < 5`, run `migrate_v4_to_v5()` within a single write transaction (consistent with the pattern established in prior migrations).

**FR-01.2**: `migrate_v4_to_v5()` MUST: open the SESSIONS table (triggering redb table creation), open the INJECTION_LOG table (triggering redb table creation), write `next_log_id = 0` to COUNTERS **only if the key does not already exist** (check-then-write — SR-05), update `schema_version` to 5.

**FR-01.3**: Migration MUST leave all existing entries, indexes, SIGNAL_QUEUE records, and all prior tables unchanged. No scan-and-rewrite is required (new tables only).

**FR-01.4**: If `schema_version >= 5`, `migrate_if_needed()` MUST return immediately without performing any writes.

**FR-01.5**: `CURRENT_SCHEMA_VERSION` constant MUST be 5 after this feature ships.

**FR-01.6**: The migration is idempotent under repeated calls. If `next_log_id` already exists in COUNTERS (partial migration recovery), the counter write MUST be skipped. Table creation is idempotent via redb's open semantics.

**Addresses**: SR-05, AC-01

---

### FR-02: SESSIONS Table — Schema and Records

**FR-02.1**: The SESSIONS table key type MUST be `&str` (session_id) and value type MUST be `&[u8]` (bincode-serialized `SessionRecord`):
```
SESSIONS: TableDefinition<&str, &[u8]>
```

**FR-02.2**: `SessionRecord` MUST have these fields:
- `session_id: String` — hook-provided session identifier
- `feature_cycle: Option<String>` — from `SessionRegister.feature`; `None` if not provided
- `agent_role: Option<String>` — from `SessionRegister.agent_role`; `None` if not provided
- `started_at: u64` — Unix epoch seconds at registration
- `ended_at: Option<u64>` — Unix epoch seconds at close; `None` if still active
- `status: SessionLifecycleStatus` — lifecycle state
- `compaction_count: u32` — number of compaction events during session
- `outcome: Option<String>` — `"success"` | `"rework"` | `"abandoned"` | `None`
- `total_injections: u32` — count of injected entries across the session

**FR-02.3**: `SessionLifecycleStatus` MUST have four variants: `Active`, `Completed`, `TimedOut`, `Abandoned` (SR-06 — distinct status for abandoned sessions, not conflated with Completed).

**FR-02.4**: `Store::insert_session(record: &SessionRecord)` MUST serialize the record to bincode and write to SESSIONS with the session_id as key. Uses a write transaction.

**FR-02.5**: `Store::update_session(session_id: &str, updater: impl FnOnce(&mut SessionRecord))` MUST read the existing record, apply the updater closure, and write the updated record back — all within a single write transaction. If the record does not exist, return `StoreError::NotFound`.

**FR-02.6**: `Store::get_session(session_id: &str) -> Result<Option<SessionRecord>>` MUST return `None` if the key is absent.

**FR-02.7**: `Store::scan_sessions_by_feature(feature_cycle: &str) -> Result<Vec<SessionRecord>>` MUST perform a full scan of SESSIONS and return all records where `session.feature_cycle == Some(feature_cycle)`. The scan MUST support an optional `status` filter: `scan_sessions_by_feature_with_status(feature_cycle, status: Option<SessionLifecycleStatus>)` for use in `from_structured_events()` to exclude `Abandoned` sessions. (SR-06)

**Addresses**: AC-01, AC-02, AC-12, SR-06

---

### FR-03: INJECTION_LOG Table — Schema and Records

**FR-03.1**: The INJECTION_LOG table key type MUST be `u64` (monotonic log_id) and value type MUST be `&[u8]` (bincode-serialized `InjectionLogRecord`):
```
INJECTION_LOG: TableDefinition<u64, &[u8]>
```

**FR-03.2**: `InjectionLogRecord` MUST have these fields:
- `log_id: u64` — monotonically assigned from `next_log_id` counter
- `session_id: String` — session that triggered the injection
- `entry_id: u64` — the injected entry's ID
- `confidence: f64` — entry's confidence score at injection time
- `timestamp: u64` — Unix epoch seconds at injection

**FR-03.3**: `Store::insert_injection_log_batch(records: &[InjectionLogRecord]) -> Result<()>` MUST allocate sequential `log_id` values from `next_log_id` in COUNTERS and write all records in a **single write transaction** (SR-12 — batch per ContextSearch response to minimize counter contention). Returns the updated counter after the batch.

**FR-03.4**: `Store::scan_injection_log_by_session(session_id: &str) -> Result<Vec<InjectionLogRecord>>` MUST perform a full scan of INJECTION_LOG and return records where `record.session_id == session_id`. This is an in-process filter scan (no secondary index). At expected volumes (<5,000 records/day), this is acceptable.

**Addresses**: AC-06, AC-07, SR-12

---

### FR-04: Session GC

**FR-04.1**: `Store::gc_sessions() -> Result<GcStats>` MUST perform the following in two phases within the same write transaction:

1. **TimedOut sweep**: For all SESSIONS records where `status == Active` and `started_at < (now - TIMED_OUT_THRESHOLD_SECS)`, set `status = TimedOut`.
2. **Delete sweep**: For all SESSIONS records where `started_at < (now - DELETE_THRESHOLD_SECS)`, delete the session record AND delete all INJECTION_LOG records whose `session_id` matches the deleted session (cascade delete — SR-04).

**FR-04.2**: Constants (named, in `sessions.rs`, not user-configurable in v1):
- `TIMED_OUT_THRESHOLD_SECS: u64 = 24 * 3600` (24 hours)
- `DELETE_THRESHOLD_SECS: u64 = 30 * 24 * 3600` (30 days)

**FR-04.3**: The cascade INJECTION_LOG deletion MUST occur in the same write transaction as session deletion. The approach: collect all session_ids to delete → full scan INJECTION_LOG filtering by session_id membership → delete matched INJECTION_LOG records → delete SESSIONS records.

**FR-04.4**: `GcStats` MUST return `timed_out_count: u32`, `deleted_session_count: u32`, `deleted_injection_log_count: u32` for observability.

**FR-04.5**: `gc_sessions()` is called from the `maintain=true` path in `context_status` tool handler — consistent with the existing maintenance sweep pattern (crt-005).

**Addresses**: AC-08, AC-09, SR-04

---

### FR-05: UDS Listener — SessionRegister Persistent Write

**FR-05.1**: On `HookRequest::SessionRegister { session_id, cwd, agent_role, feature }` dispatch, **after** the existing `registry.register_session()` call, write a `SessionRecord` via `spawn_blocking`:
```
SessionRecord {
    session_id: <sanitized session_id>,
    feature_cycle: feature,
    agent_role: agent_role,
    started_at: unix_now(),
    ended_at: None,
    status: SessionLifecycleStatus::Active,
    compaction_count: 0,
    outcome: None,
    total_injections: 0,
}
```

**FR-05.2**: `session_id` MUST be sanitized before writing: restrict to alphanumeric characters plus `-` and `_`. Characters outside this set MUST be rejected with a logged warning and the SessionRegister request returned with an error. (SR-11)

**FR-05.3**: The store write is fire-and-forget (`spawn_blocking` with `Arc<Store>` clone). SessionRegister MUST NOT wait for the write to complete before returning the response to the hook caller. Write failures are logged but do not fail the SessionRegister response.

**Addresses**: AC-02, SR-11

---

### FR-06: UDS Listener — SessionClose Persistent Write

**FR-06.1**: On `HookRequest::SessionClose { session_id, outcome }` dispatch, **after** the existing `drain_and_signal_session()` call and signal processing, update the SESSIONS record via `spawn_blocking`:

**FR-06.2**: If `drain_and_signal_session()` returns `Some(signal_output)`:
- Resolve `final_outcome` from `signal_output.final_outcome` (SessionOutcome enum)
- Determine `outcome_str`:
  - `SessionOutcome::Success` → `"success"`
  - `SessionOutcome::Rework` → `"rework"`
  - `SessionOutcome::Abandoned` → `"abandoned"`
- Determine `final_status`:
  - If `outcome == "abandoned"` → `SessionLifecycleStatus::Abandoned`
  - Otherwise → `SessionLifecycleStatus::Completed`
- Update SESSIONS record: `status = final_status`, `ended_at = unix_now()`, `outcome = outcome_str`, `total_injections = injection_count_from_signal_output`

**FR-06.3**: If `drain_and_signal_session()` returns `None` (session not registered): log a warning and skip the SESSIONS update.

**FR-06.4**: For sessions where `final_outcome` is `Success` or `Rework` AND `total_injections > 0`: trigger auto-outcome entry creation (see FR-08). Abandoned sessions MUST NOT produce auto-outcome entries.

**Addresses**: AC-03, AC-04, AC-05

---

### FR-07: UDS Listener — ContextSearch Injection Log

**FR-07.1**: On every successful `HookRequest::ContextSearch` response that injects one or more entries, after the existing `record_injection()` call, write a batch of `InjectionLogRecord` entries via `spawn_blocking` — one record per injected entry.

**FR-07.2**: The batch write MUST use `Store::insert_injection_log_batch()` (single transaction for all injected entries from one ContextSearch response). (SR-12)

**FR-07.3**: Each `InjectionLogRecord` in the batch MUST have:
- `log_id`: allocated sequentially from counter (assigned by `insert_injection_log_batch`)
- `session_id`: from the ContextSearch request's session_id
- `entry_id`: the injected entry's ID
- `confidence`: the entry's confidence score at injection time (the reranked score, not the stored confidence)
- `timestamp`: current Unix epoch seconds

**FR-07.4**: INJECTION_LOG writes are fire-and-forget. ContextSearch response to the hook MUST NOT wait for the log write to complete.

**FR-07.5**: If the session_id in a ContextSearch request is absent or not registered, INJECTION_LOG writes MUST still proceed (the injection happened; the session record may have been missed due to ordering). Log a warning for the missing session.

**Addresses**: AC-06, AC-14

---

### FR-08: Auto-Generated Session Outcomes

**FR-08.1**: `VALID_TYPES` in `crates/unimatrix-server/src/outcome_tags.rs` MUST include `"session"`.

**FR-08.2**: `validate_outcome_tags(["type:session"])` MUST return `Ok(())`.

**FR-08.3**: On SessionClose with `final_outcome ∈ {Success, Rework}` AND `total_injections > 0`, write an `EntryRecord` via `spawn_blocking` to ENTRIES with these fields:
- `title`: `"Session outcome: {session_id}"`
- `content`: `"Session {session_id} completed with outcome: {outcome_str}. Feature cycle: {feature_cycle_or_unknown}. Injected {n} entries."`
- `topic`: `"session-outcomes"`
- `category`: `"outcome"`
- `tags`:
  - `"type:session"`
  - `"result:pass"` (for Success) or `"result:rework"` (for Rework)
- `source`: `"hook"`
- `created_by`: `"cortical-implant"`
- `feature_cycle`: session's feature_cycle (empty string if None)
- `trust_source`: `"system"`
- `embedding_dim`: `0` (no ONNX embedding — SessionClose MUST NOT block on embedding)

**FR-08.4**: Before writing, apply minimum validation without going through the MCP layer (SR-11):
- Verify `category == "outcome"` is in the category allowlist (CategoryAllowlist check)
- Verify `"type:session"` passes `validate_outcome_tags()`
- `session_id` is already sanitized at FR-05.2 (alphanumeric + `-_`)

**FR-08.5**: Auto-outcome entries with `embedding_dim = 0` are NOT added to VECTOR_MAP and NOT indexed in the HNSW vector index. They are retrievable only via `context_lookup(category: "outcome", tags: ["type:session"])`.

**FR-08.6**: Auto-outcome entry write is fire-and-forget (`spawn_blocking`). Failures are logged but MUST NOT fail the SessionClose response.

**Addresses**: AC-10, AC-11, SR-11

---

### FR-09: Structured Retrospective (`from_structured_events()`)

**FR-09.1**: Add `crates/unimatrix-observe/src/structured.rs` containing:
```rust
pub fn from_structured_events(
    store: &Store,
    feature_cycle: &str,
) -> Result<RetrospectiveReport, ObserveError>
```

**FR-09.2**: `from_structured_events()` MUST:
1. Call `store.scan_sessions_by_feature(feature_cycle)` to get all sessions for the cycle.
2. Filter out sessions with `status == Abandoned` before metric computation. (SR-06)
3. For each non-abandoned session, call `store.scan_injection_log_by_session(session_id)` to get injection records.
4. Construct `ObservationRecord` instances from the injection log records, adding `confidence_at_injection` and `outcome` fields (with `#[serde(default)]` for backward compat with JSONL-based records).
5. Run the same `compute_metrics()` and `detect_hotspots()` pipeline as the JSONL path.
6. Return a `RetrospectiveReport` with `session_count` equal to the count of non-abandoned sessions.

**FR-09.3**: If `store.scan_sessions_by_feature(feature_cycle)` returns an empty list, return `RetrospectiveReport::empty(feature_cycle)` (same as if no data existed).

**FR-09.4**: `ObservationRecord` MUST gain two optional fields with `#[serde(default)]`:
- `confidence_at_injection: Option<f64>` — populated from `InjectionLogRecord.confidence`; `None` in JSONL-path records
- `session_outcome: Option<String>` — populated from `SessionRecord.outcome`; `None` in JSONL-path records

**FR-09.5**: The `context_retrospective` MCP tool handler MUST:
1. Try `from_structured_events(store, feature_cycle)` first.
2. If SESSIONS has no records for the feature_cycle (empty result), fall back to the existing JSONL `build_report()` path.
3. Log which path was used (`tracing::debug!`).

**FR-09.6**: The JSONL `build_report()` function MUST remain unchanged and fully functional as a fallback.

**Addresses**: AC-12, AC-13, AC-14, SR-06

---

### FR-10: Evidence-Limited Retrospective Output (P1)

**FR-10.1**: `context_retrospective` MCP tool MUST gain an `evidence_limit` parameter (usize, optional, default `3`):
- `evidence_limit = N` (N > 0): each `HotspotFinding.evidence` array is truncated to at most N items before serialization. The full `hotspots: Vec<HotspotFinding>` type is unchanged.
- `evidence_limit = 0`: no truncation; all evidence items are returned (backward-compatible with pre-col-010 callers).
- Default `evidence_limit = 3`: returns at most 3 representative examples per hotspot alongside `narratives`, keeping total payload ≤10KB for a 13-hotspot report while remaining immediately actionable.

**FR-10.2**: `RetrospectiveReport` retains its existing structure. The `hotspots: Vec<HotspotFinding>` field is unchanged in type. Truncation is applied server-side at serialization time based on `evidence_limit`; the in-memory report is not modified.
- Layer 1 (always populated): `feature_cycle`, `session_count`, `total_records`, `metrics`, `hotspots: Vec<HotspotFinding>`, `recommendations: Vec<Recommendation>`, `is_cached`, `baseline_comparison`, `entries_analysis`
- Layer 2 (structured-events path only): `narratives: Option<Vec<HotspotNarrative>>`

**FR-10.3**: The truncation MUST be applied per-hotspot independently. If `evidence_limit = 3` and a hotspot has 10 evidence items, only the first 3 are included in the response. The `evidence_count` field (if present on the response type) MUST reflect the original count, not the truncated count.

**FR-10.4**: `HotspotNarrative` MUST contain: `hotspot_type: String`, `summary: String` (non-empty, human-readable synthesized description), `clusters: Vec<EvidenceCluster>` (timestamp-clustered events), `top_files: Vec<(String, u32)>` (top-5 by mutation count for file_breadth hotspots), `sequence_pattern: Option<String>` (e.g., `"30s→60s→90s→120s"` for sleep_workarounds).

**FR-10.5**: `Recommendation` MUST contain: `hotspot_type: String`, `action: String` (non-empty), `rationale: String`.

**FR-10.6**: Recommendation templates MUST cover these four hotspot types in `report.rs`:
- `"permission_retries"`: action = "Add common build/test commands to settings.json allowlist"
- `"coordinator_respawns"`: action = "Review coordinator agent lifespan and handoff patterns"
- `"sleep_workarounds"`: action = "Use run_in_background + TaskOutput instead of sleep polling"
- `"compile_cycles"` (only when `measured > 10.0`): action = "Consider incremental compilation or targeted cargo test invocations"
- All other hotspot types: no recommendation produced

**FR-10.7**: Evidence synthesis in `from_structured_events()` MUST implement:
- **Timestamp clustering**: group events within a 30-second sliding window; report as `"N events within 30s at ts=X"`. Window size defined as `CLUSTER_WINDOW_SECS: u64 = 30` (named constant).
- **Sequence extraction**: detect monotone-increasing numeric values in sequential sleep workaround events; format as `"Ns→Ns→..."`. Returns `None` if no monotone sequence found.
- **Top-N file lists**: sort file_breadth evidence by mutation count, return top 5 with `"... and N more"` suffix when truncated.
- **Entry performance correlation**: for each entry in `entries_analysis`, add `injection_success_rate = successful_session_count / total_session_count` using data from SESSIONS.

**FR-10.8**: SR-03 compliance: audit existing integration tests that assert on `context_retrospective` output format **before** implementing evidence_limit. Tests that assert exact evidence array lengths MUST be updated to either pass `evidence_limit = 0` (to restore full arrays) or update their expected count to ≤ 3 items. The `evidence_limit = 0` mode MUST produce output identical to the pre-col-010 format for those callers.

**Addresses**: AC-15, AC-16, AC-17, AC-18, AC-19, SR-03

---

### FR-11: Lesson-Learned Auto-Persistence (P1)

**FR-11.1**: After `from_structured_events()` or the JSONL `build_report()` path completes with `hotspots.len() > 0 OR recommendations.len() > 0`, automatically write a `lesson-learned` entry to Unimatrix.

**FR-11.2**: The lesson-learned entry MUST be written with **full ONNX embedding** (via the existing embed pipeline). The embedding is required because retrospective narratives are semantic knowledge, not structured metadata. (Contrast with auto-outcome entries at FR-08 which have `embedding_dim = 0`.)

**FR-11.3**: To avoid blocking the `context_retrospective` response on ONNX embedding, the lesson-learned entry write MUST be fire-and-forget via `spawn_blocking`. The `context_retrospective` tool returns its report to the caller before embedding completes.

**FR-11.4**: The lesson-learned entry fields:
- `title`: `"Retrospective findings: {feature_cycle}"`
- `content`: Layer 2 narrative output — hotspot summaries + recommendations. If structured-events path was used, include `HotspotNarrative.summary` strings. If JSONL fallback was used, include the hotspot claims only (Layer 1 content, no narrative synthesis).
- `topic`: `"retrospective/{feature_cycle}"`
- `category`: `"lesson-learned"`
- `tags`: `["feature_cycle:{feature_cycle}", "hotspot_count:{n}", "source:retrospective"]`
- `created_by`: `"cortical-implant"`
- `trust_source`: `"system"` (correctness fix: ensures 0.7 trust score vs 0.3 for unknown source — SR-13)
- `feature_cycle`: the feature cycle string

**FR-11.5**: De-duplication by feature_cycle (AC-21): before writing, query for an existing active `lesson-learned` entry with `topic == "retrospective/{feature_cycle}"`. If found:
1. Deprecate the existing entry by calling the supersede pathway (`store.deprecate_and_supersede()`) — sets `superseded_by` on old entry.
2. Write new entry with `supersedes = old_entry_id`.

**FR-11.6**: The supersede check-and-write is NOT atomic under concurrent calls (SR-09 known limitation). Under concurrent `context_retrospective` calls for the same feature_cycle, two active lesson-learned entries may briefly exist. This is tolerated: the extra entry will be superseded on the next retrospective call. Document in implementation as a known limitation.

**FR-11.7**: Verify `"lesson-learned"` is in the active CategoryAllowlist before writing. If not present, log an error and skip (do not fail the retrospective call).

**Addresses**: AC-20, AC-21, AC-22, SR-07, SR-09, SR-13, SR-14

---

### FR-12: Provenance Boost for `lesson-learned` Entries (P1)

**FR-12.1**: Add `PROVENANCE_BOOST: f64 = 0.02` as a named constant in `crates/unimatrix-engine/src/confidence.rs`.

**FR-12.2**: At search result re-ranking time (in the UDS listener's ContextSearch dispatch and in tools.rs search handler), apply `PROVENANCE_BOOST` additively to the rerank score for any entry where `entry.category == "lesson-learned"`:
```
final_score = 0.85 * similarity + 0.15 * confidence + co_access_affinity + provenance_boost
```
Where `provenance_boost = PROVENANCE_BOOST` if `category == "lesson-learned"`, else `0.0`.

**FR-12.3**: `PROVENANCE_BOOST` MUST NOT modify stored `confidence` values. It is applied only at query-time re-ranking. The stored confidence weight invariant (`W_BASE + W_USAGE + W_FRESH + W_HELP + W_CORR + W_TRUST = 0.92`) is unchanged.

**FR-12.4**: Two entries with identical `similarity` and `confidence` scores where one has `category == "lesson-learned"` and the other does not: the lesson-learned entry MUST rank higher by exactly `PROVENANCE_BOOST = 0.02`.

**Addresses**: AC-23

---

## Non-Functional Requirements

### NFR-01: Performance

**NFR-01.1**: All SESSIONS and INJECTION_LOG writes in the UDS listener (SessionRegister, SessionClose, ContextSearch) MUST use `spawn_blocking` with `Arc<Store>` clone — consistent with the existing server pattern for store writes.

**NFR-01.2**: SessionClose P99 latency budget: ≤200ms (consistent with existing signal processing). The additional SESSIONS update and auto-outcome write are fire-and-forget and do not add to this budget.

**NFR-01.3**: ContextSearch injection latency: INJECTION_LOG batch write is fire-and-forget and adds zero latency to the ContextSearch response path.

**NFR-01.4**: `scan_injection_log_by_session()` full scan performance: acceptable at current volumes (<5,000 records/day, <150,000 records total at 30-day retention). A secondary index is deferred.

**NFR-01.5**: ONNX embedding in `context_retrospective` lesson-learned write: fire-and-forget. The tool MUST return its report within the existing latency envelope (no synchronous embedding).

### NFR-02: Backward Compatibility

**NFR-02.1**: The `build_report()` JSONL path in `unimatrix-observe` MUST remain unchanged and functional.

**NFR-02.2**: `context_retrospective` callers that do not pass `evidence_limit` receive the default of 3 evidence items per hotspot. Callers requiring the pre-col-010 full evidence arrays MUST pass `evidence_limit = 0`.

**NFR-02.3**: All existing tests MUST pass without modification (AC-24). `RetrospectiveReport` type is unchanged; `#[serde(default, skip_serializing_if = ...)]` applies only to the additive `narratives` field.

**NFR-02.4**: No breaking changes to any existing MCP tool signatures (tools continue to accept all prior parameters; `evidence_limit` is additive).

### NFR-03: Reliability

**NFR-03.1**: A server restart (clearing in-memory `SessionRegistry`) MUST NOT lose SESSIONS or INJECTION_LOG data from prior sessions.

**NFR-03.2**: All fire-and-forget `spawn_blocking` write failures MUST be logged at `tracing::warn!` level with the session_id and error context. They MUST NOT propagate errors to hook callers.

**NFR-03.3**: All redb write transactions for SESSIONS and INJECTION_LOG MUST be committed before the `spawn_blocking` task completes.

---

## Security Requirements

### SEC-01: Input Sanitization for session_id

**SEC-01.1**: `session_id` values received from hook callers MUST be validated before any database write. Only alphanumeric characters plus `-` and `_` are permitted. Maximum length: 128 characters.

**SEC-01.2**: A `session_id` failing validation at `SessionRegister` MUST cause the request to return an error response. The invalid session_id MUST NOT be written to SESSIONS.

**SEC-01.3**: `session_id` values written to INJECTION_LOG inherit the validated value from the in-memory session registry. No re-validation required at injection write time.

**Addresses**: SR-11

### SEC-02: Auto-Outcome Entry Pre-Validation

**SEC-02.1**: Before writing auto-outcome entries (FR-08), apply:
1. CategoryAllowlist check: `"outcome"` MUST be in the allowlist.
2. Tag validation: `validate_outcome_tags(&["type:session", "result:..."])` MUST return `Ok(())`.
3. `session_id` sanitization: inherited from SEC-01.

**SEC-02.2**: Auto-outcome entry content (title, content) MUST be sanitized: `session_id` and `feature_cycle` are validated at their respective write points. Agent-supplied values are not directly interpolated into content.

**Addresses**: SR-11

### SEC-03: trust_source for System-Generated Entries

**SEC-03.1**: All entries written by the cortical implant hook pipeline (auto-outcomes, lesson-learned) MUST set `trust_source = "system"`. This gives these entries a trust score of 0.7 in the scoring pipeline, which is the structurally correct level for system-generated knowledge (not `0.3` from the wildcard arm).

**SEC-03.2**: This is a correctness fix. It does NOT constitute a gaming mechanism — the `0.02` provenance boost (FR-12) is the signal-differentiation mechanism for lesson-learned entries.

**Addresses**: SR-13

---

## Acceptance Criteria Verification Map

Each AC from SCOPE.md is mapped to its implementing requirements and verification method.

| AC | Statement | Implementing FR | Verification Method |
|----|-----------|-----------------|---------------------|
| AC-01 | Schema v5 migration runs on Store::open() when schema version is 4 | FR-01.1–01.5 | Integration test: open v4 store, verify SESSIONS + INJECTION_LOG tables exist, next_log_id=0 in COUNTERS, schema_version=5, all prior entries intact |
| AC-02 | SessionRegister writes Active SessionRecord with started_at | FR-05.1, FR-02.4, FR-02.6 | Integration test: send SessionRegister, get_session() returns Active record with correct started_at |
| AC-03 | SessionClose Success updates to Completed/success | FR-06.1–06.2 | Integration test: register → inject entries → close with Success → verify Completed/success/total_injections |
| AC-04 | SessionClose Rework updates to Completed/rework | FR-06.2 | Integration test: register → signal rework → close → verify Completed/rework |
| AC-05 | SessionClose Abandoned writes Completed/abandoned, no auto-outcome | FR-06.2–06.4, FR-08 | Integration test: close with Abandoned → verify status=Abandoned, no outcome entry in ENTRIES |
| AC-06 | ContextSearch writes InjectionLogRecord per injected entry | FR-07.1–07.3 | Integration test: inject 3 entries → scan_injection_log_by_session → verify 3 records with correct fields |
| AC-07 | scan_injection_log_by_session returns exactly records for that session | FR-03.4 | Unit test: insert records for session-A and session-B → scan for session-A → returns only session-A records |
| AC-08 | Maintenance marks Active sessions >24h as TimedOut | FR-04.1–04.2 | Integration test: insert Active session with started_at 25h ago → gc_sessions() → status=TimedOut, not deleted |
| AC-09 | Maintenance deletes sessions >30 days | FR-04.1–04.3 | Integration test: insert session started_at 31 days ago → gc_sessions() → session deleted from SESSIONS |
| AC-10 | VALID_TYPES includes "session", validate_outcome_tags passes | FR-08.1–08.2 | Unit test: validate_outcome_tags(&["type:session"]) returns Ok(()) |
| AC-11 | Auto-outcome written on Success/Rework with injections; embedding_dim=0 | FR-08.3–08.5 | Integration test: close Success session with 3 injections → context_lookup(category:outcome, tags:type:session) returns entry with embedding_dim=0 |
| AC-12 | from_structured_events returns session_count matching SESSIONS | FR-09.1–09.2 | Integration test: 5 sessions for feature_cycle → from_structured_events → report.session_count=5 |
| AC-13 | context_retrospective prefers structured path; falls back to JSONL | FR-09.5–09.6 | Integration test: (a) with SESSIONS data → structured path used; (b) without SESSIONS data → JSONL path used |
| AC-14 | Server restart does not lose SESSIONS or INJECTION_LOG | FR-05.3, FR-07.4, NFR-03.1 | Integration test: write sessions → drop store → reopen store → get_session() returns records |
| AC-15 | evidence_limit=3 (default) returns ≤3 evidence items per hotspot; total payload ≤10KB for a 13-hotspot report | FR-10.1–10.3 | Integration test: synthetic 13-hotspot report → default evidence_limit response ≤10240 bytes; each hotspot.evidence.len() ≤ 3 |
| AC-16 | evidence_limit=0 returns complete evidence arrays, backward-compatible with pre-col-010 output | FR-10.1, FR-10.8 | Integration test: evidence_limit=0 output matches pre-col-010 format field-by-field; existing tests pass when updated to pass evidence_limit=0 |
| AC-17 | evidence_limit=3 (default) returns narratives alongside capped evidence when the structured-events path is used | FR-10.4, FR-10.8 | Integration test: structured-events path + default evidence_limit → report.narratives is Some, each hotspot.evidence.len() ≤ 3 |
| AC-18 | sleep_workarounds with escalating intervals → sequence_pattern contains sequence | FR-10.7 | Unit test: 4 sleep events at 30s/60s/90s/120s → sequence_pattern = "30s→60s→90s→120s" |
| AC-19 | permission_retries hotspot → Recommendation with non-empty action | FR-10.6 | Unit test: report with permission_retries hotspot → recommendations[0].action is non-empty string; report with no recognized hotspots → empty recommendations |
| AC-20 | After context_retrospective with ≥1 finding, lesson-learned entry exists | FR-11.1–11.4 | Integration test: retrospective with hotspots → wait for background embed → context_lookup(category:lesson-learned) returns entry with trust_source=system, embedding_dim>0 |
| AC-21 | Second context_retrospective call supersedes first lesson-learned | FR-11.5 | Integration test: run retrospective twice → exactly one active lesson-learned entry, prior entry deprecated with superseded_by set |
| AC-22 | context_search returns lesson-learned entry for related query | FR-11.2, FR-12.2 | Integration test: write lesson-learned with "permission retry" content → context_search("permission retry patterns") includes lesson-learned in results |
| AC-23 | lesson-learned ranks higher than equal-confidence entry by PROVENANCE_BOOST | FR-12.2, FR-12.4 | Unit test: identical similarity+confidence, one entry category=lesson-learned → lesson-learned score = other score + 0.02 |
| AC-24 | All existing tests pass after changes | NFR-02.2–02.3, NFR-02.4 | CI run with full test suite: all prior unit + integration tests pass |

---

## Component Breakdown and Ownership

### P0 Components (required before col-011)

| # | Component | New Files | Modified Files |
|---|-----------|-----------|----------------|
| 1 | Storage Layer (SESSIONS + INJECTION_LOG tables, schema v5) | `crates/unimatrix-store/src/sessions.rs`, `crates/unimatrix-store/src/injection_log.rs` | `schema.rs` (2 table defs + version bump), `store migration logic` |
| 2 | UDS Listener Writes (SessionRegister, SessionClose, ContextSearch) | — | `crates/unimatrix-server/src/uds_listener.rs` |
| 3 | Session GC in Maintenance Sweep | — | `crates/unimatrix-store/src/sessions.rs` (gc_sessions), `crates/unimatrix-server/src/tools.rs` (maintain=true path) |
| 4 | Auto-Generated Session Outcomes (col-001 integration) | — | `crates/unimatrix-server/src/outcome_tags.rs`, `crates/unimatrix-server/src/uds_listener.rs` |
| 5 | Structured Retrospective (`from_structured_events()`) | `crates/unimatrix-observe/src/structured.rs` | `crates/unimatrix-observe/src/types.rs` (ObservationRecord extensions), `crates/unimatrix-server/src/tools.rs` (context_retrospective path selection) |

### P1 Components (resolve issue #65, after P0 merged)

| # | Component | New Files | Modified Files |
|---|-----------|-----------|----------------|
| 6 | Evidence-Limited Output + Evidence Synthesis | — | `crates/unimatrix-observe/src/types.rs` (HotspotNarrative, Recommendation additive types), `crates/unimatrix-observe/src/report.rs` (recommendation templates), `crates/unimatrix-observe/src/structured.rs` (evidence synthesis), `crates/unimatrix-server/src/tools.rs` (evidence_limit param + server-side truncation), `crates/unimatrix-server/src/wire.rs` (evidence_limit in request) |
| 7 | Lesson-Learned + Provenance Boost | — | `crates/unimatrix-server/src/tools.rs` (auto-persist lesson-learned), `crates/unimatrix-engine/src/confidence.rs` (PROVENANCE_BOOST), `uds_listener.rs` (apply boost in rerank) |

---

## Open Questions

### OQ-01: `total_injections` Source of Truth

At `SessionClose`, the `total_injections` count should reflect injections from INJECTION_LOG (durable) rather than the in-memory `injection_history.len()`. However, the INJECTION_LOG writes are fire-and-forget — there is a race where the SESSIONS update completes before all INJECTION_LOG batch writes finish.

**Recommendation**: Use the in-memory `signal_output.injection_count` (from the existing `SessionRegistry` state) for the initial `total_injections` value in the SESSIONS record. Accept that this may under-count injections if some INJECTION_LOG writes are still in-flight. A subsequent reconciliation sweep (if needed) can read INJECTION_LOG and update the count. For col-010, the in-memory count is the practical source of truth.

### OQ-02: `compaction_count` Propagation

`SessionRecord.compaction_count` is set at registration to 0. The SCOPE.md mentions `col-008` providing `increment_compaction()` in `SessionRegistry`. The specification requires that `update_session()` increments this field when a `CompactPayload` is processed. Confirm: should the SESSIONS record be updated on every `CompactPayload` dispatch, or only at `SessionClose`?

**Recommendation**: Update SESSIONS on `SessionClose` only, reading `session_state.compaction_count` from the in-memory registry at that point. Adding a store write on every `CompactPayload` dispatch adds unnecessary write pressure.

### OQ-03: Retrospective Empty Session Set vs. No SESSIONS Data

AC-13 requires `context_retrospective` to fall back to JSONL when SESSIONS has no data for the feature_cycle. However, a feature cycle that genuinely had zero sessions should return an empty report, not trigger JSONL fallback (which might find old JSONL telemetry from a different context). Clarification needed:

**Recommendation**: Use the JSONL fallback only when `scan_sessions_by_feature()` returns an empty list AND the JSONL observation directory has files for the feature_cycle. If JSONL also returns no data, return an empty report. The structured path is authoritative once col-010 is deployed.

### OQ-04: lesson-learned entry CategoryAllowlist registration

The CategoryAllowlist initial set must include `"lesson-learned"`. Confirm this is registered in the server initialization or config. SR-14 flags this as a verification point.

**Recommendation**: Verify in `crates/unimatrix-server/src/allowlist.rs` that `INITIAL_CATEGORIES` includes `"lesson-learned"`. If not present, add it. This is a required precondition for FR-11.

---

## Risk Addressal Summary

| Risk | Severity | Specification Response |
|------|----------|----------------------|
| SR-01 — col-009 hard dependency | Critical | Not addressed in spec (gate check, col-009 must be merged first) |
| SR-02 — Bundle delivery risk | High | Explicit P0/P1 split at component level; P0 = col-011 required, P1 = issue #65 |
| SR-03 — context_retrospective default change | High | FR-10.8: audit existing tests before implementing evidence_limit; evidence_limit=0 preserves full backward compat |
| SR-04 — INJECTION_LOG orphan records on GC | Medium | FR-04.1, FR-04.3: gc_sessions() cascades deletes to INJECTION_LOG in same transaction |
| SR-05 — Migration counter idempotency | Medium | FR-01.2: check-then-write for next_log_id; write only if key absent |
| SR-06 — Abandoned status modeling | Medium | FR-02.3: Abandoned variant added; FR-09.2: excluded from metric computation; FR-06.2: Abandoned status set on SessionClose |
| SR-07 — ONNX latency in context_retrospective | Medium | FR-11.3: lesson-learned write is fire-and-forget; tool returns report before embedding completes |
| SR-08 — Evidence synthesis fragility | Medium | FR-10.7: synthesis is best-effort; CLUSTER_WINDOW_SECS is a named constant; empty sequence_pattern returns None gracefully |
| SR-09 — Concurrent supersede race | Medium | FR-11.6: documented as known limitation; tolerated edge case |
| SR-10 — Vision doc discrepancy | Low | Out of scope for spec; post-approval: update PRODUCT-VISION.md col-010 row to remove session_id field reference |
| SR-11 — Auto-outcome bypasses MCP validation | Low | SEC-01, SEC-02: session_id sanitization + category allowlist + tag validation applied before write |
| SR-12 — Counter contention on INJECTION_LOG | Low | FR-03.3: batch writes per ContextSearch response (one transaction per response, not per entry) |
| SR-13 — trust_source scoring inconsistency | Low | SEC-03, FR-08.3, FR-11.4: all system-generated entries set trust_source="system" |
| SR-14 — lesson-learned category allowlist | Low | FR-11.7: verify allowlist before write; OQ-04 flags this for implementation verification |

---

## Dependencies

| Dependency | Type | Status | What col-010 Needs |
|------------|------|--------|--------------------|
| col-009 | Hard | Must be merged | `drain_and_signal_session()`, `SignalOutput.final_outcome`, schema v4 SIGNAL_QUEUE |
| col-008 | Hard | Complete | `increment_compaction()`, `SessionState.compaction_count` |
| col-007 | Hard | Complete | `record_injection()` call site in UDS listener |
| col-001 | Existing | Active | OUTCOME_INDEX for auto-outcome feature_cycle indexing |
| col-002 | Existing | Active | `RetrospectiveReport`, `ObservationRecord`, metric/hotspot pipeline |

## Downstream Dependents

| Feature | What It Needs from col-010 |
|---------|---------------------------|
| col-011 | SESSIONS table for outcome-correlation routing; INJECTION_LOG for entry performance history |
