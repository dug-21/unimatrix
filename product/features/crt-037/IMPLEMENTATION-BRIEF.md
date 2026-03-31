# crt-037 Implementation Brief: Informs Edge Type — Cross-Feature Institutional Memory Bridge

## Source Documents

| Document | Path |
|----------|------|
| Scope | product/features/crt-037/SCOPE.md |
| Architecture | product/features/crt-037/architecture/ARCHITECTURE.md |
| Specification | product/features/crt-037/specification/SPECIFICATION.md |
| Risk Strategy | product/features/crt-037/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/crt-037/ALIGNMENT-REPORT.md |

---

## Goal

Add `RelationType::Informs` as a sixth positive edge type in the Unimatrix TypedRelationGraph, connecting empirical knowledge entries (lesson-learned, pattern) from earlier feature cycles to normative knowledge entries (decision, convention) in later feature cycles. Detection runs within the existing `run_graph_inference_tick` via a new Phase 4b HNSW scan at cosine ≥ 0.45 and a composite guard (cross-category, temporal ordering, cross-feature, NLI neutral > 0.5). `Informs` edges participate in PPR traversal so that seeding on a decision node surfaces the lessons that informed it.

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|------------|-----------|
| graph.rs (RelationType extension) | pseudocode/graph.md | test-plan/graph.md |
| graph_ppr.rs (PPR traversal extension) | pseudocode/graph_ppr.md | test-plan/graph_ppr.md |
| config.rs (InferenceConfig extension) | pseudocode/config.md | test-plan/config.md |
| nli_detection_tick.rs (Phase 4b + Phase 8b) | pseudocode/nli_detection_tick.md | test-plan/nli_detection_tick.md |
| read.rs (query_existing_informs_pairs) | pseudocode/read.md | test-plan/read.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|------------|--------|----------|
| OQ-2: Batch structure for merged NLI batch | Use `NliCandidatePair` as a Rust tagged union with `SupportsContradict` and `Informs` variants. `Informs` variant carries `InformsCandidate` struct with all guard metadata as required (non-Option) fields. Compiler enforces routing — `None`-field vacuous-pass (R-05) is impossible at the type level. Architecture updated to match spec (ADR-001 corrected, WARN-1 resolved). | Unimatrix #3941 | product/features/crt-037/architecture/ADR-001-discriminator-tag-struct.md |
| OQ-1: Cap budget split between Supports and Informs | Sequential reservation: Supports/Contradicts candidates first (sort + truncate to `max_graph_inference_per_tick`), then `remaining = cap - supports_count`; Informs candidates truncated to `remaining`. No new config field. `max_graph_inference_per_tick` remains sole throttle. | Unimatrix #3939 | product/features/crt-037/architecture/ADR-002-combined-cap-priority.md |
| OQ-3: Dedup scope for `query_existing_informs_pairs` | Directional `(source_id, target_id)` — no symmetric normalization. Temporal ordering guard makes reverse edge detection-impossible; symmetric dedup would obscure the directional contract and risk suppressing valid edges on timestamp anomalies. `INSERT OR IGNORE` is the secondary backstop. | Unimatrix #3940 | product/features/crt-037/architecture/ADR-003-directional-dedup.md |
| OQ-4: Neutral threshold configurability | Fixed constant `0.5`. Parameterizing the neutral floor would tune the model output, not the domain — out of scope for v1. | SCOPE.md §Open Questions | — |
| OQ-5: Delivery gate structure | Functional correctness (AC-13 through AC-23 integration tests) + zero regression on CC@5/ICD/MRR. ICD delta measured post-delivery at first tick and ~3-tick accumulation. | SCOPE.md §Open Questions | — |

---

## Files to Create/Modify

| File | Action | Summary |
|------|--------|---------|
| `crates/unimatrix-engine/src/graph.rs` | Modify | Add `RelationType::Informs` variant; extend `as_str()` and `from_str()`; update module doc comment at line 16 to include `Informs` in the non-Supersedes examples |
| `crates/unimatrix-engine/src/graph_ppr.rs` | Modify | Add fourth `edges_of_type(node_idx, RelationType::Informs, Direction::Outgoing)` call in both `personalized_pagerank` inner loop and `positive_out_degree_weight` |
| `crates/unimatrix-server/src/infra/config.rs` | Modify | Add three new `InferenceConfig` fields (`informs_category_pairs`, `nli_informs_cosine_floor`, `nli_informs_ppr_weight`) with serde defaults and `validate()` range checks |
| `crates/unimatrix-server/src/services/nli_detection_tick.rs` | Modify | Define module-private `PairOrigin` enum and `NliCandidatePair` struct; add Phase 4b HNSW scan; extend Phase 5 cap logic; extend Phase 7 merged batch; add Phase 8b Informs write loop |
| `crates/unimatrix-store/src/read.rs` | Modify | Add `query_existing_informs_pairs() -> Result<HashSet<(u64, u64)>>` method, mirroring `query_existing_supports_pairs` with directional (non-normalized) tuple |

No new files. No schema migration. No new crates.

---

## Data Structures

### `RelationType` (extended enum, `graph.rs`)

```
Supersedes    -- penalty/supersession traversal only
Contradicts   -- NLI contradiction signal
Supports      -- NLI entailment signal; positive PPR
CoAccess      -- behavioral co-occurrence; positive PPR
Prerequisite  -- reserved; positive PPR
Informs       -- NEW: empirical→normative cross-feature bridge; positive PPR
```

`as_str()` returns the variant name exactly. `from_str` is case-sensitive. Penalty functions
(`graph_penalty`, `find_terminal_active`) use `Supersedes` only — `Informs` is invisible to
penalty logic (SR-01 invariant).

### `NliCandidatePair` (module-private tagged union, `nli_detection_tick.rs`)

```rust
#[derive(Debug, Clone)]
enum NliCandidatePair {
    SupportsContradict { source_id: u64, target_id: u64, cosine: f32, nli_scores: NliScores },
    Informs { candidate: InformsCandidate, nli_scores: NliScores },
}

#[derive(Debug, Clone)]
struct InformsCandidate {
    source_id: u64,
    target_id: u64,
    cosine: f32,
    source_created_at: i64,       // required — temporal ordering guard; not Option
    target_created_at: i64,       // required — temporal ordering guard; not Option
    source_feature_cycle: String, // required — cross-feature guard; not Option
    target_feature_cycle: String, // required — cross-feature guard; not Option
    source_category: String,      // required — category pair filter; not Option
    target_category: String,      // required — category pair filter; not Option
}
```

All guard metadata in `InformsCandidate` is required. The compiler makes `None`-field vacuous-pass (R-05 risk) impossible. Phase 8b routes via pattern matching — `NliCandidatePair::Informs { candidate, nli_scores }` — not a discriminator field check. (Architecture corrected to match spec FR-10; WARN-1 resolved. ADR-001 updated, Unimatrix #3941.)

### `InferenceConfig` (three new fields, `config.rs`)

| Field | Type | Default | Range |
|-------|------|---------|-------|
| `informs_category_pairs` | `Vec<[String; 2]>` | 4 SE pairs (frozen at v1) | — |
| `nli_informs_cosine_floor` | `f32` | `0.45` | `(0.0, 1.0)` exclusive |
| `nli_informs_ppr_weight` | `f32` | `0.6` | `[0.0, 1.0]` inclusive |

Default `informs_category_pairs` (four pairs — frozen at four for v1, SR-04):
- `["lesson-learned", "decision"]`
- `["lesson-learned", "convention"]`
- `["pattern", "decision"]`
- `["pattern", "convention"]`

---

## Function Signatures

### `Store::query_existing_informs_pairs` (`unimatrix-store/src/read.rs`)

```rust
pub async fn query_existing_informs_pairs(&self) -> Result<HashSet<(u64, u64)>>
```

SQL (directional, no min/max normalization):
```sql
SELECT source_id, target_id
FROM graph_edges
WHERE relation_type = 'Informs' AND bootstrap_only = 0
```

### `InferenceConfig` new default functions (`config.rs`)

```rust
fn default_informs_category_pairs() -> Vec<[String; 2]>
fn default_nli_informs_cosine_floor() -> f32  // returns 0.45
fn default_nli_informs_ppr_weight() -> f32    // returns 0.6
```

### `validate()` additions (`config.rs`)

```rust
// nli_informs_cosine_floor: (0.0, 1.0) exclusive
if self.nli_informs_cosine_floor <= 0.0 || self.nli_informs_cosine_floor >= 1.0 { ... }

// nli_informs_ppr_weight: [0.0, 1.0] inclusive
if self.nli_informs_ppr_weight < 0.0 || self.nli_informs_ppr_weight > 1.0 { ... }
```

### Phase 8b write call (`nli_detection_tick.rs`)

```rust
// Pattern match — compiler enforces Informs variant only
if let NliCandidatePair::Informs { candidate, nli_scores } = pair {
    // composite guard verified (neutral > 0.5, temporal, cross-feature, category)
    write_nli_edge(
        store,
        candidate.source_id,
        candidate.target_id,
        "Informs",                                              // RelationType::Informs.as_str()
        candidate.cosine * config.nli_informs_ppr_weight,      // weight: f32, must be finite
        timestamp,
        &metadata_json,                                         // include "nli_neutral": nli_scores.neutral
    ).await;
}
```

**Delivery note — `format_nli_metadata`:** The existing helper serializes only `entailment` and `contradiction`. For `Informs` edges, `neutral` is the decision criterion. Add `"nli_neutral": nli_scores.neutral` to the metadata JSON when writing `Informs` edges. Non-blocking, but worth doing while touching this code path for future debugging and score distribution monitoring.

---

## Tick Phase Structure (post-crt-037)

```
Phase 2  — query_existing_supports_pairs (unchanged) + query_existing_informs_pairs (NEW)
Phase 3  — select_source_candidates (unchanged)
Phase 4  — HNSW scan @ supports_candidate_threshold (0.50) → NliCandidatePair::SupportsContradict { ... }
Phase 4b — HNSW scan @ nli_informs_cosine_floor (0.45) (NEW)
             cross-category, temporal, cross-feature, dedup guards applied before NLI scoring
             → NliCandidatePair::Informs { candidate: InformsCandidate { ... }, nli_scores: _ }
Phase 5  — Sequential reservation cap (ADR-002):
             supports_pairs truncated to max_cap;
             remaining = max_cap - supports_pairs.len();
             informs_pairs truncated to remaining;
             debug log: candidates accepted/dropped
Phase 6  — text fetch for all merged pairs (unchanged)
Phase 7  — single rayon spawn: score_batch on all pairs; NliScores embedded in each NliCandidatePair variant
Phase 8  — pattern match NliCandidatePair::SupportsContradict; write Supports/Contradicts (unchanged)
Phase 8b — pattern match NliCandidatePair::Informs { candidate, nli_scores } (NEW)
             composite guard: nli_scores.neutral > 0.5 AND temporal AND cross-feature from InformsCandidate
             write Informs edge via write_nli_edge; metadata includes "nli_neutral": nli_scores.neutral
```

---

## Constraints

**Non-negotiable technical constraints:**

- **C-01**: No schema migration. `GRAPH_EDGES.relation_type` is free-text with no CHECK constraint. Delivery must confirm this via DDL inspection (OQ-S1) before Phase C begins.
- **C-02**: No new ML model. Existing `NliServiceHandle` / `CrossEncoderProvider` reused.
- **C-03**: No new tick infrastructure. Detection runs inside `run_graph_inference_tick`.
- **C-04 (W1-2 contract)**: All `score_batch` calls via `rayon_pool.spawn()`. No inline async NLI, no `spawn_blocking`.
- **C-05 (C-14/R-09)**: Rayon closure in Phase 7 remains synchronous CPU-bound. No `tokio::runtime::Handle::current()`, no `.await` inside rayon. CI gate: `grep -n 'Handle::current' nli_detection_tick.rs` returns empty.
- **C-06 (SR-01)**: `graph_penalty` and `find_terminal_active` filter exclusively to `Supersedes`. `Informs` must not appear in penalty traversal.
- **C-07**: All PPR traversal via `edges_of_type()` — no direct `.edges_directed()` calls.
- **C-08**: `max_graph_inference_per_tick` is the sole tick-level throttle. No new top-level cap field.
- **C-09**: `nli.neutral > 0.5` is a fixed constant. Not configurable.
- **C-10**: Default `informs_category_pairs` frozen at four entries for v1.
- **C-11**: Phase 7 batch element type is a typed discriminator. Parallel index-matched vecs are prohibited.
- **C-12**: Domain vocabulary strings (`"lesson-learned"`, `"decision"`, `"pattern"`, `"convention"`) must not appear as string literals in `nli_detection_tick.rs`. CI gate: `grep -n '"lesson-learned"\|"decision"\|"pattern"\|"convention"' nli_detection_tick.rs` returns empty.
- **C-13**: `Informs` edge weight must be finite (NaN/±Inf rejected) before any write (NF-08).
- **C-14**: Fourth `edges_of_type` call for `Informs` in PPR uses `Direction::Outgoing`. `Direction::Incoming` silently produces zero mass flow.
- **C-15**: crt-036 must be merged before Phase C delivery begins.

**Pre-delivery gates — CLEARED:**

- **OQ-S1 (CLEARED)**: DDL confirmed — `GRAPH_EDGES.relation_type` is plain `TEXT NOT NULL` with only a `UNIQUE(source_id, target_id, relation_type)` index. No CHECK constraint. `"Informs"` writes without restriction.
- **OQ-S2 (CLEARED)**: `NliScores.neutral` is a true 3-class softmax output (`entailment + neutral + contradiction ≈ 1.0`, invariant documented in `cross_encoder.rs` lines 32–42). `neutral > 0.5` means the model assigns >50% probability to "unrelated" as a first-class output. Threshold is correctly calibrated. WARN-2 resolved.

---

## Dependencies

| Dependency | Type | Notes |
|------------|------|-------|
| `crt-036` | Feature (logistical) | Must merge to main before crt-037 Phase C begins |
| `unimatrix-engine` | Crate | `graph.rs` + `graph_ppr.rs` modified (pure, no I/O) |
| `unimatrix-server` | Crate | `nli_detection_tick.rs` + `config.rs` modified |
| `unimatrix-store` | Crate | `read.rs` modified: new `query_existing_informs_pairs` |
| `NliServiceHandle` / `CrossEncoderProvider` | Existing infra | Reused; no change |
| `write_nli_edge` (`nli_detection.rs:532`) | `pub(crate)` fn | Reused; no change |
| `EDGE_SOURCE_NLI` constant | Named constant | Value `"nli"`; entry #3591 |
| `current_timestamp_secs()` | `pub(crate)` helper | Reused; no change |
| `petgraph` | Crate dependency | `Direction::Outgoing` enum variant; already present |
| `sqlx` | Crate dependency | SQLite query in `query_existing_informs_pairs`; already present |

---

## NOT in Scope

- Config-extensible relation types (`[[inference.relation_types]]` TOML blocks) — deferred
- Schema migration — `"Informs"` stored as free-text string; no DDL change
- `Extended(String)` or open-ended `RelationType` variants — deferred
- Changes to `run_post_store_nli` — `Informs` detection is background-tick-only
- Changes to the `Contradicts` detection path or `suppress_contradicts`
- Textual reference extraction (`Mentions` edges)
- Feature co-membership detection (`ImplementsDecision` edges)
- LLM-at-store-time annotation
- Changes to graph compaction, build order, or `VECTOR_MAP`
- Changes to `write_inferred_edges_with_cap` — `Informs` write path calls `write_nli_edge` directly
- ICD as a delivery gate criterion — ICD is post-delivery tracking only
- Configurable neutral threshold (`nli.neutral > 0.5` is fixed)
- Symmetric dedup in `query_existing_informs_pairs`
- A fifth or additional default `informs_category_pairs` entry

---

## Alignment Status

**Overall: PASS with two WARNs. No human action required before delivery begins.**

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Directly implements Wave 1A typed relationship graph intelligence layer |
| Milestone Fit | PASS | Wave 1 / Wave 1A appropriate; no future-wave capability prematurely built |
| Scope Gaps | WARN | OQ-S1 and OQ-S2 deferred to delivery (see Pre-Delivery Gates above) |
| Scope Additions | PASS | No additions beyond SCOPE.md requests |
| Architecture Consistency | WARN | `NliCandidatePair` modeled differently in ARCHITECTURE.md (flat struct) vs SPECIFICATION.md (tagged union). Spec model is stronger — implementer must follow spec. Architecture's flat-struct shape is superseded by spec on this point. |
| Risk Completeness | PASS | All SR-01 through SR-08 risks traced to architecture decisions and test scenarios |

**WARN-1 (NliCandidatePair):** The spec's tagged-union form (`NliCandidatePair::Informs { candidate: InformsCandidate, nli_scores }`) is the correct target. It eliminates `None`-field vacuous-pass risk (R-05) at the type level and satisfies FR-10's "misrouting is a compile-time error" requirement. The architecture's flat-struct with `Option<>` fields is acceptable but weaker. Delivery reviewer must confirm tagged-union implementation was used.

**WARN-2 (OQ-S2):** `NliScores.neutral` computation model property (direct logit vs. residual) must be confirmed before Phase C. FR-11's entailment exclusion guard provides partial mitigation but does not replace confirmation. See Pre-Delivery Gates.
