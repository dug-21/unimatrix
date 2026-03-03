# col-010b Pseudocode Overview

## Component Interaction

```
context_retrospective handler (tools.rs)
  |
  |-- [existing] parse sessions, attribute, detect hotspots, compute metrics
  |
  |-- [Component 2] synthesize_narratives(hotspots) -> Vec<HotspotNarrative>
  |-- [Component 2] recommendations_for_hotspots(hotspots) -> Vec<Recommendation>
  |
  |-- populate report.narratives (Some on structured path, None on JSONL)
  |-- populate report.recommendations (both paths)
  |
  |-- [Component 3] spawn lesson-learned write (fire-and-forget)
  |       |-- self.clone() into tokio::spawn
  |       |-- embed content
  |       |-- supersede check + deprecation
  |       |-- server.insert_with_audit() (atomic ENTRIES + VECTOR_MAP + HNSW + audit)
  |       |-- confidence seed
  |
  |-- [Component 1] clone-and-truncate for serialization
  |       |-- clone report
  |       |-- truncate each hotspot.evidence to evidence_limit
  |       |-- serialize truncated clone
  |
  v
  return formatted report
```

## Data Flow

1. `context_retrospective` builds full report (existing flow)
2. Narratives synthesized from full evidence (Component 2)
3. Recommendations generated from hotspot data (Component 2)
4. Report fields populated: `narratives` + `recommendations`
5. Lesson-learned task spawned on full report (Component 3)
6. Clone-and-truncate for response serialization (Component 1)
7. Provenance boost applied at search re-ranking time (Component 4)

## Shared Types (unimatrix-observe/src/types.rs)

```
HotspotNarrative {
    hotspot_type: String,
    summary: String,
    clusters: Vec<EvidenceCluster>,
    top_files: Vec<(String, u32)>,
    sequence_pattern: Option<String>,
}

EvidenceCluster {
    window_start: u64,
    event_count: u32,
    description: String,
}

Recommendation {
    hotspot_type: String,
    action: String,
    rationale: String,
}
```

## RetrospectiveReport Extension

Two new fields added to `RetrospectiveReport`:
- `narratives: Option<Vec<HotspotNarrative>>` -- `#[serde(default, skip_serializing_if = "Option::is_none")]`
- `recommendations: Vec<Recommendation>` -- `#[serde(default, skip_serializing_if = "Vec::is_empty")]`

## RetrospectiveParams Extension

One new field:
- `evidence_limit: Option<usize>` -- default 3, 0 = unlimited

## Critical Architectural Constraint: ADR-002

The lesson-learned write MUST use `self.clone()` + `insert_with_audit()`:
- `UnimatrixServer` derives `Clone`, all fields are `Arc`-wrapped
- The spawned task calls `server_clone.insert_with_audit(entry, embedding, audit_event)`
- This provides: atomic ENTRIES + VECTOR_MAP write, HNSW insertion, audit trail
- `embedding_dim` MUST be set from `embedding.len()` in `insert_with_audit`, not hardcoded to 0

## Critical Fix: embedding_dim in insert_with_audit and correct_with_audit

Both `insert_with_audit` and `correct_with_audit` currently hardcode `embedding_dim: 0`.
The fix: set `embedding_dim: embedding.len() as u16` in both functions, using the
actual embedding vector that is passed as a parameter.

## Component Dependencies

- Component 1 depends on Component 2 (recommendations populated before truncation)
- Component 3 depends on Component 2 (narratives used in lesson-learned content)
- Component 4 is independent (search re-ranking constant)
- All depend on shared types in types.rs
