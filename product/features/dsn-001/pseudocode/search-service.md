# Pseudocode: search-service

**File**: `crates/unimatrix-server/src/services/search.rs` (modified)

## Purpose

Replaces the four hardcoded `entry.category == "lesson-learned"` comparisons
in `SearchService` with a `HashSet<String>` lookup against a `boosted_categories`
field. The `HashSet` is constructed at `SearchService` construction time from the
config-supplied list. No other search logic changes.

---

## Existing State (Pre-dsn-001)

`search.rs` has a module-level constant:
```
const PROVENANCE_BOOST: f64 = unimatrix_engine::confidence::PROVENANCE_BOOST;
```

And four hardcoded comparisons at lines ~413, ~418, ~484, ~489:
```
let prov_a = if entry_a.category == "lesson-learned" { PROVENANCE_BOOST } else { 0.0 };
let prov_b = if entry_b.category == "lesson-learned" { PROVENANCE_BOOST } else { 0.0 };
```
(appearing in two different sort closures — two occurrences each)

`SearchService::new(...)` has no `boosted_categories` parameter.

---

## Struct Change

```
// BEFORE:
pub(crate) struct SearchService {
    store: Arc<Store>,
    vector_store: ...,
    entry_store: ...,
    embed_service: ...,
    adapt_service: ...,
    gateway: ...,
    confidence_state: ConfidenceStateHandle,
    effectiveness_state: EffectivenessStateHandle,
    cached_snapshot: Arc<Mutex<EffectivenessSnapshot>>,
    supersession_state: SupersessionStateHandle,
}

// AFTER: add boosted_categories field
pub(crate) struct SearchService {
    store: Arc<Store>,
    vector_store: ...,
    entry_store: ...,
    embed_service: ...,
    adapt_service: ...,
    gateway: ...,
    confidence_state: ConfidenceStateHandle,
    effectiveness_state: EffectivenessStateHandle,
    cached_snapshot: Arc<Mutex<EffectivenessSnapshot>>,
    supersession_state: SupersessionStateHandle,
    // dsn-001: config-driven provenance boost targets.
    // Constructed from config.knowledge.boosted_categories at SearchService construction.
    // Replaces the four hardcoded entry.category == "lesson-learned" comparisons.
    boosted_categories: HashSet<String>,
}
```

---

## Constructor Change

```
// BEFORE:
pub(crate) fn new(
    store: Arc<Store>,
    vector_store: Arc<AsyncVectorStore<VectorAdapter>>,
    entry_store: Arc<Store>,
    embed_service: Arc<EmbedServiceHandle>,
    adapt_service: Arc<AdaptationService>,
    gateway: Arc<SecurityGateway>,
    confidence_state: ConfidenceStateHandle,
    effectiveness_state: EffectivenessStateHandle,
    supersession_state: SupersessionStateHandle,
) -> Self

// AFTER: add boosted_categories parameter
pub(crate) fn new(
    store: Arc<Store>,
    vector_store: Arc<AsyncVectorStore<VectorAdapter>>,
    entry_store: Arc<Store>,
    embed_service: Arc<EmbedServiceHandle>,
    adapt_service: Arc<AdaptationService>,
    gateway: Arc<SecurityGateway>,
    confidence_state: ConfidenceStateHandle,
    effectiveness_state: EffectivenessStateHandle,
    supersession_state: SupersessionStateHandle,
    boosted_categories: HashSet<String>,   // NEW
) -> Self

BODY (only the addition):
    SearchService {
        store,
        vector_store,
        entry_store,
        embed_service,
        adapt_service,
        gateway,
        confidence_state,
        effectiveness_state,
        cached_snapshot: EffectivenessSnapshot::new_shared(),
        supersession_state,
        boosted_categories,  // NEW
    }
```

**ServiceLayer construction** in `services/mod.rs` (or wherever `SearchService::new` is
called) must be updated to pass the `boosted_categories` `HashSet`. That `HashSet` comes
from startup wiring — see startup-wiring.md. If `SearchService::new` is called from
`ServiceLayer::new`, the `boosted_categories` must be threaded through `ServiceLayer::new`
as well, or `ServiceLayer` must provide a setter. The exact mechanism depends on how
`ServiceLayer` is constructed; delivery agent should examine `services/mod.rs`.

The implementation note: `ServiceLayer::new` currently takes specific parameters. The
simplest approach is to add `boosted_categories: HashSet<String>` to `ServiceLayer::new`
and thread it to `SearchService::new`. Alternatively, expose a post-construction setter
on `SearchService`. Given that `SearchService` is internal and constructed once, adding
the parameter to `ServiceLayer::new` is the preferred approach.

---

## Four Comparison Replacements

All four occurrences of `entry.category == "lesson-learned"` become
`self.boosted_categories.contains(&entry.category)`.

The two sort closures in the search method each have two occurrences:

```
// BEFORE (occurrence 1 — first sort closure, lines ~413-417):
let prov_a = if entry_a.category == "lesson-learned" {
    PROVENANCE_BOOST
} else {
    0.0
};

// AFTER:
let prov_a = if self.boosted_categories.contains(&entry_a.category) {
    PROVENANCE_BOOST
} else {
    0.0
};

// BEFORE (occurrence 2 — first sort closure, lines ~418-422):
let prov_b = if entry_b.category == "lesson-learned" {
    PROVENANCE_BOOST
} else {
    0.0
};

// AFTER:
let prov_b = if self.boosted_categories.contains(&entry_b.category) {
    PROVENANCE_BOOST
} else {
    0.0
};

// BEFORE (occurrence 3 — second sort closure, lines ~484-488):
let prov_a = if entry_a.category == "lesson-learned" {
    PROVENANCE_BOOST
} else {
    0.0
};

// AFTER:
let prov_a = if self.boosted_categories.contains(&entry_a.category) {
    PROVENANCE_BOOST
} else {
    0.0
};

// BEFORE (occurrence 4 — second sort closure, lines ~489-493):
let prov_b = if entry_b.category == "lesson-learned" {
    PROVENANCE_BOOST
} else {
    0.0
};

// AFTER:
let prov_b = if self.boosted_categories.contains(&entry_b.category) {
    PROVENANCE_BOOST
} else {
    0.0
};
```

The `PROVENANCE_BOOST` constant and its value (0.02) are unchanged.

---

## AC-03 Verification: Zero Remaining "lesson-learned" Comparisons

After the change, `grep '"lesson-learned"' search.rs` must return zero matches.
The string "lesson-learned" may still appear in comments but must not appear in
any runtime comparison. This is a mandatory pre-PR gate (AC-03).

---

## Key Test Scenarios

1. **No "lesson-learned" in search.rs** (AC-03):
   Static grep confirms zero `entry.category == "lesson-learned"` comparisons remain.

2. **Custom boosted category receives boost** (AC-03 integration):
   - Construct `SearchService` with `boosted_categories = HashSet::from(["decision".to_string()])`.
   - Execute search returning both a "decision" entry and a "lesson-learned" entry with
     identical `rerank_score` values.
   - Assert "decision" ranks higher (it gets `PROVENANCE_BOOST`).
   - Assert "lesson-learned" does NOT get the boost (it is not in `boosted_categories`).

3. **Default boosted_categories preserves existing behavior** (AC-01):
   - Construct `SearchService` with `boosted_categories = HashSet::from(["lesson-learned".to_string()])`.
   - Assert "lesson-learned" entries receive `PROVENANCE_BOOST`.
   - This is the default config value; no behavioral change when no config is present.

4. **Empty boosted_categories** (EC-02):
   - Construct `SearchService` with `boosted_categories = HashSet::new()`.
   - Assert no panic occurs in search re-ranking.
   - Assert all entries have `prov_a = 0.0` and `prov_b = 0.0`.

5. **All four comparisons replaced** (IR-03):
   - Search results are ranked consistently between the two sort closures.
   - A test that exercises both the standard and co-access sort paths should be added.

---

## Error Handling

`SearchService::new` is infallible. `HashSet::contains` is infallible.
The `boosted_categories` field cannot be modified after construction — it is
set once at startup and does not change (config is loaded once per process).

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-server` — no patterns found for HashSet field injection in SearchService. The approach is standard Rust constructor injection.
- Deviations from established patterns: none.
