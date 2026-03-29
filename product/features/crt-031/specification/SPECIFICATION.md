# SPECIFICATION: crt-031 — Category Lifecycle Policy (Pinned vs Adaptive)

## Objective

Unimatrix currently treats all knowledge categories as requiring explicit operator action for
deprecation. This feature introduces a two-tier lifecycle policy — `pinned` vs `adaptive` — that
distinguishes categories eligible for automated management (e.g., `lesson-learned`) from those
that must only be superseded by human action (e.g., `decision`, `convention`). The policy is
config-driven, validated at startup, and exposed in `context_status` output. It also establishes
a tested insertion point in the maintenance tick for the future auto-deprecation pass in #409.

---

## Ubiquitous Language

| Term | Definition |
|------|------------|
| **Pinned** | A category whose entries are never touched by automated retention logic. Deprecation requires explicit operator action only. All categories default to pinned unless listed in `adaptive_categories`. |
| **Adaptive** | A category whose entries are candidates for automated lifecycle management (e.g., auto-deprecation by #409). The category must appear in both `categories` and `adaptive_categories` in `[knowledge]` config. |
| **CategoryPolicy** | The runtime encoding of the pinned/adaptive distinction, held inside `CategoryAllowlist` alongside the category presence set. |
| **Lifecycle label** | The string `"adaptive"` or `"pinned"` assigned to each configured category for display in `context_status`. |
| **Lifecycle guard** | A conditional in `maintenance_tick` that calls `is_adaptive()` before dispatching any automated retention action. In this feature it is a no-op stub; #409 fills in the body. |
| **AdaptiveCategoryNotInAllowlist** | The `ConfigError` variant emitted when `validate_config` detects a category listed in `adaptive_categories` that is absent from `categories`. |
| **default_adaptive_categories** | The serde default function returning `vec!["lesson-learned".to_string()]`, applied when `adaptive_categories` is omitted from config. |

---

## Functional Requirements

### FR-01: KnowledgeConfig field

`KnowledgeConfig` (in `config.rs`) MUST gain a new field:

```
pub adaptive_categories: Vec<String>
```

annotated with `#[serde(default = "default_adaptive_categories")]`. The backing function
`default_adaptive_categories()` MUST return `vec!["lesson-learned".to_string()]`. Serialization
round-trips of this field MUST preserve all values without loss.

### FR-02: Deserialization default

A `[knowledge]` section in `config.toml` that omits `adaptive_categories` entirely MUST produce
a `KnowledgeConfig` with `adaptive_categories == ["lesson-learned"]` after deserialization. No
config migration is required.

### FR-03: Explicit adaptive_categories in config

A `[knowledge]` section that specifies `adaptive_categories = ["lesson-learned", "convention"]`
MUST produce a `KnowledgeConfig` with exactly both values in `adaptive_categories`.

### FR-04: validate_config cross-check

`validate_config()` MUST check every entry in `adaptive_categories` against the `category_set`
`HashSet<&str>` already constructed during the `boosted_categories` cross-check. The check MUST
occur immediately after the existing `boosted_categories` block, reusing the same `category_set`
binding. On any mismatch, MUST return:

```
ConfigError::AdaptiveCategoryNotInAllowlist { path: path.into(), category: <offending>.clone() }
```

### FR-05: ConfigError variant and display

A new `ConfigError` variant MUST be added:

```
AdaptiveCategoryNotInAllowlist { path: PathBuf, category: String }
```

Its `Display` impl MUST follow the style of `BoostedCategoryNotInAllowlist`:

```
"config error in {path}: [knowledge] adaptive_categories contains {category:?} \
 which is not present in the categories list; add it to [knowledge] categories first"
```

### FR-06: CategoryAllowlist constructor API

A new constructor MUST be added:

```
pub fn from_categories_with_policy(cats: Vec<String>, adaptive: Vec<String>) -> Self
```

This becomes the canonical implementation path. `from_categories(cats)` MUST delegate to
`from_categories_with_policy(cats, vec!["lesson-learned".to_string()])`. `new()` MUST continue to
delegate to `from_categories(INITIAL_CATEGORIES)` as before. No existing call site is broken.

### FR-07: Adaptive set storage in CategoryAllowlist

`CategoryAllowlist` MUST hold a second `RwLock<HashSet<String>>` field storing the adaptive
set, populated by `from_categories_with_policy`. The implementation choice for the field name
and whether it uses a single `RwLock<(HashSet, HashSet)>` or two separate fields is left to the
architect, subject to the file-size constraint (FR-14).

### FR-08: is_adaptive method

`CategoryAllowlist` MUST expose:

```
pub fn is_adaptive(&self, category: &str) -> bool
```

The method MUST return `true` if and only if `category` is present in the adaptive set.
It MUST return `false` for any category not in the adaptive set, including categories that are
not in the allowlist at all (unknown categories are not adaptive). Poison recovery MUST follow the
existing `.unwrap_or_else(|e| e.into_inner())` pattern — the method MUST NOT panic on a
poisoned lock.

### FR-09: main.rs call sites updated

Both `CategoryAllowlist::from_categories(knowledge_categories)` call sites in `main.rs`
(the project-config path and the global-config path, approximately lines 550 and 940) MUST be
updated to `CategoryAllowlist::from_categories_with_policy(knowledge_categories,
config.knowledge.adaptive_categories)` so the loaded policy is wired into the allowlist
from startup.

### FR-10: StatusReport lifecycle field

`StatusReport` (in `mcp/response/status.rs`) MUST gain a new field:

```
pub category_lifecycle: Vec<(String, String)>
```

with a `Default` of empty `Vec`. Each element is `(category_name, lifecycle_label)` where
`lifecycle_label` is `"adaptive"` or `"pinned"`. The vector MUST be sorted alphabetically by
category name. `StatusReport::Default` impl MUST initialize this field to `vec![]`.

### FR-11: StatusService populates category_lifecycle

`StatusService::compute_report()` MUST populate `category_lifecycle` by calling
`category_allowlist.list_categories()` and tagging each category via `is_adaptive()`. The
`StatusService` MUST receive `Arc<CategoryAllowlist>` (it already holds `Arc<CategoryAllowlist>`
if not, this wiring is added in the same PR).

### FR-12: context_status summary format

The summary text format of `format_status_report` MUST include a line listing only the adaptive
categories (since `pinned` is the default, the summary shows only categories that differ):

```
Adaptive categories: [lesson-learned]
```

If `adaptive_categories` is empty, this line MUST be omitted from the summary. The summary MUST
NOT list pinned categories individually — their omission from the adaptive list is sufficient.

### FR-13: context_status JSON format

The JSON format of `format_status_report` MUST include a `category_lifecycle` object containing
all configured categories with their lifecycle labels. All categories MUST appear, regardless of
whether they are adaptive or pinned. Example:

```json
"category_lifecycle": {
  "convention": "pinned",
  "decision": "pinned",
  "lesson-learned": "adaptive",
  "pattern": "pinned",
  "procedure": "pinned"
}
```

### FR-14: categories.rs file-size constraint

`categories.rs` is currently 454 lines. If adding the adaptive-set field, `is_adaptive` method,
updated constructors, and tests causes the file to exceed 500 lines, the architect MUST plan a
module split (e.g., extract a `lifecycle.rs` submodule) before implementation begins. This
decision is binding on the architect.

### FR-15: maintenance_tick lifecycle guard stub

`maintenance_tick` MUST accept `Arc<CategoryAllowlist>` as an additional parameter. After Step 10
(`run_maintenance`) and before the dead-knowledge migration step, MUST add a lifecycle guard stub:

1. On each tick, log a single `tracing::debug!` message listing the categories currently
   configured as adaptive (iterating `category_allowlist.list_categories()` filtered by
   `is_adaptive()`).
2. The debug log MUST NOT fire if `adaptive_categories` is empty (conditional on non-empty
   adaptive set).
3. Immediately following, a guard stub MUST call `is_adaptive(category)` for a representative
   category (demonstrating the future call site for #409) and be annotated with a comment:
   `// TODO(#409): replace stub with auto-deprecation dispatch when adaptive signal is implemented`
4. The stub body MUST be a no-op — no entries are deprecated, modified, or touched.

### FR-16: spawn_background_tick and background_tick_loop wiring

`spawn_background_tick` and `background_tick_loop` MUST each receive `Arc<CategoryAllowlist>` as
a parameter and forward it to `maintenance_tick`. The architect SHOULD evaluate whether bundling
`Arc<CategoryAllowlist>` into a composite config struct reduces the growing parameter count (SR-02
recommendation), but this is an architect decision — the spec requires the parameter to reach
`maintenance_tick` by whatever internal structure is chosen.

### FR-17: merge_configs adaptive_categories field

`merge_configs` in `config.rs` MUST include `adaptive_categories` in the `KnowledgeConfig`
merge block, following the same per-project-wins-else-global pattern used for `boosted_categories`.

---

## Non-Functional Requirements

### NFR-01: No database schema changes

Lifecycle policy is config-only. No tables, columns, or migrations are added. The schema version
remains unchanged.

### NFR-02: No MCP tool surface changes

No new MCP tools are added. No existing tool signatures change. `context_status` output format is
additive (new field, no removals).

### NFR-03: Backward compatibility

Existing `config.toml` files that omit `adaptive_categories` silently receive the built-in
default `["lesson-learned"]`. No operator action is required on upgrade.

### NFR-04: Poison-safety

All `RwLock` reads and writes on the adaptive set MUST use `.unwrap_or_else(|e| e.into_inner())`
recovery. The same pattern as the existing `categories` field in `CategoryAllowlist` applies
without exception.

### NFR-05: Zero performance regression on hot paths

`is_adaptive` is a `HashSet::contains` call behind an `RwLock::read`. It MUST NOT be called on
MCP request hot paths (search, lookup, briefing). It is only called from `maintenance_tick` (once
per tick) and `compute_report` (once per `context_status` call). No caching is required.

### NFR-06: File-size rule

All modified files MUST remain under 500 lines after the change. If `categories.rs` would exceed
this, a module split is required (see FR-14). `background.rs` and `config.rs` are already large;
the architect must confirm they remain compliant or plan extraction.

### NFR-07: Test count

An estimated 20–28 new tests are expected across the following modules:
- `categories.rs` / `lifecycle.rs`: ~12 tests covering `is_adaptive`, `from_categories_with_policy`,
  poison recovery on the adaptive lock, `add_category` defaulting to pinned.
- `config.rs`: ~8 tests covering `AdaptiveCategoryNotInAllowlist` validation, empty
  `adaptive_categories`, multi-value `adaptive_categories`, default deserialization,
  merge behavior.
- `mcp/response/status.rs`: ~5 tests covering `category_lifecycle` field presence in both
  summary and JSON formats, empty adaptive list suppression in summary, golden-output tests for
  both formats.
- `background.rs`: ~3 tests covering the guard stub invocation and debug log gate.

---

## Acceptance Criteria

Criteria carry their original SCOPE.md IDs (AC-01 through AC-15) plus additional criteria
introduced for SR-03 and SR-05.

### AC-01
`KnowledgeConfig` has an `adaptive_categories: Vec<String>` field with
`#[serde(default = "default_adaptive_categories")]` defaulting to `["lesson-learned"]`.
Serialization round-trips correctly.
Verification: Unit test serializes and deserializes a `KnowledgeConfig` with
`adaptive_categories = ["custom-a", "custom-b"]` and asserts round-trip identity.

### AC-02
A config file omitting `adaptive_categories` produces a `KnowledgeConfig` with
`adaptive_categories == ["lesson-learned"]` after deserialization.
Verification: Unit test deserializes a minimal TOML string with no `adaptive_categories` key and
asserts the field equals `["lesson-learned"]`.

### AC-03
A config file specifying `adaptive_categories = ["lesson-learned", "convention"]` produces a
`KnowledgeConfig` with both values.
Verification: Unit test deserializes TOML with two values and asserts both are present.

### AC-04
`validate_config` rejects a config where any entry in `adaptive_categories` is absent from
`categories`, returning `ConfigError::AdaptiveCategoryNotInAllowlist` with the offending
category name and config file path in the error message.
Verification: Unit test passes a config with `categories = ["lesson-learned"]` and
`adaptive_categories = ["lesson-learned", "unknown-cat"]`; asserts the error variant and that
`category` field equals `"unknown-cat"`.

### AC-05
`CategoryAllowlist::is_adaptive("lesson-learned")` returns `true` when constructed with the
default policy (via `new()` or `from_categories(INITIAL_CATEGORIES)`).
Verification: Unit test calls `CategoryAllowlist::new()` and asserts
`al.is_adaptive("lesson-learned") == true`.

### AC-06
`CategoryAllowlist::is_adaptive("decision")` returns `false` when constructed with the default
policy.
Verification: Unit test calls `CategoryAllowlist::new()` and asserts
`al.is_adaptive("decision") == false`.

### AC-07
`CategoryAllowlist::is_adaptive` returns `false` for any category not in the allowlist (unknown
category is not adaptive).
Verification: Unit test calls `al.is_adaptive("no-such-category")` on a default allowlist and
asserts `false`.

### AC-08
Poison recovery on the adaptive set follows the same `.unwrap_or_else(|e| e.into_inner())`
pattern — `is_adaptive` does not panic on a poisoned lock.
Verification: Unit test poisons the adaptive `RwLock` using the same helper pattern as the
existing `poison_allowlist` test helper, then calls `is_adaptive` and asserts no panic and
returns a valid `bool`.

### AC-09
`context_status` output includes a per-category lifecycle section listing each configured
category and its label (`"adaptive"` or `"pinned"`). Both summary and JSON formats include this
data.
Verification: Two unit tests — one for `ResponseFormat::Summary` asserting "Adaptive categories"
line presence, one for `ResponseFormat::Json` asserting `category_lifecycle` key with all
categories labeled.

### AC-10
`maintenance_tick` logs a `tracing::debug!` message listing the adaptive categories at each tick.
The log does NOT fire if `adaptive_categories` is empty.
Verification: Unit test using a `CategoryAllowlist` with adaptive set `["lesson-learned"]`
confirms the debug path is reachable; test with empty adaptive set confirms it is skipped (via
a bool guard or `is_empty()` check visible in the stub logic).

### AC-11
The lifecycle guard stub in `maintenance_tick` calls `is_adaptive()` and is annotated with a
comment referencing #409 as the consumer. The stub is a no-op (no actual deprecation).
Verification: Code review (no behavioral test needed for the no-op stub body); a compile test
confirms `is_adaptive` is called within `maintenance_tick`.

### AC-12
All existing `CategoryAllowlist` tests continue to pass without modification.
Verification: `cargo test -p unimatrix-server -- categories` passes with zero failures and
zero test renames.

### AC-13
`CategoryAllowlist::new()` is equivalent to constructing with default
`adaptive_categories = ["lesson-learned"]` — no behavior regression.
Verification: Unit test constructs `CategoryAllowlist::new()` and
`CategoryAllowlist::from_categories_with_policy(INITIAL_CATEGORIES.to_vec(), vec!["lesson-learned".to_string()])`
and asserts `is_adaptive` returns identical results for all 5 initial categories.

### AC-14
`validate_config` accepts a config where `adaptive_categories` is an empty list `[]` (disabling
adaptive management entirely is valid).
Verification: Unit test passes a config with `adaptive_categories = []` and asserts
`validate_config` returns `Ok(())`.

### AC-15
`validate_config` accepts a config where `adaptive_categories` is a proper subset of `categories`
with multiple entries (e.g. two adaptive categories).
Verification: Unit test passes `categories = ["a", "b", "c"]` and
`adaptive_categories = ["a", "b"]` and asserts `validate_config` returns `Ok(())`.

### AC-16 (SR-03 mitigation)
Every `KnowledgeConfig` struct literal in tests that sets `categories` to a non-default value
MUST also explicitly set both `boosted_categories: vec![]` AND `adaptive_categories: vec![]`
to prevent cross-check false failures. This is verified by the passing test suite: any test that
constructs `KnowledgeConfig { categories: <custom>, ..Default::default() }` while `validate_config`
is called MUST either use the default (which satisfies the cross-check because both
`boosted_categories` and `adaptive_categories` default to `["lesson-learned"]` which is in the
default `categories`) or must explicitly zero both parallel lists. Existing helpers
`config_with_categories`, `config_with_half_life`, etc. MUST be updated to include
`adaptive_categories: vec![]` alongside any existing `boosted_categories: vec![]` modifications.
Verification: `cargo test -p unimatrix-server -- validate_config` passes with zero failures.

### AC-17 (SR-05 mitigation)
The `server.rs` field that initializes `CategoryAllowlist` (using `CategoryAllowlist::new()` or
`from_categories`) MUST produce an instance that carries the correct lifecycle policy (default
`["lesson-learned"]` as adaptive). This is guaranteed by the `new()` and `from_categories()`
delegation chain (FR-06). A compile-level test analogous to the `PhaseFreqTableHandle` wiring
test (R-14 in `background.rs`) MUST be added, asserting that `CategoryAllowlist` passed through
the `Arc` chain to `maintenance_tick` responds correctly to `is_adaptive("lesson-learned")`.
Verification: New test in `background.rs` or `server.rs` module creates a default
`CategoryAllowlist` (via `new()`), wraps in `Arc`, and calls `is_adaptive` — asserting `true`
for `"lesson-learned"` and `false` for `"decision"`.

---

## Domain Models

### CategoryAllowlist (extended)

```
CategoryAllowlist {
  categories: RwLock<HashSet<String>>   // existing: presence validation
  adaptive:   RwLock<HashSet<String>>   // new: lifecycle policy (subset of categories)
}

Methods:
  from_categories_with_policy(cats, adaptive) -> Self   // canonical constructor
  from_categories(cats) -> Self                          // delegates with ["lesson-learned"]
  new() -> Self                                          // delegates to from_categories
  validate(&self, category) -> Result<(), ServerError>   // unchanged
  add_category(&self, category)                          // unchanged; defaults to pinned
  list_categories(&self) -> Vec<String>                  // unchanged
  is_adaptive(&self, category) -> bool                   // NEW
```

### KnowledgeConfig (extended)

```
KnowledgeConfig {
  categories:               Vec<String>   // existing
  boosted_categories:       Vec<String>   // existing
  adaptive_categories:      Vec<String>   // NEW; default = ["lesson-learned"]
  freshness_half_life_hours: Option<f64>  // existing
}
```

### ConfigError (extended)

```
ConfigError::AdaptiveCategoryNotInAllowlist { path: PathBuf, category: String }
```

Invariant: any category in `adaptive_categories` MUST also appear in `categories`. Validated at
startup by `validate_config`. Emits `AdaptiveCategoryNotInAllowlist` on violation.

### StatusReport (extended)

```
StatusReport {
  ...existing fields...
  category_lifecycle: Vec<(String, String)>   // NEW; (category, "pinned"|"adaptive"), sorted
}
```

Default: empty Vec. Populated by `StatusService::compute_report()`.

### Lifecycle guard (stub)

A no-op code path in `maintenance_tick`, located between Step 10 (`run_maintenance`) and the
dead-knowledge migration step. Calls `is_adaptive()` and emits a `tracing::debug!`. Annotated
with `// TODO(#409)`. This constitutes the defined insertion point contract that #409 may rely on
without further refactoring.

---

## User Workflows

### Operator: configure adaptive categories

1. Edit `config.toml`; add `adaptive_categories = ["lesson-learned"]` under `[knowledge]`.
2. Start (or restart) the server. `validate_config` runs at startup.
3. If any listed category is absent from `categories`, server exits with
   `AdaptiveCategoryNotInAllowlist` error including the offending category name and path.
4. If valid, `CategoryAllowlist` is constructed with the policy loaded.
5. Operator calls `context_status` to verify configuration: summary shows
   `Adaptive categories: [lesson-learned]`; JSON shows per-category lifecycle labels.

### Operator: disable adaptive management entirely

1. Set `adaptive_categories = []` in `config.toml`.
2. Restart server. `validate_config` accepts empty list (AC-14).
3. `context_status` summary omits the "Adaptive categories" line.
4. `maintenance_tick` skips the debug log (AC-10) because adaptive set is empty.

### Future: #409 auto-deprecation

1. #409 implementer finds the lifecycle guard stub in `maintenance_tick`.
2. The `is_adaptive()` guard already exists and is tested.
3. #409 replaces the stub body with auto-deprecation dispatch, leaving the outer `is_adaptive()`
   guard in place.
4. No changes to `CategoryAllowlist`, `KnowledgeConfig`, or `StatusReport` are required.

---

## Constraints

1. `CategoryAllowlist` is `pub`. Constructor signature changes must preserve the `new()` and
   `from_categories()` call sites without modification. All changes are additive.
2. `from_categories_with_policy` is the new canonical constructor; the old constructors delegate.
3. `StatusReport` has a `Default` impl; `category_lifecycle` MUST have a sensible default (empty
   `Vec`), compatible with the thin report shell constructed in `maintenance_tick` (line 816).
4. `spawn_background_tick` currently has 22 parameters and already carries
   `#[allow(clippy::too_many_arguments)]`. Adding `Arc<CategoryAllowlist>` is acceptable; a
   thread-local or global is not.
5. No database schema changes. No migration files.
6. No runtime MCP tool for changing lifecycle policy. Operators use `config.toml` only.
7. `add_category(&self, category: String)` silently defaults new runtime categories to `pinned`.
   No API change.
8. `categories.rs` is 454 lines. Adding the new field, methods, and tests may exceed 500 lines.
   Architect MUST plan a module split if needed before implementation.
9. `background.rs` is large. Adding `Arc<CategoryAllowlist>` as a parameter is the correct path.
   Architect may bundle with a composite struct to reduce parameter count.

---

## Dependencies

### Crates (no new dependencies)

All changes are within `unimatrix-server`. No new crate dependencies are required.
- `std::collections::HashSet` — existing
- `std::sync::RwLock` — existing
- `serde` — existing

### Existing Components

| Component | Location | Relationship |
|-----------|----------|--------------|
| `CategoryAllowlist` | `src/infra/categories.rs` | Modified (new field + method) |
| `KnowledgeConfig` | `src/infra/config.rs` | Modified (new field) |
| `validate_config` | `src/infra/config.rs` | Modified (new cross-check) |
| `merge_configs` | `src/infra/config.rs` | Modified (new field in merge block) |
| `StatusReport` | `src/mcp/response/status.rs` | Modified (new field) |
| `format_status_report` | `src/mcp/response/status.rs` | Modified (summary + JSON paths) |
| `StatusService::compute_report` | `src/services/status.rs` | Modified (populate new field) |
| `maintenance_tick` | `src/background.rs` | Modified (new parameter + stub) |
| `spawn_background_tick` | `src/background.rs` | Modified (new parameter) |
| `background_tick_loop` | `src/background.rs` | Modified (new parameter) |
| `main.rs` | `src/main.rs` | Modified (two call sites) |

### #409 Dependency Contract

Feature #409 (auto-deprecation signal implementation) MAY assume the following from this feature:

1. `CategoryAllowlist::is_adaptive(&self, category: &str) -> bool` exists and is stable. Its
   signature will not change.
2. A lifecycle guard stub exists in `maintenance_tick` between Step 10 and the dead-knowledge
   migration step. #409 replaces the stub body without touching the outer guard.
3. `Arc<CategoryAllowlist>` is already wired into `maintenance_tick` as a parameter.
4. `KnowledgeConfig::adaptive_categories` exists and is serde-deserialized. #409 may add new
   entries to the operator-facing documentation without changing the field type.
5. #409 MUST NOT add decay schedules, score thresholds, or any signal mechanics to the
   `CategoryAllowlist` struct — those belong in a separate service layer.

---

## NOT in Scope

- Entry auto-deprecation logic — #409's responsibility.
- PPR weighting, co-access scoring, or any search ranking signal changes.
- Wiring lifecycle policy to the existing effectiveness-based auto-quarantine path.
- Database schema changes.
- Runtime MCP tool for changing lifecycle policy.
- Decay schedules, score thresholds, or signal mechanics.
- `adaptive_categories` support in `DomainPackConfig` — domain pack categories default silently
  to `pinned` via `add_category`.
- Markdown/summary format changes to any `context_status` section other than adding
  "Adaptive categories" line.
- Any changes to the 5 existing `INITIAL_CATEGORIES` entries or their set membership.

---

## Open Questions

OQ-01 (resolved by locked design): Constructor API is `from_categories_with_policy` (new) +
`from_categories` delegates with `["lesson-learned"]` default. No callsite breakage.

OQ-02 (resolved by locked design): Status summary shows only adaptive categories; JSON includes
all with labels. The asymmetry is intentional and MUST be documented with a golden-output test
(SR-04 recommendation).

OQ-03 (resolved by locked design): `add_category` at runtime silently defaults to `pinned`. No
API change required.

OQ-04 (open, for architect): Should the adaptive `HashSet` be stored as a second `RwLock<HashSet>` field or as a `RwLock<(HashSet, HashSet)>`? Both satisfy FR-04 and FR-08. Architect decides during design phase based on FR-14 file-size impact.

OQ-05 (open, for architect): Should `Arc<CategoryAllowlist>` be bundled into a composite struct
alongside `ConfidenceParams` and `InferenceConfig` to address SR-02 (22-parameter count)?
This is an architect decision — the spec requires the parameter reaches `maintenance_tick` by
whatever path is chosen.

OQ-06 (open): The exact test count gate requirement is not stated in IMPLEMENTATION-BRIEF (not
yet written). The rough estimate of 20–28 new tests is non-binding; the gate verifies all tests
pass, not a specific count delta.

---

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing — returned 18 entries; most relevant: #3715
  (INITIAL_CATEGORIES lockstep rule — confirmed does NOT apply here as no new category is added),
  #86 (CategoryAllowlist ADR-003 runtime-extensible HashSet), entry #2312 precedent for
  `boosted_categories` default trap (SR-03 cross-reference). No results directly described the
  pinned/adaptive lifecycle pattern; this feature establishes a new convention.
