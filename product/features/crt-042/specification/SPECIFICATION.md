# SPECIFICATION: crt-042 — PPR Expander

## Objective

The current PPR implementation treats HNSW k=20 results as the complete candidate pool, making
cross-category entries that are graph-reachable but embedding-distant invisible to retrieval.
This feature introduces `graph_expand`, a BFS traversal of `TypedRelationGraph` that widens the
candidate pool before PPR scoring runs, enabling PPR to assign non-zero personalization mass to
cross-category entries connected to HNSW seeds via positive edges. The expander ships behind a
feature flag (`ppr_expander_enabled = false`) and is gated by an A/B eval before default
enablement.

---

## Functional Requirements

### FR-01: graph_expand function

A pure, synchronous function `graph_expand` must be implemented in a new submodule
`graph_expand.rs` within `unimatrix-engine`, following the `graph_ppr.rs` /
`graph_suppression.rs` submodule split pattern (entry #3740).

Signature:
```
fn graph_expand(
    graph: &TypedRelationGraph,
    seed_ids: &[u64],
    depth: usize,
    max_candidates: usize,
) -> HashSet<u64>
```

The function must be re-exported from `graph.rs`.

### FR-02: graph_expand traversal contract

`graph_expand` must perform BFS from each seed entry ID, traversing positive edge types
(CoAccess, Supports, Informs, Prerequisite) up to `depth` hops. It must return the set of
reachable entry IDs excluding the seed IDs themselves. It must stop collecting when
`max_candidates` results are reached, processing the BFS frontier in sorted node-ID order to
guarantee determinism. A visited-set must prevent revisiting nodes.

**Behavioral contract (cite entry #3754):** an entry A must appear in the returned set when
HNSW seed B is present and edge B→A exists via any positive edge type (Outgoing from seed B).
Entry C must NOT appear in the returned set when seed B exists and only edge C→B exists (an
incoming edge to the seed). The traversal direction is expressed solely by this behavioral
outcome; no Direction:: constant is authoritative in this specification.

Excluded edge types: `Supersedes` (structural chain, not retrieval relevance), `Contradicts`
(suppression signal, negative).

### FR-03: graph_expand degenerate cases

`graph_expand` must return an empty `HashSet<u64>` in all of the following conditions:
- `seed_ids` is empty
- the graph contains no nodes
- `depth` is 0

### FR-04: graph_expand traversal boundary

All traversal within `graph_expand` must use `edges_of_type()` exclusively. Direct calls to
`.edges_directed()` or `.neighbors_directed()` on the petgraph `StableGraph` are prohibited at
all new traversal sites (established boundary per entry #3627, crt-030).

### FR-05: Phase 0 integration in search.rs

A new Phase 0 must be inserted into Step 6d of the search pipeline in `search.rs`, executing
before Phase 1 (personalization vector construction). Phase 0 must:

1. Collect the HNSW seed entry IDs from `results_with_scores`.
2. Call `graph_expand(&typed_graph, &seed_ids, expansion_depth, max_expansion_candidates)`.
3. For each returned entry ID not already present in `results_with_scores`:
   a. Fetch the entry via `entry_store.get(expanded_id)`.
   b. Apply the quarantine check (`SecurityGateway::is_quarantined(&entry.status)`). Silently
      skip quarantined entries.
   c. Retrieve the stored embedding via `vector_store.get_embedding(expanded_id)`. Silently
      skip entries with no stored embedding.
   d. Compute cosine similarity between the query embedding and the stored entry embedding.
   e. Push `(entry, cosine_sim)` into `results_with_scores`.
4. Record wall-clock duration of Phase 0 in a `debug!` trace (see NFR-01).

Phase 0 must execute only when `ppr_expander_enabled = true`. When the flag is `false`, Phase 0
is entirely bypassed and no code path in Step 6d changes.

### FR-06: Phase 0 guard — quarantine caller responsibility

`graph_expand` itself is pure and performs no quarantine checks. The quarantine check in Phase 0
(FR-05 step 3b) is the sole enforcement point for expanded entries. This is an explicit contract:
future callers of `graph_expand` outside `search.rs` must independently apply the quarantine
check before adding returned IDs to any result set.

### FR-07: InferenceConfig additions

Three new fields must be added to `InferenceConfig` in `infra/config.rs`:

| Field | Type | Default | Valid range |
|---|---|---|---|
| `ppr_expander_enabled` | `bool` | `false` | — |
| `expansion_depth` | `usize` | `2` | [1, 10] |
| `max_expansion_candidates` | `usize` | `200` | [1, 1000] |

All three fields must use `#[serde(default = "fn_name")]` following the existing PPR field
pattern. Each must be added in four coordinated locations: struct body, `impl Default`,
default value function, and `InferenceConfig::validate()`. The serde default function and
`Default::default()` value must match atomically (entry #3817). All sites where
`InferenceConfig` struct literals are constructed must be updated to include the new fields
or use `..Default::default()` (entries #2730, #4044).

### FR-08: Config validation — always enforce, not only when enabled

`InferenceConfig::validate()` must enforce range constraints on `expansion_depth` and
`max_expansion_candidates` at server start, regardless of the value of `ppr_expander_enabled`.
Specifically:
- `expansion_depth` must be in [1, 10]; values outside this range must return a validation error.
- `max_expansion_candidates` must be in [1, 1000]; values outside this range must return a
  validation error.

Rationale: the NLI pattern of validating only when a flag is enabled was a source of subtle
config bugs (SCOPE.md Design Decision Q4). Pre-validating catches invalid configs at server
start before the flag is ever flipped in production.

### FR-09: SearchService struct and wiring

Three new fields must be added to `SearchService`:
- `ppr_expander_enabled: bool`
- `expansion_depth: usize`
- `max_expansion_candidates: usize`

Wired from `InferenceConfig` in `SearchService::new()`, following the existing five-field PPR
wiring pattern.

### FR-10: Eval profile

A new eval profile `ppr-expander-enabled.toml` must be committed to
`product/research/ass-037/harness/profiles/`:

```toml
[profile]
name = "ppr-expander-enabled"
description = "PPR expander enabled (crt-042). HNSW k=20 seeds -> graph_expand depth=2 max=200 -> expanded pool -> PPR -> fused scoring."
distribution_change = true

[inference]
ppr_expander_enabled = true
expansion_depth = 2
max_expansion_candidates = 200
```

The profile must be executable via `run_eval.py --profile ppr-expander-enabled.toml` without
error.

---

## Non-Functional Requirements

### NFR-01: Latency instrumentation (mandatory before flag can default to true)

Phase 0 must emit a `debug!` trace recording wall-clock duration in milliseconds for every
invocation, regardless of whether any entries were expanded. The trace must include the seed
count, expanded entry count before quarantine filtering, and final added count after filtering.
Example form: `"[Phase 0] graph_expand: seeds={}, expanded_raw={}, added={}, elapsed={}ms"`.

This instrumentation is a prerequisite gate for enabling the flag by default. The flag must
remain `false` by default in this feature. Enabling it by default is a separate decision
contingent on measured latency data from the A/B eval run.

### NFR-02: Flag-off regression safety (bit-identical)

When `ppr_expander_enabled = false`, the search output must be bit-for-bit identical to
pre-crt-042 behavior for all existing test cases. No code path that runs when the flag is
`false` may be altered. Phase 0 must be wholly behind the flag guard.

### NFR-03: Quarantine safety

No quarantined entry may appear in `results_with_scores` as a result of Phase 0 expansion.
`SecurityGateway::is_quarantined(&entry.status)` is the authoritative check. Quarantined
entries must be silently skipped (no error, no log at warn/error level).

### NFR-04: Determinism

`graph_expand` must produce the same output for the same inputs on every call. The BFS
frontier must be processed in sorted node-ID order. No non-deterministic data structures
(e.g., unordered iteration over a HashMap) may influence traversal order.

### NFR-05: Synchronous and pure

`graph_expand` must be synchronous, with no async operations, no I/O, and no side effects.
It operates solely on the passed `&TypedRelationGraph` reference. This matches the contract
established for `personalized_pagerank` (ADR-002 crt-030, entry #3732).

### NFR-06: Lock order preservation

`graph_expand` operates on the cloned `typed_graph` value — the read lock on
`TypedGraphStateHandle` must already be released before Phase 0 executes. The existing
lock-ordering invariant in `search.rs` must not be violated.

### NFR-07: No per-query store reads in graph_expand

`graph_expand` itself must issue zero SQLite queries. The `entry_store.get()` calls for
expanded entries (FR-05 step 3a) are async and occur in the `search.rs` caller, not inside
`graph_expand`. This matches the existing PPR Phase 5 injection pattern.

### NFR-08: Combined expansion ceiling (Phase 0 + Phase 5)

Phase 0 injects up to `max_expansion_candidates` (default 200) entries into the candidate
pool. Phase 5 (existing PPR-only injection) injects up to `ppr_max_expand` (default 50)
additional entries. The maximum combined candidate pool beyond HNSW k=20 is 250 entries.
This ceiling must be documented in the Phase 0 implementation comment. Phase 5 behavior is
unchanged; the two mechanisms are complementary, not conflicting.

### NFR-09: File size limit

`graph_expand.rs` must not exceed 500 lines. If inline unit tests push the file over the
limit, tests move to a separate `graph_expand_tests.rs` following the codebase split pattern.

---

## Acceptance Criteria

All criteria are written behaviorally per lesson entry #3754: traversal correctness is
expressed as observable outcome, not as Direction:: enum values.

### Gate: SR-03 — S1/S2 Edge Directionality Prerequisite (blocking)

**AC-00 (Prerequisite Gate):** Before any Phase 0 implementation code is written, the delivery
agent must inspect the crt-041 write site and confirm whether S1 (tag co-occurrence Informs)
and S2 (structural vocabulary Informs) edges are written bidirectionally (both A→B and B→A)
or single-direction only (source_id < target_id convention). If single-direction only, this is
a **blocking issue**: a separate issue must be filed to back-fill bidirectional edges at the
crt-041 write site before crt-042 ships, following the CoAccess back-fill pattern (entry
#3889). Work on Phase 0 must not begin until this gate is resolved.

### Flag-Off Safety

**AC-01:** When `ppr_expander_enabled = false` (the default), all existing search integration
tests pass with no output differences. No result set, score, or ordering changes from
pre-crt-042 baselines.

### Phase 0 Invocation

**AC-02:** When `ppr_expander_enabled = true` and HNSW returns a seed set containing entry B,
`graph_expand` is called with B's entry ID before the PPR personalization vector is computed.
Verified by: unit test asserting Phase 1 receives an expanded `results_with_scores` slice that
contains entries sourced from graph traversal.

### Behavioral Traversal Correctness

**AC-03:** Given seed entry B and a graph edge B→A of type CoAccess, Supports, Informs, or
Prerequisite — entry A appears in the `graph_expand` return set. This must hold for each of
the four positive edge types independently.

**AC-04:** Given seed entry B and a graph edge C→B (B is the target, not the source) — entry C
does NOT appear in the `graph_expand` return set unless C is also reachable via a forward
edge from another seed. (Traversal is forward from seeds; backward edges are not followed
unless write-time bidirectionality places them.)

**AC-05:** Given seeds {B} and graph path B→A→D (two hops, positive edges) with
`expansion_depth = 2` — both A and D appear in the `graph_expand` return set.

**AC-06:** Given seeds {B} and graph path B→A→D with `expansion_depth = 1` — A appears in
the return set, D does not.

**AC-07:** Given seed entry B and a graph edge B→X of type Supersedes or Contradicts — entry
X does NOT appear in the `graph_expand` return set.

### Seed Exclusion

**AC-08:** No seed entry ID appears in the `graph_expand` return set, even if the graph
contains an edge that creates a path from one seed back to another seed.

### Candidate Cap

**AC-09:** When the BFS traversal reaches exactly `max_candidates` entries, `graph_expand`
returns a set of that size and stops. Additional reachable entries are not included.

### Degenerate Cases

**AC-10:** `graph_expand` returns an empty set when `seed_ids` is empty.

**AC-11:** `graph_expand` returns an empty set when the graph has no nodes.

**AC-12:** `graph_expand` returns an empty set when `depth = 0`.

### Quarantine Safety

**AC-13:** When Phase 0 expansion surfaces entry Q whose status is quarantined, Q does not
appear in `results_with_scores`. The result set is identical to what it would be if Q were
not in the graph. No warning or error is logged for the skip.

**AC-14:** A unit/integration test constructs a scenario where a graph-reachable entry has
quarantined status. The test asserts Q is absent from the final result set with the expander
enabled.

### Embedding Skip

**AC-15:** When Phase 0 expansion surfaces an entry with no stored embedding (vector lookup
returns None), that entry is silently skipped and does not appear in `results_with_scores`.

### traversal boundary (edges_of_type)

**AC-16:** No call to `.edges_directed()` or `.neighbors_directed()` appears in
`graph_expand.rs`. All traversal is performed via `edges_of_type()`. Verified by code
inspection.

### Config

**AC-17:** A TOML file that omits `ppr_expander_enabled`, `expansion_depth`, and
`max_expansion_candidates` entirely loads without error and produces the default values
(`false`, `2`, `200`).

**AC-18:** `InferenceConfig::validate()` returns an error for `expansion_depth = 0` regardless
of the value of `ppr_expander_enabled`.

**AC-19:** `InferenceConfig::validate()` returns an error for `expansion_depth = 11` regardless
of the value of `ppr_expander_enabled`.

**AC-20:** `InferenceConfig::validate()` returns an error for `max_expansion_candidates = 0`
regardless of the value of `ppr_expander_enabled`.

**AC-21:** `InferenceConfig::validate()` returns an error for `max_expansion_candidates = 1001`
regardless of the value of `ppr_expander_enabled`.

### Eval Profile

**AC-22:** `product/research/ass-037/harness/profiles/ppr-expander-enabled.toml` exists and
`run_eval.py --profile ppr-expander-enabled.toml` executes to completion without error.

### Eval Gate

**AC-23:** With `ppr_expander_enabled = true`, eval run produces MRR >= 0.2856 (no regression
from baseline). P@5 is measured and recorded; any value above 0.1115 is evidence the expander
functions. The eval result must be recorded as part of the feature completion evidence.

### Latency Instrumentation

**AC-24:** When `ppr_expander_enabled = true`, every search invocation emits a `debug!` log
line from Phase 0 containing: seed count, raw expanded count, final added count, and elapsed
milliseconds.

### Cross-Category Regression Test

**AC-25:** An integration test demonstrates that with `ppr_expander_enabled = true`, an entry
whose embedding is dissimilar to the query (would not appear in HNSW k=20) but is connected
by a positive graph edge to an HNSW seed appears in the final result set. With
`ppr_expander_enabled = false`, the same entry is absent from results. This test encodes the
core behavioral guarantee of the feature.

---

## Domain Model

### Entities

**TypedRelationGraph** — the in-memory petgraph `StableGraph<u64, RelationEdge>` built by the
background tick. Nodes are entry IDs (u64). Edges are typed (`RelationType`) and directed.
All new traversal accesses this via `edges_of_type()` (the sole traversal boundary, per
entry #3627). Exposed via `TypedGraphStateHandle: Arc<RwLock<TypedGraphState>>`.

**Seed Set** — the set of entry IDs returned by HNSW k=20 at the start of Step 6d. These are
the starting nodes for `graph_expand` and the initial contents of `results_with_scores`.

**Expanded Pool** — the union of the seed set and all entries added by Phase 0
(`graph_expand` results that pass the quarantine and embedding checks). This is the full
input to Phase 1 (personalization vector construction) and PPR.

**Positive Edge Types** — edge types eligible for expansion traversal: `CoAccess`, `Supports`,
`Informs`, `Prerequisite`. Excluded: `Supersedes` (structural), `Contradicts` (negative signal).

**Phase 0** — the new first phase of Step 6d, responsible for widening the seed set into the
expanded pool via `graph_expand`. Executes only when `ppr_expander_enabled = true`.

**Phase 5** — the existing PPR-only injection phase (unchanged). Injects entries reachable by
PPR mass diffusion that exceed `ppr_inclusion_threshold`. Operates on top of the expanded pool.

**Combined Ceiling** — Phase 0 max 200 + Phase 5 max 50 = 250 entries maximum beyond HNSW
k=20. These are the maximum additional candidates before PPR scoring and final truncation.

**InferenceConfig** — the runtime-tunable configuration struct in `infra/config.rs`. All
expander parameters live here. Fields are backward-compatible via `#[serde(default)]`.

**SecurityGateway::is_quarantined** — the authoritative quarantine check. Called by the Phase
0 integration in `search.rs` for every expanded entry. Not called inside `graph_expand`
itself (pure function contract).

### Ubiquitous Language

- **seed** — an entry in the HNSW k=20 result set; the starting point for graph expansion.
- **expanded entry** — an entry surfaced by `graph_expand` that was not in the seed set.
- **positive edge** — an edge of type CoAccess, Supports, Informs, or Prerequisite.
- **hop** — one traversal step along a positive edge from node to neighbor.
- **depth** — the maximum number of hops from any seed; controlled by `expansion_depth`.
- **expander flag** — `ppr_expander_enabled`; when false, Phase 0 is wholly bypassed.

---

## User Workflows

### Workflow 1: Normal retrieval with expander disabled (default)

1. Agent/user issues a search query.
2. HNSW returns k=20 candidates. PPR re-ranks them (existing behavior). Step 6d runs
   Phases 1–5 exactly as before crt-042. No change.

### Workflow 2: Retrieval with expander enabled (eval or opt-in)

1. Agent/user issues a search query.
2. HNSW returns k=20 candidate seeds.
3. Phase 0: `graph_expand` traverses positive edges from seeds to depth 2, collecting up to
   200 expanded entry IDs. Each passes quarantine and embedding checks. Expanded entries
   receive true cosine similarity scores.
4. Phase 1–5: PPR runs over the full expanded pool (seeds + expanded entries). Cross-category
   entries now receive non-zero personalization mass and compete in fused scoring.
5. Final result set is truncated to k after NLI scoring (if enabled).

### Workflow 3: Operator enables expander for A/B eval

1. Operator creates `ppr-expander-enabled.toml` profile (already committed by crt-042).
2. `run_eval.py` runs eval scenarios against a snapshot with expander enabled.
3. MRR and P@5 are compared against `conf-boost-c.toml` baseline.
4. If MRR >= 0.2856 and P@5 > 0.1115, the feature is confirmed to produce improvement.
5. Latency delta is read from Phase 0 `debug!` traces in the eval log.
6. A separate decision sets `ppr_expander_enabled = true` as the default once the latency
   ceiling is defined and met.

---

## Constraints

### C-01: SR-03 Prerequisite Gate (blocking)

S1/S2 (Informs) edge directionality from crt-041 must be confirmed before Phase 0
implementation begins. If single-direction (source_id < target_id only), a crt-041 write-site
back-fill is required before crt-042 ships. This is a blocking constraint, not a post-ship
check. See AC-00.

### C-02: O(N) embedding lookup

`vector_store.get_embedding(id)` is O(N) per call (entry #3658). Up to 200 expanded entries
produces 200 × O(N) scans (~1.4M comparisons at 7k corpus). This is the primary latency
concern. The feature flag defaults to `false` specifically to allow measurement before
default enablement. Latency instrumentation (NFR-01 / AC-24) is mandatory.

### C-03: SR-01 traversal boundary

All TypedRelationGraph traversal must go through `edges_of_type()`. Direct
`.edges_directed()` or `.neighbors_directed()` calls are prohibited at new traversal sites.
Established in crt-030 (entry #3627). Verified by code inspection (AC-16).

### C-04: Lock order

The typed graph read lock must be released before Phase 0 executes. `graph_expand` operates
on the already-cloned `typed_graph` value. The lock-ordering comment in `search.rs` must
not be violated.

### C-05: Async boundary

`graph_expand` must be synchronous and pure. The `entry_store.get()` async calls for
expanded entries run in the existing async search handler context in `search.rs`. The
function boundary must not be crossed with async/await inside `graph_expand`.

### C-06: No schema migration

`InferenceConfig` fields use `#[serde(default)]`. No SQLite schema change. `GRAPH_EDGES`
is unchanged. No migration required.

### C-07: No new edge types

This feature only reads `TypedRelationGraph`. Edge writing is crt-040/crt-041 scope.
No `RelationType` variants are added.

### C-08: No background tick usage

`graph_expand` is a hot-path, query-time function only. It must not be invoked from the
background tick.

### C-09: SR-02 budget-boundary bias (documented, not fixed)

BFS processed in sorted node-ID order creates a deterministic bias toward older (lower-ID)
entries when `max_candidates` is hit. This is acceptable for the initial feature. A
post-measurement follow-up (sort by edge weight) is the documented path forward once the
expander proves its value.

---

## Dependencies

### Crates

| Crate | Usage |
|---|---|
| `unimatrix-engine` | `TypedRelationGraph`, `edges_of_type`, `RelationType`, `NodeIndex`, `RelationEdge`, `graph_ppr.rs` pattern |
| `unimatrix-server` | `InferenceConfig` (infra/config.rs), `SearchService` (search.rs), `SecurityGateway` |
| `petgraph` | `StableGraph`, traversal (via `edges_of_type` boundary only) |
| `std::collections::HashSet` | Return type of `graph_expand` |

### Existing Components

| Component | Role |
|---|---|
| `personalized_pagerank` | Unchanged PPR function; receives expanded pool as input |
| `graph_ppr.rs` | Submodule split pattern to follow for `graph_expand.rs` |
| `graph_suppression.rs` | Submodule split pattern to follow for `graph_expand.rs` |
| `SecurityGateway::is_quarantined` | Quarantine check called in search.rs Phase 0 |
| `entry_store.get()` | Async entry fetch for expanded IDs; existing Phase 5 pattern |
| `vector_store.get_embedding()` | O(N) embedding lookup for cosine similarity computation |
| `InferenceConfig::validate()` | Must be extended with range checks for new fields |
| `run_eval.py` | Eval harness; must accept new profile without modification |

### External Prerequisites

| Prerequisite | Status | Blocking? |
|---|---|---|
| crt-041 (S1/S2/S8 edges merged) | Must be merged before crt-042 ships for P@5 improvement | No (expander works on any graph; improvement depends on density) |
| S1/S2 edge directionality confirmed (SR-03 / AC-00) | Must be confirmed before Phase 0 implementation | Yes — blocking |

---

## NOT in Scope

- Any change to `personalized_pagerank` internals (`graph_ppr.rs`). The PPR algorithm is
  unchanged; only the input pool is widened.
- Any change to the fused scoring formula (weights, normalization, co-access boost).
- Writing new graph edges or adding new `RelationType` variants. Edge writing is
  crt-040/crt-041 scope.
- SQL neighbor queries at query time. `graph_expand` is in-memory only.
- Schema migration. `GRAPH_EDGES` and all SQLite tables are unchanged.
- Expander invocation from the background tick. Hot-path only.
- `TypedGraphState::rebuild()` changes. Graph build/cache infrastructure is unchanged.
- Enabling `ppr_expander_enabled = true` as the default. That is a post-eval decision.
- Goal-conditioned or behavioral signal integration (Groups 5/6 scope).
- Batch embedding lookup or O(1) index-based embedding retrieval (future optimization).
- Sorting the BFS frontier by edge weight (future optimization, post SR-02 follow-up).

---

## Open Questions for Architect

**OQ-01 (from SR-03):** Does crt-041 write S1 (tag co-occurrence Informs) and S2 (structural
vocabulary Informs) edges bidirectionally (A→B and B→A) or single-direction only
(source_id < target_id)? This must be confirmed by the delivery agent before any Phase 0 code
is written. If single-direction, a back-fill issue must be filed and resolved before crt-042
ships. The answer changes the effective graph density the expander sees for these edge types.

**OQ-02 (from SR-01):** Does `vector_store.get_embedding()` have any O(1) or O(log N) lookup
path via entry ID (e.g., a parallel HashMap from ID to embedding vector)? If yes, the 200 ×
O(N) concern (C-02) is largely resolved and the latency ceiling can be set pre-measurement.
If no, the measurement-first approach (NFR-01 / AC-24) stands and the latency ceiling remains
a post-measurement gate.

**OQ-03 (from SR-04):** Should a hard cap on the total post-expansion candidate pool size
(before PPR scoring) be enforced in `search.rs`? The current design allows Phase 0 (max 200)
+ Phase 5 (max 50) + HNSW k=20 = up to 270 total candidates. If PPR scoring over 270 entries
has unacceptable latency, the architect should specify a combined ceiling enforced in
`search.rs` after Phase 5 completes.

**OQ-04 (from SR-05):** If the eval gate fails (MRR < 0.2856 or P@5 shows no improvement),
who owns the resolution path? The scope delegates this to "measure and decide" but does not
assign an owner. A pre-specified owner and decision timeline should be recorded in the
delivery brief.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned 17 ranked entries. Most relevant:
  entry #3754 (direction semantics lesson, crt-030 post-merge correction — cited in all
  traversal ACs), entry #3750 (ADR-003 corrected direction spec), entry #3739 (SR-05 PPR
  O(N) graph concern), entry #3817 (InferenceConfig dual-site atomic change pattern), entry
  #3627 (ADR-002 edges_of_type sole traversal boundary), entry #3769 (InferenceConfig
  procedure), entries #2730 and #4044 (InferenceConfig hidden test sites pattern). No
  results unavailable; server was responsive throughout.
