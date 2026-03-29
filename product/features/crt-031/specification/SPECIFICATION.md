# SPECIFICATION: crt-031 — Category Lifecycle Policy + boosted_categories De-hardcoding

## Objective

Unimatrix currently treats all knowledge categories as requiring explicit operator action for
deprecation, and the value `["lesson-learned"]` for `boosted_categories` is duplicated as a
compile-time literal in seven locations outside the config load path. This feature introduces a
two-tier lifecycle policy — `pinned` vs `adaptive` — that distinguishes categories eligible for
automated management from those requiring human action. It also consolidates `boosted_categories`
so the value is expressed only in config and its serde default, not in scattered Rust literals.
The policy is config-driven, startup-validated, exposed in `context_status`, and establishes a
tested insertion point in the maintenance tick for the future auto-deprecation pass in #409.

---

## Domain Model

### Ubiquitous Language

| Term | Definition |
|------|------------|
| **Pinned** | A category whose entries are never touched by automated retention logic. Deprecation requires explicit operator action. All categories are pinned unless listed in `adaptive_categories`. |
| **Adaptive** | A category whose entries are candidates for automated lifecycle management (e.g., auto-deprecation by #409). Must appear in both `categories` and `adaptive_categories`. |
| **CategoryPolicy** | The runtime encoding of the pinned/adaptive distinction, held inside `CategoryAllowlist` alongside the category presence set. |
| **Lifecycle label** | The string `"adaptive"` or `"pinned"` assigned to each configured category for display in `context_status`. |
| **Lifecycle guard** | A conditional in `maintenance_tick` that calls `is_adaptive()` before dispatching any automated retention action. A no-op stub in this feature; #409 fills in the body. |
| **AdaptiveCategoryNotInAllowlist** | The `ConfigError` variant emitted when `validate_config` detects a category listed in `adaptive_categories` that is absent from `categories`. |
| **default_adaptive_categories** | The serde default function returning `vec!["lesson-learned".to_string()]`, applied when `adaptive_categories` is omitted from config. |
| **default_boosted_categories** | The serde default function returning `vec!["lesson-learned".to_string()]`, applied when `boosted_categories` is omitted from config. |
| **Hardcoded literal** | A `HashSet::from(["lesson-learned".to_string()])` construct appearing outside the config load path — the seven sites identified in SCOPE.md §Background. |

### Key Entities and Relationships

```
KnowledgeConfig
  ├── categories: Vec<String>           -- allowlist (validated subset, 1–64 entries)
  ├── boosted_categories: Vec<String>   -- search re-ranking boost, subset of categories
  ├── adaptive_categories: Vec<String>  -- lifecycle policy, subset of categories
  └── freshness_half_life_hours: Option<f64>

CategoryAllowlist
  ├── categories: RwLock<HashSet<String>>   -- presence set (existing)
  └── adaptive: RwLock<HashSet<String>>     -- adaptive set (new)
  Methods:
    from_categories(cats) -> Self           -- existing, delegates with ["lesson-learned"] default
    from_categories_with_policy(cats, adaptive) -> Self   -- new constructor
    is_adaptive(&self, category: &str) -> bool            -- new method
    validate / add_category / list_categories             -- unchanged

validate_config(config, path) -> Result<(), ConfigError>
  -- Validates adaptive_categories subset constraint (new, same pattern as boosted)

merge_configs(global, project) -> UnimatrixConfig
  -- adaptive_categories follows project-overrides-global (same as boosted_categories)

StatusReport
  └── category_lifecycle: Vec<(String, String)>   -- new field: (category, "adaptive"|"pinned")

maintenance_tick(...)
  -- Receives Arc<CategoryAllowlist>; lifecycle guard stub calls is_adaptive() (new)
```

---

## Functional Requirements

### Part A: adaptive_categories

**FR-01: KnowledgeConfig field**
`KnowledgeConfig` gains an `adaptive_categories: Vec<String>` field annotated with
`#[serde(default = "default_adaptive_categories")]`. The `default_adaptive_categories()` private
fn returns `vec!["lesson-learned".to_string()]`. The field must serialize and deserialize
correctly in a round-trip through `toml::to_string` / `toml::from_str`.

**FR-02: Serde deserialization default**
When a config file omits `adaptive_categories`, deserialization produces
`adaptive_categories == ["lesson-learned"]`. When the field is present it takes whatever value
the operator supplies, including an empty list `[]`.

**FR-03: validate_config cross-check**
`validate_config` adds an `adaptive_categories` cross-check immediately after the existing
`boosted_categories` check. It iterates `config.knowledge.adaptive_categories` against the
`category_set: HashSet<&str>` already built for the boosted check (no redundant work). On
mismatch it returns `ConfigError::AdaptiveCategoryNotInAllowlist { path, category }` with the
offending category name and the config file path. An empty `adaptive_categories` list passes
validation unconditionally. Multiple-entry lists are accepted when all entries are present in
`categories`.

**FR-04: ConfigError variant**
A new `ConfigError::AdaptiveCategoryNotInAllowlist { path: PathBuf, category: String }` variant
is added. Its `Display` impl follows the same pattern as `BoostedCategoryNotInAllowlist`:
`"config error in {path}: [knowledge] adaptive_categories contains {category:?} which is not present in the categories list; add it to [knowledge] categories first"`.

**FR-05: CategoryAllowlist constructor**
A new `pub fn from_categories_with_policy(cats: Vec<String>, adaptive: Vec<String>) -> Self`
constructor is added. It initialises both the `categories` `RwLock<HashSet<String>>` and a new
`adaptive: RwLock<HashSet<String>>` field. The existing `from_categories(cats)` delegates to
`from_categories_with_policy(cats, vec!["lesson-learned".to_string()])` so all existing call
sites remain valid without modification.

**FR-06: CategoryAllowlist::new() backward compatibility**
`new()` continues to delegate to `from_categories(INITIAL_CATEGORIES...)`, which delegates to
`from_categories_with_policy` with the `["lesson-learned"]` adaptive default. Observable
behavior of `new()` is unchanged for all existing call sites.

**FR-07: is_adaptive method**
`pub fn is_adaptive(&self, category: &str) -> bool` reads the `adaptive` RwLock with
`.unwrap_or_else(|e| e.into_inner())` poison recovery and returns whether `category` is present
in the adaptive set. Returns `false` for any category not in the adaptive set, including
categories not in the allowlist at all.

**FR-08: Poison recovery**
All RwLock accesses on the new `adaptive` field use the `.unwrap_or_else(|e| e.into_inner())`
pattern throughout — `from_categories_with_policy` (write), `is_adaptive` (read). No panics.

**FR-09: main.rs wiring**
Both CategoryAllowlist construction call sites in `main.rs` (lines ~460 and ~550 for the global
path; lines ~849 and ~940 for the project path) are updated to use
`from_categories_with_policy(knowledge_categories, adaptive_categories)` passing
`config.knowledge.adaptive_categories` as the second argument.

**FR-10: merge_configs**
`merge_configs` adds `adaptive_categories` handling in the `knowledge:` block following the
identical project-overrides-global comparison pattern used for `boosted_categories`:
```
adaptive_categories: if project.knowledge.adaptive_categories
    != default.knowledge.adaptive_categories
{
    project.knowledge.adaptive_categories
} else {
    global.knowledge.adaptive_categories
},
```

**FR-11: context_status lifecycle output**
`StatusReport` gains a `category_lifecycle: Vec<(String, String)>` field with a `Default` of
`vec![]`. `StatusService::compute_report()` populates it by calling
`category_allowlist.list_categories()` and tagging each with `is_adaptive()`. Summary text
(human-readable) lists only the adaptive categories. JSON output includes all categories with
their lifecycle labels. The asymmetry is intentional and must be documented in code comments.

**FR-12: maintenance_tick lifecycle guard stub**
`maintenance_tick` receives an additional `category_allowlist: &Arc<CategoryAllowlist>` parameter.
Between the existing Step 10 (`run_maintenance`) and Step 11 (dead-knowledge migration) it
contains a lifecycle guard stub:
- Iterates the configured categories.
- Calls `category_allowlist.is_adaptive(category)` for each.
- Emits `tracing::debug!("lifecycle guard: category={} is_adaptive={}", category, is_adaptive)`
  only when at least one adaptive category exists (guard does NOT fire on empty list).
- Is a no-op otherwise — no entries are modified.
- Is annotated with a comment: `// #409: insert auto-deprecation dispatch here`.

**FR-13: add_category runtime path**
`CategoryAllowlist::add_category` is unchanged — domain packs added at runtime default to
`pinned`. No `adaptive` parameter is added. Operators control lifecycle via `config.toml` only.

### Part B: boosted_categories de-hardcoding

**FR-14: KnowledgeConfig Default impl**
`KnowledgeConfig::default()` is changed so `boosted_categories` returns `vec![]` (empty).
A `default_boosted_categories()` private fn is added returning `vec!["lesson-learned".to_string()]`
and attached to the field via `#[serde(default = "default_boosted_categories")]`. Any config
file omitting `boosted_categories` still gets `["lesson-learned"]` after deserialization.

**FR-15: eval/profile/layer.rs Step 12**
The `HashSet::from(["lesson-learned".to_string()])` literal at `eval/profile/layer.rs` line ~277
is replaced by derivation from `profile.config_overrides.knowledge.boosted_categories`. The
fix uses the existing `EvalProfile.config_overrides: UnimatrixConfig` path — no new config
injection is added. See OQ-01 (below) for the conditional note on accessibility.

**FR-16: Seven test infrastructure literals removed**
The following seven `HashSet::from(["lesson-learned".to_string()])` literals are removed. Each
site is replaced by a call to a shared helper — either the public function
`default_boosted_categories_set() -> HashSet<String>` exported from `infra/config.rs`, or an
equivalent zero-dependency constant. The helper must be importable from all seven sites without
creating a circular dependency.

| Site | Location |
|------|----------|
| 1 | `eval/profile/layer.rs` line ~277 (production — see FR-15) |
| 2 | `server.rs` line ~287 (test infrastructure) |
| 3 | `infra/shutdown.rs` line ~308 (test — shutdown test 1) |
| 4 | `infra/shutdown.rs` line ~408 (test — shutdown test 2) |
| 5 | `test_support.rs` line ~129 (`build_service_layer_for_test`) |
| 6 | `services/index_briefing.rs` line ~627 (test code) |
| 7 | `uds/listener.rs` line ~2783 (test code) |

**FR-17: main_tests.rs test rewrite**
`test_default_config_boosted_categories_is_lesson_learned` in `main_tests.rs` (line 393–404) is
updated to assert the serde deserialization default: parse an empty TOML string (`""`) into
`UnimatrixConfig` and assert `knowledge.boosted_categories == ["lesson-learned"]`. The test must
NOT call `UnimatrixConfig::default()` (which now returns `[]`). The test name and comment must
be revised to reflect the new invariant: the serde default governs production config; `Default`
is for programmatic construction.

**FR-18: config.rs workaround removal**
`config.rs` test `test_empty_categories_documented_behavior` currently sets
`boosted_categories: vec![]` with the comment "empty boosted list to avoid allowlist check".
This explicit override is removed. The test must work without it because `KnowledgeConfig::default()`
now returns `boosted_categories: vec![]` naturally.

**FR-19: Pre-implementation grep precondition**
Before writing any implementation code, the implementer must run:
```
grep -rn "KnowledgeConfig::default()" crates/
```
and inspect every hit for implicit reliance on `boosted_categories == ["lesson-learned"]`. Any
test or call site that expects `["lesson-learned"]` from `Default` must be updated in the same
PR. This is a mandatory pre-implementation step, not optional cleanup.

**FR-20: README.md documentation**
The `[knowledge]` example block in `README.md` (line ~239–248) is updated to include both:
```toml
boosted_categories = ["lesson-learned"]  # Categories that receive a provenance boost in search re-ranking
adaptive_categories = ["lesson-learned"] # Categories eligible for automated lifecycle management (see #409)
```
The prose description of list fields in the config section is updated to mention both fields.
No other documentation files are created or modified.

---

## Non-Functional Requirements

**NFR-01: Zero effective behavior change**
The application behaves identically after this feature for all operators who do not add
`adaptive_categories` to their config. The only observable change is that `UnimatrixConfig::default()`
returns `boosted_categories: []` — callers relying on `Default` directly (rather than parsing a
TOML string) will see this, and tests are updated accordingly. All other runtime behavior is
unchanged.

**NFR-02: File size limit**
`categories.rs` is currently 454 lines. Adding a second `RwLock<HashSet<String>>` field, the new
constructor, and `is_adaptive` is expected to reach or exceed the 500-line limit. If the file
will breach 500 lines, the architect must plan a module split (e.g., extract lifecycle logic into
`infra/categories/lifecycle.rs`) before implementation begins. The spec records this as a
pre-implementation decision for the architect (see SR-01).

**NFR-03: No panic on poisoned lock**
All new `RwLock` accesses use `.unwrap_or_else(|e| e.into_inner())`. Tests must cover this path
for the new `adaptive` lock (mirroring the existing poison-recovery tests in `categories.rs`).

**NFR-04: No database schema changes**
Lifecycle is config-only in this feature. No migrations, no new tables, no schema version bump.

**NFR-05: Circular dependency prevention**
The shared `default_boosted_categories_set()` helper (or equivalent) in `infra/config.rs` must
be reachable from all seven call sites listed in FR-16 without creating a circular import. The
architect must verify this before the implementation brief is finalised.

**NFR-06: Startup validation is fail-fast**
An `adaptive_categories` entry absent from `categories` causes startup abort via
`ConfigError::AdaptiveCategoryNotInAllowlist`. The error message names the offending category
and the config file path. Same fail-fast contract as `BoostedCategoryNotInAllowlist`.

**NFR-07: maintenance_tick parameter count**
`spawn_background_tick` and `background_tick_loop` already carry 22+ parameters. Adding
`Arc<CategoryAllowlist>` is acceptable per the constraint in SCOPE.md. A composite
`BackgroundTickConfig` struct is an architect option (SR-02) but is not mandated by this spec.

---

## Acceptance Criteria

All 23 AC from SCOPE.md are included verbatim. Additional ACs for SR-03 and SR-09 are added as
AC-24 through AC-27.

### adaptive_categories (AC-01 – AC-16)

**AC-01** `KnowledgeConfig` has an `adaptive_categories: Vec<String>` field with
`#[serde(default)]` defaulting to `["lesson-learned"]`. Serialization round-trips correctly.
Verification: unit test in `config.rs` tests.

**AC-02** A config file omitting `adaptive_categories` produces a `KnowledgeConfig` with
`adaptive_categories == ["lesson-learned"]` after deserialization.
Verification: unit test parsing an empty/minimal TOML string.

**AC-03** A config file specifying `adaptive_categories = ["lesson-learned", "convention"]`
produces a `KnowledgeConfig` with both values present.
Verification: unit test with explicit TOML string.

**AC-04** `validate_config` rejects a config where any entry in `adaptive_categories` is absent
from `categories`, returning `ConfigError::AdaptiveCategoryNotInAllowlist` with the offending
category name and config file path in the error message.
Verification: unit test in `config.rs` tests, `matches!` macro.

**AC-05** `CategoryAllowlist::is_adaptive("lesson-learned")` returns `true` when constructed
with the default policy.
Verification: unit test in `categories.rs` tests.

**AC-06** `CategoryAllowlist::is_adaptive("decision")` returns `false` when constructed with
the default policy.
Verification: unit test in `categories.rs` tests.

**AC-07** `CategoryAllowlist::is_adaptive` returns `false` for any category not in the
allowlist (unknown category is not adaptive).
Verification: unit test with `is_adaptive("nonexistent")`.

**AC-08** Poison recovery on the adaptive set follows the `.unwrap_or_else(|e| e.into_inner())`
pattern — `is_adaptive` does not panic on a poisoned lock.
Verification: unit test using the existing `poison_allowlist` helper pattern.

**AC-09** `context_status` output includes a per-category lifecycle section listing each
configured category and its label (`"adaptive"` or `"pinned"`). Summary text lists only
adaptive categories; JSON output includes all categories with lifecycle labels.
Verification: integration test or unit test on `StatusReport` content.

**AC-10** `maintenance_tick` logs a `tracing::debug!` message listing the adaptive categories
at each tick. The log does NOT fire if `adaptive_categories` is empty.
Verification: unit test with `tracing_test` or equivalent subscriber capture.

**AC-11** The lifecycle guard stub in `maintenance_tick` calls `is_adaptive()` and is annotated
with a comment referencing `#409` as the consumer. The stub is a no-op (no actual deprecation).
Verification: code review + compilation; no entries are modified by the tick.

**AC-12** All existing `CategoryAllowlist` tests continue to pass without modification.
Verification: `cargo test --workspace` green; existing test names unchanged.

**AC-13** `CategoryAllowlist::new()` is equivalent to constructing with default
`adaptive_categories = ["lesson-learned"]` — no behavior regression.
Verification: unit test asserting `new().is_adaptive("lesson-learned") == true`.

**AC-14** `validate_config` accepts a config where `adaptive_categories` is an empty list `[]`
(disabling adaptive management entirely is valid).
Verification: unit test.

**AC-15** `validate_config` accepts a config where `adaptive_categories` is a proper subset of
`categories` with multiple entries (e.g., two adaptive categories).
Verification: unit test with two-entry `adaptive_categories`.

**AC-16** `merge_configs` handles `adaptive_categories` with project-overrides-global semantics,
matching the existing `boosted_categories` merge pattern.
Verification: unit test exercising `merge_configs` with differing global/project values.

### boosted_categories de-hardcoding (AC-17 – AC-23)

**AC-17** `KnowledgeConfig::Default` impl returns `boosted_categories: vec![]`. A dedicated
unit test asserts `KnowledgeConfig::default().boosted_categories.is_empty()`.
Verification: unit test in `config.rs` tests.

**AC-18** Deserializing an empty TOML string (`""`) into `UnimatrixConfig` produces
`knowledge.boosted_categories == ["lesson-learned"]`. The test
`test_default_config_boosted_categories_is_lesson_learned` is updated to cover this case (not
`Default`). Test name and comment updated to match the new invariant.
Verification: updated test in `main_tests.rs`.

**AC-19** `eval/profile/layer.rs` contains no `HashSet::from(["lesson-learned"...])` literal.
The eval layer's `boosted_categories` is derived from config, not a compile-time literal.
Verification: `grep -n 'lesson-learned' eval/profile/layer.rs` returns no hits.

**AC-20** `server.rs`, `shutdown.rs` (both occurrences), `test_support.rs`,
`services/index_briefing.rs`, and `uds/listener.rs` contain no
`HashSet::from(["lesson-learned"...])` literals. Each site reads from a shared source.
Verification: `grep -rn 'HashSet::from.*lesson-learned'` returns zero hits across these files.

**AC-21** `config.rs` test `test_empty_categories_documented_behavior` does not explicitly set
`boosted_categories: vec![]` — the `Default` impl produces `[]` naturally, making the override
redundant. The workaround comment is also removed.
Verification: code review; test still passes without the explicit override.

**AC-22** `README.md` `[knowledge]` example block includes both `adaptive_categories` and
`boosted_categories` entries, each with an inline comment explaining its purpose.
Verification: manual review of `README.md` diff.

**AC-23** No test regressions — `cargo test --workspace` passes with no new failures.
Verification: CI green.

### SR-03 and SR-09 mitigations (AC-24 – AC-27)

**AC-24 (SR-03: parallel list zeroing)** Every test fixture that uses partial `KnowledgeConfig`
construction (struct literal with `..Default::default()`) must explicitly zero BOTH
`boosted_categories: vec![]` AND `adaptive_categories: vec![]` when the test needs an empty
category list to avoid `validate_config` allowlist errors. The following test helpers are the
minimum affected set and must be audited:
- `config_with_custom_weights` (config.rs)
- `config_with_categories` (config.rs, if present)
- Any test using `KnowledgeConfig { categories: vec![], ..Default::default() }` or equivalent
- `test_empty_categories_documented_behavior` (config.rs — covered by AC-21 for `boosted`; must
  also zero `adaptive_categories`)

Verification: `grep -n 'KnowledgeConfig {' crates/` scan + manual audit; `cargo test` green.

**AC-25 (SR-03: validate_config fixture isolation)** A dedicated `validate_config` test for
`AdaptiveCategoryNotInAllowlist` uses a `KnowledgeConfig` with a known `categories` list,
`boosted_categories: vec![]`, and `adaptive_categories: vec!["nonexistent".to_string()]`. The
test must not trigger a `BoostedCategoryNotInAllowlist` error first (both lists must be zeroed
except the one under test).
Verification: test in `config.rs` tests, isolated from boosted check.

**AC-26 (SR-09: pre-implementation grep)** Before submitting the PR, the implementer documents
in the PR description the output of:
```
grep -rn "KnowledgeConfig::default()" crates/
```
and confirms every hit has been reviewed and updated if it relied on `boosted_categories ==
["lesson-learned"]`. PR review gates on this confirmation.
Verification: PR description checklist item.

**AC-27 (SR-09: Default impl unit test)** A new unit test named
`test_knowledge_config_default_boosted_is_empty` asserts
`KnowledgeConfig::default().boosted_categories.is_empty()`. This test is distinct from AC-17
and serves as the canonical guard that `Default` no longer implies `["lesson-learned"]`.
Verification: test in `config.rs` tests or `main_tests.rs`.

---

## Dependency Contract: Issue #409

This feature is a direct prerequisite for #409 (signal-driven entry auto-deprecation). The
contract between crt-031 and #409 is:

1. **`is_adaptive(category)` is the entry point.** #409 calls
   `category_allowlist.is_adaptive(entry.category)` before dispatching any auto-deprecation
   action. If this returns `false`, the entry is skipped unconditionally.

2. **The lifecycle guard stub in `maintenance_tick` is the insertion point.** #409 replaces the
   no-op comment `// #409: insert auto-deprecation dispatch here` with its dispatch logic. The
   stub is sized as a block, not a bare `if` condition, so #409 does not need to refactor
   the outer guard structure.

3. **No schema changes are provided.** #409 is responsible for any schema changes needed by its
   deprecation logic. crt-031 provides only the policy query interface.

4. **`adaptive_categories` is operator-configured.** #409 must not add or remove entries from
   the adaptive set at runtime. Any change to the adaptive set requires operator config edit and
   server restart.

5. **`add_category` at runtime is always pinned.** Domain pack categories registered via
   `add_category` are never adaptive. #409 must not assume runtime-added categories are adaptive
   even if the operator later adds them to `adaptive_categories` in config without restart.

---

## User Workflows

### Operator: enable adaptive management for a custom category

1. Add the category to `[knowledge] categories` in `~/.unimatrix/config.toml`.
2. Add the same category to `[knowledge] adaptive_categories` in the same file.
3. Restart the server. `validate_config` confirms the cross-check passes.
4. Call `context_status` to verify the category appears with label `"adaptive"` in the lifecycle
   section.

### Operator: disable all adaptive management

1. Set `adaptive_categories = []` in `[knowledge]` section of `config.toml`.
2. Restart the server.
3. All categories now appear as `"pinned"` in `context_status`.

### Operator: verify current lifecycle policy

1. Call `context_status` (any format).
2. Summary text lists the current adaptive categories by name.
3. JSON output lists all categories with their lifecycle labels.

### Developer: add auto-deprecation logic (#409)

1. Locate the stub comment `// #409: insert auto-deprecation dispatch here` in `background.rs`.
2. Implement dispatch logic inside the stub block.
3. Call `category_allowlist.is_adaptive(category)` to gate each candidate entry.
4. The outer guard structure is already in place — no refactoring of `maintenance_tick` needed.

---

## Constraints

1. **CategoryAllowlist is pub.** `server.rs`, both `main.rs` call sites, and tests reference it.
   The new `from_categories_with_policy` constructor must be `pub`. The `new()` and
   `from_categories()` signatures must not change. All call sites must compile without
   modification — `from_categories` delegates, not a breaking change.

2. **StatusReport has a Default impl.** The new `category_lifecycle: Vec<(String, String)>`
   field must carry `#[serde(default)]` or be initialised to `vec![]` in the `Default` impl.

3. **No database schema changes.** No migrations, no new tables, no schema version bump.

4. **File size: categories.rs.** Currently 454 lines. The 500-line ceiling applies. If the
   additions breach this, the architect must split the file before implementation (SR-01). A
   `categories/lifecycle.rs` companion module is the recommended approach.

5. **validate_config is already long (~200 lines).** The new adaptive cross-check is inserted
   directly after the `boosted_categories` check using the same `category_set` — no
   architectural change, no extract-method refactor required.

6. **maintenance_tick parameter count.** Adding `Arc<CategoryAllowlist>` as a raw parameter is
   acceptable (SCOPE.md constraint). The architect may choose a composite `BackgroundTickConfig`
   struct (SR-02) — this decision must be made before pseudocode is written, not during
   implementation.

7. **config.toml is user-managed and not in the repository.** This feature does NOT create or
   modify any `config.toml` file. The README.md example block is the canonical documentation
   of defaults.

8. **No MCP tool for runtime lifecycle change.** Operators use `config.toml` and restart.

9. **Circular dependency prevention.** The `default_boosted_categories_set()` helper in
   `infra/config.rs` must be importable from all seven test infrastructure sites without
   introducing circular imports. Architect must confirm before implementation (SR-08).

---

## Dependencies

| Dependency | Type | Notes |
|------------|------|-------|
| `std::sync::RwLock<HashSet<String>>` | stdlib | Second lock for adaptive set in `CategoryAllowlist` |
| `serde` | existing crate | `#[serde(default = "fn")]` attribute on new fields |
| `toml` | existing crate | Deserialization of `adaptive_categories` from config file |
| `tracing` | existing crate | `debug!` macro in maintenance tick lifecycle guard stub |
| `infra/categories.rs` | internal | `CategoryAllowlist` — extended, not replaced |
| `infra/config.rs` | internal | `KnowledgeConfig`, `validate_config`, `merge_configs` — all extended |
| `services/status.rs` | internal | `StatusReport` + `StatusService::compute_report()` |
| `background.rs` | internal | `maintenance_tick` — gains `Arc<CategoryAllowlist>` parameter |
| `main.rs` | internal | Two call-site pairs updated to pass `adaptive_categories` |
| `README.md` | documentation | `[knowledge]` example block updated |
| `#409` | downstream issue | Consumes `is_adaptive()` and the maintenance tick stub |

---

## NOT in Scope

The following are explicitly excluded to prevent scope creep:

- Entry auto-deprecation logic — that is #409's responsibility.
- Changes to PPR weighting, co-access scoring, or any ranking signal.
- Wiring the lifecycle policy to the existing effectiveness-based auto-quarantine path.
- `DomainPackConfig.adaptive_categories` — domain packs add categories to the allowlist;
  lifecycle is a separate operator concern.
- Modifying the operator's `config.toml` — user-managed, not in the repository.
- A default `config.toml` installed by `unimatrix init` — separate future issue.
- Decay schedules, score thresholds, or signal mechanics for #409.
- A runtime MCP tool for changing lifecycle policy.
- New documentation files (README.md update only).
- Any change to the behavior of `merge_configs` for `boosted_categories` — existing logic is
  correct and unchanged.

---

## Open Questions for Architect

**OQ-01 (SR-07 / eval harness — conditional, non-blocking)**
Is `profile.config_overrides.knowledge.boosted_categories` accessible at Step 12 of
`eval/profile/layer.rs`? The SCOPE.md records `EvalProfile.config_overrides: UnimatrixConfig`
as the expected path. If this path is reachable at the literal point of the `HashSet::from`
construction, FR-15 is a one-line fix. If it is not reachable (e.g., `config_overrides` is not
in scope at Step 12), a config-threading change is required and must be designed before
implementation begins. Write the AC-19 fix implementation assuming accessibility; flag in the
implementation brief if threading is required.

**OQ-02 (SR-01 / categories.rs file size)**
Will the additions breach the 500-line ceiling? Architect must decide module layout before
pseudocode is written. The spec assumes a module split (`categories/lifecycle.rs`) is used if
needed — implementer follows whatever structure the architect specifies in the implementation
brief.

**OQ-03 (SR-02 / maintenance_tick parameter count)**
Should `Arc<CategoryAllowlist>` be added as a raw parameter or bundled into a
`BackgroundTickConfig` composite struct? Decision belongs to architect; the spec requires the
lifecycle guard stub to receive the allowlist reference by whichever mechanism the architect
chooses.

**OQ-04 (SR-08 / circular dependency for shared helper)**
Confirm that a `pub fn default_boosted_categories_set() -> HashSet<String>` in `infra/config.rs`
is importable from all seven sites in FR-16 (specifically: `eval/profile/layer.rs`,
`server.rs`, `infra/shutdown.rs`, `test_support.rs`, `services/index_briefing.rs`,
`uds/listener.rs`). If any site cannot import from `infra/config.rs` without a circular dep,
an alternative (e.g., a dedicated `infra/defaults.rs` module) is required.

---

## Self-Check

- [x] All 23 AC from SCOPE.md present (AC-01 through AC-23)
- [x] AC-24 through AC-27 address SR-03 and SR-09
- [x] #409 dependency contract section included
- [x] boosted_categories de-hardcoding requirements fully spec'd (FR-14 through FR-20)
- [x] Every functional requirement is testable with an explicit verification method
- [x] Non-functional requirements include measurable targets (line counts, error message format)
- [x] Domain Models section defines key terms
- [x] NOT in scope section is explicit
- [x] No placeholder or TBD sections — unknowns are open questions for architect
- [x] OQ-01 (eval harness) written as conditional note, non-blocking

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned 20 entries. Directly relevant: #3772
  (crt-031 CategoryAllowlist ADR), #3770 (parallel category list pattern), #3771 (shared-default
  collision trap), #3774 (Default impl change impact on tests), #2312 (validate_config fixture
  pattern with empty categories). Poison recovery pattern confirmed from #86 (ADR-003).
  Two-level merge pattern confirmed from prior `merge_configs` reading. All findings applied.
