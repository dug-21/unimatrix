# Component: Briefing Effectiveness Tiebreaker

**File**: `crates/unimatrix-server/src/services/briefing.rs` (modified)

**Purpose**: Incorporate effectiveness category as a secondary sort key in the injection history
and convention lookup sort paths within `BriefingService`. Add `EffectivenessStateHandle` as a
required constructor parameter (ADR-004). Use the same generation-cached snapshot pattern as
`SearchService`.

---

## New Fields on `BriefingService`

```
struct BriefingService {
    entry_store: Arc<AsyncEntryStore<StoreAdapter>>,
    search: SearchService,
    gateway: Arc<SecurityGateway>,
    semantic_k: usize,

    /// crt-018b (ADR-004): effectiveness classification handle.
    /// Required parameter — missing wiring is a compile error.
    effectiveness_state: EffectivenessStateHandle,   // NEW

    /// crt-018b (ADR-001): generation-cached snapshot shared across rmcp clones.
    cached_snapshot: Arc<Mutex<EffectivenessSnapshot>>,  // NEW
}
```

---

## Modified: `BriefingService::new()` Signature (ADR-004)

```
pub(crate) fn new(
    entry_store: Arc<AsyncEntryStore<StoreAdapter>>,
    search: SearchService,
    gateway: Arc<SecurityGateway>,
    semantic_k: usize,
    effectiveness_state: EffectivenessStateHandle,   // NEW — required, non-optional
) -> Self:
    BriefingService {
        entry_store,
        search,
        gateway,
        semantic_k,
        effectiveness_state,
        cached_snapshot: EffectivenessSnapshot::new_shared(),
    }
```

`EffectivenessStateHandle` is NOT `Option<EffectivenessStateHandle>`. Any construction site that
does not provide the handle fails to compile. This is the ADR-004 compile-time safety guarantee.

---

## New Free Function: `effectiveness_priority`

Module-level function in `briefing.rs`:

```
fn effectiveness_priority(category: Option<EffectivenessCategory>) -> i32:
    match category:
        Some(Effective)   =>  2
        Some(Settled)     =>  1
        None              =>  0
        Some(Unmatched)   =>  0
        Some(Ineffective) => -1
        Some(Noisy)       => -2
```

Scale from ARCHITECTURE Component 4 (IMPLEMENTATION-BRIEF canonical scale). This supersedes
the 3-2-1-0 scale in SPECIFICATION FR-07. The ARCHITECTURE scale is used consistently.

When `EffectivenessState` is empty (cold start), `category` is `None` for all entries.
`effectiveness_priority(None) = 0` — sort degrades to confidence-only. No special-casing needed.

---

## Modified: `BriefingService::assemble()` — Snapshot at Top

Insert immediately after Step 1 (input validation), before Step 2 (budget initialization):

```
// crt-018b (ADR-001): snapshot effectiveness categories under short read lock.
// Same generation-cache pattern as SearchService. Lock ordering: read generation,
// drop read guard, then acquire mutex (R-01).
let categories: HashMap<u64, EffectivenessCategory> = {
    let current_generation = {
        let guard = self.effectiveness_state.read()
            .unwrap_or_else(|e| e.into_inner())
        let gen = guard.generation
        // read guard drops here
        gen
    }
    // Read guard is now dropped
    let mut cache = self.cached_snapshot.lock()
        .unwrap_or_else(|e| e.into_inner())
    if cache.generation != current_generation:
        let guard = self.effectiveness_state.read()
            .unwrap_or_else(|e| e.into_inner())
        cache.generation = guard.generation
        cache.categories = guard.categories.clone()
        // guard drops here
    cache.categories.clone()
}
// categories: HashMap<u64, EffectivenessCategory> — valid for duration of assemble()
// No SQL or embedding computation holds any lock on EffectivenessState (NFR-01, NFR-03)
```

The `categories` HashMap is passed to the sort comparators in Steps 4 and 5. No lock is held
during SQL execution (convention query) or the semantic search path.

---

## Modified: `process_injection_history` — Sort with Tiebreaker

The method receives `categories` as a parameter (passed from `assemble()`):

```
async function process_injection_history(
    &self,
    history: &[InjectionEntry],
    char_budget: usize,
    categories: &HashMap<u64, EffectivenessCategory>,  // NEW parameter
) -> Result<(InjectionSections, usize), ServiceError>
```

### Step 3: Sort Each Group with Composite Key

Replace the three existing single-key sorts:

**Before (current)**:
```
decisions.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Equal));
injections.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Equal));
conventions.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Equal));
```

**After (crt-018b)**:
```
decisions.sort_by(|a, b|:
    // Primary: confidence descending
    let conf_ord = b.1.partial_cmp(&a.1).unwrap_or(Equal)
    if conf_ord != Equal:
        return conf_ord
    // Secondary: effectiveness_priority descending (tiebreaker)
    let pri_a = effectiveness_priority(categories.get(&a.0.id).copied())
    let pri_b = effectiveness_priority(categories.get(&b.0.id).copied())
    pri_b.cmp(&pri_a)
)

// Same comparator for injections and conventions
injections.sort_by(same composite comparator as decisions)
conventions.sort_by(same composite comparator as decisions)
```

When `categories` is empty (cold start), `effectiveness_priority(None) = 0` for all entries.
`0.cmp(&0) == Equal` — sort degrades to confidence-only. Correct behavior with no branching.

---

## Modified: Convention Lookup Sort in `assemble()` Step 5

The existing convention sort (when `params.feature` is Some) is:

```
// Current: feature-tagged entries first, then confidence descending
conv_entries.sort_by(|a, b|:
    let a_has = a.tags.iter().any(|t| t == feature)
    let b_has = b.tags.iter().any(|t| t == feature)
    match (a_has, b_has):
        (true, false)  => Less
        (false, true)  => Greater
        _              => b.confidence.partial_cmp(&a.confidence).unwrap_or(Equal)
)
```

**After (crt-018b)**:
```
conv_entries.sort_by(|a, b|:
    let a_has = a.tags.iter().any(|t| t == feature)
    let b_has = b.tags.iter().any(|t| t == feature)
    match (a_has, b_has):
        (true, false)  => Less    // feature-tagged first (unchanged)
        (false, true)  => Greater // non-feature-tagged last (unchanged)
        _ =>
            // Among entries with same feature-tag status: confidence then effectiveness
            let conf_ord = b.confidence.partial_cmp(&a.confidence).unwrap_or(Equal)
            if conf_ord != Equal:
                return conf_ord
            // Tiebreaker: effectiveness_priority descending
            let pri_a = effectiveness_priority(categories.get(&a.id).copied())
            let pri_b = effectiveness_priority(categories.get(&b.id).copied())
            pri_b.cmp(&pri_a)
)
```

When `params.feature` is None, the convention entries are sorted by confidence only (current
behavior). Add the effectiveness tiebreaker to that path as well:

```
// When feature is None: sort by (confidence DESC, effectiveness_priority DESC)
conv_entries.sort_by(|a, b|:
    let conf_ord = b.confidence.partial_cmp(&a.confidence).unwrap_or(Equal)
    if conf_ord != Equal:
        return conf_ord
    let pri_a = effectiveness_priority(categories.get(&a.id).copied())
    let pri_b = effectiveness_priority(categories.get(&b.id).copied())
    pri_b.cmp(&pri_a)
)
```

---

## Modified: `services/mod.rs` — `BriefingService` Construction

In `ServiceLayer::with_rate_config()`, update the `BriefingService::new()` call site:

```
// 1. Create EffectivenessStateHandle once
let effectiveness_handle = EffectivenessState::new_handle()

// 2. Pass Arc::clone to SearchService (new parameter)
let search = SearchService::new(
    ...existing params...,
    Arc::clone(&confidence_state_handle),
    Arc::clone(&effectiveness_handle),   // NEW
)

// 3. Pass Arc::clone to BriefingService (new required parameter)
let briefing = BriefingService::new(
    Arc::clone(&entry_store),
    search.clone(),
    Arc::clone(&gateway),
    semantic_k,
    Arc::clone(&effectiveness_handle),   // NEW
)

// 4. Store handle in ServiceLayer for external access (e.g., background tick)
ServiceLayer {
    search,
    store_ops,
    confidence,
    briefing,
    status,
    usage,
    effectiveness_handle,   // NEW field
}
```

Add accessor method to `ServiceLayer`:

```
pub fn effectiveness_state_handle(&self) -> EffectivenessStateHandle:
    Arc::clone(&self.effectiveness_handle)
```

Mirrors `ServiceLayer::confidence_state_handle()`.

---

## Semantic Search Path

No changes needed. `BriefingService::assemble()` delegates the semantic path to
`SearchService::search()`, which already applies `utility_delta` (Component 3). The semantic
results therefore benefit from utility delta automatically without explicit tiebreaker logic
in `briefing.rs`.

---

## Error Handling

| Error | Behavior |
|-------|----------|
| `categories.get(&entry_id)` returns None | `effectiveness_priority(None) = 0` — sort degrades to confidence-only (NFR-06, R-07) |
| `effectiveness_state` read lock poisoned | `.unwrap_or_else(|e| e.into_inner())` — stale or empty state |
| `cached_snapshot` mutex poisoned | `.unwrap_or_else(|e| e.into_inner())` — stale snapshot |
| Cold start (empty categories) | All priorities = 0; all tiebreakers are equal; sort = confidence-only (correct behavior) |

---

## Key Test Scenarios

**Scenario 1 — Injection history sort: tiebreaker activates on equal confidence (AC-07, R-09)**
- Entry A: confidence=0.60, category=Effective
- Entry B: confidence=0.60, category=Ineffective
- Call `process_injection_history` with both entries and populated categories
- Assert A appears before B in decisions output
- Assert A.effectiveness_priority(Effective) = 2 > B.effectiveness_priority(Ineffective) = -1

**Scenario 2 — Injection history sort: confidence is primary key (AC-07, R-09)**
- Entry A: confidence=0.90, category=Ineffective
- Entry B: confidence=0.40, category=Effective
- Assert A appears before B (higher confidence wins despite negative effectiveness)

**Scenario 3 — Convention lookup tiebreaker: feature-sort preserved (AC-08)**
- Entry A: confidence=0.60, category=Effective, has feature tag
- Entry B: confidence=0.90, category=Effective, no feature tag
- Entry C: confidence=0.60, category=Ineffective, has feature tag
- Assert order: A (feature), C (feature, lower priority), B (non-feature, higher confidence)
- Feature-tagged entries always precede non-feature-tagged, regardless of effectiveness

**Scenario 4 — Convention lookup: effectiveness tiebreaker among non-feature entries (AC-08)**
- Entry A: confidence=0.60, category=Effective, no feature tag
- Entry B: confidence=0.60, category=Noisy, no feature tag
- Assert A before B

**Scenario 5 — Empty EffectivenessState degrades to confidence-only sort (R-07)**
- Call assemble() with empty EffectivenessState
- Assert result ordering matches pre-crt-018b (confidence descending only)
- Assert no panic

**Scenario 6 — BriefingService::new() requires EffectivenessStateHandle (ADR-004)**
- Verified by compilation: attempting to call `BriefingService::new()` without the handle
  parameter produces a compile error
- Unit test: construct `BriefingService` with a valid handle, assert no panic on `assemble()`

**Scenario 7 — Clone sharing for briefing cache (R-06)**
- Create BriefingService and clone it (simulating rmcp clone)
- Trigger background tick to increment generation
- Call assemble() on both instances; assert both see updated categories
  (shared `Arc<Mutex<EffectivenessSnapshot>>` ensures consistency)
