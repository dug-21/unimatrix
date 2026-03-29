# crt-031: Category Lifecycle Policy (Pinned vs Adaptive)

## Problem Statement

Unimatrix accumulates entries without bound in categories like `lesson-learned` because the
retention machinery has no way to distinguish which categories are candidates for automated
management versus which require explicit operator action. Every category is implicitly treated
as requiring human supersession — but `lesson-learned` entries, by their nature, should
eventually age out rather than accumulate indefinitely.

Issue #445 (ASS-032 ROADMAP) identifies this as a prerequisite for the signal-driven entry
auto-deprecation work in #409. The policy layer must exist — encoding which categories are
`adaptive` versus `pinned` — before any retention logic can safely fire. Without it, a future
auto-deprecation pass would have no categorical guard and could silently touch ADRs or
conventions that must only be superseded by explicit human action.

## Goals

1. Add an `adaptive_categories` parallel list to `[knowledge]` in `config.toml` with a built-in
   default of `["lesson-learned"]`, keeping all other categories `pinned` by default.
2. Extend `CategoryAllowlist` with an `is_adaptive(&self, category: &str) -> bool` method that
   consults the loaded policy at runtime.
3. Add a startup validation rule: every entry in `adaptive_categories` must also appear in
   `categories` — fail-fast with a descriptive `ConfigError` variant.
4. Expose per-category lifecycle in `context_status` output so operators can verify policy
   configuration at runtime.
5. Add a lifecycle guard stub in the maintenance tick that calls `is_adaptive()` before any
   future auto-deprecation step, with a `tracing::debug!` log when the guard fires — so #409
   has a clear, tested insertion point.

## Non-Goals

- Does NOT implement entry auto-deprecation logic — that is #409's responsibility.
- Does NOT change PPR weighting, co-access scoring, or any ranking signal.
- Does NOT wire the lifecycle policy to the existing effectiveness-based auto-quarantine path
  (which acts on quarantine, not deprecation).
- Does NOT add database schema changes — lifecycle is config-only at this stage.
- Does NOT expose a runtime MCP tool for changing lifecycle policy — operators use config.toml.
- Does NOT implement decay schedules, score thresholds, or signal mechanics for #409.
- Does NOT add `adaptive_categories` support to `DomainPackConfig` (domain packs add categories
  to the allowlist; lifecycle policy is a separate operator concern).

## Background Research

### CategoryAllowlist (categories.rs)

`CategoryAllowlist` is a `struct` wrapping `RwLock<HashSet<String>>`. It exposes:
- `from_categories(Vec<String>) -> Self` — primary constructor, called from `main.rs` startup
- `new() -> Self` — delegates to `from_categories(INITIAL_CATEGORIES)`
- `validate(&self, category: &str) -> Result<(), ServerError>`
- `add_category(&self, category: String)` — runtime extension for domain packs
- `list_categories(&self) -> Vec<String>`

The struct currently carries no lifecycle information — only presence/absence in the set.
Poison recovery uses `.unwrap_or_else(|e| e.into_inner())` throughout; the new `is_adaptive`
method must follow the same pattern.

`INITIAL_CATEGORIES: [&str; 5]` — `["lesson-learned", "decision", "convention", "pattern",
"procedure"]`. All 5 are currently implicitly `pinned`. The test suite enforces exactly 5 entries
and explicitly guards against `outcome` (retired, ADR-005 crt-025).

### KnowledgeConfig (config.rs)

`KnowledgeConfig` currently has three fields:
- `categories: Vec<String>` — defaults to `INITIAL_CATEGORIES`
- `boosted_categories: Vec<String>` — defaults to `["lesson-learned"]`
- `freshness_half_life_hours: Option<f64>`

The `boosted_categories` field is the direct structural predecessor for `adaptive_categories`:
same pattern (parallel list, must be a subset of `categories`, validated in `validate_config`
with a dedicated `ConfigError` variant: `BoostedCategoryNotInAllowlist`). The new field follows
the exact same approach — `AdaptiveCategoryNotInAllowlist` error variant; validation iterates
`adaptive_categories` against the `category_set` HashSet already built for the boosted check.

`validate_config()` in `config.rs` is the single post-parse validation entry point called at
startup. Adding the `adaptive_categories` cross-check here is the correct insertion point.

### startup wiring (main.rs)

Two call sites (lines ~550 and ~940 — the project and global config paths) construct
`CategoryAllowlist::from_categories(knowledge_categories)`. Both will need to pass the
`adaptive_categories` list so the allowlist carries the policy. The cleanest approach is to
update `CategoryAllowlist::from_categories` to accept an additional `adaptive: Vec<String>`
parameter — or provide a builder/secondary constructor `from_categories_with_policy`. The
existing `new()` constructor (used in tests and `server.rs` default) should default to the
built-in policy (`["lesson-learned"]` is adaptive).

### context_status (services/status.rs + mcp/response/status.rs)

`StatusReport` is a large flat struct with `category_distribution: Vec<(String, u64)>`.
Lifecycle policy would be exposed as a new field:
`category_lifecycle: Vec<(String, &'static str)>` (or `String` for the policy value) listing
each configured category and its lifecycle label (`"adaptive"` or `"pinned"`). The
`format_status_report` summary and JSON paths both need updating.

`StatusService::compute_report()` constructs the `StatusReport` — this is where the lifecycle
field gets populated from the `CategoryAllowlist`.

### maintenance tick (background.rs)

`maintenance_tick()` (line 787) is `async fn` receiving `status_svc`, `store`, `nli_enabled`,
and other parameters. The guard stub belongs between Step 10 (`run_maintenance`) and Step 11
(dead-knowledge migration). It checks `category_allowlist.is_adaptive(category)` before
invoking any future deprecation action. In this feature it is a no-op stub with a
`tracing::debug!` call and a comment pointing to #409.

The stub should log which categories are configured as adaptive at tick start (single
`tracing::debug!` at info verbosity — once per tick, not per entry).

### Prior patterns (from Unimatrix knowledge base)

- Entry #3715 / #3721: When modifying `INITIAL_CATEGORIES`, there are 5 locations that must
  change in lockstep: `categories.rs` const array + size, `config.rs`, `main.rs` (×2 call
  sites), `unimatrix-observe`. This feature does NOT add a new category — it adds policy metadata
  to existing categories — so the 5-location lockstep rule does NOT apply here.
- Entry #2312: Tests for `validate_config` with an empty categories list rely on the default
  `KnowledgeConfig` having `boosted_categories = ["lesson-learned"]`. The analogous default for
  `adaptive_categories` will be `["lesson-learned"]` — tests must account for this.
- Entry #86: `CategoryAllowlist` ADR-003 established it as a runtime-extensible `HashSet`. This
  feature adds a second `RwLock<HashSet<String>>` for the adaptive set, or a
  `RwLock<(HashSet<String>, HashSet<String>)>` — the implementation choice is a design detail
  for the spec phase.

## Proposed Approach

1. **`KnowledgeConfig`**: Add `adaptive_categories: Vec<String>` with `#[serde(default)]`
   pointing to a `default_adaptive_categories()` fn returning `vec!["lesson-learned"]`.

2. **`validate_config`**: After the `boosted_categories` cross-check, add an
   `adaptive_categories` cross-check using the same `category_set` — emit
   `ConfigError::AdaptiveCategoryNotInAllowlist { path, category }` on mismatch.

3. **`CategoryAllowlist`**: Add a second `RwLock<HashSet<String>> adaptive` field. Extend
   `from_categories` to accept `adaptive: Vec<String>` (or add a new constructor). Add
   `is_adaptive(&self, category: &str) -> bool`. Keep `new()` defaulting to
   `["lesson-learned"]` as the adaptive set. Poison recovery follows existing pattern.

4. **`main.rs` (two call sites)**: Pass `config.knowledge.adaptive_categories` to the
   `CategoryAllowlist` constructor.

5. **`StatusReport`**: Add `category_lifecycle: Vec<(String, String)>` field. Populate in
   `StatusService::compute_report()` by calling `category_allowlist.list_categories()` and
   tagging each with `is_adaptive`. Expose in summary text and JSON.

6. **`maintenance_tick`**: Accept `Arc<CategoryAllowlist>` parameter. Add lifecycle guard stub
   after Step 10 with `tracing::debug!` log.

Backward compatibility: existing configs omitting `adaptive_categories` silently get the
built-in default (`["lesson-learned"]`), matching current semantic intent. No migration needed.

## Acceptance Criteria

- AC-01: `KnowledgeConfig` has an `adaptive_categories: Vec<String>` field with
  `#[serde(default)]` defaulting to `["lesson-learned"]`. Serialization round-trips correctly.

- AC-02: A config file omitting `adaptive_categories` produces a `KnowledgeConfig` with
  `adaptive_categories == ["lesson-learned"]` after deserialization.

- AC-03: A config file specifying `adaptive_categories = ["lesson-learned", "convention"]`
  produces a `KnowledgeConfig` with both values.

- AC-04: `validate_config` rejects a config where any entry in `adaptive_categories` is absent
  from `categories`, returning `ConfigError::AdaptiveCategoryNotInAllowlist` with the offending
  category name and the config file path in the error message.

- AC-05: `CategoryAllowlist::is_adaptive("lesson-learned")` returns `true` when constructed
  with the default policy.

- AC-06: `CategoryAllowlist::is_adaptive("decision")` returns `false` when constructed with
  the default policy.

- AC-07: `CategoryAllowlist::is_adaptive` returns `false` for any category not in the
  allowlist (unknown category is not adaptive).

- AC-08: Poison recovery on the adaptive set follows the same `.unwrap_or_else(|e| e.into_inner())`
  pattern — `is_adaptive` does not panic on a poisoned lock.

- AC-09: `context_status` output includes a per-category lifecycle section listing each
  configured category and its label (`"adaptive"` or `"pinned"`). Both summary and JSON
  formats include this data.

- AC-10: `maintenance_tick` logs a `tracing::debug!` message listing the adaptive categories
  at each tick. The log does NOT fire if `adaptive_categories` is empty.

- AC-11: The lifecycle guard stub in `maintenance_tick` calls `is_adaptive()` and is annotated
  with a comment referencing #409 as the consumer. The stub is a no-op (no actual deprecation).

- AC-12: All existing `CategoryAllowlist` tests continue to pass without modification.

- AC-13: `CategoryAllowlist::new()` is equivalent to constructing with default
  `adaptive_categories = ["lesson-learned"]` — no behavior regression.

- AC-14: `validate_config` accepts a config where `adaptive_categories` is an empty list `[]`
  (disabling adaptive management entirely is valid).

- AC-15: `validate_config` accepts a config where `adaptive_categories` is a proper subset of
  `categories` with multiple entries (e.g. two adaptive categories).

## Constraints

- `CategoryAllowlist` is `pub` and used in `server.rs` (field `categories: Arc<CategoryAllowlist>`)
  and both `main.rs` call sites. Constructor signature changes must not break these call sites,
  or must update them in the same PR.
- The `new()` constructor is used in tests (`main_tests.rs`, `categories.rs` tests, `server.rs`
  default init). Its behavior must remain backward-compatible.
- `StatusReport` has a `Default` impl; the new `category_lifecycle` field must have a
  sensible default (empty `Vec`).
- The maintenance tick's `spawn_background_tick` and `background_tick_loop` function signatures
  already carry 17+ parameters. Adding `Arc<CategoryAllowlist>` is acceptable; a thread-local
  or global is not — the allowlist is already `Arc<CategoryAllowlist>` in `server.rs`.
- No database schema changes — lifecycle is config-only in this feature.
- File size rule: `categories.rs` is currently 454 lines (under 500). Adding the new field and
  method may approach the limit; split defensively if needed.
- The `validate_config` function is already long (~200 lines). The new validation block follows
  the established `boosted_categories` pattern directly — no architectural change needed.

## Open Questions

1. **Constructor API**: Should `CategoryAllowlist::from_categories` gain an `adaptive: Vec<String>`
   parameter (breaking change to callers), or should a new constructor
   `from_categories_with_policy(cats, adaptive)` be added and `from_categories` delegate with
   `["lesson-learned"]` as default? The latter preserves the existing public API but adds a
   constructor. Recommendation: new constructor + keep `from_categories` with default — but this
   needs confirmation from the human before speccing.

2. **Status output format**: Should `category_lifecycle` be a flat list like
   `[("decision", "pinned"), ("lesson-learned", "adaptive")]`, or should the summary text be
   condensed to only list the adaptive ones (since pinned is the default)? The JSON should
   include all; the summary may be more readable showing only adaptive categories.

3. **`DomainPackConfig` and `add_category` runtime path**: When `add_category` is called at
   runtime for domain pack categories, what lifecycle do they get? Default to `pinned` unless
   the operator explicitly adds them to `adaptive_categories`. The current `add_category` API
   has no lifecycle parameter — this is consistent with defaulting to `pinned`.

4. **Test count impact**: The project has 2169 unit tests as of col-022. The spec should estimate
   how many new tests are added (rough estimate: 10-15 for `CategoryAllowlist`, 8-10 for
   `validate_config`, 4-6 for `StatusReport` formatting). Does the gate require an exact count
   in the IMPLEMENTATION-BRIEF?

## Tracking

Will be updated with GH Issue link after Session 1.
