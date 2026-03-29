# crt-031: Category Lifecycle Policy (Pinned vs Adaptive) + boosted_categories De-hardcoding

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

A second problem is discovered during scope expansion: `boosted_categories` is de-facto
hardcoded in seven places outside the config load path — in test helpers, the eval profile
layer, and indirectly in the config.rs `Default` impl. While `KnowledgeConfig` has a
`boosted_categories` field read from TOML, the value `["lesson-learned"]` is duplicated as
a `HashSet::from(...)` literal in `eval/profile/layer.rs`, `server.rs`, `shutdown.rs`,
`test_support.rs`, `services/index_briefing.rs`, and `uds/listener.rs`, bypassing config
entirely for those code paths. These two problems share the same fix surface and are
addressed together.

## Goals

### adaptive_categories (original scope)

1. Add an `adaptive_categories` parallel list to `[knowledge]` in the `KnowledgeConfig`
   struct with a built-in default of `["lesson-learned"]`, keeping all other categories
   `pinned` by default.
2. Extend `CategoryAllowlist` with an `is_adaptive(&self, category: &str) -> bool` method
   that consults the loaded policy at runtime.
3. Add a startup validation rule: every entry in `adaptive_categories` must also appear in
   `categories` — fail-fast with a descriptive `ConfigError` variant.
4. Expose per-category lifecycle in `context_status` output so operators can verify policy
   configuration at runtime.
5. Add a lifecycle guard stub in the maintenance tick that calls `is_adaptive()` before any
   future auto-deprecation step, with a `tracing::debug!` log when the guard fires — so #409
   has a clear, tested insertion point.

### boosted_categories de-hardcoding (expanded scope)

6. Remove the hardcoded `HashSet::from(["lesson-learned".to_string()])` literal from
   `eval/profile/layer.rs` (Step 12 at line ~277). The eval layer must derive
   `boosted_categories` from `profile.config_overrides` or an equivalent config path —
   not a literal.
7. Remove the six hardcoded `HashSet::from(["lesson-learned".to_string()])` literals in test
   infrastructure files (`server.rs`, `shutdown.rs`, `test_support.rs`,
   `services/index_briefing.rs`, `uds/listener.rs`). These must read from config or use a
   shared constant/helper so the value is expressed in one place.
8. Change the Rust `Default` impl for `KnowledgeConfig::boosted_categories` to `vec![]`.
   The operative default `["lesson-learned"]` is expressed in the README example config
   and in the `#[serde(default)]` deserialization default function — not in the struct
   `Default` impl. This makes the Rust compiled default `[]`, while any config file omitting
   `boosted_categories` gets `["lesson-learned"]` from the serde default.
9. Add `boosted_categories = ["lesson-learned"]` and `adaptive_categories = ["lesson-learned"]`
   to the `[knowledge]` example block in `README.md`, with brief inline comments explaining
   each field's purpose. This is the canonical documentation of the operative defaults.
10. Zero effective behavior change: the application behaves identically after this feature.
    The only change is where the policy is expressed — config/documentation, not compiled
    Rust literals scattered across the codebase.

## Non-Goals

- Does NOT implement entry auto-deprecation logic — that is #409's responsibility.
- Does NOT change PPR weighting, co-access scoring, or any ranking signal.
- Does NOT wire the lifecycle policy to the existing effectiveness-based auto-quarantine path
  (which acts on quarantine, not deprecation).
- Does NOT add database schema changes — lifecycle is config-only at this stage.
- Does NOT expose a runtime MCP tool for changing lifecycle policy — operators use config.toml.
- Does NOT implement decay schedules, score thresholds, or signal mechanics for #409.
- Does NOT add `adaptive_categories` support to `DomainPackConfig` (domain packs add
  categories to the allowlist; lifecycle policy is a separate operator concern).
- Does NOT modify the operator's `config.toml` — that file is gitignored and user-managed.
  The operator adds `boosted_categories` and `adaptive_categories` to their own config.
  A default `config.toml` installed by `unimatrix init` is a separate future issue.
- The README example block is updated for documentation parity with the new fields.
- Does NOT change the behavior of `merge_configs` for `boosted_categories` — the existing
  project-overrides-global logic is correct and unchanged.

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
"procedure"]`. All 5 are currently implicitly `pinned`. The test suite enforces exactly 5
entries and explicitly guards against `outcome` (retired, ADR-005 crt-025).

### KnowledgeConfig (config.rs)

`KnowledgeConfig` currently has three fields:
- `categories: Vec<String>` — defaults to `INITIAL_CATEGORIES`
- `boosted_categories: Vec<String>` — defaults to `["lesson-learned"]` (in the `Default`
  impl AND intended to become the serde deserialization default)
- `freshness_half_life_hours: Option<f64>`

The `boosted_categories` field is the direct structural predecessor for `adaptive_categories`:
same pattern (parallel list, must be a subset of `categories`, validated in `validate_config`
with a dedicated `ConfigError` variant: `BoostedCategoryNotInAllowlist`). The new
`adaptive_categories` field follows the exact same approach —
`AdaptiveCategoryNotInAllowlist` error variant; validation iterates `adaptive_categories`
against the `category_set` HashSet already built for the boosted check.

`validate_config()` in `config.rs` is the single post-parse validation entry point called at
startup. Adding the `adaptive_categories` cross-check here is the correct insertion point.

The `merge_configs` function (line ~1801) already handles `boosted_categories` with
project-overrides-global semantics. The new `adaptive_categories` field follows the same
pattern in `merge_configs`.

### Hardcoded boosted_categories locations (exhaustive)

The following locations construct `boosted_categories` as a `HashSet` literal with
`["lesson-learned"]` instead of reading from config:

1. **`eval/profile/layer.rs` line ~277** (production code path):
   ```
   let boosted_categories: HashSet<String> = HashSet::from(["lesson-learned".to_string()]);
   ```
   This is the eval harness `ServiceLayer` builder. It bypasses config entirely.

2. **`server.rs` line ~287** (test infrastructure):
   ```
   std::collections::HashSet::from(["lesson-learned".to_string()])
   ```
   Test server construction.

3. **`infra/shutdown.rs` line ~308** (test infrastructure):
   ```
   std::collections::HashSet::from(["lesson-learned".to_string()])
   ```
   First test in shutdown tests.

4. **`infra/shutdown.rs` line ~408** (test infrastructure):
   ```
   std::collections::HashSet::from(["lesson-learned".to_string()])
   ```
   Second test in shutdown tests.

5. **`test_support.rs` line ~129** (shared test helper):
   ```
   std::collections::HashSet::from(["lesson-learned".to_string()])
   ```
   `build_service_layer_for_test` — the central test fixture used by many tests.

6. **`services/index_briefing.rs` line ~627** (test code in briefing service):
   ```
   std::collections::HashSet::from(["lesson-learned".to_string()])
   ```

7. **`uds/listener.rs` line ~2783** (test code in UDS listener):
   ```
   std::collections::HashSet::from(["lesson-learned".to_string()])
   ```

Additionally, `config.rs` `KnowledgeConfig::default()` at line ~144 has:
```
boosted_categories: vec!["lesson-learned".to_string()],
```
This is the Rust `Default` impl that becomes `[]` after the change; the serde default
function takes over expressing `["lesson-learned"]` for deserialization.

### Test asserting the default value (main_tests.rs)

**`main_tests.rs` line 393–404**: `test_default_config_boosted_categories_is_lesson_learned`
asserts `config.knowledge.boosted_categories == ["lesson-learned"]`. This test was written
against the `Default` impl. After Goal 8 changes `Default` to `[]`, this test must be
updated — it should assert against the serde-deserialized default (an empty TOML string),
not `UnimatrixConfig::default()` (which will return `[]`). The test comment documents this
AC and should be revised to reflect the new invariant.

### config.rs test requiring empty boosted_categories workaround

**`config.rs` line ~3033–3041**: `test_empty_categories_documented_behavior` explicitly sets
`boosted_categories: vec![]` with the comment "empty boosted list to avoid allowlist check"
because `Default` would produce `["lesson-learned"]` which is not in an empty category list.
After Goal 8, this workaround is no longer needed — `KnowledgeConfig::default()` will
produce `boosted_categories: vec![]` naturally. The test should be updated to remove the
explicit override and the workaround comment.

### No canonical config.toml in repository

There is no `config.toml` file shipped in the repository (confirmed by filesystem search).
The `~/.unimatrix/config.toml` path is user-side only. The canonical documentation of
defaults and available fields lives in `README.md`, specifically the `[knowledge]` example
block at line ~239–248. This block must be updated to include `adaptive_categories` and
retain `boosted_categories` with correct comments.

### startup wiring (main.rs)

Two call sites (lines ~460 and ~849) construct `boosted_categories` from config correctly:
```rust
let boosted_categories: HashSet<String> = config.knowledge.boosted_categories.iter().cloned().collect();
```
These are the correct pattern. The problem is that six other locations in non-production
and eval code bypass this pattern and use literals.

Two call sites (lines ~550 and ~940 — the project and global config paths) construct
`CategoryAllowlist::from_categories(knowledge_categories)`. Both will need to pass the
`adaptive_categories` list so the allowlist carries the policy.

### context_status (services/status.rs + mcp/response/status.rs)

`StatusReport` is a large flat struct with `category_distribution: Vec<(String, u64)>`.
Lifecycle policy would be exposed as a new field:
`category_lifecycle: Vec<(String, String)>` listing each configured category and its
lifecycle label (`"adaptive"` or `"pinned"`). The `format_status_report` summary and JSON
paths both need updating.

`StatusService::compute_report()` constructs the `StatusReport` — this is where the
lifecycle field gets populated from the `CategoryAllowlist`.

### maintenance tick (background.rs)

`maintenance_tick()` (line 787) is `async fn` receiving `status_svc`, `store`,
`nli_enabled`, and other parameters. The guard stub belongs between Step 10
(`run_maintenance`) and Step 11 (dead-knowledge migration). It checks
`category_allowlist.is_adaptive(category)` before invoking any future deprecation action.
In this feature it is a no-op stub with a `tracing::debug!` call and a comment pointing
to #409.

### Prior patterns (from Unimatrix knowledge base)

- Entry #3715 / #3721: When modifying `INITIAL_CATEGORIES`, there are 5 locations that must
  change in lockstep. This feature does NOT add a new category — it adds policy metadata to
  existing categories — so the 5-location lockstep rule does NOT apply here.
- Entry #2312: Tests for `validate_config` with an empty categories list rely on the default
  `KnowledgeConfig` having `boosted_categories = ["lesson-learned"]`. After Goal 8, the
  `Default` impl returns `[]`, eliminating the need for explicit overrides in those tests.
- Entry #86: `CategoryAllowlist` ADR-003 established it as a runtime-extensible `HashSet`.
  This feature adds a second `RwLock<HashSet<String>>` for the adaptive set, or a
  `RwLock<(HashSet<String>, HashSet<String>)>`.
- Entry #2395: Two-level TOML config merge pattern — `merge_configs` handles `boosted_categories`
  and `adaptive_categories` identically using project-overrides-global semantics.

## Proposed Approach

### adaptive_categories

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

5. **`merge_configs`**: Add `adaptive_categories` field handling following the same
   project-overrides-global pattern as `boosted_categories`.

6. **`StatusReport`**: Add `category_lifecycle: Vec<(String, String)>` field. Populate in
   `StatusService::compute_report()` by calling `category_allowlist.list_categories()` and
   tagging each with `is_adaptive`. Expose in summary text and JSON.

7. **`maintenance_tick`**: Accept `Arc<CategoryAllowlist>` parameter. Add lifecycle guard
   stub after Step 10 with `tracing::debug!` log.

### boosted_categories de-hardcoding

8. **`KnowledgeConfig::Default` impl**: Change `boosted_categories` to `vec![]`. Add a
   `default_boosted_categories()` fn returning `vec!["lesson-learned"]` and attach it via
   `#[serde(default = "default_boosted_categories")]` on the field — so any config file
   omitting the field still gets `["lesson-learned"]` after deserialization.

9. **`eval/profile/layer.rs`**: Replace the `HashSet::from(["lesson-learned"...])` literal
   at Step 12 with a derivation from `profile.config_overrides.knowledge.boosted_categories`
   (or the appropriate config override path already present in the eval profile struct).

10. **Test infrastructure** (`server.rs`, `shutdown.rs` ×2, `test_support.rs`,
    `services/index_briefing.rs`, `uds/listener.rs`): Replace each
    `HashSet::from(["lesson-learned".to_string()])` literal. The correct fix depends on
    whether these tests have access to a `KnowledgeConfig` — if so, call the serde default
    function; if not, use `KnowledgeConfig::default_boosted_categories()` as a shared
    constant/helper. A private module-level constant `DEFAULT_BOOSTED: &[&str] =
    &["lesson-learned"]` in `infra/config.rs` visible to tests is the simplest option.

11. **`main_tests.rs`**: Update `test_default_config_boosted_categories_is_lesson_learned`
    to test the serde deserialization default (parse an empty TOML string and assert
    `boosted_categories == ["lesson-learned"]`), not the `Default` trait impl (which will
    return `[]`). The test name and comment should be updated to match the new invariant.

12. **`config.rs` test `test_empty_categories_documented_behavior`**: Remove the explicit
    `boosted_categories: vec![]` override — it is no longer needed since `Default` now
    returns `[]`.

13. **`README.md`**: Add `adaptive_categories = ["lesson-learned"]` to the `[knowledge]`
    example block with a comment. Retain `boosted_categories = ["lesson-learned"]` with
    an updated comment. Update the prose description of list fields in the config section.

Backward compatibility: existing configs omitting `adaptive_categories` silently get
`["lesson-learned"]` from the serde default. Existing configs omitting `boosted_categories`
continue to get `["lesson-learned"]` from the serde default (behavior unchanged). The only
observable change is that `UnimatrixConfig::default()` now returns `boosted_categories: []`
— callers relying on `Default` directly (rather than parsing a TOML string) will see this.
The test suite is updated accordingly.

## Acceptance Criteria

### adaptive_categories

- AC-01: `KnowledgeConfig` has an `adaptive_categories: Vec<String>` field with
  `#[serde(default)]` defaulting to `["lesson-learned"]`. Serialization round-trips
  correctly.

- AC-02: A config file omitting `adaptive_categories` produces a `KnowledgeConfig` with
  `adaptive_categories == ["lesson-learned"]` after deserialization.

- AC-03: A config file specifying `adaptive_categories = ["lesson-learned", "convention"]`
  produces a `KnowledgeConfig` with both values.

- AC-04: `validate_config` rejects a config where any entry in `adaptive_categories` is
  absent from `categories`, returning `ConfigError::AdaptiveCategoryNotInAllowlist` with
  the offending category name and config file path in the error message.

- AC-05: `CategoryAllowlist::is_adaptive("lesson-learned")` returns `true` when constructed
  with the default policy.

- AC-06: `CategoryAllowlist::is_adaptive("decision")` returns `false` when constructed with
  the default policy.

- AC-07: `CategoryAllowlist::is_adaptive` returns `false` for any category not in the
  allowlist (unknown category is not adaptive).

- AC-08: Poison recovery on the adaptive set follows the same
  `.unwrap_or_else(|e| e.into_inner())` pattern — `is_adaptive` does not panic on a
  poisoned lock.

- AC-09: `context_status` output includes a per-category lifecycle section listing each
  configured category and its label (`"adaptive"` or `"pinned"`). Both summary and JSON
  formats include this data.

- AC-10: `maintenance_tick` logs a `tracing::debug!` message listing the adaptive categories
  at each tick. The log does NOT fire if `adaptive_categories` is empty.

- AC-11: The lifecycle guard stub in `maintenance_tick` calls `is_adaptive()` and is
  annotated with a comment referencing #409 as the consumer. The stub is a no-op (no actual
  deprecation).

- AC-12: All existing `CategoryAllowlist` tests continue to pass without modification.

- AC-13: `CategoryAllowlist::new()` is equivalent to constructing with default
  `adaptive_categories = ["lesson-learned"]` — no behavior regression.

- AC-14: `validate_config` accepts a config where `adaptive_categories` is an empty list
  `[]` (disabling adaptive management entirely is valid).

- AC-15: `validate_config` accepts a config where `adaptive_categories` is a proper subset
  of `categories` with multiple entries (e.g. two adaptive categories).

- AC-16: `merge_configs` handles `adaptive_categories` with project-overrides-global
  semantics, matching the existing `boosted_categories` merge pattern.

### boosted_categories de-hardcoding

- AC-17: `KnowledgeConfig::Default` impl returns `boosted_categories: vec![]`. A unit test
  asserts this.

- AC-18: Deserializing an empty TOML string (`""`) into `UnimatrixConfig` produces
  `knowledge.boosted_categories == ["lesson-learned"]`. The existing test
  `test_default_config_boosted_categories_is_lesson_learned` is updated to cover this
  case (not `Default`).

- AC-19: `eval/profile/layer.rs` contains no `HashSet::from(["lesson-learned"...])` literal.
  The eval layer's `boosted_categories` is derived from config, not a compile-time literal.

- AC-20: `server.rs`, `shutdown.rs` (both occurrences), `test_support.rs`,
  `services/index_briefing.rs`, and `uds/listener.rs` contain no
  `HashSet::from(["lesson-learned"...])` literals. Each site reads from a shared source.

- AC-21: `config.rs` test `test_empty_categories_documented_behavior` does not explicitly
  set `boosted_categories: vec![]` — the `Default` impl produces `[]` naturally.

- AC-22: `README.md` `[knowledge]` example block includes `adaptive_categories` with a
  comment explaining it governs categories eligible for automated lifecycle management.
  The `boosted_categories` entry is retained with an updated comment.

- AC-23: No test regressions — `cargo test --workspace` passes with no new failures.

## Constraints

- `CategoryAllowlist` is `pub` and used in `server.rs` (field
  `categories: Arc<CategoryAllowlist>`) and both `main.rs` call sites. Constructor
  signature changes must not break these call sites, or must update them in the same PR.
- The `new()` constructor is used in tests (`main_tests.rs`, `categories.rs` tests,
  `server.rs` default init). Its behavior must remain backward-compatible.
- `StatusReport` has a `Default` impl; the new `category_lifecycle` field must have a
  sensible default (empty `Vec`).
- The maintenance tick's `spawn_background_tick` and `background_tick_loop` function
  signatures already carry 17+ parameters. Adding `Arc<CategoryAllowlist>` is acceptable;
  a thread-local or global is not — the allowlist is already `Arc<CategoryAllowlist>` in
  `server.rs`.
- No database schema changes — lifecycle is config-only in this feature.
- File size rule: `categories.rs` is currently 454 lines (under 500). Adding the new field
  and method may approach the limit; split defensively if needed.
- The `validate_config` function is already long (~200 lines). The new validation block
  follows the established `boosted_categories` pattern directly — no architectural change
  needed.
- The eval harness (`eval/profile/layer.rs`) currently has no access to a parsed
  `UnimatrixConfig` at the point of Step 12. The profile struct `EvalProfile` does carry
  `config_overrides: UnimatrixConfig`. The fix must use this existing path rather than
  inventing a new config injection mechanism.
- The seven test infrastructure sites all call `ServiceLayer::with_rate_config` directly.
  The shared helper pattern (a single function or constant) must be importable from all
  seven call sites without creating a circular dependency. Using a public helper function
  in `infra/config.rs` (e.g., `default_boosted_categories_set() -> HashSet<String>`) is
  one clean option.
- `README.md` is the only documentation artifact that needs updating for this feature.
  No new documentation files are created.

## Open Questions

All resolved.

1. **Constructor API** — RESOLVED: `from_categories_with_policy(cats, adaptive)` new
   constructor; `from_categories` delegates with `["lesson-learned"]` default. No callsite
   breakage.

2. **Status output format** — RESOLVED: Summary text shows only adaptive categories; JSON
   includes all categories with lifecycle labels.

3. **`add_category` runtime path** — RESOLVED: Domain pack `add_category` defaults to
   `pinned`. No lifecycle parameter. Intentional opt-in via config only.

4. **Test count** — Not a gate requirement. Qualitative estimate sufficient in brief.

5. **Eval harness config path** — RESOLVED AS ARCHITECT INVESTIGATION ITEM: The fix is
   unambiguously needed (`eval/profile/layer.rs` must not hardcode `["lesson-learned"]`).
   The architect must trace whether `config_overrides.knowledge.boosted_categories` is
   already accessible at Step 12 in `layer.rs`. If yes, one-line fix. If not, a threading
   change is required. This must be determined before implementation begins, not blocked
   on at scope time.

6. **Default impl vs serde default** — RESOLVED: Pattern is sound. `KnowledgeConfig::default()`
   returns `boosted_categories: []` (clean for programmatic test construction);
   serde deserialization of a config file omitting the field yields `["lesson-learned"]`
   via `default_boosted_categories()` (governs production behavior). Implementer must grep
   for any test calling `KnowledgeConfig::default()` that implicitly expects `["lesson-learned"]`
   in `boosted_categories` — these will now get `[]` and need updating.

## Tracking

Will be updated with GH Issue link after Session 1.
