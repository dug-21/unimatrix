# Specification: crt-040 — Cosine Supports Edge Detection

## Objective

Restore `Supports` edge production to the Unimatrix graph by implementing a pure-cosine
detection path (Path C) inside `run_graph_inference_tick`. The deleted NLI post-store path
(removed in crt-038) left zero `Supports` edges in production; ASS-035 validated that cosine
similarity at threshold ≥ 0.65 correctly identifies true `Supports` pairs with zero false
positives on the production corpus. Path C reuses the Phase 4 candidate set already computed
by the tick — no new HNSW scan — and tags every written edge with the named constant
`EDGE_SOURCE_COSINE_SUPPORTS = "cosine_supports"` so GNN feature construction and PPR
traversal can distinguish the signal source.

---

## Domain Models

### Ubiquitous Language

| Term | Definition |
|------|-----------|
| **Path A** | Unconditional structural Informs detection in `run_graph_inference_tick`. Writes `Informs` edges via `write_nli_edge` with `source='nli'`. Gated by cosine >= `nli_informs_cosine_floor` and `informs_category_pairs` filter. Unchanged by crt-040. |
| **Path B** | NLI-model Supports/Contradicts detection in `run_graph_inference_tick`. Gated by `get_provider()` (requires `nli_enabled=true`). Currently dead in production. Unchanged by crt-040. |
| **Path C** | New pure-cosine Supports detection introduced by crt-040. Runs unconditionally. Reuses Phase 4 `candidate_pairs`. Writes `Supports` edges with `source = EDGE_SOURCE_COSINE_SUPPORTS`. |
| **candidate_pairs** | `Vec<(u64, u64, f32)>` of `(source_id, target_id, cosine)` produced by Phase 4's HNSW scan. Already computed; Path C consumes it without a new scan. |
| **existing_supports_pairs** | `HashSet<(u64, u64)>` pre-fetched in Phase 2 from the database. Used by Path C as an O(1) pre-filter to skip already-written pairs. `INSERT OR IGNORE` is the authoritative dedup backstop. |
| **informs_category_pairs** | Config field listing allowed `[source_category, target_category]` pairs. Defaults to 4 pairs: `["lesson-learned","decision"]`, `["lesson-learned","convention"]`, `["pattern","decision"]`, `["pattern","convention"]`. Reused as-is by Path C — no new config field for Supports category filter. |
| **supports_cosine_threshold** | New `InferenceConfig` field (f32, default 0.65). Path C gate: only pairs with cosine >= threshold qualify. |
| **EDGE_SOURCE_COSINE_SUPPORTS** | Named constant `"cosine_supports"`. Written to `graph_edges.source` for all Path C edges. Defined in `unimatrix-store::read`, re-exported from crate root. |
| **write_graph_edge** | New general-purpose edge writer in `nli_detection.rs` accepting `source: &str` as a parameter. Sibling to `write_nli_edge` — NOT a replacement. `write_nli_edge` remains unchanged. |
| **MAX_COSINE_SUPPORTS_PER_TICK** | Module-level constant `= 50`. Independent per-tick budget for Path C. Does not share `max_graph_inference_per_tick` (Path B's budget). |
| **nli_post_store_k** | Dead field in `InferenceConfig`. Consumer (`run_post_store_nli`) deleted in crt-038. Removed in this delivery. |
| **tick ordering invariant** | `compaction → co_access_promotion → graph-rebuild → contradiction_scan (conditional) → extraction_tick → structural_graph_tick (always)`. Path C runs inside `structural_graph_tick`. Invariant is unchanged. |
| **supports_edge_count** | Field in `GraphCohesionMetrics`. Counts all `graph_edges` rows with `relation_type = 'Supports'` regardless of `source`. Source-agnostic — picks up both Path B and Path C edges. Used as the eval-gate metric. |
| **inferred_edge_count** | Field in `GraphCohesionMetrics`. Counts only `source = 'nli'` edges. Semantically stale after crt-040 (cosine Supports edges have a different source). Left unchanged; a follow-up issue tracks the rename. |
| **UNIQUE constraint** | `graph_edges` table DDL: `UNIQUE(source_id, target_id, relation_type)`. Does NOT include `source`. When `nli_enabled=true`, Path B and Path C may both attempt the same `(source_id, target_id, Supports)` pair; `INSERT OR IGNORE` silently discards the second write. That is correct behavior — one edge per pair per type. |

### Key Entities

```
InferenceConfig
  + supports_cosine_threshold: f32   // NEW — default 0.65
  - nli_post_store_k: usize          // REMOVED — dead field

graph_edges (table, no schema change)
  source_id:     u64
  target_id:     u64
  relation_type: &str    // "Supports" for Path C writes
  weight:        f32     // = cosine value (Resolved Decision 3)
  source:        &str    // = EDGE_SOURCE_COSINE_SUPPORTS = "cosine_supports"
  metadata:      &str    // JSON: '{"cosine": <f32>}' (Resolved Decision 4)
  bootstrap_only: 0

EDGE_SOURCE_COSINE_SUPPORTS: &str = "cosine_supports"
  // defined in unimatrix-store::read, re-exported from lib.rs

write_graph_edge(store, source_id, target_id, relation_type, weight, created_at, metadata, source)
  // new pub(crate) function in nli_detection.rs
  // INSERT OR IGNORE; log warn on SQL error; returns bool

MAX_COSINE_SUPPORTS_PER_TICK: usize = 50
  // module-level constant in nli_detection_tick.rs
```

---

## Functional Requirements

### FR-01: Path C Detection Logic
Path C iterates over `candidate_pairs` (Phase 4 output). For each pair `(source_id, target_id, cosine)`, Path C writes a `Supports` edge if ALL of the following hold:
- `cosine >= config.supports_cosine_threshold`
- `[source_category, target_category]` is in `config.informs_category_pairs`
- `(source_id, target_id)` is not in `existing_supports_pairs` (pre-filter; `INSERT OR IGNORE` is backstop)
- The per-tick budget has not been exhausted: fewer than `MAX_COSINE_SUPPORTS_PER_TICK` edges written this tick by Path C.

### FR-02: Category Pair Filter Reuse
Path C uses `informs_category_pairs` as-is. No new config field for the Supports category
allow-list. Adding a separate `supports_category_pairs` field is an explicit out-of-scope
extension point (SR-05).

### FR-03: No Temporal Ordering Guard
Path C does NOT apply the temporal ordering guard (`source_created_at < target_created_at`)
that Path A uses. ASS-035 found no benefit from temporal filtering for Supports pairs at
threshold ≥ 0.65. The semantic relationship is considered direction-neutral for cosine
detection purposes.

### FR-04: No same_feature_cycle Guard
Path C does NOT require same-feature-cycle filtering. ASS-035 Group D confirmed correctness
without it at threshold ≥ 0.65.

### FR-05: Canonical Direction
Phase 4's HNSW scan already normalizes pairs to canonical `(lo, hi)` form where
`lo = source_id.min(&neighbour_id)`. Path C output is naturally canonical. Path C does not
write the reverse direction.

### FR-06: Edge Writer — write_graph_edge
A new `pub(crate) async fn write_graph_edge` is added to `nli_detection.rs`. It accepts
`source: &str` as a parameter (in addition to the existing parameters of `write_nli_edge`).
It issues `INSERT OR IGNORE` into `graph_edges`. On SQL error, it logs at `warn!` level and
returns `false` — it does NOT propagate the error. `write_nli_edge` is NOT modified. It
remains as a thin wrapper or independent function using the hardcoded `'nli'` source. See
FR-12 for the constraint on `write_nli_edge`.

### FR-07: EDGE_SOURCE_COSINE_SUPPORTS Constant
`EDGE_SOURCE_COSINE_SUPPORTS: &str = "cosine_supports"` is added to
`crates/unimatrix-store/src/read.rs`, following the pattern of `EDGE_SOURCE_NLI` (entry
#3591) and `EDGE_SOURCE_CO_ACCESS` (entry #3882). It is re-exported from
`crates/unimatrix-store/src/lib.rs` alongside the existing two constants.

### FR-08: Config Field — supports_cosine_threshold
`InferenceConfig` gains a new field `supports_cosine_threshold: f32` with:
- `#[serde(default = "default_supports_cosine_threshold")]`
- `default_supports_cosine_threshold() -> f32 { 0.65 }`
- Explicit value `0.65` in the `impl Default for InferenceConfig` struct literal (dual-site
  atomic change per pattern #3817 — see AC-17).
- Range validation in `InferenceConfig::validate()`: `(0.0, 1.0)` exclusive, matching the
  pattern for `nli_informs_cosine_floor`.
- Config merge function updated to propagate project-level override, following the
  `nli_informs_cosine_floor` merge pattern (f32 epsilon comparison).

### FR-09: Dead Field Removal — nli_post_store_k
`nli_post_store_k: usize` is removed from `InferenceConfig`. All removal sites:
- Field declaration in the struct (with `#[serde(default = "default_nli_post_store_k")]`
  annotation and its doc comment)
- `default_nli_post_store_k()` backing function
- Value in `impl Default for InferenceConfig` struct literal
- Validation block in `InferenceConfig::validate()` (`[1, 100]` range check)
- All test assertions referencing `nli_post_store_k`
- Config merge function reference (lines 2222-2227 in current config.rs)

No migration required. `serde` silently ignores unknown fields during TOML deserialization
(no `#[serde(deny_unknown_fields)]` is in use on `InferenceConfig`). Existing `config.toml`
files that still contain `nli_post_store_k = ...` will deserialize without error; the field
is silently discarded. This must be verified as part of AC-18.

### FR-10: Edge Metadata
Each Path C `Supports` edge is written with metadata JSON: `{"cosine": <f32>}`. Shorter
than the Informs metadata format; consistent with how cosine values appear elsewhere in the
codebase. Resolved Decision 4.

### FR-11: Edge Weight
`weight = cosine` (the raw cosine similarity value, no multiplier). Resolved Decision 3.
No PPR weight config knob — not validated for cosine Supports path yet.

### FR-12: write_nli_edge Immutability
`write_nli_edge` MUST NOT be modified to accept a `source` parameter. It hardcodes
`source='nli'` and `created_by='nli'`. Silently retagging existing Informs or NLI Supports
edges would break GNN feature construction and graph cohesion metrics. Path C writes via
`write_graph_edge` exclusively.

### FR-13: Path C Unconditional Execution
Path C is NOT gated by `nli_enabled`, `get_provider()`, or any NLI provider check. It runs
on every `structural_graph_tick` invocation, regardless of NLI configuration.

### FR-14: Tick-Internal Ordering
Path C runs after Path A (Informs writes) and before or after Path B (NLI Supports gate).
The ordering relative to Path B must be documented in code. The `existing_supports_pairs`
pre-filter is populated at Phase 2 (tick start) and does not update during the tick.
If Path B runs before Path C and writes a new Supports edge in the same tick, Path C's
pre-filter is stale for that pair — `INSERT OR IGNORE` is the authoritative dedup. This
is the accepted behavior (SR-09).

### FR-15: Path C Infallibility
Path C errors (SQL failures, embedding lookup errors) MUST NOT propagate. `run_graph_inference_tick`
returns `()`. All Path C error paths log at `warn!` level and continue processing the next
candidate. No `?` operator, no `unwrap()` in Path C production code.

---

## Non-Functional Requirements

### NFR-01: No New HNSW Scan
Path C reuses `candidate_pairs` from Phase 4. No additional `vector_index.search()` call.
Any implementation that adds a new HNSW scan fails this requirement.

### NFR-02: No Rayon or spawn_blocking in Path C
Path C is pure-cosine — no NLI model calls. It must not call `score_batch`, `rayon_pool.spawn()`,
or `spawn_blocking`. It runs in the async Tokio context, same as Path A.

### NFR-03: Per-Tick Budget Cap
Path C is capped at `MAX_COSINE_SUPPORTS_PER_TICK = 50` edges per tick. This budget is
independent of `max_graph_inference_per_tick` (Path B's budget) and `MAX_INFORMS_PER_TICK`
(Path A's budget). The constant is not config-promoted in this delivery.

### NFR-04: MRR Non-Regression
The eval gate requires MRR >= 0.2875 on `product/research/ass-039/harness/scenarios.jsonl`
after delivery. The cosine Supports path must not regress search ranking.

### NFR-05: supports_edge_count Increase
After Path C is active (at least one tick on a populated database), `supports_edge_count`
in `GraphCohesionMetrics` must be > 0. This is verified by the eval gate scenario set.

### NFR-06: inferred_edge_count Backward Compatibility
`GraphCohesionMetrics.inferred_edge_count` continues to count only `source = 'nli'` edges.
Path C edges (`source = 'cosine_supports'`) do not appear in this metric. No change to the
SQL queries in `compute_graph_cohesion_metrics`. A follow-up issue is filed to track the
semantic staleness of the `inferred_edge_count` name.

### NFR-07: File Size Awareness
`nli_detection_tick.rs` exceeds 2,000 lines including tests. If Path C implementation
adds significant volume, delivery must evaluate whether extraction to a helper module is
warranted (500-line rule per Rust workspace conventions). This evaluation happens at
implementation time.

### NFR-08: Serde Forward Compatibility
Removing `nli_post_store_k` must not break deserialization of existing config files that
contain the field. Verified by the serde forward-compatibility test (AC-18).

### NFR-09: Category Data Resolution — HashMap Pre-Build Required
Category data for Path C must be resolved from a `HashMap<u64, String>` (entry_id →
category) pre-built once from `all_active` after Phase 2, before the Path C candidate
loop begins. Per-pair DB lookup inside the Path C loop is **prohibited**. An implementation
that issues a SQL query per candidate to resolve category fails this requirement.

### NFR-10: Path C Observability Log
An unconditional `debug!` log must fire after the Path C write loop — even when both
counts are zero. Required fields: `cosine_supports_candidates` (usize, number of pairs
evaluated), `cosine_supports_edges_written` (usize, number of edges actually written).
The log fires on every tick that executes Path C, enabling passive monitoring of graph
enrichment progress.

---

## Acceptance Criteria

All 15 ACs from SCOPE.md are reproduced below with their original IDs. Two additional ACs
(AC-16, AC-17, AC-18) are added to cover the `impl Default` trap and `nli_post_store_k`
removal.

### Path C Detection Logic

**AC-01**: When `candidate_pairs` contains a pair with `cosine >= supports_cosine_threshold`
AND `[source_category, target_category]` in `informs_category_pairs`, a `Supports` edge is
written with `source = EDGE_SOURCE_COSINE_SUPPORTS = "cosine_supports"`.
Verification: unit test with synthetic candidate_pairs containing a qualifying pair; assert
`graph_edges` row exists with `source = 'cosine_supports'` and `relation_type = 'Supports'`.

**AC-02**: When `cosine < supports_cosine_threshold`, no `Supports` edge is written for that
pair by Path C.
Verification: unit test with pair below threshold; assert no `Supports` edge written.

**AC-03**: When the category pair is NOT in `informs_category_pairs`, no `Supports` edge is
written for that pair even if `cosine >= threshold`.
Verification: unit test with pair above threshold but disallowed category pair; assert no
edge written.

**AC-04**: Pairs already in `existing_supports_pairs` are skipped (pre-filter). `INSERT OR IGNORE`
is the authoritative dedup backstop.
Verification: unit test pre-populating `existing_supports_pairs` with a qualifying pair; assert
no duplicate edge written (DB row count = 1 after tick).

**AC-05**: Cosine Supports detection runs unconditionally — NOT gated by `nli_enabled` or
`get_provider()`.
Verification: integration test with `nli_enabled = false`; assert Path C still writes edges
when candidates qualify.

### Path Isolation

**AC-06**: Informs path (Path A) behavior is unchanged — `informs_category_pairs` usage in
Path A and guards (temporal ordering, cross-feature, cosine floor) are not modified.
Verification: existing Path A tests pass without modification.

**AC-07**: NLI Supports path (Path B) behavior is unchanged — Phase 6/7/8 and
`supports_edge_threshold` are not modified.
Verification: existing Path B tests pass without modification.

### Constant and Store

**AC-08**: `EDGE_SOURCE_COSINE_SUPPORTS = "cosine_supports"` is defined as a named constant
in `unimatrix-store::read` and re-exported from the crate root (`lib.rs`).
Verification: unit test asserts `EDGE_SOURCE_COSINE_SUPPORTS == "cosine_supports"`; accessible from `unimatrix_store::*` import.

**AC-11**: The Path C write path uses `write_graph_edge` with `source = EDGE_SOURCE_COSINE_SUPPORTS`
— NOT the hardcoded `'nli'` source.
Verification: inspect written `graph_edges.source`; assert value is `"cosine_supports"`, not
`"nli"`.

### Config — supports_cosine_threshold

**AC-09**: `supports_cosine_threshold` config field added to `InferenceConfig` with default 0.65,
range `(0.0, 1.0)` exclusive. `InferenceConfig::validate()` rejects out-of-range values with a
structured error naming the field `"supports_cosine_threshold"`.
Verification: unit tests for `validate()` with values 0.0 (fail), 1.0 (fail), 0.65 (pass),
0.001 (pass), 0.999 (pass).

**AC-10**: `InferenceConfig::default()` returns `supports_cosine_threshold = 0.65`.
Verification: `assert_eq!(InferenceConfig::default().supports_cosine_threshold, 0.65_f32)`.

### Budget

**AC-12**: Path C is capped at `MAX_COSINE_SUPPORTS_PER_TICK = 50` edges per tick. This budget
is independent of `max_graph_inference_per_tick` and `MAX_INFORMS_PER_TICK`.
Verification: unit test with 60 qualifying candidate pairs; assert exactly 50 edges written.

### Tick Ordering

**AC-13**: Tick ordering invariant is preserved. Path C runs inside `run_graph_inference_tick`,
which always runs last in the tick sequence.
Verification: code inspection; tick ordering is enforced by the caller in `background.rs`.

### Eval Gate

**AC-14**: Eval gate passes: MRR >= 0.2875 on `product/research/ass-039/harness/scenarios.jsonl`
after delivery.
Verification: run eval harness (`python product/research/ass-039/harness/run_eval.py`); assert
MRR >= 0.2875.

### Graph Cohesion Metrics

**AC-15**: `inferred_edge_count` in `GraphCohesionMetrics` continues to count only
`source = 'nli'` edges. Path C edges do not appear in this metric (backward compat).
Verification: unit test writing a `source = 'cosine_supports'` edge; assert `inferred_edge_count`
is unchanged.

### impl Default Trap (pattern #3817 / #4011)

**AC-16** (ADDED): `default_supports_cosine_threshold()` serde backing function and the
`impl Default for InferenceConfig` struct literal agree: both return `0.65`.
Verification: `assert_eq!(default_supports_cosine_threshold(), 0.65_f32)` AND
`assert_eq!(InferenceConfig::default().supports_cosine_threshold, 0.65_f32)` in the same
test or in adjacent tests that both pass. This mirrors crt-041's AC-17 pattern.

### nli_post_store_k Removal

**AC-17** (ADDED): `nli_post_store_k` field is absent from `InferenceConfig`: no struct field,
no `default_nli_post_store_k()` function, no `impl Default` entry, no `validate()` block, no
config merge branch, no test assertions.
Verification: `grep -n "nli_post_store_k" crates/unimatrix-server/src/infra/config.rs` returns
zero results after the removal commit.

**AC-18** (ADDED): Deserializing a TOML string that contains `nli_post_store_k = 5` into
`InferenceConfig` succeeds without error (serde silently discards the unknown field).
Verification: unit test deserializing TOML with `nli_post_store_k = 5` asserts
`toml::from_str::<InferenceConfig>` returns `Ok(_)`. Confirms `deny_unknown_fields` is NOT
active on `InferenceConfig`.

**AC-19** (ADDED): After the Path C write loop, an unconditional `debug!` log fires with
fields `cosine_supports_candidates: usize` and `cosine_supports_edges_written: usize`.
The log fires even when both values are zero.
Verification: unit test or integration test asserts the log fires on a tick where Path C
runs, and includes both fields regardless of whether any edges were written.

---

## User Workflows

### Workflow 1: Automatic Supports Edge Production (Every Tick)

1. Background tick fires (`structural_graph_tick`).
2. Phase 2: `existing_supports_pairs` loaded from DB.
3. Phase 4: HNSW scan produces `candidate_pairs: Vec<(u64, u64, f32)>`.
4. Path A runs: writes `Informs` edges from Phase 4b (separate scan).
5. **Path C runs**: iterates `candidate_pairs`, applies cosine threshold and category filter,
   writes qualifying `Supports` edges via `write_graph_edge` with
   `source = EDGE_SOURCE_COSINE_SUPPORTS`, up to `MAX_COSINE_SUPPORTS_PER_TICK`.
6. Path B gate: if `nli_enabled=true`, Phase 6/7/8 run (unchanged).
7. `supports_edge_count` in `GraphCohesionMetrics` increases on next `context_status` call.

### Workflow 2: Config Override

An operator sets `supports_cosine_threshold = 0.70` in `config.toml`. The server loads the
config, `InferenceConfig::validate()` accepts the value (in range `(0.0, 1.0)`), and Path C
uses `0.70` as the gate. Fewer pairs qualify; `supports_edge_count` growth rate decreases.

### Workflow 3: Migration from Previous Config (nli_post_store_k present)

An operator has a `config.toml` containing `nli_post_store_k = 10` from a pre-crt-040
deployment. After the crt-040 upgrade, the server deserializes the config — serde discards
the unknown field silently. The server starts without error. The operator can clean the stale
field from their config file at their convenience.

---

## Constraints

1. **No new HNSW scan**: Path C must reuse `candidate_pairs` from Phase 4. A second
   `vector_index.search()` call violates this constraint.

2. **`informs_category_pairs` reuse is mandatory**: No new allow-list config field for
   Supports category pairs in this delivery.

3. **`write_nli_edge` must not be modified**: The `'nli'` source is hardcoded in the INSERT.
   Changing it would silently retag all existing Informs and NLI Supports edges. Path C writes
   exclusively via the new `write_graph_edge`.

4. **No NLI model calls in Path C**: Path C is pure-cosine. No `score_batch`, no
   `rayon_pool.spawn()`, no `spawn_blocking`. Path C stays in the async Tokio context.

5. **No schema migration**: The `source` column in `graph_edges` (TEXT, NOT NULL DEFAULT '')
   already exists. No DDL change required.

6. **Tick infallibility**: `run_graph_inference_tick` returns `()`. Path C errors must not
   propagate. All error paths use `warn!` + continue.

7. **UNIQUE constraint scope**: `UNIQUE(source_id, target_id, relation_type)` — confirmed from
   `db.rs` DDL at migration step. Does NOT include `source`. This means Path B + Path C
   collision on the same pair in the same tick is silently resolved by `INSERT OR IGNORE`.
   Delivery must not treat the silent discard as a bug.

8. **serde deny_unknown_fields**: NOT active on `InferenceConfig`. Verified by AC-18.
   Removal of `nli_post_store_k` is safe for existing config files.

9. **Module rename deferred**: `nli_detection_tick.rs` rename to `graph_inference_tick.rs` is
   deferred to Group 3. crt-040 does not rename the module.

---

## Dependencies

### Crates
- `unimatrix-store` — `graph_edges` table, `EDGE_SOURCE_*` constants, `GraphCohesionMetrics`,
  `Store::write_pool_server()`
- `unimatrix-server` — `InferenceConfig`, `run_graph_inference_tick`, `write_nli_edge`,
  `nli_detection.rs`, `nli_detection_tick.rs`
- `sqlx` — `INSERT OR IGNORE` execution
- `serde` / `toml` — config deserialization
- `tracing` — `warn!` logging in error paths

### Existing Components Consumed
- `candidate_pairs: Vec<(u64, u64, f32)>` — Phase 4 HNSW output (source_id, target_id, cosine)
- `existing_supports_pairs: HashSet<(u64, u64)>` — Phase 2 pre-fetch
- `config.informs_category_pairs` — category allow-list
- `config.supports_cosine_threshold` — new field (FR-08)
- `write_nli_edge` — NOT used by Path C; serves as structural template for `write_graph_edge`
- `EDGE_SOURCE_NLI`, `EDGE_SOURCE_CO_ACCESS` — naming pattern for `EDGE_SOURCE_COSINE_SUPPORTS`

### Prerequisite Features
- **crt-039** (merged, PR #486): `structural_graph_tick` runs unconditionally. Path C
  requires this — it must not be gated by the former `if nli_enabled` outer guard.

---

## NOT in Scope

- **NLI Supports path (Path B)**: Phase 6/7/8 and `supports_edge_threshold` are not modified.
- **PPR expander (Group 4)**: Graph enrichment enables it; implementation is not in scope.
- **S1, S2, S8 edge sources**: Only cosine Supports detection (Path C) is in scope.
- **schema migration**: No `signal_origin` column. The existing `source` column serves this
  role. No DDL change.
- **same_feature_cycle guard**: Not required per ASS-035 Group D.
- **Same-category Supports pairs**: Only `informs_category_pairs` pairs (cross-category subset).
- **`inferred_edge_count` rename**: Semantically stale after this feature; follow-up issue only.
- **Module rename** (`nli_detection_tick.rs` → `graph_inference_tick.rs`): Deferred to Group 3.
- **`supports_category_pairs` config field**: Extension point for follow-on features; out of
  scope here (SR-05).
- **Config-promotion of MAX_COSINE_SUPPORTS_PER_TICK**: Hardcoded constant; can be promoted
  later if an operator needs it. Not speculated in this delivery.

---

## Open Questions

None. All design decisions were resolved in the SCOPE.md §Resolved Design Decisions section
before specification authoring. The following items are confirmed deferred rather than open:

- `inferred_edge_count` semantic staleness: filed as follow-up issue (not blocking).
- `MAX_COSINE_SUPPORTS_PER_TICK` config promotion: deferred (not blocking).
- File extraction of `nli_detection_tick.rs` if Path C adds significant volume: evaluation
  deferred to delivery agent per the 500-line rule.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned 17 entries; most relevant:
  - #4025 (pattern): `write_nli_edge` hardcodes source='nli'; new edge signal origins require
    `write_graph_edge` sibling — directly informs FR-06 and FR-12.
  - #3817 (pattern): serde fn and `impl Default` dual-site atomic change — directly informs
    AC-16 and the impl Default trap note.
  - #3713 (lesson-learned): `supports_edge_threshold = 0.7` caused near-zero edge writes;
    default must be validated against corpus — directly informs NFR-04 rationale and the 0.65
    default choice.
  - #3591 (decision): `EDGE_SOURCE_NLI` naming pattern from col-029 — directly informs FR-07
    and AC-08.
- Queried: `context_search("InferenceConfig serde default field validation range")` — returned
  #3817 (pattern), #3769 (procedure), #2730 (pattern), #4013 (pattern); reinforced the
  dual-site requirement and test-site discovery obligation for config changes.
- Queried: `context_search("graph cohesion metrics supports edge count eval gate")` — returned
  #3592 (decision), #3591 (decision); confirmed `supports_edge_count` is source-agnostic
  (correct eval gate metric) and `inferred_edge_count` counts only `source='nli'`.
