## ADR-001: CategoryAllowlist Lifecycle Policy Constructor and Config Model

### Context

crt-031 requires that `CategoryAllowlist` carry per-category lifecycle classification
(adaptive vs pinned) loaded from operator config. Four design questions needed resolution
before the spec could be written.

**Question 1 — Constructor API**: Should `from_categories` gain an `adaptive: Vec<String>`
parameter (breaking change to all callers), or should a new constructor
`from_categories_with_policy(cats, adaptive)` be added with `from_categories` delegating to
it using `["lesson-learned"]` as the default?

`from_categories` is called from `main.rs` at two sites and from tests. A parameter change
breaks all call sites. The delegation approach preserves every existing call site and every
existing test without modification.

**Question 2 — Config model**: Should `adaptive_categories` be a top-level `[lifecycle]` table
in `config.toml`, or a parallel `Vec<String>` field on the existing `[knowledge]` section
alongside `boosted_categories`?

A new `[lifecycle]` table requires new config parsing infrastructure. `boosted_categories` on
`KnowledgeConfig` is a direct structural precedent: same shape, same subset validation, same
`ConfigError` variant pattern (entry #3770). Reusing the established pattern keeps the config
surface minimal.

**Question 3 — Internal struct layout**: Should the adaptive set live under a single wide
`RwLock<(HashSet, HashSet)>` or as two independent `RwLock<HashSet<String>>` fields?

A single wide lock makes reads of the category set block on adaptive-set operations. ADR-003
(entry #86) established `RwLock<HashSet<String>>` as the correct per-set primitive. `validate()`
is on the hot path (every `context_store`); lifecycle classification should not add contention.

**Question 4 — KnowledgeConfig::Default for boosted_categories**: Should `Default` continue
to return `vec!["lesson-learned"]`, or should it return `vec![]` with the serde deserialization
default function expressing the operative value?

`Default` returning a policy value rather than absence creates a persistent trap: tests
constructing via `Default` that then assert on the field will silently fail when the policy
value changes (entry #3774). The serde default function is the correct home for the operative
default — it governs what a config file omitting the field receives, which is the production
path. The Rust `Default` impl governs programmatic construction (test fixtures, `Default::default()`
calls) and should express absence.

**Question 5 — StatusService wiring for lifecycle output**: `StatusService` does not hold
`Arc<CategoryAllowlist>`. Should lifecycle data be passed to `compute_report` as a parameter,
or should `Arc<CategoryAllowlist>` be added as a new `StatusService` field?

`compute_report` is called from `mcp/tools.rs` through `self.services.status` — threading a
new parameter through the MCP handler call site is possible but adds call-site noise. Adding
`Arc<CategoryAllowlist>` as a `StatusService` field follows the same pattern as `observation_registry`
(col-023, entry #3722) and is the cleanest long-term approach given that lifecycle policy is
a stable startup-time config, not a per-request value.

### Decision

1. Add `from_categories_with_policy(cats: Vec<String>, adaptive: Vec<String>) -> Self` as the
   canonical `CategoryAllowlist` constructor. `from_categories(cats)` delegates to it with
   `adaptive = vec!["lesson-learned"]`. `new()` delegates to `from_categories`. No existing
   call site changes.

2. Add `adaptive_categories: Vec<String>` to `KnowledgeConfig` with
   `#[serde(default = "default_adaptive_categories")]` where `default_adaptive_categories()`
   returns `vec!["lesson-learned"]`. Validation follows the `boosted_categories` pattern:
   iterate the list against the `category_set` HashSet in `validate_config`, emit
   `ConfigError::AdaptiveCategoryNotInAllowlist { path, category }` on mismatch. An empty
   list is valid. The two `main.rs` call sites are updated to call
   `from_categories_with_policy(knowledge_categories, config.knowledge.adaptive_categories)`.

3. `CategoryAllowlist` carries two independent `RwLock<HashSet<String>>` fields: the existing
   `categories` and a new `adaptive`. `is_adaptive(&self, category: &str) -> bool` and
   `list_adaptive(&self) -> Vec<String>` read only the `adaptive` lock. Poison recovery on
   both locks uses `.unwrap_or_else(|e| e.into_inner())` following the established pattern.

4. `KnowledgeConfig::Default` returns `boosted_categories: vec![]` and
   `adaptive_categories: vec![]`. The operative production default `["lesson-learned"]` for
   both fields is expressed in `default_boosted_categories()` and `default_adaptive_categories()`
   serde default functions — not in the `Default` impl. A public helper
   `default_boosted_categories_set() -> HashSet<String>` in `infra/config.rs` replaces the
   six hardcoded `HashSet::from(["lesson-learned"...])` literals in test infrastructure.

5. `Arc<CategoryAllowlist>` is added as a new field on `StatusService`, passed via
   `StatusService::new()` and `ServiceLayer::new()`. Both `main.rs` `ServiceLayer::new()`
   call sites pass `Arc::clone(&categories)`. `compute_report()` uses `self.category_allowlist`
   to populate `StatusReport::category_lifecycle`.

   A compile-level wiring test verifies that a `CategoryAllowlist` constructed with a known
   `adaptive_categories` list reports `is_adaptive` correctly (SR-05 mandate, analogous to the
   R-14 `PhaseFreqTableHandle` wiring test).

### Consequences

**Easier:**
- Every existing `CategoryAllowlist` test continues to pass without modification.
- `from_categories` and `new` callers in test code get `["lesson-learned"]` as the adaptive
  default — correct for the vast majority of test scenarios.
- `adaptive_categories` is self-documenting alongside `boosted_categories`. Operators learn
  one config pattern.
- `validate_config` rejects misconfigured `adaptive_categories` at startup with a specific error.
- `is_adaptive` reads only the `adaptive` lock; the hot `categories` path is unaffected.
- `KnowledgeConfig::default()` returning `vec![]` eliminates the awkward workaround in
  `test_empty_categories_documented_behavior` (explicit `boosted_categories: vec![]` override
  with workaround comment no longer needed).
- `default_boosted_categories_set()` is the single expression of the default value for tests;
  importable from all seven sites without circular dependency.

**Harder:**
- Both `main.rs` call sites must be updated. The spec must enumerate both.
- Any future test that constructs `KnowledgeConfig` with a custom `categories` list must
  zero out BOTH `boosted_categories` AND `adaptive_categories` to avoid cross-check failures
  (SR-03 trap, entry #3771). The spec must document this invariant explicitly.
- `main_tests.rs` line 393 (`test_default_config_boosted_categories_is_lesson_learned`) must
  be rewritten to test the serde deserialization path, not `Default` (AC-17, AC-18, entry #3774).
- The `adaptive` lock adds a second RwLock; poison recovery tests needed for it.
- `StatusService::new()` and `ServiceLayer::new()` gain a new parameter; both `main.rs`
  call sites must be updated alongside the two `CategoryAllowlist` constructor call sites.
- `server.rs` default init (`CategoryAllowlist::new()`) does not read from operator config —
  correct for test defaults but must be documented to prevent future confusion.
