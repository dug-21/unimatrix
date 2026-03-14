# Component: Search Utility Delta

**File**: `crates/unimatrix-server/src/services/search.rs` (modified)

**Purpose**: Apply a per-entry additive utility delta based on effectiveness classification at all
four `rerank_score` call sites in `SearchService::search()`. The delta is derived from a
generation-cached snapshot of `EffectivenessState.categories` taken under a short read lock at
the top of `search()`.

---

## New Fields on `SearchService`

```
struct SearchService {
    // ... existing fields unchanged ...

    /// crt-018b (ADR-001): effectiveness classification snapshot for utility delta.
    /// Arc clone received from ServiceLayer; shared with BriefingService and background tick.
    effectiveness_state: EffectivenessStateHandle,

    /// crt-018b (ADR-001): generation-cached snapshot shared across rmcp clones.
    /// Arc<Mutex<_>> ensures all clones of SearchService share one cache object (R-06).
    cached_snapshot: Arc<Mutex<EffectivenessSnapshot>>,
}
```

---

## Modified: `SearchService::new()`

Add two parameters:

```
pub(crate) fn new(
    store: Arc<Store>,
    vector_store: Arc<AsyncVectorStore<VectorAdapter>>,
    entry_store: Arc<AsyncEntryStore<StoreAdapter>>,
    embed_service: Arc<EmbedServiceHandle>,
    adapt_service: Arc<AdaptationService>,
    gateway: Arc<SecurityGateway>,
    confidence_state: ConfidenceStateHandle,
    effectiveness_state: EffectivenessStateHandle,   // NEW
) -> Self:
    SearchService {
        store,
        vector_store,
        entry_store,
        embed_service,
        adapt_service,
        gateway,
        confidence_state,
        effectiveness_state,
        cached_snapshot: EffectivenessSnapshot::new_shared(),  // NEW
    }
```

---

## New Free Function: `utility_delta`

Defined as a module-level free function (or associated function) in `search.rs`:

```
fn utility_delta(category: Option<EffectivenessCategory>) -> f64:
    match category:
        Some(Effective)   =>  UTILITY_BOOST      // +0.05
        Some(Settled)     =>  SETTLED_BOOST      // +0.01
        Some(Ineffective) => -UTILITY_PENALTY    // -0.05
        Some(Noisy)       => -UTILITY_PENALTY    // -0.05
        Some(Unmatched)   =>  0.0
        None              =>  0.0
```

Where `UTILITY_BOOST`, `SETTLED_BOOST`, `UTILITY_PENALTY` are imported from
`unimatrix_engine::effectiveness`.

---

## Modified: `SearchService::search()` — Snapshot at Top

Insert immediately after the existing `confidence_weight` snapshot (lines 123-131 of current
`search.rs`), before Step 0 (rate check):

```
// crt-018b (ADR-001): snapshot effectiveness categories under short read lock.
// Generation comparison skips the HashMap clone on the common path (no state change).
// LOCK ORDERING: acquire read lock, read generation, DROP guard, then acquire mutex (R-01).
let categories: HashMap<u64, EffectivenessCategory> = {
    let current_generation = {
        let guard = self.effectiveness_state.read()
            .unwrap_or_else(|e| e.into_inner())
        let gen = guard.generation
        // read guard drops here (end of scope)
        gen
    }
    // Read guard is now dropped. Safe to acquire the mutex.
    let mut cache = self.cached_snapshot.lock()
        .unwrap_or_else(|e| e.into_inner())
    if cache.generation != current_generation:
        // State has changed since last call — re-clone categories
        let guard = self.effectiveness_state.read()
            .unwrap_or_else(|e| e.into_inner())
        cache.generation = guard.generation
        cache.categories = guard.categories.clone()
        // guard drops here
    // Return a clone of the cached categories for this call's use
    // This clone happens at most once per 15-minute background tick
    cache.categories.clone()
}
```

NOTE: The two-step acquire (read generation, drop, then conditionally re-acquire for clone) is
required to avoid holding both `effectiveness_state` read and `cached_snapshot` mutex
simultaneously, which would violate the lock ordering invariant (R-01, ADR-001).

For the common path (generation unchanged): 1 read lock acquisition, 1 mutex acquisition, 1
`HashMap` clone of the cached copy. The clone of `categories` for local use in `search()` is
unavoidable but is a local `HashMap<u64, EffectivenessCategory>` clone (not the stored state).

**Applies only to `RetrievalMode::Flexible`**: In `RetrievalMode::Strict`, the utility delta
is still computed (categories snapshot is taken regardless), but the entries have already been
hard-filtered to Active-only in Step 6a. The delta does not distort Strict results because the
formula still applies correctly to Active entries. The architecture specifies that the delta
"applies only to Flexible mode" — this means Strict mode entries are not present to be affected,
not that the code path branches. Implementation agent may choose to gate the snapshot acquisition
behind a `RetrievalMode::Flexible` check for performance clarity, but correctness is maintained
either way.

---

## Modified: Step 7 Sort (Initial Sort)

Replace the existing Step 7 sort comparator body:

**Before (current)**:
```
let base_a = rerank_score(*sim_a, entry_a.confidence, confidence_weight) + prov_a;
let base_b = rerank_score(*sim_b, entry_b.confidence, confidence_weight) + prov_b;
let penalty_a = penalty_map.get(&entry_a.id).copied().unwrap_or(1.0);
let penalty_b = penalty_map.get(&entry_b.id).copied().unwrap_or(1.0);
let final_a = base_a * penalty_a;
let final_b = base_b * penalty_b;
```

**After (crt-018b)**:
```
let prov_a = if entry_a.category == "lesson-learned" { PROVENANCE_BOOST } else { 0.0 }
let prov_b = if entry_b.category == "lesson-learned" { PROVENANCE_BOOST } else { 0.0 }
let delta_a = utility_delta(categories.get(&entry_a.id).copied())
let delta_b = utility_delta(categories.get(&entry_b.id).copied())
let base_a = rerank_score(*sim_a, entry_a.confidence, confidence_weight) + delta_a + prov_a
let base_b = rerank_score(*sim_b, entry_b.confidence, confidence_weight) + delta_b + prov_b
let penalty_a = penalty_map.get(&entry_a.id).copied().unwrap_or(1.0)
let penalty_b = penalty_map.get(&entry_b.id).copied().unwrap_or(1.0)
let final_a = base_a * penalty_a
let final_b = base_b * penalty_b
// compare final_b <=> final_a (descending)
```

ADR-003 placement: `utility_delta` is inside the parentheses, before the `* penalty` multiplication.
A Deprecated Effective entry: `(rerank + 0.05 + prov) * 0.7`. Not `rerank * 0.7 + 0.05`.

---

## Modified: Step 8 Sort (Co-Access Re-sort)

Replace the existing Step 8 comparator body:

**Before (current)**:
```
let base_a = rerank_score(*sim_a, entry_a.confidence, confidence_weight);
let base_b = rerank_score(*sim_b, entry_b.confidence, confidence_weight);
let boost_a = boost_map.get(&entry_a.id).copied().unwrap_or(0.0);
let boost_b = boost_map.get(&entry_b.id).copied().unwrap_or(0.0);
let prov_a = if entry_a.category == "lesson-learned" { PROVENANCE_BOOST } else { 0.0 };
let prov_b = if entry_b.category == "lesson-learned" { PROVENANCE_BOOST } else { 0.0 };
let penalty_a = penalty_map.get(&entry_a.id).copied().unwrap_or(1.0);
let penalty_b = penalty_map.get(&entry_b.id).copied().unwrap_or(1.0);
let final_a = (base_a + boost_a + prov_a) * penalty_a;
let final_b = (base_b + boost_b + prov_b) * penalty_b;
```

**After (crt-018b)**:
```
let base_a = rerank_score(*sim_a, entry_a.confidence, confidence_weight)
let base_b = rerank_score(*sim_b, entry_b.confidence, confidence_weight)
let boost_a = boost_map.get(&entry_a.id).copied().unwrap_or(0.0)
let boost_b = boost_map.get(&entry_b.id).copied().unwrap_or(0.0)
let prov_a = if entry_a.category == "lesson-learned" { PROVENANCE_BOOST } else { 0.0 }
let prov_b = if entry_b.category == "lesson-learned" { PROVENANCE_BOOST } else { 0.0 }
let delta_a = utility_delta(categories.get(&entry_a.id).copied())
let delta_b = utility_delta(categories.get(&entry_b.id).copied())
let penalty_a = penalty_map.get(&entry_a.id).copied().unwrap_or(1.0)
let penalty_b = penalty_map.get(&entry_b.id).copied().unwrap_or(1.0)
let final_a = (base_a + delta_a + boost_a + prov_a) * penalty_a
let final_b = (base_b + delta_b + boost_b + prov_b) * penalty_b
// compare final_b <=> final_a (descending)
```

---

## Modified: Step 11 (ScoredEntry Construction)

The `final_score` stored in `ScoredEntry` should also reflect the utility delta for consistency
with external callers that inspect scores:

```
// Step 11: Build ScoredEntry with utility_delta included in final_score
for (entry, sim) in results_with_scores:
    let penalty = penalty_map.get(&entry.id).copied().unwrap_or(1.0)
    let delta = utility_delta(categories.get(&entry.id).copied())
    ScoredEntry {
        entry: entry.clone(),
        final_score: (rerank_score(*sim, entry.confidence, confidence_weight) + delta) * penalty,
        similarity: *sim,
        confidence: entry.confidence,
    }
```

---

## Call Site Count Verification

The implementation must contain exactly four locations where `utility_delta` is applied:
1. Step 7 comparator: `delta_a`, `delta_b` inside `results_with_scores.sort_by(...)` (2 uses)
2. Step 8 comparator: `delta_a`, `delta_b` inside the co-access `sort_by(...)` (2 uses)

Total: 4 call sites. Step 11 (`ScoredEntry` construction) is a 5th use of `utility_delta` but
it is not a "rerank_score call site" per the architecture — it is score reporting. AC-04
requires all four `rerank_score` call sites include the delta; Step 11 should include it too
for consistency.

---

## Combined Formula Reference

```
confidence_weight = clamp(spread * 1.25, 0.15, 0.25)      // from crt-019

final_score = (
    (1 - confidence_weight) * similarity
    + confidence_weight * confidence
    + utility_delta(category)            // [-0.05, +0.01, +0.05] or 0.0
    + co_access_boost                    // [0.0, +0.03]
    + provenance_boost                   // 0.0 or +0.02
) * status_penalty                       // 0.5, 0.7, or 1.0
```

---

## Error Handling

| Error | Behavior |
|-------|----------|
| `categories.get(&entry_id)` returns None | `utility_delta(None) = 0.0` — no panic, no default-to-penalty (NFR-06, R-07) |
| `effectiveness_state` read lock poisoned | `.unwrap_or_else(|e| e.into_inner())` — use stale or empty state |
| `cached_snapshot` mutex poisoned | `.unwrap_or_else(|e| e.into_inner())` — use stale snapshot |

---

## Key Test Scenarios

**Scenario 1 — Utility delta applied at all four call sites (AC-04, R-02)**
- Create SearchService with EffectivenessState containing entry 1 = Effective
- Run a search returning entry 1 in both Step 7 and Step 8 results
- Assert Step 7 score for entry 1 includes +UTILITY_BOOST vs. baseline
- Assert Step 8 score for entry 1 includes +UTILITY_BOOST vs. baseline
- Repeat for Ineffective, Settled, Unmatched, None categories

**Scenario 2 — Effective outranks near-equal Ineffective (AC-05)**
- Entry A: sim=0.75, conf=0.60, category=Effective
- Entry B: sim=0.76, conf=0.60, category=Ineffective
- With confidence_weight=0.18375 (initial):
  - A base = 0.18375*0.60 + 0.81625*0.75 + 0.05 = 0.8198...
  - B base = 0.18375*0.60 + 0.81625*0.76 - 0.05 = 0.7305...
- Assert A ranks above B

**Scenario 3 — Utility delta inside penalty multiplication (ADR-003, R-05)**
- Entry: sim=0.75, conf=0.60, category=Effective, status=Deprecated (penalty=0.7)
- Expected: (rerank(0.75,0.60,cw) + 0.05 + 0) * 0.7
- Assert: NOT (rerank(0.75,0.60,cw) * 0.7 + 0.05)
- Numeric: rerank = 0.1838*0.60 + 0.8163*0.75 = 0.7234; with delta = 0.7734; * 0.7 = 0.5414
- Vs wrong placement: 0.7234 * 0.7 + 0.05 = 0.5564. Assert ~0.5414 not ~0.5564.

**Scenario 4 — Empty EffectivenessState produces zero delta (AC-06, R-07)**
- Create SearchService with empty EffectivenessState
- Run search; assert all entries receive delta = 0.0
- Assert result ordering is identical to pre-crt-018b (no delta applied)

**Scenario 5 — Generation cache prevents re-clone on successive calls (ADR-001, R-06)**
- Create two clones of SearchService (same Arc<Mutex<EffectivenessSnapshot>>)
- Simulate background tick: write to EffectivenessState, increment generation
- Call search() on clone 1: assert it re-clones (generation mismatch)
- Call search() on clone 2: assert it does NOT re-clone (generation already updated in cache)

**Scenario 6 — Read lock released before mutex acquisition (R-01)**
- Verify via code review that the `effectiveness_state` read guard is dropped
  (goes out of scope) before `cached_snapshot.lock()` is called
- No test can assert lock ordering directly; this is a code review check

**Scenario 7 — SETTLED_BOOST < co-access max (AC-03, R-10)**
- Assert SETTLED_BOOST < 0.03 (constant invariant)
- Assert UTILITY_BOOST == 0.05
- Assert UTILITY_PENALTY == 0.05
