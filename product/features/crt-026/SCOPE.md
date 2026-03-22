# crt-026: WA-2 Session Context Enrichment

## Problem Statement

`context_search` is session-blind. Every query is ranked by the six-term fused score
(`w_nli·nli + w_sim·sim + w_conf·conf + w_coac·coac + w_util·util + w_prov·prov`) without
any signal from what the current session has been doing. An agent deep in a design session
(storing decisions and patterns) gets the same result distribution as a retro session
(storing lessons and outcomes), even on identical queries.

The session registry already tracks per-session state across calls. The category histogram
of what has been stored in the current session is a real-time signal that indicates which
SDLC phase the session is in — without requiring agents to explicitly declare it. This
implicit signal complements the explicit phase signal from WA-1 (`current_phase`), which
requires the SM to call `context_cycle(type: "phase", ...)`.

The 0.05 headroom in the WA-0 shipped formula (`sum=0.95`) was explicitly reserved for
this boost term. The infrastructure to carry it — `session_id` on `SearchParams`, the
`SessionRegistry`, `SessionState.current_phase` from WA-1 — already exists. The gap is
that `session_id` is never threaded into `ServiceSearchParams` and `SessionState` has no
`category_counts` field.

**Affected parties**: every agent session that calls `context_search`. The boost is
additive and cold-starts at zero, so sessions with no histogram produce identical results
to the current behavior — no regression for agents not yet using the signal.

## Goals

1. Add `category_counts: HashMap<String, u32>` to `SessionState` to accumulate a per-session
   histogram of stored knowledge categories, in-memory with no schema changes.
2. Call `record_category_store(session_id, category)` on `SessionRegistry` after each
   successful `context_store`, alongside existing usage recording.
3. Add `session_id: Option<String>` to `ServiceSearchParams` so the search pipeline can
   receive session context from the tool handler.
4. Thread `session_id` from `ctx.audit_ctx.session_id` into `ServiceSearchParams` in the
   `context_search` handler.
5. Apply a phase-conditioned category affinity boost in `SearchService` as a final additive
   step after NLI re-ranking and co-access boosting, using both `current_phase` (explicit,
   WA-1) and `category_counts` (implicit histogram) when available.
6. Make the affinity weight constants (`PHASE_AFFINITY_WEIGHT`, `HISTOGRAM_AFFINITY_WEIGHT`)
   config-driven from day one, as required by the product vision.
7. Include a category histogram summary in the UDS `CompactPayload` synthesis output at
   PreCompact time (additive change to `format_compaction_payload`).

## Non-Goals

- **No Markov model or transition prediction.** The boost is computed directly from the
  current-session histogram. Learning `P(next_category | sequence)` is W3-1 (GNN).
- **No phase-annotated CoAccess edges.** Adding phase context to graph edges requires W1-5
  (Observation Pipeline Generalization) and is a separate design decision.
- **No GNN training data production.** The histogram feeds the ranking formula at query time
  only. It does not write training labels.
- **No changes to `context_briefing` behavior.** The affinity boost applies in `SearchService`
  (the `context_search` hot path) only. `context_briefing` is out of scope.
- **No changes to `context_lookup`.** Lookup is deterministic (filter-based), not
  similarity-ranked. The affinity signal is not meaningful there.
- **No histogram decay.** Starting without time-decay on older counts; add later if
  long-running sessions show stale histogram degrading results (OQ-02).
- **No schema migrations.** The histogram is in-memory per-session state only.
- **No changes to WA-3 (MissedRetrieval) or WA-4 (Proactive Delivery).** This feature
  unblocks WA-4 but does not implement it.
- **No changes to the `context_cycle` tool or phase machinery** — those are WA-1 (crt-025,
  already complete).

## Background Research

### What Already Exists

**WA-1 complete (crt-025, GH #330).**
`SessionState.current_phase: Option<String>` is set via `set_current_phase()` on every
`context_cycle(type: "phase", ...)` event. The `context_store` handler already snapshots
`current_phase` synchronously before any await (ADR-001 crt-025 SR-07) to avoid a race
with a concurrent `phase-end` event. This pattern is the template for the `category_counts`
snapshot.

**`session_id` already flows into `SearchParams` (MCP input layer).**
`SearchParams.session_id: Option<String>` is present at line 64 of `mcp/tools.rs` with
`#[serde(default)]`. It flows through `build_context()` into `ctx.audit_ctx.session_id`
where it is used for usage recording and query log entries. It is **not** passed to
`ServiceSearchParams` — the search ranking pipeline never sees it.

**`ServiceSearchParams` currently has no `session_id` field.**
`struct ServiceSearchParams` (in `services/search.rs`, line 203) has `query`, `k`,
`filters`, `similarity_floor`, `confidence_floor`, `feature_tag`, `co_access_anchors`,
`caller_agent_id`, and `retrieval_mode`. The `session_id` field is the only addition
required to close the gap.

**`SearchService` already has WA-2 extension stubs.**
`FusedScoreInputs` has a comment at line 55: `WA-2 extension: add phase_boost_norm: f64
here when WA-2 is implemented`. `FusionWeights` has a parallel comment at line 89. The
pure function `compute_fused_score` has a comment at line 179. These are integration points,
not a promise to use that specific struct field — the boost can be applied as a final
additive step after `compute_fused_score` returns, which is the approach specified by the
product vision and ASS-028.

**WA-0 shipped formula has 0.05 headroom reserved for WA-2.**
The shipped `InferenceConfig` defaults sum to `0.95`:
`w_nli(0.35) + w_sim(0.25) + w_conf(0.15) + w_coac(0.10) + w_util(0.05) + w_prov(0.05) = 0.95`.
The product vision explicitly notes `sum=0.95, 0.05 headroom for WA-2`.

**`InferenceConfig` is the established config-driven extension point.**
All six fusion weights are declared in `InferenceConfig` under `[inference]` in `config.toml`
with `#[serde(default = "default_w_*")]`. The same pattern applies for `w_phase_explicit`
and `w_phase_histogram`. The `FusionWeights::from_config()` constructor builds the struct
from `InferenceConfig`; this is where the new fields are read.

**`format_compaction_payload` is the UDS injection synthesis surface.**
`handle_compact_payload` in `uds/listener.rs` builds a `CompactionCategories` struct
(decisions, injections, conventions), calls `format_compaction_payload`, and returns a
`HookResponse::BriefingContent`. The category histogram summary is appended as an
informational block to the formatted payload content before returning. The function
already reads `session_state` from `SessionRegistry` — adding `category_counts` access is
an incremental change at that call site.

**The `context_store` handler already has a clean success gate.**
The handler performs: validate → category validate → snapshot `current_phase` → build
`NewEntry` → call `insert()` → guard on `insert_result.duplicate_of` → seed confidence →
record usage. The `record_category_store` call belongs after the duplicate guard and before
confidence seeding (step 8), mirroring the placement of usage recording.

### Technical Landscape: Existing Ranking Pipeline

Post-WA-0 (crt-024) the ranking pipeline in `SearchService` is:

```
HNSW(k=20 when NLI active, else k) → try_nli_rerank → compute_fused_score (per candidate)
→ sort by final_score → status_penalty → co-access boost (additive) → top-k
```

The affinity boost is applied **after** the existing pipeline as a final additive step —
same integration position as co-access. Neither the fused score struct nor the pure function
needs to change; the boost is computed separately using the session histogram and added to
`final_score` before the final sort.

### Constraints Discovered

**WA-2 extension stubs in search.rs are informational, not structural requirements.**
The stubs suggest integrating `phase_boost_norm` into `FusedScoreInputs` and adding `w_phase`
to `FusionWeights`. This is a valid approach but requires the boost to be normalized to
[0,1] and plugged into `compute_fused_score`. The product vision formula is additive outside
the fused score function. Both are architecturally sound; the choice affects whether the
phase boost participates in the `FusionWeights` sum-constraint invariant. **This is an
open question for the architect** (see Open Questions OQ-01).

**`FusionWeights` has an invariant:** `w_sim + w_nli + w_conf + w_coac + w_util + w_prov
<= 1.0` enforced by `InferenceConfig::validate()` at startup. Adding `w_phase_explicit +
w_phase_histogram` to this sum means the existing defaults (`sum=0.95`) would need to be
reduced by 0.02 total to stay within the invariant, OR the invariant is extended to
`sum <= 1.0 + phase_budget`. The simpler path is to treat the phase boost as an uncapped
additive post-step (as ASS-028 and the product vision text describe), keeping it outside
`compute_fused_score` and outside the invariant. This avoids touching `InferenceConfig::validate`.

**No `Arc<SessionRegistry>` on `SearchService` currently.**
`SearchService` receives `session_id` via `ServiceSearchParams`. To look up the histogram,
`SearchService::search()` needs access to `SessionRegistry`. Currently `SearchService` holds
references to `Store`, `AsyncVectorStore`, `EmbedServiceHandle`, etc., but not
`SessionRegistry`. Either `SessionRegistry` is added as a field on `SearchService`, or the
histogram is resolved before calling `search()` and passed directly in `ServiceSearchParams`.
**This is a structural decision for the architect** (see Open Questions OQ-02).

## Proposed Scope

### Component List

**1. `SessionState.category_counts` field + `record_category_store()` method**
File: `crates/unimatrix-server/src/infra/session.rs`

Add `category_counts: HashMap<String, u32>` to `SessionState`. Initialize as
`HashMap::new()` in `register_session`. Add method to `SessionRegistry`:

```rust
pub fn record_category_store(&self, session_id: &str, category: &str) {
    let mut sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(state) = sessions.get_mut(session_id) {
        *state.category_counts.entry(category.to_string()).or_insert(0) += 1;
    }
    // Unregistered session: silent no-op (consistent with record_injection)
}
```

Also add a getter: `pub fn get_category_histogram(&self, session_id: &str) -> HashMap<String, u32>`
that returns a clone of the histogram (or an empty map if not registered), for use in
`SearchService` and `format_compaction_payload`.

**2. `context_store` handler: call `record_category_store` after success**
File: `crates/unimatrix-server/src/mcp/tools.rs`

After the duplicate guard (step 7), before confidence seeding (step 8), add:

```rust
// crt-026: record category histogram for session affinity boost (WA-2)
if let Some(ref sid) = ctx.audit_ctx.session_id {
    self.session_registry.record_category_store(sid, &params.category);
}
```

This follows the same `if let Some(ref sid)` guard pattern used for injection recording
and is fire-and-synchronous (no spawn needed — lock hold is microseconds).

**3. `ServiceSearchParams.session_id` field**
File: `crates/unimatrix-server/src/services/search.rs`

Add `pub session_id: Option<String>` to `ServiceSearchParams`. Mark existing dead-code
fields consistently. This is a data-carrier field — no logic in `ServiceSearchParams` itself.

**4. `context_search` handler: thread `session_id` through**
File: `crates/unimatrix-server/src/mcp/tools.rs`

In `ServiceSearchParams` construction (step 4 in the handler), add:
```rust
session_id: ctx.audit_ctx.session_id.clone(),
```

**5. `SearchService`: category affinity boost**
File: `crates/unimatrix-server/src/services/search.rs`

After the existing pipeline produces a sorted `Vec<ScoredEntry>`, apply the affinity boost
as a final additive re-scoring step:

**Boost formula (per product vision WA-2 section, Decision 1):**

When `current_phase` is set (explicit signal from WA-1):
```
boost += phase_category_weight(entry.category, current_phase) * PHASE_AFFINITY_WEIGHT
```
where `phase_category_weight` returns the fraction of entries in `expected_categories(phase)`
that match `entry.category` — or 1.0 for an exact match (simple binary encoding for
cold-start; W3-1 replaces with learned weights).

When histogram is available (implicit fallback, always applies when histogram is non-empty):
```
p(category) = count(category) / total_stores
boost += p(entry.category) * HISTOGRAM_AFFINITY_WEIGHT
```

Both terms are additive. When both signals are present, both apply (product vision: "When
both signals are present, both apply"). Cold-start (empty histogram, no phase): boost = 0.0
for all entries — existing behavior preserved exactly.

`PHASE_AFFINITY_WEIGHT` and `HISTOGRAM_AFFINITY_WEIGHT` are read from `InferenceConfig`
(see Component 6).

To access the histogram, `SearchService` either receives a pre-resolved `HashMap<String, u32>`
via `ServiceSearchParams`, or holds an `Arc<SessionRegistry>`. See OQ-02.

**6. Config: `PHASE_AFFINITY_WEIGHT` and `HISTOGRAM_AFFINITY_WEIGHT` constants in `InferenceConfig`**
File: `crates/unimatrix-server/src/infra/config.rs`

Add two fields to `InferenceConfig`:

```toml
# [inference]
w_phase_explicit = 0.015   # boost weight for explicit phase signal (WA-1 current_phase)
w_phase_histogram = 0.005  # boost weight for implicit histogram signal
```

Defaults: `w_phase_explicit = 0.015`, `w_phase_histogram = 0.005` per product vision WA-2.
These are outside the `compute_fused_score` sum-invariant (`<= 1.0`) and therefore do not
require changing the existing validation. Max combined boost per entry: `0.015 + 0.005 = 0.02`.
This falls within the 0.05 headroom.

The `FusionWeights` struct receives corresponding `w_phase_explicit: f64` and
`w_phase_histogram: f64` fields, constructed in `FusionWeights::from_config()`.

**7. UDS injection: category histogram summary in `format_compaction_payload`**
File: `crates/unimatrix-server/src/uds/listener.rs`

In `handle_compact_payload`, after resolving `session_state`, extract `category_counts` and
pass it to `format_compaction_payload`. Append an informational block to the payload output:

```
Recent session activity: decision × 3, pattern × 2 (design phase signal)
```

Format: emit only categories with count > 0, sorted by count descending, capped at top-5
categories. If histogram is empty, omit the block entirely. Apply to the `MAX_INJECTION_BYTES`
budget — the block is small (< 100 bytes for typical sessions).

### Affinity Boost Formula: Discrepancy Between ASS-028 and Product Vision

**ASS-028 specifies:** a single flat `AFFINITY_WEIGHT = 0.02`, applied as
`p(entry.category) * 0.02` from the histogram only. No explicit phase signal term.

**Product vision WA-2 specifies:** two separate terms:
- Explicit phase signal: `phase_category_weight(entry.category, current_phase) * 0.015`
- Implicit histogram fallback: `p(entry.category) * 0.005`

Both apply when both signals are present. The explicit signal is 3× the histogram signal.
Product vision Decision 1 (p.780) confirms: `phase_boost * 0.015 + histogram_boost * 0.005`.

**Interpretation:** The product vision is authoritative — it was written after ASS-028 and
explicitly supersedes the flat formula with a two-term design. The ASS-028 single-term
formula is the simplification the spike used before the final design was settled.

**Flag for architect review:** The two-term formula requires defining
`phase_category_weight(category, phase)` — a mapping from phase strings to expected category
sets. Since phase strings are opaque to Unimatrix (product vision WA-1: "the phase string is
opaque, stored as metadata, not interpreted"), a static mapping in the codebase would couple
the ranking to the SM's phase vocabulary. See OQ-03.

## Acceptance Criteria

- AC-01: `SessionState` has a `category_counts: HashMap<String, u32>` field, initialized to
  empty on `register_session`.
- AC-02: A successful `context_store` call increments `category_counts[category]` by 1 in
  the session registry. A duplicate store (where `insert_result.duplicate_of.is_some()`)
  does NOT increment the histogram.
- AC-03: `record_category_store` is a silent no-op when `session_id` is `None` or the
  session is not registered.
- AC-04: `ServiceSearchParams` has a `session_id: Option<String>` field.
- AC-05: The `context_search` handler passes `ctx.audit_ctx.session_id.clone()` into
  `ServiceSearchParams.session_id`.
- AC-06: When `session_id` is present and the session histogram is non-empty, `SearchService`
  applies the histogram affinity boost (`p(category) * w_phase_histogram`) to each result
  entry's final score.
- AC-07: When `current_phase` is set on the session (WA-1), `SearchService` applies the
  explicit phase affinity boost (`phase_category_weight * w_phase_explicit`) to each result
  entry's final score.
- AC-08: When the session histogram is empty (cold start), the affinity boost is 0.0 for all
  entries — `SearchService` output is identical to current behavior.
- AC-09: Both `w_phase_explicit` and `w_phase_histogram` are configurable in `[inference]`
  config with defaults `0.015` and `0.005` respectively.
- AC-10: The boost terms are applied AFTER the existing fused score + co-access pipeline,
  not inside `compute_fused_score`.
- AC-11: The `CompactPayload` synthesis output includes a `Recent session activity: ...`
  block when the session has a non-empty histogram. The block is omitted when the histogram
  is empty.
- AC-12: Test: a result whose category matches the session histogram ranks higher than an
  otherwise equal result whose category is absent from the histogram.
- AC-13: Test: a result whose category is NOT in the session histogram receives boost = 0.0
  from the histogram term.
- AC-14: `FusionWeights` and `FusedScoreInputs` WA-2 extension stubs are resolved — either
  the struct fields are added (if boost is integrated into `compute_fused_score`) or the
  stubs are updated to document the post-pipeline approach.

## Constraints

- **WA-1 dependency.** `current_phase` on `SessionState` (and `set_current_phase()`) are
  provided by crt-025 (GH #330, complete). WA-2 uses them but does not modify them.
- **No schema changes.** `category_counts` is in-memory per-session state only. No new
  tables, no migrations.
- **No new crates.** All changes are in `crates/unimatrix-server`.
- **`FusionWeights` sum-invariant.** The phase boost terms must remain outside
  `compute_fused_score` if they are to avoid touching `InferenceConfig::validate`. If
  integrated into `compute_fused_score`, the existing weight defaults must be adjusted
  (e.g., `w_prov` from 0.05 to 0.03) to keep `sum <= 1.0`. Architect must decide.
- **Boost must be bounded.** `p(category)` is in [0,1]; max boost = `w_phase_explicit +
  w_phase_histogram = 0.02` with the default weights. Must not push a weak-similarity
  entry above a strong-similarity entry. The 0.02 ceiling maintains this invariant with
  current weight calibrations (NLI dominant at 0.35, similarity at 0.25).
- **Cold-start safety.** Empty histogram → zero boost for all entries. Exact behavioral
  parity with current pipeline when no session context exists.
- **`SessionRegistry` access in `SearchService`.** Either `Arc<SessionRegistry>` is added
  as a `SearchService` field, or the histogram is resolved by the tool handler and passed
  through `ServiceSearchParams`. Architect must decide (OQ-02).
- **Phase string vocabulary is opaque.** Unimatrix does not interpret phase strings (WA-1
  ADR). The `phase_category_weight` function must either use a config-driven mapping or
  treat `current_phase` as a pure histogram signal (ignoring the explicit phase for this
  feature). See OQ-03.
- **Hook timeout budget.** The UDS injection path operates under a 40ms total budget
  (`HOOK_TIMEOUT`). The histogram summary addition is a string-formatting step on pre-resolved
  data — no I/O, no blocking — and adds negligible latency.

## Resolved Decisions

**OQ-01 → RESOLVED: Integrate into `compute_fused_score`.**
The histogram affinity term ships as a first-class dimension in `FusedScoreInputs` /
`FusionWeights` / `compute_fused_score`. W3-1 sees the full weight vector and can tune
`w_phase_histogram` alongside all other terms. The WA-0 headroom (`sum=0.95`) was reserved
for exactly this. The stubs in `search.rs` (`FusedScoreInputs`, `FusionWeights`,
`compute_fused_score`) are the implementation contract.

**OQ-02 → RESOLVED: Pre-resolve histogram in handler, pass through `ServiceSearchParams`.**
The tool handler resolves `(category_histogram, current_phase)` from `SessionRegistry`
before constructing `ServiceSearchParams`. `ServiceSearchParams` carries the resolved data
as a plain struct field — it does not receive `Arc<SessionRegistry>`. This keeps
`SearchService` dependency-free of session infrastructure and matches the crt-025 SR-07
snapshot pattern.

**OQ-03 → RESOLVED: Ship histogram term only; defer explicit phase term to W3-1.**
crt-026 ships `w_phase_histogram * p(entry.category)` only. The explicit phase term
(`w_phase_explicit * phase_category_weight(category, phase)`) is deferred: `current_phase`
is opaque to Unimatrix and a static mapping would couple ranking to SM vocabulary.
W3-1 will learn the phase→category relationship from training data. `w_phase_explicit`
field reserved in `InferenceConfig` at default `0.0` as a placeholder for W3-1.

**OQ-04 → RESOLVED: UDS search path also passes `session_id`.**
`handle_context_search` in `uds/listener.rs` passes `session_id` through
`ServiceSearchParams` so the histogram boost fires on hook-driven searches as well as
direct MCP calls.

## Tracking

GH Issue: #341
