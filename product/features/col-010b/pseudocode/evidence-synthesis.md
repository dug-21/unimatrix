# Component 2: Evidence Synthesis — Pseudocode

## Files Modified/Created

- `crates/unimatrix-observe/src/types.rs` — New types + RetrospectiveReport extension
- `crates/unimatrix-observe/src/synthesis.rs` — NEW file: narrative synthesis logic
- `crates/unimatrix-observe/src/report.rs` — Recommendation templates
- `crates/unimatrix-observe/src/lib.rs` — Re-export synthesis module + new types
- `crates/unimatrix-server/src/tools.rs` — Call synthesis + populate report fields

## 1. New Types (types.rs)

```pseudo
#[derive(Debug, Clone, Serialize, Deserialize)]
struct HotspotNarrative {
    hotspot_type: String,       // matches HotspotFinding.rule_name
    summary: String,            // human-readable, non-empty
    clusters: Vec<EvidenceCluster>,
    top_files: Vec<(String, u32)>,  // max 5
    sequence_pattern: Option<String>,  // monotone sequence for sleep_workarounds
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EvidenceCluster {
    window_start: u64,    // unix epoch millis of first event
    event_count: u32,
    description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Recommendation {
    hotspot_type: String,
    action: String,       // non-empty actionable text
    rationale: String,
}
```

## 2. RetrospectiveReport Extension (types.rs)

Add two fields to `RetrospectiveReport`:

```pseudo
struct RetrospectiveReport {
    // ... existing fields unchanged ...

    #[serde(default, skip_serializing_if = "Option::is_none")]
    narratives: Option<Vec<HotspotNarrative>>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    recommendations: Vec<Recommendation>,
}
```

Update `build_report()` to accept and pass through the new fields:

```pseudo
function build_report(
    feature_cycle, records, metrics, hotspots, baseline, entries_analysis,
    narratives: Option<Vec<HotspotNarrative>>,   // NEW
    recommendations: Vec<Recommendation>,          // NEW
) -> RetrospectiveReport:
    // ... existing logic ...
    return RetrospectiveReport {
        // ... existing fields ...
        narratives,
        recommendations,
    }
```

## 3. Narrative Synthesis (synthesis.rs — NEW)

```pseudo
const CLUSTER_WINDOW_SECS: u64 = 30

function synthesize_narratives(hotspots: &[HotspotFinding]) -> Vec<HotspotNarrative>:
    return hotspots.iter().map(synthesize_one).collect()

function synthesize_one(hotspot: &HotspotFinding) -> HotspotNarrative:
    let clusters = cluster_evidence(&hotspot.evidence)
    let top_files = extract_top_files(&hotspot.evidence, 5)
    let sequence_pattern = extract_sequence_pattern(hotspot)
    let summary = build_summary(hotspot, &clusters, &top_files)
    return HotspotNarrative {
        hotspot_type: hotspot.rule_name.clone(),
        summary,
        clusters,
        top_files,
        sequence_pattern,
    }

function cluster_evidence(evidence: &[EvidenceRecord]) -> Vec<EvidenceCluster>:
    if evidence.is_empty():
        return vec![]

    // Sort by timestamp
    let sorted = evidence sorted by ts ascending
    let mut clusters = vec![]
    let mut current_start = sorted[0].ts
    let mut current_count = 1
    let mut descriptions = vec![sorted[0].description]
    let window_ms = CLUSTER_WINDOW_SECS * 1000

    for event in sorted[1..]:
        if event.ts - current_start <= window_ms:
            current_count += 1
            descriptions.push(event.description)
        else:
            // Finalize current cluster
            clusters.push(EvidenceCluster {
                window_start: current_start,
                event_count: current_count,
                description: format!("{} events: {}", current_count,
                    descriptions.join("; ").truncate(200)),
            })
            // Start new cluster
            current_start = event.ts
            current_count = 1
            descriptions = vec![event.description]

    // Finalize last cluster
    clusters.push(EvidenceCluster {
        window_start: current_start,
        event_count: current_count,
        description: format!("{} events: {}", current_count,
            descriptions.join("; ").truncate(200)),
    })

    return clusters

function extract_sequence_pattern(hotspot: &HotspotFinding) -> Option<String>:
    // Only for sleep_workarounds
    if hotspot.rule_name != "sleep_workarounds":
        return None

    // Extract numeric values from evidence descriptions
    let mut values: Vec<u64> = vec![]
    for ev in &hotspot.evidence:
        // Parse numbers from description (regex: \d+ followed by 's' or seconds)
        for number in extract_numbers_from_description(&ev.description):
            values.push(number)

    if values.len() < 2:
        return None

    // Check strictly monotonically increasing
    for i in 1..values.len():
        if values[i] <= values[i-1]:
            return None

    // Format: "30s->60s->90s->120s"
    return Some(values.iter().map(|v| format!("{}s", v)).join("->"))

function extract_top_files(evidence: &[EvidenceRecord], limit: usize) -> Vec<(String, u32)>:
    let mut file_counts: HashMap<String, u32> = HashMap::new()

    for ev in evidence:
        // Extract file paths from description and detail
        for path in extract_file_paths(&ev.description):
            *file_counts.entry(path).or_default() += 1
        for path in extract_file_paths(&ev.detail):
            *file_counts.entry(path).or_default() += 1

    // Sort by count descending
    let mut sorted: Vec<(String, u32)> = file_counts.into_iter().collect()
    sorted.sort_by(|a, b| b.1.cmp(&a.1))
    sorted.truncate(limit)
    return sorted

function build_summary(
    hotspot: &HotspotFinding,
    clusters: &[EvidenceCluster],
    top_files: &[(String, u32)],
) -> String:
    let mut summary = format!("{}: {}", hotspot.rule_name, hotspot.claim)

    if !clusters.is_empty():
        summary += format!(". {} event cluster(s) detected", clusters.len())

    if !top_files.is_empty():
        let file_names: Vec<&str> = top_files.iter().map(|(f, _)| f.as_str()).collect()
        summary += format!(". Top files: {}", file_names.join(", "))

    return summary
```

## 4. Recommendation Templates (report.rs)

```pseudo
function recommendations_for_hotspots(hotspots: &[HotspotFinding]) -> Vec<Recommendation>:
    return hotspots.iter().filter_map(recommendation_for).collect()

function recommendation_for(hotspot: &HotspotFinding) -> Option<Recommendation>:
    match hotspot.rule_name:
        "permission_retries" => Some(Recommendation {
            hotspot_type: "permission_retries",
            action: "Add common build/test commands to settings.json allowlist",
            rationale: format!("{} permission retries detected -- agents lose time waiting for approval",
                hotspot.measured as u64),
        })
        "coordinator_respawns" => Some(Recommendation {
            hotspot_type: "coordinator_respawns",
            action: "Review coordinator agent lifespan and handoff patterns",
            rationale: format!("{} coordinator respawns detected -- may indicate premature termination or context overflow",
                hotspot.measured as u64),
        })
        "sleep_workarounds" => Some(Recommendation {
            hotspot_type: "sleep_workarounds",
            action: "Use run_in_background + TaskOutput instead of sleep polling",
            rationale: format!("{} sleep workaround events detected -- sleep polling wastes agent time",
                hotspot.measured as u64),
        })
        "compile_cycles" if hotspot.measured > 10.0 => Some(Recommendation {
            hotspot_type: "compile_cycles",
            action: "Consider incremental compilation or targeted cargo test invocations",
            rationale: format!("{:.0} compile cycles detected (threshold: 10) -- consider narrowing test scope",
                hotspot.measured),
        })
        _ => None

```

## 5. Integration Point (tools.rs)

In `context_retrospective`, after building the report (step 10c):

```pseudo
// After step 10c: report = build_report(...)

// [Component 2] Synthesize narratives and recommendations
let recommendations = recommendations_for_hotspots(&report.hotspots);

// Narratives: None on JSONL path, Some on structured path
// The JSONL path is the current path (col-010 P0's from_structured_events()
// would be the structured path). Since the current handler uses the JSONL
// parsing path, narratives = None for now.
//
// When structured path is used: narratives = Some(synthesize_narratives(&report.hotspots))
// JSONL path: narratives = None
let narratives = None;  // JSONL path -- no structured events data yet

// Populate report fields
// These are set via build_report() parameters or post-construction
report.narratives = narratives;
report.recommendations = recommendations;
```

**Path gating logic**: The current `context_retrospective` handler uses JSONL-based
parsing. When/if a structured events path (`from_structured_events()`) is added as
an alternative code path, that path would set `narratives = Some(synthesize_narratives(...))`.
The JSONL path MUST set `narratives = None`.

## 6. lib.rs Re-exports

```pseudo
pub mod synthesis;

// Add to re-exports:
pub use synthesis::synthesize_narratives;
pub use report::recommendations_for_hotspots;
pub use types::{HotspotNarrative, EvidenceCluster, Recommendation};
```
