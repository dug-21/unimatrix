# col-010b Architecture: Retrospective Evidence Synthesis & Lesson-Learned Persistence

**Feature cycle**: col-010b
**Architect**: col-010b-agent-1-architect
**Date**: 2026-03-02
**Prerequisite**: col-010 P0 merged (PR #77, schema v5)

---

## Overview

col-010b delivers four application-logic components that extend the col-002 retrospective pipeline and the search re-ranking path. No schema migration. No new tables. No new crates. All changes are additive modifications to existing files in `unimatrix-observe`, `unimatrix-engine`, and `unimatrix-server`.

### Components

| # | Component | Primary Crate | Key Files |
|---|-----------|--------------|-----------|
| 1 | Evidence-Limited Output + Wire Type | unimatrix-server | `tools.rs`, `tools.rs` (RetrospectiveParams) |
| 2 | Evidence Synthesis (Narrative + Recommendations) | unimatrix-observe | `types.rs`, `report.rs`, `structured.rs` (new) |
| 3 | Lesson-Learned Auto-Persistence | unimatrix-server | `tools.rs` |
| 4 | Provenance Boost | unimatrix-engine, unimatrix-server | `confidence.rs`, `tools.rs`, `uds_listener.rs` |

---

## 1. Evidence-Limited Output

### 1.1 Wire Type Change

`RetrospectiveParams` in `tools.rs` gains one field:

```rust
pub struct RetrospectiveParams {
    pub feature_cycle: String,
    pub agent_id: Option<String>,
    pub evidence_limit: Option<usize>,  // NEW: default 3, 0 = unlimited
}
```

### 1.2 Server-Side Truncation

In the `context_retrospective` tool handler, AFTER building the `RetrospectiveReport` and BEFORE serialization:

```rust
let evidence_limit = params.evidence_limit.unwrap_or(3);
if evidence_limit > 0 {
    // Clone and truncate — never mutate the original report
    let mut truncated_report = report.clone();
    for hotspot in &mut truncated_report.hotspots {
        hotspot.evidence.truncate(evidence_limit);
    }
    return Ok(format_retrospective_report(&truncated_report));
}
```

**Critical design decision (ADR-001)**: Truncation operates on a clone. The in-memory `report` is never modified. This preserves the full evidence for:
- Lesson-learned content generation (Component 3 uses the full report)
- Narrative synthesis (Component 2 uses full evidence arrays)

### 1.3 R-09 Blocking Gate

Before implementing any code in Component 1, the developer MUST:
1. Search all integration tests for `context_retrospective` that assert on `hotspot.evidence.len()` or equivalent
2. Update those tests to pass `evidence_limit: Some(0)` to restore pre-col-010b behavior
3. Verify all tests pass with the updated parameters

This is a pre-implementation audit, not a code change. The current codebase has no tests that assert exact evidence array lengths (verified: detection rules use `evidence.len()` internally but tests do not assert on serialized evidence counts). The audit confirms this and documents the finding.

---

## 2. Evidence Synthesis

### 2.1 New Types (Additive)

In `crates/unimatrix-observe/src/types.rs`:

```rust
/// Synthesized narrative for a hotspot finding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotspotNarrative {
    /// Hotspot type (matches HotspotFinding.rule_name).
    pub hotspot_type: String,
    /// Human-readable summary of the hotspot.
    pub summary: String,
    /// Timestamp-clustered event groups.
    pub clusters: Vec<EvidenceCluster>,
    /// Top files by mutation count (max 5).
    pub top_files: Vec<(String, u32)>,
    /// Monotone sequence pattern for sleep_workarounds (e.g., "30s->60s->90s->120s").
    pub sequence_pattern: Option<String>,
}

/// A cluster of events within a time window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceCluster {
    /// Start of the time window (unix epoch millis).
    pub window_start: u64,
    /// Number of events in this cluster.
    pub event_count: u32,
    /// Human-readable description.
    pub description: String,
}

/// Actionable recommendation derived from hotspot findings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recommendation {
    /// Hotspot type this recommendation addresses.
    pub hotspot_type: String,
    /// Actionable text.
    pub action: String,
    /// Rationale for the recommendation.
    pub rationale: String,
}
```

### 2.2 RetrospectiveReport Extension

```rust
pub struct RetrospectiveReport {
    // Existing fields (unchanged)
    pub feature_cycle: String,
    pub session_count: usize,
    pub total_records: usize,
    pub metrics: MetricVector,
    pub hotspots: Vec<HotspotFinding>,          // TYPE UNCHANGED
    pub is_cached: bool,
    #[serde(default)]
    pub baseline_comparison: Option<Vec<BaselineComparison>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entries_analysis: Option<Vec<EntryAnalysis>>,

    // NEW: additive fields
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub narratives: Option<Vec<HotspotNarrative>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recommendations: Vec<Recommendation>,
}
```

`narratives` is `None` when the JSONL fallback path is used. `recommendations` is empty when no recognized hotspot types are present.

### 2.3 Narrative Synthesis Logic

New file: `crates/unimatrix-observe/src/synthesis.rs`

```rust
pub const CLUSTER_WINDOW_SECS: u64 = 30;

pub fn synthesize_narratives(hotspots: &[HotspotFinding]) -> Vec<HotspotNarrative> {
    hotspots.iter().map(|h| synthesize_one(h)).collect()
}

fn synthesize_one(hotspot: &HotspotFinding) -> HotspotNarrative {
    let clusters = cluster_evidence(&hotspot.evidence);
    let top_files = extract_top_files(&hotspot.evidence, 5);
    let sequence_pattern = extract_sequence_pattern(hotspot);
    let summary = build_summary(hotspot, &clusters, &top_files);
    HotspotNarrative {
        hotspot_type: hotspot.rule_name.clone(),
        summary,
        clusters,
        top_files,
        sequence_pattern,
    }
}
```

**Timestamp clustering**: Group evidence events into clusters where consecutive events are within `CLUSTER_WINDOW_SECS * 1000` ms of each other. Each cluster reports `window_start` (first event ts), `event_count`, and a description.

**Sequence extraction**: For `sleep_workarounds` rule_name only, scan evidence descriptions for numeric values. If the extracted numbers form a monotonically increasing sequence of >= 2 values, format as `"Ns->Ns->..."`. Otherwise `None`.

**Top-N files**: Parse evidence descriptions for file paths, count occurrences, sort descending, take top 5. If more than 5, append `"... and N more"` to the summary.

### 2.4 Recommendation Templates

In `crates/unimatrix-observe/src/report.rs`:

```rust
pub fn recommendations_for_hotspots(hotspots: &[HotspotFinding]) -> Vec<Recommendation> {
    hotspots.iter().filter_map(|h| recommendation_for(h)).collect()
}

fn recommendation_for(hotspot: &HotspotFinding) -> Option<Recommendation> {
    match hotspot.rule_name.as_str() {
        "permission_retries" => Some(Recommendation {
            hotspot_type: "permission_retries".into(),
            action: "Add common build/test commands to settings.json allowlist".into(),
            rationale: format!("{} permission retries detected — agents lose time waiting for approval", hotspot.measured as u64),
        }),
        "coordinator_respawns" => Some(Recommendation {
            hotspot_type: "coordinator_respawns".into(),
            action: "Review coordinator agent lifespan and handoff patterns".into(),
            rationale: format!("{} coordinator respawns detected — may indicate premature termination or context overflow", hotspot.measured as u64),
        }),
        "sleep_workarounds" => Some(Recommendation {
            hotspot_type: "sleep_workarounds".into(),
            action: "Use run_in_background + TaskOutput instead of sleep polling".into(),
            rationale: format!("{} sleep workaround events detected — sleep polling wastes agent time", hotspot.measured as u64),
        }),
        "compile_cycles" if hotspot.measured > 10.0 => Some(Recommendation {
            hotspot_type: "compile_cycles".into(),
            action: "Consider incremental compilation or targeted cargo test invocations".into(),
            rationale: format!("{:.0} compile cycles detected (threshold: 10) — consider narrowing test scope", hotspot.measured),
        }),
        _ => None,
    }
}
```

### 2.5 Integration Point: `from_structured_events()` Extension

The existing `from_structured_events()` function (delivered in col-010 P0, if it exists as a code path in `tools.rs`) is extended to populate `narratives` and `recommendations`:

```rust
// After building the report via the structured path:
let narratives = synthesize_narratives(&report.hotspots);
let recommendations = recommendations_for_hotspots(&report.hotspots);
report.narratives = if narratives.is_empty() { None } else { Some(narratives) };
report.recommendations = recommendations;
```

For the JSONL fallback path:
```rust
report.narratives = None;
report.recommendations = recommendations_for_hotspots(&report.hotspots);
```

Recommendations are populated on both paths (they only need hotspot data). Narratives require the structured-events path (they need richer evidence from SESSIONS/INJECTION_LOG).

**Architecture clarification**: The current codebase does not have a `structured.rs` file in `unimatrix-observe`. The col-010 P0 `from_structured_events()` function is implemented in `tools.rs` as inline logic within the `context_retrospective` handler. col-010b's synthesis code lives in a new `synthesis.rs` file in `unimatrix-observe`, called from the handler.

---

## 3. Lesson-Learned Auto-Persistence

### 3.1 Trigger

After `context_retrospective` builds the report (and BEFORE evidence truncation):

```rust
if !report.hotspots.is_empty() || !report.recommendations.is_empty() {
    spawn_lesson_learned_write(store, embed, &report, &feature_cycle);
}
```

### 3.2 Fire-and-Forget Write (ADR from col-010 ADR-004)

```rust
fn spawn_lesson_learned_write(
    store: Arc<Store>,
    embed: Arc<EmbedHandle>,
    report: &RetrospectiveReport,
    feature_cycle: &str,
) {
    let content = build_lesson_learned_content(report);
    let title = format!("Retrospective findings: {}", feature_cycle);
    let topic = format!("retrospective/{}", feature_cycle);
    let fc = feature_cycle.to_string();
    let tags = vec![
        format!("feature_cycle:{}", feature_cycle),
        format!("hotspot_count:{}", report.hotspots.len()),
        "source:retrospective".to_string(),
    ];

    tokio::spawn(async move {
        // 1. Check CategoryAllowlist
        // (access via store or server state — TBD at implementation)
        // If "lesson-learned" not in allowlist, log error and return

        // 2. Supersede check: lookup existing active entry with topic
        // If found, deprecate it (set superseded_by on old entry)

        // 3. Embed content
        let embedding = match tokio::task::spawn_blocking({
            let embed = Arc::clone(&embed);
            let title = title.clone();
            let content = content.clone();
            move || embed.embed(&title, &content)
        }).await {
            Ok(Ok(emb)) => Some(emb),
            _ => {
                tracing::warn!("lesson-learned embedding failed for {}", fc);
                None
            }
        };

        // 4. Write entry to store
        // Fields: category="lesson-learned", topic, trust_source="system",
        //         created_by="cortical-implant", tags, embedding
        // If embedding failed: write with embedding_dim=0
    });
}
```

### 3.3 Content Generation

```rust
fn build_lesson_learned_content(report: &RetrospectiveReport) -> String {
    let mut content = String::new();
    // Include narrative summaries if available (structured path)
    if let Some(narratives) = &report.narratives {
        for n in narratives {
            content.push_str(&format!("- {}: {}\n", n.hotspot_type, n.summary));
        }
    } else {
        // JSONL fallback: include hotspot claims only
        for h in &report.hotspots {
            content.push_str(&format!("- {}: {}\n", h.rule_name, h.claim));
        }
    }
    // Include recommendations
    for r in &report.recommendations {
        content.push_str(&format!("Recommendation ({}): {}\n", r.hotspot_type, r.action));
    }
    content
}
```

### 3.4 Supersede De-duplication

Before writing, query for an existing active `lesson-learned` entry with `topic == "retrospective/{feature_cycle}"`. If found:
1. Deprecate the existing entry (set `status = Deprecated`, `superseded_by = new_id`)
2. Write new entry with `supersedes = old_id`

This check runs inside the `tokio::spawn` task, synchronously, before the embedding step. The check-then-write is NOT atomic — concurrent `context_retrospective` calls may briefly produce two active entries (inherited known limitation from col-010 SR-09).

---

## 4. Provenance Boost

### 4.1 Constant Definition

In `crates/unimatrix-engine/src/confidence.rs`:

```rust
/// Query-time boost for lesson-learned category entries.
/// Applied in search re-ranking alongside co-access affinity.
/// Does NOT modify the stored confidence formula invariant.
pub const PROVENANCE_BOOST: f64 = 0.02;
```

### 4.2 Application Sites

Both `tools.rs` (MCP context_search) and `uds_listener.rs` (ContextSearch hook) apply the boost identically, at the same location where co-access boost is applied:

```rust
// In the re-sort block where co-access boost is applied:
let boost_a = boost_map.get(&entry_a.id).copied().unwrap_or(0.0);
let boost_b = boost_map.get(&entry_b.id).copied().unwrap_or(0.0);
// NEW: provenance boost
let prov_a = if entry_a.category == "lesson-learned" { PROVENANCE_BOOST } else { 0.0 };
let prov_b = if entry_b.category == "lesson-learned" { PROVENANCE_BOOST } else { 0.0 };
let final_a = base_a + boost_a + prov_a;
let final_b = base_b + boost_b + prov_b;
```

**Also in the no-co-access fallback sort** (when boost_map is empty): the initial sort already computes `rerank_score`. The provenance boost must also be applied here. Add a second sort pass or integrate into the initial sort:

```rust
// Initial sort (before co-access):
let score_a = rerank_score(*sim_a, entry_a.confidence)
    + if entry_a.category == "lesson-learned" { PROVENANCE_BOOST } else { 0.0 };
let score_b = rerank_score(*sim_b, entry_b.confidence)
    + if entry_b.category == "lesson-learned" { PROVENANCE_BOOST } else { 0.0 };
```

### 4.3 Invariant Preservation

- `PROVENANCE_BOOST` is query-time only
- Never written to `EntryRecord.confidence`
- `W_BASE + W_USAGE + W_FRESH + W_HELP + W_CORR + W_TRUST = 0.92` unchanged
- Co-access affinity max (0.08) unchanged
- `PROVENANCE_BOOST = 0.02` < co-access max (0.03), acts as tiebreaker

---

## Component-to-File Mapping

| Component | Files Modified |
|-----------|---------------|
| 1: Evidence-Limited Output | `crates/unimatrix-server/src/tools.rs` (RetrospectiveParams + truncation logic) |
| 2: Evidence Synthesis | `crates/unimatrix-observe/src/types.rs` (new types + RetrospectiveReport extension), `crates/unimatrix-observe/src/synthesis.rs` (NEW — narrative synthesis), `crates/unimatrix-observe/src/report.rs` (recommendation templates), `crates/unimatrix-observe/src/lib.rs` (re-export synthesis module) |
| 3: Lesson-Learned | `crates/unimatrix-server/src/tools.rs` (auto-persist logic in context_retrospective handler) |
| 4: Provenance Boost | `crates/unimatrix-engine/src/confidence.rs` (PROVENANCE_BOOST constant), `crates/unimatrix-server/src/tools.rs` (search re-ranking), `crates/unimatrix-server/src/uds_listener.rs` (ContextSearch re-ranking) |

---

## Risk Mitigations

| Risk | Resolution |
|------|-----------|
| SR-04: evidence_limit breaks existing tests | R-09 blocking gate: audit before implementation. Current tests do not assert on evidence array lengths (verified). |
| SR-05: Dual representation (truncated vs full) | ADR-001: clone-and-truncate, never mutate original |
| SR-07: Provenance boost at two callsites | Both sites import `PROVENANCE_BOOST` from `confidence.rs`. No magic numbers. |
| SR-01: Fire-and-forget embedding failure | Entry written with `embedding_dim = 0`, recoverable via supersede on next retrospective call |
| SR-08: Concurrent supersede race | Inherited known limitation. Tolerated — next call resolves. |
