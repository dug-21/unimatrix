## ADR-001: CategoryAllowlist Lifecycle Policy Constructor and Config Model

### Context

crt-031 requires that `CategoryAllowlist` carry per-category lifecycle classification
(adaptive vs pinned) loaded from operator config. Two design questions needed resolution
before the spec could be written:

**Question 1 — Constructor API**: Should `from_categories` gain an `adaptive: Vec<String>`
parameter (breaking change to all callers), or should a new constructor
`from_categories_with_policy(cats, adaptive)` be added with `from_categories` delegating to
it using `["lesson-learned"]` as the default?

`from_categories` is called from `main.rs` at two sites and is part of the public API
referenced in tests. A parameter change would break all call sites and all tests that construct
`CategoryAllowlist::from_categories(...)`. The alternative — a new canonical constructor with
delegation — preserves every existing call site and every existing test without modification.

**Question 2 — Config model**: Should `adaptive_categories` be a top-level `[lifecycle]` table
in `config.toml`, or a parallel `Vec<String>` field on the existing `[knowledge]` section
alongside `boosted_categories`?

A new `[lifecycle]` table would require new config parsing infrastructure. The `boosted_categories`
field on `KnowledgeConfig` is a direct structural precedent: same shape (`Vec<String>` parallel
list, must be subset of `categories`, validated with a dedicated `ConfigError` variant). Reusing
the established pattern keeps the config surface minimal and the implementation path clear
(entry #3770).

**Question 3 — Internal struct layout**: Should the adaptive set live as a single
`RwLock<(HashSet<String>, HashSet<String>)>` (both sets under one lock) or as two independent
`RwLock<HashSet<String>>` fields?

A single wide lock would make reads of the category set block on adaptive-set operations and
vice versa. ADR-003 (entry #86) established that `RwLock<HashSet<String>>` is the correct
primitive for concurrent read / rare write. The original `categories` field is on a hot path
(called on every `context_store`). Adding lifecycle classification should not increase contention
on that path.

### Decision

1. Add `from_categories_with_policy(cats: Vec<String>, adaptive: Vec<String>) -> Self` as the
   canonical `CategoryAllowlist` constructor. `from_categories(cats)` delegates to it with
   `adaptive = vec!["lesson-learned"]`. `new()` delegates to `from_categories`. No existing
   call site changes.

2. Add `adaptive_categories: Vec<String>` to `KnowledgeConfig` with
   `#[serde(default = "default_adaptive_categories")]` where `default_adaptive_categories()`
   returns `vec!["lesson-learned"]`. Validation follows the `boosted_categories` pattern:
   iterate the list against the already-built `category_set` in `validate_config`, emit
   `ConfigError::AdaptiveCategoryNotInAllowlist { path, category }` on mismatch. An empty
   list is valid.

3. `CategoryAllowlist` carries two independent `RwLock<HashSet<String>>` fields: the existing
   `categories` and a new `adaptive`. `is_adaptive(&self, category: &str) -> bool` reads only
   the `adaptive` lock. Poison recovery on both locks uses `.unwrap_or_else(|e| e.into_inner())`
   following the established pattern.

4. The two `main.rs` call sites are updated from `from_categories(knowledge_categories)` to
   `from_categories_with_policy(knowledge_categories, config.knowledge.adaptive_categories)`.
   `server.rs`'s `CategoryAllowlist::new()` default field initializer is unchanged — it
   implicitly carries the `["lesson-learned"]` default and does not read from operator config.

   A compile-level wiring test verifies that a `CategoryAllowlist` constructed with a known
   `adaptive_categories` list reports `is_adaptive` correctly (SR-05 mandate, analogous to
   R-14 `PhaseFreqTableHandle` test).

### Consequences

**Easier:**
- Every existing `CategoryAllowlist` test continues to pass without modification (no constructor
  signature breakage).
- `from_categories` and `new` callers in test code get `["lesson-learned"]` as the adaptive
  default — correct for the vast majority of test scenarios.
- The `adaptive_categories` config field is self-documenting alongside `boosted_categories`.
  Operators learn one pattern.
- `validate_config` rejects misconfigured `adaptive_categories` at startup with a specific
  error variant and the offending category name — no silent runtime no-ops.
- `is_adaptive` reads only the `adaptive` lock; the hot `categories` read path (called on every
  `context_store`) is unaffected.
- `from_categories_with_policy` is the clear canonical path for #409 and any future policy
  extension — the name signals intent.

**Harder:**
- `main.rs` must be updated at both call sites. The spec must explicitly enumerate both.
- Any future test that constructs `KnowledgeConfig` with a custom `categories` list must
  zero out both `boosted_categories` and `adaptive_categories` to avoid cross-check failures
  (SR-03 trap). The spec must document this test construction invariant.
- The `adaptive` lock adds a second RwLock to the struct; poison recovery must be added for it.
  The pattern is well-established but adds test surface.
- `server.rs` default init (`CategoryAllowlist::new()`) does not read from operator config —
  this is correct behavior (server.rs builds a default server for tests, not from a loaded
  config), but the distinction must be documented to prevent future confusion.
