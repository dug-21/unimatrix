# Architecture: crt-002 Confidence Evolution

## System Overview

crt-002 adds a confidence computation engine to Unimatrix that transforms raw usage signals into a single quality score per knowledge entry. The computation is embedded directly into existing write paths -- no new crates, no new tables, no new background processes.

The feature touches two crates:
- **unimatrix-store**: New `update_confidence()` method for targeted confidence-only writes. Extended `record_usage()` to accept a confidence computation function and apply it inline.
- **unimatrix-server**: New `confidence.rs` module containing the formula, component functions, and Wilson score implementation. Search re-ranking logic in the `context_search` handler.

The architecture follows three principles:
1. **Merge into existing write transactions** -- confidence computation happens inside `record_usage()`, not as a separate read-modify-write cycle (SR-03, SR-08).
2. **Pure computation, impure integration** -- the confidence formula and all component functions are pure (deterministic, no side effects). Integration with tool handlers is the only impure code.
3. **f64 intermediates, f32 result** -- all statistical computation (Wilson score, logarithms) uses f64 to avoid precision loss (SR-01). The final composite is cast to f32 for storage.

```
Claude Code / MCP Client
        |
        | stdio (JSON-RPC 2.0)
        v
unimatrix-server (binary)
  |-- confidence.rs (NEW)         -- Pure functions: formula, Wilson, components
  |-- tools.rs                    -- EXTENDED: context_search re-ranking
  |-- server.rs                   -- EXTENDED: confidence on insert, correct, deprecate
  |     |
  |     | record_usage_for_entries() passes confidence_fn to store
  |     v
  v
unimatrix-store
  |-- write.rs                    -- EXTENDED: record_usage() computes confidence inline
  |                                  NEW: update_confidence() for mutation paths
  |-- schema.rs                   -- unchanged (confidence field already exists)
```

## Component Breakdown

### C1: Confidence Module (`crates/unimatrix-server/src/confidence.rs`) -- NEW

Pure computation module. No I/O, no state, no dependencies beyond `std`.

**Constants:**
```rust
/// Weights for the additive composite formula. Must sum to 1.0.
pub const W_BASE: f32 = 0.20;
pub const W_USAGE: f32 = 0.15;
pub const W_FRESH: f32 = 0.20;
pub const W_HELP: f32 = 0.15;
pub const W_CORR: f32 = 0.15;
pub const W_TRUST: f32 = 0.15;

/// Access counts beyond this contribute negligible signal.
pub const MAX_MEANINGFUL_ACCESS: f64 = 50.0;

/// Freshness half-life in hours (1 week).
pub const FRESHNESS_HALF_LIFE_HOURS: f64 = 168.0;

/// Minimum votes (helpful + unhelpful) before Wilson score deviates from neutral.
pub const MINIMUM_SAMPLE_SIZE: u32 = 5;

/// Wilson score z-value for 95% confidence interval.
pub const WILSON_Z: f64 = 1.96;

/// Similarity weight for search re-ranking blend.
pub const SEARCH_SIMILARITY_WEIGHT: f32 = 0.85;
```

**Component functions** (all pure, all `f64 -> f64` internally, documented bounds):

```rust
/// Base quality proxy. Active entries = 0.5, Deprecated = 0.2.
pub fn base_score(status: Status) -> f64

/// Log-transformed access frequency, clamped to [0.0, 1.0].
/// usage_score(0) = 0.0, usage_score(50) = 1.0, usage_score(500) = 1.0 (clamped)
pub fn usage_score(access_count: u32) -> f64

/// Exponential decay from reference timestamp. [0.0, 1.0].
/// freshness_score(just_now) ~= 1.0, freshness_score(1_week_ago) ~= 0.37
pub fn freshness_score(last_accessed_at: u64, created_at: u64, now: u64) -> f64

/// Wilson score lower bound with minimum sample guard. [0.0, 1.0].
/// Returns 0.5 when total_votes < MINIMUM_SAMPLE_SIZE.
pub fn helpfulness_score(helpful_count: u32, unhelpful_count: u32) -> f64

/// Correction chain quality signal. [0.0, 1.0].
/// 0 corrections = 0.5, 1-2 = 0.8, 3-5 = 0.6, 6+ = 0.3
pub fn correction_score(correction_count: u32) -> f64

/// Trust source of creator. [0.0, 1.0].
/// "human" = 1.0, "system" = 0.7, "agent" = 0.5, other = 0.3
pub fn trust_score(trust_source: &str) -> f64

/// Wilson score lower bound calculation (95% confidence).
/// Internal helper, not public.
fn wilson_lower_bound(positive: f64, total: f64) -> f64
```

**Composite function:**
```rust
/// Compute confidence for an entry at the given timestamp.
///
/// Returns f32 in [0.0, 1.0]. All intermediate computation uses f64.
/// The function is pure: given the same inputs, it always returns the same output.
pub fn compute_confidence(entry: &EntryRecord, now: u64) -> f32
```

This function calls all six component functions, applies weights, sums, and clamps to [0.0, 1.0]. The `EntryRecord` import is from `unimatrix_store` (re-exported through `unimatrix_core`).

**Re-ranking function:**
```rust
/// Blend similarity and confidence for search result re-ranking.
///
/// final_score = SEARCH_SIMILARITY_WEIGHT * similarity + (1 - SEARCH_SIMILARITY_WEIGHT) * confidence
pub fn rerank_score(similarity: f32, confidence: f32) -> f32
```

### C2: Store Confidence Update (`crates/unimatrix-store/src/write.rs`) -- EXTENDED

Two additions to the store layer:

**Inline confidence in `record_usage()`:**

Extend `record_usage()` to accept an optional confidence computation function. After updating counters for each entry, if the function is provided, compute new confidence and write it in the same transaction.

```rust
impl Store {
    /// Record usage for a batch of entries in a single write transaction.
    /// If `confidence_fn` is provided, recompute and update confidence for each entry
    /// after applying counter updates. This avoids a separate read-modify-write cycle.
    pub fn record_usage_with_confidence(
        &self,
        all_ids: &[u64],
        access_ids: &[u64],
        helpful_ids: &[u64],
        unhelpful_ids: &[u64],
        decrement_helpful_ids: &[u64],
        decrement_unhelpful_ids: &[u64],
        confidence_fn: Option<&dyn Fn(&EntryRecord, u64) -> f32>,
    ) -> Result<()>
```

The existing `record_usage()` method is preserved as-is (calls `record_usage_with_confidence` with `confidence_fn: None`) to maintain backward compatibility for the `EntryStore::record_access` trait implementation.

**Targeted confidence-only update:**

```rust
impl Store {
    /// Update only the confidence field of an entry.
    /// Reads the entry, sets confidence, writes back. No index diffs.
    /// Used for mutation paths (insert, correct, deprecate) where
    /// record_usage is not called.
    pub fn update_confidence(&self, entry_id: u64, confidence: f32) -> Result<()>
```

This is a minimal read-modify-write that skips all index table operations. It reads from ENTRIES, deserializes, sets `record.confidence = confidence`, re-serializes, and writes back to ENTRIES. No other tables are touched.

### C3: Server Integration -- Retrieval Path (`crates/unimatrix-server/src/server.rs`) -- EXTENDED

Modify `record_usage_for_entries()` to pass the confidence function to the store:

```rust
pub(crate) async fn record_usage_for_entries(
    &self,
    agent_id: &str,
    trust_level: TrustLevel,
    entry_ids: &[u64],
    helpful: Option<bool>,
    feature: Option<&str>,
) {
    // ... existing dedup logic (steps 1-2 unchanged) ...

    // Step 3: Record usage WITH confidence computation
    let store = Arc::clone(&self.store);
    // ... clone owned data ...

    let usage_result = tokio::task::spawn_blocking(move || {
        store.record_usage_with_confidence(
            &all_ids,
            &access_ids_owned,
            &helpful_owned,
            &unhelpful_owned,
            &dec_helpful_owned,
            &dec_unhelpful_owned,
            Some(&confidence::compute_confidence),
        )
    }).await;

    // ... existing error handling ...
    // Step 4: Feature entries (unchanged)
}
```

The confidence computation happens inside the same `spawn_blocking` as usage recording. No additional async task, no additional write transaction.

### C4: Server Integration -- Mutation Paths (`crates/unimatrix-server/src/server.rs` + `tools.rs`) -- EXTENDED

Three mutation paths need confidence updates:

**context_store (insert):**
After the entry is inserted (in the existing `spawn_blocking` in the tool handler), compute initial confidence and call `store.update_confidence(id, confidence)`. This is a second write transaction, but inserts are infrequent (not per-retrieval) so the overhead is acceptable.

```rust
// In context_store handler, after insert completes:
let entry = store.get(id)?;
let confidence = confidence::compute_confidence(&entry, now);
let _ = store.update_confidence(id, confidence);
```

**context_correct (correction chain):**
The correction creates a new entry (the correction) and deprecates the old one. Both need confidence updates:
- New correction entry: initial confidence computed from its fields
- Old original entry: status changed to Deprecated, recompute with reduced base_score

These updates happen after the existing combined write transaction commits, using `update_confidence()`.

**context_deprecate:**
After `deprecate_with_audit()` completes, recompute confidence for the deprecated entry with the reduced base_score (0.2).

All mutation-path confidence updates follow fire-and-forget: errors are logged, not propagated.

### C5: Search Re-ranking (`crates/unimatrix-server/src/tools.rs`) -- EXTENDED

In the `context_search` handler, after fetching full entries for search results (step 9), re-rank using the confidence blend:

```rust
// Step 9: Fetch full entries for results (existing)
let mut results_with_scores = Vec::new();
for sr in &search_results {
    match self.entry_store.get(sr.entry_id).await {
        Ok(entry) => results_with_scores.push((entry, sr.similarity)),
        Err(_) => continue,
    }
}

// Step 9b: Re-rank by blended score (NEW)
results_with_scores.sort_by(|(a, sim_a), (b, sim_b)| {
    let score_a = confidence::rerank_score(*sim_a, a.confidence);
    let score_b = confidence::rerank_score(*sim_b, b.confidence);
    score_b.partial_cmp(&score_a).unwrap_or(std::cmp::Ordering::Equal)
});
```

This re-ranking applies ONLY to `context_search`. Per human clarification:
- `context_get`: single entry by ID, no ranking
- `context_lookup`: deterministic metadata query, no similarity scores, ordering unchanged
- `context_briefing`: internal search component gets re-ranked (via `context_search` call path), lookup/get paths remain deterministic

## Component Interactions

### Retrieval Flow (with Confidence)

```
Agent calls context_search(query, feature="crt-002", helpful=true)
    |
    v
1-6. Identity, capability, validation, category, scanning, embed (unchanged)
7.   Vector search -> top-k candidates
8.   Fetch full entries
9.   RE-RANK by blended score (NEW - C5)
10.  Format response (uses entry.confidence which is now meaningful)
11.  Audit (unchanged)
12.  Usage recording WITH confidence (MODIFIED - C3):
     a. Dedup checks (unchanged)
     b. record_usage_with_confidence() in spawn_blocking:
        - Update counters (existing)
        - Compute confidence from updated record (NEW - C1)
        - Write updated entry with new confidence (same transaction)
     c. Feature entries (unchanged)
```

Key: The confidence shown in the response (step 10) is from the PREVIOUS retrieval's computation. The current retrieval updates confidence AFTER formatting the response. This is a deliberate one-retrieval lag -- the alternative (computing confidence before formatting) would require a write transaction before the response, defeating the fire-and-forget pattern. The lag is one retrieval at most, and for most entries, the freshness component is the only part that changes between retrievals.

### Mutation Flow (Insert)

```
Agent calls context_store(title, content, ...)
    |
    v
1-8. Identity, capability, validation, category, scanning, embed, near-dup, insert (unchanged)
9.   Compute initial confidence (NEW - C4):
     a. Read inserted entry
     b. compute_confidence(entry, now) -> f32
     c. update_confidence(id, confidence) -> fire-and-forget
10.  Format response (confidence is now non-zero)
```

### Mutation Flow (Correction)

```
Agent calls context_correct(original_id, title, content, ...)
    |
    v
1-14. Existing correction flow (unchanged — combined write transaction)
15.   Confidence updates (NEW - C4):
      a. compute_confidence(new_correction, now)
      b. update_confidence(new_id, confidence)
      c. Recompute confidence for deprecated original
      d. update_confidence(original_id, confidence)
```

### Transaction Model

```
Retrieval path:
    READ txn (query/search)
    |
    WRITE txn (record_usage_with_confidence)  <-- ONE transaction for both usage + confidence
    |
    WRITE txn (record_feature_entries)         <-- conditional, unchanged

Mutation path (insert):
    WRITE txn (insert entry + indexes)         <-- existing
    |
    WRITE txn (update_confidence)              <-- NEW, confidence only

Mutation path (correct):
    WRITE txn (correction chain + indexes)     <-- existing combined txn
    |
    WRITE txn (update_confidence for new)      <-- NEW
    WRITE txn (update_confidence for original) <-- NEW
```

The retrieval path has NO additional write transactions compared to crt-001 -- confidence is merged into the existing `record_usage` transaction (SR-03 resolved).

## Technology Decisions

### ADR-001: Inline Confidence in Usage Write Transaction
See `architecture/ADR-001-inline-confidence-in-usage-write.md`.

### ADR-002: f64 Intermediate Computation
See `architecture/ADR-002-f64-intermediate-computation.md`.

### ADR-003: No Confidence Floor
See `architecture/ADR-003-no-confidence-floor.md`.

### ADR-004: One-Retrieval Confidence Lag
See `architecture/ADR-004-one-retrieval-confidence-lag.md`.

### ADR-005: Search Re-ranking Scope
See `architecture/ADR-005-search-reranking-scope.md`.

## Integration Points

### Upstream Dependencies (Consumed)
- **unimatrix-store `EntryRecord`** -- all 26 fields, especially: `confidence`, `access_count`, `last_accessed_at`, `created_at`, `helpful_count`, `unhelpful_count`, `correction_count`, `version`, `trust_source`, `status`
- **unimatrix-store `record_usage()`** -- extended to `record_usage_with_confidence()`
- **unimatrix-store `Store::get()`** -- read entry for mutation-path confidence computation
- **unimatrix-store `serialize_entry` / `deserialize_entry`** -- used in `update_confidence()`
- **unimatrix-server `record_usage_for_entries()`** -- extended to pass confidence function
- **unimatrix-server tool handlers** -- `context_search` (re-ranking), `context_store` (seed), `context_correct` (recompute), `context_deprecate` (recompute)
- **unimatrix-store `Status` enum** -- used in `base_score()` to differentiate active vs deprecated

### Downstream Consumers (Served)
- **crt-003** (contradiction detection) -- reads `confidence` to assess whether flagged entries are high-confidence contradictions (higher severity)
- **crt-004** (co-access boosting) -- reads `confidence` as baseline before applying co-access boost
- **mtx-002** (knowledge explorer) -- displays confidence in UI, confidence trends
- **All retrieval tools** -- display computed (non-zero) confidence in responses

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `compute_confidence` | `pub fn compute_confidence(entry: &EntryRecord, now: u64) -> f32` | `crates/unimatrix-server/src/confidence.rs` |
| `rerank_score` | `pub fn rerank_score(similarity: f32, confidence: f32) -> f32` | `crates/unimatrix-server/src/confidence.rs` |
| `base_score` | `pub fn base_score(status: Status) -> f64` | `crates/unimatrix-server/src/confidence.rs` |
| `usage_score` | `pub fn usage_score(access_count: u32) -> f64` | `crates/unimatrix-server/src/confidence.rs` |
| `freshness_score` | `pub fn freshness_score(last_accessed_at: u64, created_at: u64, now: u64) -> f64` | `crates/unimatrix-server/src/confidence.rs` |
| `helpfulness_score` | `pub fn helpfulness_score(helpful_count: u32, unhelpful_count: u32) -> f64` | `crates/unimatrix-server/src/confidence.rs` |
| `correction_score` | `pub fn correction_score(correction_count: u32) -> f64` | `crates/unimatrix-server/src/confidence.rs` |
| `trust_score` | `pub fn trust_score(trust_source: &str) -> f64` | `crates/unimatrix-server/src/confidence.rs` |
| `wilson_lower_bound` | `fn wilson_lower_bound(positive: f64, total: f64) -> f64` (private) | `crates/unimatrix-server/src/confidence.rs` |
| `W_BASE` | `pub const W_BASE: f32 = 0.20` | `crates/unimatrix-server/src/confidence.rs` |
| `W_USAGE` | `pub const W_USAGE: f32 = 0.15` | `crates/unimatrix-server/src/confidence.rs` |
| `W_FRESH` | `pub const W_FRESH: f32 = 0.20` | `crates/unimatrix-server/src/confidence.rs` |
| `W_HELP` | `pub const W_HELP: f32 = 0.15` | `crates/unimatrix-server/src/confidence.rs` |
| `W_CORR` | `pub const W_CORR: f32 = 0.15` | `crates/unimatrix-server/src/confidence.rs` |
| `W_TRUST` | `pub const W_TRUST: f32 = 0.15` | `crates/unimatrix-server/src/confidence.rs` |
| `MAX_MEANINGFUL_ACCESS` | `pub const MAX_MEANINGFUL_ACCESS: f64 = 50.0` | `crates/unimatrix-server/src/confidence.rs` |
| `FRESHNESS_HALF_LIFE_HOURS` | `pub const FRESHNESS_HALF_LIFE_HOURS: f64 = 168.0` | `crates/unimatrix-server/src/confidence.rs` |
| `MINIMUM_SAMPLE_SIZE` | `pub const MINIMUM_SAMPLE_SIZE: u32 = 5` | `crates/unimatrix-server/src/confidence.rs` |
| `WILSON_Z` | `pub const WILSON_Z: f64 = 1.96` | `crates/unimatrix-server/src/confidence.rs` |
| `SEARCH_SIMILARITY_WEIGHT` | `pub const SEARCH_SIMILARITY_WEIGHT: f32 = 0.85` | `crates/unimatrix-server/src/confidence.rs` |
| `Store::record_usage_with_confidence` | `pub fn record_usage_with_confidence(&self, all_ids: &[u64], access_ids: &[u64], helpful_ids: &[u64], unhelpful_ids: &[u64], decrement_helpful_ids: &[u64], decrement_unhelpful_ids: &[u64], confidence_fn: Option<&dyn Fn(&EntryRecord, u64) -> f32>) -> Result<()>` | `crates/unimatrix-store/src/write.rs` |
| `Store::update_confidence` | `pub fn update_confidence(&self, entry_id: u64, confidence: f32) -> Result<()>` | `crates/unimatrix-store/src/write.rs` |
