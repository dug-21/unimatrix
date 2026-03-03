# col-010b Implementation Brief: Retrospective Evidence Synthesis & Lesson-Learned Persistence

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/col-010b/SCOPE.md |
| Scope Risk Assessment | product/features/col-010b/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/col-010b/architecture/ARCHITECTURE.md |
| Specification | product/features/col-010b/specification/SPECIFICATION.md |
| Risk Strategy | product/features/col-010b/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/col-010b/ALIGNMENT-REPORT.md |

## ADR Files

| ADR | File |
|-----|------|
| ADR-001: Clone-and-Truncate for Evidence Limiting | product/features/col-010b/architecture/ADR-001-clone-and-truncate-evidence.md |

## Inherited ADRs (from col-010)

| ADR | File | Relevance |
|-----|------|-----------|
| ADR-004: Lesson-Learned Fire-and-Forget Embedding | product/features/col-010/architecture/ADR-004-lesson-learned-fire-and-forget-embedding.md | Component 3 fire-and-forget pattern |
| ADR-005: Provenance Boost Query-Time Constant | product/features/col-010/architecture/ADR-005-provenance-boost-query-time-constant.md | Component 4 boost mechanism |

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| evidence-limiting | pseudocode/evidence-limiting.md | test-plan/evidence-limiting.md |
| evidence-synthesis | pseudocode/evidence-synthesis.md | test-plan/evidence-synthesis.md |
| lesson-learned | pseudocode/lesson-learned.md | test-plan/lesson-learned.md |
| provenance-boost | pseudocode/provenance-boost.md | test-plan/provenance-boost.md |

### Cross-Cutting Artifacts

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

---

## Goal

col-010b extends the col-002 retrospective pipeline with four application-logic components: evidence-limited output (reduce ~87KB payload to ~10KB default), narrative synthesis (structured evidence summaries + actionable recommendations), lesson-learned auto-persistence (knowledge entries from retrospective findings, embedded via ONNX), and provenance boost (lesson-learned entries rank higher in search). No schema migration. No new tables.

---

## Resolved Decisions

| Decision | Resolution | Source |
|----------|------------|--------|
| Evidence truncation mechanism | Clone-and-truncate — never mutate in-memory report. Full evidence preserved for synthesis and lesson-learned content. | col-010b ADR-001 |
| Lesson-learned ONNX embedding | Fire-and-forget via `tokio::spawn`. `context_retrospective` returns before embedding completes. On failure: entry with `embedding_dim = 0`. | col-010 ADR-004 |
| Provenance boost mechanism | `PROVENANCE_BOOST = 0.02` query-time constant. Stored `0.92` invariant unchanged. | col-010 ADR-005 |
| `trust_source = "system"` | All cortical-implant-generated entries use `trust_source = "system"` for 0.7 trust score | col-010 SPEC SEC-03 |
| Supersede race tolerated | Concurrent retrospective calls may briefly produce two active lesson-learned entries | col-010 SPEC FR-11.6 |
| `evidence_limit = 0` backward compat | Callers requiring full evidence pass `evidence_limit = 0`; default is 3 | SCOPE.md |
| `hotspots` type unchanged | `Vec<HotspotFinding>` type not changed. Truncation is server-side only. | SCOPE.md |
| `narratives` additive only | `Option<Vec<HotspotNarrative>>` with `#[serde(default, skip_serializing_if)]` | SCOPE.md |
| JSONL path unchanged | `build_report()` JSONL path unmodified; `narratives = None` when JSONL used | SPEC NFR-02.2 |
| Recommendations on both paths | `recommendations` populated from hotspot data on both structured and JSONL paths | SPEC FR-05.4 |

---

## Files to Create

| File | Summary |
|------|---------|
| `crates/unimatrix-observe/src/synthesis.rs` | `synthesize_narratives()`, `cluster_evidence()`, `extract_sequence_pattern()`, `extract_top_files()`, `build_summary()`. Constants: `CLUSTER_WINDOW_SECS = 30`. |

## Files to Modify

| File | Change Summary |
|------|---------------|
| `crates/unimatrix-observe/src/types.rs` | Add `HotspotNarrative`, `EvidenceCluster`, `Recommendation` types. Add `narratives: Option<Vec<HotspotNarrative>>` and `recommendations: Vec<Recommendation>` to `RetrospectiveReport`. |
| `crates/unimatrix-observe/src/report.rs` | Add `recommendations_for_hotspots()` covering 4 hotspot type templates. Update `build_report()` signature to accept recommendations parameter. |
| `crates/unimatrix-observe/src/lib.rs` | Re-export `synthesis` module and new types. |
| `crates/unimatrix-engine/src/confidence.rs` | Add `pub const PROVENANCE_BOOST: f64 = 0.02`. |
| `crates/unimatrix-server/src/tools.rs` | Add `evidence_limit: Option<usize>` to `RetrospectiveParams`. Add evidence truncation (clone-and-truncate). Add narrative synthesis call. Add recommendation generation. Add lesson-learned fire-and-forget write. Add `PROVENANCE_BOOST` to search re-ranking (both initial sort and co-access re-sort). |
| `crates/unimatrix-server/src/uds_listener.rs` | Add `PROVENANCE_BOOST` to ContextSearch re-ranking (both initial sort and co-access re-sort). |

---

## Data Structures

### New Types (`types.rs`)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotspotNarrative {
    pub hotspot_type: String,
    pub summary: String,
    pub clusters: Vec<EvidenceCluster>,
    pub top_files: Vec<(String, u32)>,
    pub sequence_pattern: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceCluster {
    pub window_start: u64,
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

### RetrospectiveReport Extension (`types.rs`)

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

    // NEW (col-010b)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub narratives: Option<Vec<HotspotNarrative>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recommendations: Vec<Recommendation>,
}
```

### RetrospectiveParams Extension (`tools.rs`)

```rust
pub struct RetrospectiveParams {
    pub feature_cycle: String,
    pub agent_id: Option<String>,
    pub evidence_limit: Option<usize>,  // NEW: default 3, 0 = unlimited
}
```

---

## Function Signatures

### Narrative Synthesis (`synthesis.rs` — NEW)

```rust
pub const CLUSTER_WINDOW_SECS: u64 = 30;

pub fn synthesize_narratives(hotspots: &[HotspotFinding]) -> Vec<HotspotNarrative>;
fn synthesize_one(hotspot: &HotspotFinding) -> HotspotNarrative;
fn cluster_evidence(evidence: &[EvidenceRecord]) -> Vec<EvidenceCluster>;
fn extract_sequence_pattern(hotspot: &HotspotFinding) -> Option<String>;
fn extract_top_files(evidence: &[EvidenceRecord], limit: usize) -> Vec<(String, u32)>;
fn build_summary(
    hotspot: &HotspotFinding,
    clusters: &[EvidenceCluster],
    top_files: &[(String, u32)],
) -> String;
```

### Recommendation Templates (`report.rs`)

```rust
pub fn recommendations_for_hotspots(hotspots: &[HotspotFinding]) -> Vec<Recommendation>;
fn recommendation_for(hotspot: &HotspotFinding) -> Option<Recommendation>;
```

### Provenance Boost (`confidence.rs`)

```rust
pub const PROVENANCE_BOOST: f64 = 0.02;
```

---

## Constraints

### Hard Constraints

- **No schema migration**: application logic only. No new tables.
- **`hotspots` type unchanged**: `Vec<HotspotFinding>` with `evidence: Vec<EvidenceRecord>`.
- **Clone-and-truncate**: never mutate in-memory report. Truncation on clone only (ADR-001).
- **Fire-and-forget for lesson-learned embedding**: `context_retrospective` returns before embedding completes (col-010 ADR-004).
- **Stored weight invariant**: `PROVENANCE_BOOST` is query-time only. `0.92` invariant preserved (col-010 ADR-005).
- **R-09 blocking gate**: Audit existing `context_retrospective` tests for evidence array length assertions before implementing Component 1.
- **`narratives` on structured path only**: `None` when JSONL fallback used.
- **`recommendations` on both paths**: populated from hotspot data regardless of path.
- **Edition 2024, MSRV 1.89**.
- **All existing tests pass (AC-10)**: additive changes only with `#[serde(default)]`.

### Soft Constraints

- `CLUSTER_WINDOW_SECS = 30`: named constant, tunable.
- `PROVENANCE_BOOST = 0.02`: named constant, tunable.
- Lesson-learned content is best-effort: uses narratives when available, hotspot claims as fallback.

---

## Dependencies

| Dependency | Type | Status | Needed For |
|------------|------|--------|-----------|
| col-010 P0 (PR #77) | Hard | Merged | SESSIONS, INJECTION_LOG, `from_structured_events()` path |
| col-002 / col-002b | Existing | Active | `RetrospectiveReport`, `HotspotFinding`, `build_report()` |
| col-009 | Existing | Merged | Signal processing, `entries_analysis` field on report |
| unimatrix-embed | Existing | Active | ONNX embedding for lesson-learned entries |
| unimatrix-engine | Existing | Active | `confidence.rs` for PROVENANCE_BOOST |

---

## NOT in Scope

- Schema migration of any kind
- SESSIONS / INJECTION_LOG write paths (delivered in col-010 P0)
- `session_id: Option<String>` on `EntryRecord` (deferred indefinitely)
- Sophisticated narrative ML (deterministic heuristics only)
- `helpful_count` seeding on lesson-learned entries
- Category-specific `MINIMUM_SAMPLE_SIZE` reduction
- Secondary index on INJECTION_LOG

---

## Alignment Status

**Overall**: PASS. No variances requiring human approval.

col-010b completes the retrospective intelligence loop: observation -> synthesis -> persistence -> search ranking -> automatic delivery. Well-aligned with M5 vision goals.

---

## Known Limitations

- SR-09: Concurrent `context_retrospective` calls for the same feature_cycle may briefly produce two active lesson-learned entries. Tolerated.
- Embedding failure: lesson-learned entry written with `embedding_dim = 0`, invisible to `context_search` until next supersede.
- JSONL path: `narratives = None` — no structured synthesis available without SESSIONS data.
