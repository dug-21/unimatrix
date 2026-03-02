# Pseudocode: structured-retrospective

Component: Structured Retrospective — from_structured_events() (P1)
Files:
  - `crates/unimatrix-observe/src/structured.rs` (new)
  - `crates/unimatrix-observe/src/types.rs` (additive changes)
  - `crates/unimatrix-observe/src/report.rs` (additive changes)
  - `crates/unimatrix-server/src/tools.rs` (retrospective path selection)

---

## Purpose

Provide a structured data entry point for the retrospective pipeline that reads from SESSIONS + INJECTION_LOG tables instead of parsing JSONL files. Excludes Abandoned and TimedOut sessions. Adds `HotspotNarrative`, `EvidenceCluster`, `Recommendation` types. Updates `RetrospectiveReport` with additive `narratives` and `recommendations` fields.

Note: `unimatrix-observe` has no dependency on `unimatrix-store` (ADR-001 in lib.rs). `from_structured_events` receives pre-loaded data from the caller (tools.rs), not a `Store` reference. The function signature in the architecture shows `store: &Store` but this violates the crate boundary. Resolution: tools.rs loads sessions and injection log records, converts them to observe types, and passes them to `from_structured_events`.

---

## 1. types.rs Changes (additive)

### New Types

```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotspotNarrative {
    pub hotspot_type: String,   // matches HotspotFinding.rule_name
    pub summary: String,         // non-empty, human-readable
    pub clusters: Vec<EvidenceCluster>,
    pub top_files: Vec<(String, u32)>,    // top-5 by mutation count
    pub sequence_pattern: Option<String>, // e.g. "30s->60s->90s->120s"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceCluster {
    pub window_start: u64,   // unix epoch seconds
    pub event_count: u32,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recommendation {
    pub hotspot_type: String,
    pub action: String,
    pub rationale: String,
}
```

### RetrospectiveReport Additive Changes

Add two new fields at the end of the struct:

```
pub struct RetrospectiveReport {
    // ... all existing fields unchanged ...
    pub feature_cycle: String,
    pub session_count: usize,
    pub total_records: usize,
    pub metrics: MetricVector,
    pub hotspots: Vec<HotspotFinding>,
    pub is_cached: bool,
    #[serde(default)]
    pub baseline_comparison: Option<Vec<BaselineComparison>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entries_analysis: Option<Vec<EntryAnalysis>>,

    // NEW (col-010): always present; empty vec when no hotspots
    pub recommendations: Vec<Recommendation>,

    // NEW (col-010): structured-events path only; None for JSONL path
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub narratives: Option<Vec<HotspotNarrative>>,
}
```

### ObservationRecord Additive Changes

```
pub struct ObservationRecord {
    // ... all existing fields unchanged ...
    pub ts: u64,
    pub hook: HookType,
    pub session_id: String,
    pub tool: Option<String>,
    pub input: Option<serde_json::Value>,
    pub response_size: Option<u64>,
    pub response_snippet: Option<String>,

    // NEW (col-010): populated by structured path only; default None
    #[serde(default)]
    pub confidence_at_injection: Option<f64>,
    #[serde(default)]
    pub session_outcome: Option<String>,
}
```

Note: `#[serde(default)]` alone is NOT sufficient for bincode positional encoding. These fields must be at the END of the struct and are not serialized by bincode (ObservationRecord uses JSON serde only for JSONL parsing). Safe to add.

---

## 2. report.rs Changes — recommendations_for_hotspots

```
pub fn recommendations_for_hotspots(hotspots: &[HotspotFinding]) -> Vec<Recommendation>:
    result = Vec::new()
    for hotspot in hotspots:
        match hotspot.rule_name.as_str():
            "permission_retries" =>
                result.push(Recommendation {
                    hotspot_type: "permission_retries".to_string(),
                    action: "Add the tool to the Always Allow list in Claude settings".to_string(),
                    rationale: format!("{:.0} permission retries detected; each adds latency and context overhead", hotspot.measured),
                })
            "coordinator_respawns" =>
                result.push(Recommendation {
                    hotspot_type: "coordinator_respawns".to_string(),
                    action: "Review coordinator memory management; consider increasing compaction budget".to_string(),
                    rationale: format!("{:.0} coordinator respawns detected", hotspot.measured),
                })
            "sleep_workarounds" =>
                result.push(Recommendation {
                    hotspot_type: "sleep_workarounds".to_string(),
                    action: "Replace sleep-based polling with event-driven synchronization or use run_in_background".to_string(),
                    rationale: format!("{:.0} sleep workaround events detected", hotspot.measured),
                })
            "compile_cycles" if hotspot.measured > 10.0 =>
                result.push(Recommendation {
                    hotspot_type: "compile_cycles".to_string(),
                    action: "Split large crates or use incremental compilation; consider pre-building common dependencies".to_string(),
                    rationale: format!("{:.1} compile cycles detected (threshold: 10.0)", hotspot.measured),
                })
            _ =>
                // No recommendation for unrecognized hotspot types
                ()
    result
```

Update `build_report` to accept and include recommendations:

```
pub fn build_report(
    feature_cycle: &str,
    records: &[ObservationRecord],
    metrics: MetricVector,
    hotspots: Vec<HotspotFinding>,
    baseline: Option<Vec<BaselineComparison>>,
    entries_analysis: Option<Vec<EntryAnalysis>>,
) -> RetrospectiveReport:
    // existing logic...
    let recommendations = recommendations_for_hotspots(&hotspots)
    RetrospectiveReport {
        feature_cycle: feature_cycle.to_string(),
        session_count,
        total_records: records.len(),
        metrics,
        hotspots,
        is_cached: false,
        baseline_comparison: baseline,
        entries_analysis,
        recommendations,  // NEW
        narratives: None,  // NEW: None for JSONL path
    }
```

---

## 3. structured.rs (new file)

### Constant

```
pub const CLUSTER_WINDOW_SECS: u64 = 30
```

### Input Types (passed from tools.rs)

```
pub struct StructuredSessionData {
    pub sessions: Vec<SessionRecord>,        // from store.scan_sessions_by_feature()
    pub injection_log: Vec<InjectionLogRecord>, // all for this feature_cycle
}
```

Note: `SessionRecord` and `InjectionLogRecord` from `unimatrix-store` are available to `tools.rs` but not to `unimatrix-observe`. To maintain crate boundary (ADR-001 in lib.rs), tools.rs converts them to an observe-internal type before passing.

Alternative (simpler): define the `from_structured_events` function in `tools.rs` directly, or add a thin wrapper type in `unimatrix-observe` that accepts pre-parsed data. Use the simpler path — define the function in `tools.rs` since it needs store access.

**Resolution**: `from_structured_events` lives in `tools.rs` (server crate), not in `unimatrix-observe`. The architecture doc placed it in `structured.rs` for conceptual clarity but the crate boundary constraint means server code handles it. Create `structured.rs` as a helper module within `unimatrix-observe` that accepts `Vec<ObservationRecord>` (observe's own type) and returns the report components.

### Public API (observe crate — structured.rs)

```
/// Build a RetrospectiveReport from pre-converted observation records.
/// Records should be pre-filtered (Abandoned and TimedOut sessions excluded).
/// session_count: number of qualifying sessions (Completed only).
pub fn from_observation_stream(
    feature_cycle: &str,
    records: &[ObservationRecord],
    session_count: usize,
    baseline: Option<Vec<BaselineComparison>>,
    entries_analysis: Option<Vec<EntryAnalysis>>,
) -> Result<RetrospectiveReport, ObserveError>:
    if records.is_empty() && session_count == 0:
        return Ok(RetrospectiveReport::empty(feature_cycle))

    // Run existing pipeline (same as JSONL path)
    let metrics = compute_metric_vector(records)
    let hotspots = detect_hotspots(records, &default_rules())

    // Layer 2: narrative synthesis (structured path only)
    let narratives = synthesize_narratives(records, &hotspots)
    let recommendations = recommendations_for_hotspots(&hotspots)

    Ok(RetrospectiveReport {
        feature_cycle: feature_cycle.to_string(),
        session_count,
        total_records: records.len(),
        metrics,
        hotspots,
        is_cached: false,
        baseline_comparison: baseline,
        entries_analysis,
        recommendations,
        narratives: Some(narratives),
    })
```

### RetrospectiveReport::empty

```
impl RetrospectiveReport {
    pub fn empty(feature_cycle: &str) -> Self:
        RetrospectiveReport {
            feature_cycle: feature_cycle.to_string(),
            session_count: 0,
            total_records: 0,
            metrics: MetricVector::default(),
            hotspots: Vec::new(),
            is_cached: false,
            baseline_comparison: None,
            entries_analysis: None,
            recommendations: Vec::new(),
            narratives: Some(Vec::new()),  // Some([]) indicates structured path used
        }
}
```

### synthesize_narratives

```
fn synthesize_narratives(
    records: &[ObservationRecord],
    hotspots: &[HotspotFinding],
) -> Vec<HotspotNarrative>:
    result = Vec::new()
    for hotspot in hotspots:
        narrative = build_narrative_for_hotspot(hotspot, records)
        result.push(narrative)
    result

fn build_narrative_for_hotspot(
    hotspot: &HotspotFinding,
    records: &[ObservationRecord],
) -> HotspotNarrative:
    // 1. Timestamp clustering: group evidence by 30s sliding window
    clusters = cluster_evidence_by_window(&hotspot.evidence, CLUSTER_WINDOW_SECS)

    // 2. Top-5 files by mutation count (from evidence detail fields)
    top_files = extract_top_files(&hotspot.evidence, 5)

    // 3. Sequence pattern (sleep_workarounds only)
    sequence_pattern = if hotspot.rule_name == "sleep_workarounds":
        extract_sleep_sequence(&hotspot.evidence)
    else:
        None

    // 4. Summary string
    summary = build_summary(hotspot, &clusters)

    HotspotNarrative {
        hotspot_type: hotspot.rule_name.clone(),
        summary,
        clusters,
        top_files,
        sequence_pattern,
    }
```

### cluster_evidence_by_window

```
fn cluster_evidence_by_window(
    evidence: &[EvidenceRecord],
    window_secs: u64,
) -> Vec<EvidenceCluster>:
    if evidence.is_empty():
        return Vec::new()

    // Sort evidence by timestamp (ts is epoch millis in EvidenceRecord)
    sorted: Vec<&EvidenceRecord> = evidence.iter().sorted_by_key(|e| e.ts).collect()

    clusters = Vec::new()
    current_window_start: u64 = sorted[0].ts / 1000  // convert ms to secs
    current_window_events: Vec<&EvidenceRecord> = Vec::new()

    for ev in sorted:
        ev_secs = ev.ts / 1000
        if ev_secs < current_window_start + window_secs:
            current_window_events.push(ev)
        else:
            // Emit current cluster
            if !current_window_events.is_empty():
                clusters.push(EvidenceCluster {
                    window_start: current_window_start,
                    event_count: current_window_events.len() as u32,
                    description: format!("{} events", current_window_events.len()),
                })
            // Start new window
            current_window_start = ev_secs
            current_window_events = vec![ev]

    // Emit final cluster
    if !current_window_events.is_empty():
        clusters.push(EvidenceCluster {
            window_start: current_window_start,
            event_count: current_window_events.len() as u32,
            description: format!("{} events", current_window_events.len()),
        })

    clusters
```

### extract_sleep_sequence

```
fn extract_sleep_sequence(evidence: &[EvidenceRecord]) -> Option<String>:
    // Extract sleep durations from evidence detail strings
    // Expected format: "sleep NNs" or similar
    durations: Vec<u64> = []
    for ev in evidence:
        if let Some(secs) = parse_sleep_duration_from_detail(&ev.detail):
            durations.push(secs)

    if durations.len() < 2:
        return None

    // Check monotone-increasing
    for window in durations.windows(2):
        if window[0] >= window[1]:
            return None  // Not monotone; no pattern

    // Format: "30s->60s->90s->120s"
    Some(durations.iter().map(|d| format!("{}s", d)).collect::<Vec<_>>().join("->"))

fn parse_sleep_duration_from_detail(detail: &str) -> Option<u64>:
    // Parse patterns like "sleep 30s", "30 seconds", "sleep(30)"
    // Use regex or simple string scan
    // Return None if no recognizable pattern
```

---

## 4. tools.rs — Retrospective Path Selection

In `handle_context_retrospective` (or equivalent):

```
// NEW (col-010): try structured path first
let sessions = store.scan_sessions_by_feature(&feature_cycle)?

let report = if sessions.is_empty():
    // JSONL fallback (OQ-03: only if JSONL directory has files)
    let has_jsonl = check_jsonl_directory_has_files(&obs_dir, &feature_cycle)
    if has_jsonl:
        tracing::debug!(feature_cycle = %feature_cycle, "retrospective: JSONL fallback path")
        build_jsonl_report(...)   // existing path
    else:
        tracing::debug!(feature_cycle = %feature_cycle, "retrospective: no data; returning empty")
        RetrospectiveReport::empty(&feature_cycle)
else:
    tracing::debug!(feature_cycle = %feature_cycle, sessions = %sessions.len(), "retrospective: structured path")
    // Convert SessionRecord+InjectionLog to ObservationRecord stream
    let injection_log = store.scan_injection_log_for_feature(&feature_cycle)?  // new helper
    let (obs_records, qualified_session_count) = convert_to_observation_records(&sessions, &injection_log)
    from_observation_stream(&feature_cycle, &obs_records, qualified_session_count, baseline, entries_analysis)?
```

Note: `scan_injection_log_for_feature` is a convenience wrapper in `sessions.rs` or `injection_log.rs` that:
1. Gets all session_ids for the feature_cycle from SESSIONS.
2. Scans INJECTION_LOG and returns records for those session_ids.

Or: collect session_ids from `sessions`, then call `scan_injection_log_by_session` for each.

### convert_to_observation_records

```
fn convert_to_observation_records(
    sessions: &[SessionRecord],
    injection_log: &[InjectionLogRecord],
) -> (Vec<ObservationRecord>, usize):
    // Exclude Abandoned and TimedOut
    qualified_sessions: Vec<&SessionRecord> = sessions
        .iter()
        .filter(|s| s.status != Abandoned && s.status != TimedOut)
        .collect()
    qualified_count = qualified_sessions.len()

    // Build a lookup: session_id -> session outcome
    outcome_lookup: HashMap<&str, &str> = qualified_sessions
        .iter()
        .map(|s| (s.session_id.as_str(), s.outcome.as_deref().unwrap_or("")))
        .collect()

    // Convert injection log records to ObservationRecord-like structs
    // Each injection becomes a PostToolUse-like record
    records: Vec<ObservationRecord> = []
    for log_record in injection_log:
        if let Some(outcome) = outcome_lookup.get(log_record.session_id.as_str()):
            records.push(ObservationRecord {
                ts: log_record.timestamp * 1000,  // secs to ms
                hook: HookType::PostToolUse,
                session_id: log_record.session_id.clone(),
                tool: Some("ContextSearch".to_string()),
                input: None,
                response_size: None,
                response_snippet: None,
                confidence_at_injection: Some(log_record.confidence),
                session_outcome: Some(outcome.to_string()),
            })

    (records, qualified_count)
```

---

## Key Test Scenarios

1. `from_observation_stream` with 0 sessions → empty RetrospectiveReport (session_count=0, narratives=Some([])).
2. 5 sessions (3 Completed, 1 Abandoned, 1 TimedOut) → session_count=3; Abandoned and TimedOut excluded.
3. Structured path: `narratives` is `Some`; JSONL path: `narratives` is `None`.
4. `cluster_evidence_by_window` with 4 events at ts=0,10,20,35 (CLUSTER_WINDOW_SECS=30) → 2 clusters: [0,10,20] and [35].
5. `extract_sleep_sequence` with [30, 60, 90, 120] → `Some("30s->60s->90s->120s")`.
6. `extract_sleep_sequence` with [30, 60, 50] (non-monotone) → `None`.
7. `recommendations_for_hotspots` with `permission_retries` → Vec with 1 Recommendation with non-empty action.
8. `recommendations_for_hotspots` with `compile_cycles` at measured=8.0 → empty Vec (below threshold).
9. `recommendations_for_hotspots` with `compile_cycles` at measured=12.0 → Vec with 1 Recommendation.
10. Path selection: SESSIONS populated → structured path used (debug log "structured path").
11. Path selection: SESSIONS empty + JSONL exists → JSONL path.
12. Path selection: SESSIONS empty + no JSONL → empty report.
