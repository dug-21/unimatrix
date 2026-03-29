# crt-031 Architecture: Category Lifecycle Policy + boosted_categories De-hardcoding

## OQ-5 Resolution (Eval Harness Path — Investigated First per SR-07)

**Finding: one-line fix. No threading change. No ADR-002.**

`layer.rs:from_profile` receives `profile: &EvalProfile` as a function parameter.
`EvalProfile` carries `config_overrides: UnimatrixConfig`. This field is already used at
Step 2 (line 122: `profile.config_overrides.inference`) and Step 3 (line 149:
`&profile.config_overrides`). At Step 12 (line 277), `profile` is fully in scope throughout
`from_profile`.

Replacement for line 277:
```rust
// Step 12: Boosted categories — derived from profile config, not a literal (crt-031 Goal 6)
let boosted_categories: HashSet<String> =
    profile.config_overrides.knowledge.boosted_categories.iter().cloned().collect();
```

SR-07 is resolved as in-scope. No new ADR-002.

---

## System Overview

crt-031 addresses two related gaps that share the same fix surface:

1. **Lifecycle policy gap** — `CategoryAllowlist` has no concept of which categories are eligible
   for automated management. Without this, any future auto-deprecation pass (GH #409) has no
   categorical guard and could silently touch ADRs or conventions that must only be superseded by
   explicit human action.

2. **boosted_categories hardcoding** — The config-loaded value in `KnowledgeConfig.boosted_categories`
   is bypassed at seven locations that construct `HashSet::from(["lesson-learned".to_string()])`
   directly: one production code path (`eval/profile/layer.rs`) and six test infrastructure files.

This feature adds the policy layer and eliminates all hardcoded literals. It is strictly
config-expressiveness work: zero net behavior change, no schema changes, no new MCP tools.

The feature touches components across the server crate:

| Component | Files |
|-----------|-------|
| Config & validation | `infra/config.rs` |
| Category allowlist | `infra/categories.rs` → split to `infra/categories/` |
| Status reporting | `mcp/response/status.rs`, `services/status.rs` |
| Background maintenance | `background.rs` |
| Server wiring | `main.rs` (two call sites) |
| Eval harness | `eval/profile/layer.rs` |
| Test infrastructure | `server.rs`, `infra/shutdown.rs` ×2, `test_support.rs`, `services/index_briefing.rs`, `uds/listener.rs` |

---

## Component Breakdown

### Component 1: `infra/categories/` (module split from `categories.rs`)

**Responsibility:** Runtime category validation and lifecycle policy enforcement.

`categories.rs` is currently 454 lines. Adding two `RwLock<HashSet<String>>` fields, a new
constructor, `is_adaptive`, `list_adaptive`, and associated tests will exceed the 500-line
file-size rule. The module is split before implementation:

```
crates/unimatrix-server/src/infra/
  categories/
    mod.rs        — struct definition, INITIAL_CATEGORIES const, all impl methods, all tests
    lifecycle.rs  — (reserved for future lifecycle-specific extensions; initially minimal)
```

The simpler split places all code in `mod.rs` and reserves `lifecycle.rs` for future growth.
The public import path `crate::infra::categories::CategoryAllowlist` is unchanged — no other
file needs updating.

**Struct layout (ADR-001 decision 3):**

```rust
pub struct CategoryAllowlist {
    categories: RwLock<HashSet<String>>,   // existing hot-path field
    adaptive:   RwLock<HashSet<String>>,   // new — lifecycle policy set
}
```

Two independent locks. `is_adaptive` reads only `adaptive`; the hot `validate` path reads only
`categories` — no added contention on the write-critical path.

**Constructor hierarchy (ADR-001 decision 1):**

```
new()
  └─ from_categories(cats: Vec<String>)
       └─ from_categories_with_policy(cats: Vec<String>, adaptive: Vec<String>) -> Self
```

- `from_categories_with_policy` is the canonical constructor; all field initialization lives here.
- `from_categories(cats)` delegates: `from_categories_with_policy(cats, vec!["lesson-learned"])`.
- `new()` delegates: `from_categories(INITIAL_CATEGORIES...)` — signature unchanged.
- No existing call site breaks. The two `main.rs` sites are updated to call
  `from_categories_with_policy` to wire operator `adaptive_categories` config.

**New public methods:**

```rust
// Returns true if category is in the adaptive set.
// Poison recovery: .unwrap_or_else(|e| e.into_inner()) — same as all existing methods.
pub fn is_adaptive(&self, category: &str) -> bool

// Returns sorted list of adaptive categories (for status population and tick logging).
pub fn list_adaptive(&self) -> Vec<String>
```

**`add_category` (domain pack runtime path):** Unchanged signature. Domain pack categories
default to pinned. An operator wanting a custom adaptive category adds it to
`adaptive_categories` in `config.toml` before startup.

### Component 2: `infra/config.rs` — KnowledgeConfig extension

**Responsibility:** Carry `adaptive_categories` from config file to startup wiring; express
the `boosted_categories` default exclusively via serde deserialization, not the Rust `Default` impl.

**KnowledgeConfig changes:**

```rust
pub struct KnowledgeConfig {
    pub categories: Vec<String>,
    pub boosted_categories: Vec<String>,   // serde default fn: ["lesson-learned"]
    pub adaptive_categories: Vec<String>,  // NEW — serde default fn: ["lesson-learned"]
    pub freshness_half_life_hours: Option<f64>,
}

// CHANGED: Default impl returns empty vecs — separates Rust programmatic default
// from production deserialization default (see ADR-001 decision 2).
impl Default for KnowledgeConfig {
    fn default() -> Self {
        KnowledgeConfig {
            categories: INITIAL_CATEGORIES.iter().map(|s| s.to_string()).collect(),
            boosted_categories: vec![],    // changed from vec!["lesson-learned"]
            adaptive_categories: vec![],   // new
            freshness_half_life_hours: None,
        }
    }
}

// Serde default functions — govern what a config file omitting the field receives.
fn default_boosted_categories() -> Vec<String>  { vec!["lesson-learned".to_string()] }
fn default_adaptive_categories() -> Vec<String> { vec!["lesson-learned".to_string()] }
```

Fields use `#[serde(default = "default_boosted_categories")]` and
`#[serde(default = "default_adaptive_categories")]` respectively.

**New `ConfigError` variant:**

```rust
ConfigError::AdaptiveCategoryNotInAllowlist { path: PathBuf, category: String }
```

**`validate_config` insertion point:** Immediately after the existing
`BoostedCategoryNotInAllowlist` block. Uses the same `category_set` `HashSet` already built
for that check. Empty `adaptive_categories` is valid (disables automated management).

**`merge_configs` insertion point:** Immediately after the existing `boosted_categories`
project-overrides-global block. Same replace semantics (project overrides global).

**Public helper for test infrastructure (SR-08 resolution):**

```rust
// Public — importable from all 7 literal-replacement sites without circular dependency.
pub fn default_boosted_categories_set() -> HashSet<String> {
    default_boosted_categories().into_iter().collect()
}
```

`infra/config.rs` has no upward dependency on `server.rs`, `shutdown.rs`, `test_support.rs`,
`index_briefing.rs`, or `uds/listener.rs`, so this import is safe from all seven sites.

### Component 3: `main.rs` — startup wiring (two call sites)

Both call sites (project-config path ~line 550, global-config path ~line 940) currently:
```rust
let categories = Arc::new(CategoryAllowlist::from_categories(knowledge_categories));
```

Both change to:
```rust
let adaptive_categories: Vec<String> = config.knowledge.adaptive_categories.clone();
let categories = Arc::new(CategoryAllowlist::from_categories_with_policy(
    knowledge_categories,
    adaptive_categories,
));
```

The `adaptive_categories` extraction follows the same pattern as the existing
`knowledge_categories` extraction immediately above it in each call site.

### Component 4: `services/status.rs` — StatusService field addition

**Responsibility:** Expose per-category lifecycle policy in `context_status` output.

`StatusService` does not currently hold `Arc<CategoryAllowlist>`. It is constructed via
`StatusService::new(...)` which is called from `ServiceLayer::new()` in `main.rs`. The cleanest
wiring is to add `Arc<CategoryAllowlist>` as a new field on `StatusService`, following the same
pattern as `observation_registry` (added in col-023).

**`StatusService` gains:**
```rust
category_allowlist: Arc<CategoryAllowlist>,
```

`StatusService::new()` gains a corresponding parameter. `ServiceLayer::new()` also gains
`Arc<CategoryAllowlist>` — threaded from the `categories` local in `main.rs`. Both `main.rs`
`ServiceLayer::new()` call sites pass `Arc::clone(&categories)`.

**`StatusReport` new field (in `mcp/response/status.rs`):**
```rust
/// Per-category lifecycle label. Empty vec when allowlist carries no lifecycle data.
pub category_lifecycle: Vec<(String, String)>,  // (category_name, "adaptive"|"pinned")
```
`StatusReport::default()` returns `category_lifecycle: vec![]`.

**Population in `compute_report()`:**
```rust
let lifecycle: Vec<(String, String)> = self.category_allowlist
    .list_categories()
    .into_iter()
    .map(|cat| {
        let label = if self.category_allowlist.is_adaptive(&cat) { "adaptive" } else { "pinned" };
        (cat, label.to_string())
    })
    .collect();
report.category_lifecycle = lifecycle;
```

**Output format asymmetry (AC-09, SR-04 documented):**
- Summary text: lists only adaptive categories (pinned is the silent default; showing all adds
  operator noise). Example: `Adaptive categories: lesson-learned`.
- JSON output: full `category_lifecycle` vec with all categories labeled.

This asymmetry is intentional and must be locked by a golden-output assertion test.

### Component 5: `background.rs` — maintenance tick lifecycle guard stub

**Responsibility:** Provide a tested, clearly-marked #409 insertion point.

`maintenance_tick`, `background_tick_loop`, and `spawn_background_tick` each gain one new
parameter: `category_allowlist: Arc<CategoryAllowlist>`.

`maintenance_tick` already has `#[allow(clippy::too_many_arguments)]`. The parameter count
increases from 11 to 12 there, and from 22 to 23 in `spawn_background_tick`. The
`BackgroundTickConfig` composite struct (SR-02) remains deferred — out of scope for crt-031.

**Guard stub placement:** Between Step 10 (`run_maintenance`) and Step 11 (dead-knowledge
migration) in `maintenance_tick`:

```rust
// --- Step 10b: Lifecycle guard stub (crt-031) — #409 insertion point ---
// Lists adaptive categories once per tick. Only fires when adaptive list is non-empty (AC-10).
{
    let adaptive = category_allowlist.list_adaptive();
    if !adaptive.is_empty() {
        tracing::debug!(
            categories = ?adaptive,
            "lifecycle guard: adaptive categories eligible for auto-deprecation (stub, #409)"
        );
        // TODO(#409): for each candidate entry in these categories, call
        // category_allowlist.is_adaptive(category) before any deprecation action.
        // If is_adaptive returns false, skip. The outer guard is in place; #409 fills the body.
    }
}
```

Both `main.rs` `spawn_background_tick` call sites pass `Arc::clone(&categories)`.

### Component 6: `eval/profile/layer.rs` — boosted_categories de-hardcoding

Replace line 277:
```rust
// Before:
let boosted_categories: HashSet<String> = HashSet::from(["lesson-learned".to_string()]);
// After:
let boosted_categories: HashSet<String> =
    profile.config_overrides.knowledge.boosted_categories.iter().cloned().collect();
```

One line. No other changes to `layer.rs`.

### Component 7: Test infrastructure — hardcoded literal removal (six sites)

Each site replaces its `HashSet::from(["lesson-learned".to_string()])` literal with the
shared helper from `infra/config.rs`:

| File | Approx line | Replacement |
|------|-------------|-------------|
| `server.rs` | ~287 | `crate::infra::config::default_boosted_categories_set()` |
| `infra/shutdown.rs` | ~308 | `crate::infra::config::default_boosted_categories_set()` |
| `infra/shutdown.rs` | ~408 | `crate::infra::config::default_boosted_categories_set()` |
| `test_support.rs` | ~129 | `crate::infra::config::default_boosted_categories_set()` |
| `services/index_briefing.rs` | ~627 | `crate::infra::config::default_boosted_categories_set()` |
| `uds/listener.rs` | ~2783 | `crate::infra::config::default_boosted_categories_set()` |

---

## Component Interactions

```
config.toml (operator)
    │
    ▼
validate_config()  ←── rejects adaptive_categories not in categories at startup
    │                   (ConfigError::AdaptiveCategoryNotInAllowlist)
    ▼
KnowledgeConfig
  .categories
  .adaptive_categories        .boosted_categories
    │                              │
    ▼                              ▼
CategoryAllowlist::              main.rs: boosted_categories: HashSet<String>
from_categories_with_policy          (passed to ServiceLayer::new)
    │
    ├── validate(cat)          ← hot path: reads categories lock only
    ├── is_adaptive(cat)       ← reads adaptive lock only
    ├── list_categories()      ← reads categories lock
    └── list_adaptive()        ← reads adaptive lock
         │
         ├── StatusService (via Arc<CategoryAllowlist> field)
         │     └── compute_report()
         │           └── category_lifecycle: Vec<(String, String)>
         │                 ├── summary text: adaptive categories only
         │                 └── JSON: all categories with label
         │
         └── maintenance_tick (via Arc<CategoryAllowlist> param)
               └── Step 10b: lifecycle guard stub
                     └── tracing::debug! (adaptive list, if non-empty)
                           └── TODO(#409): auto-deprecation loop body
```

---

## Technology Decisions

See ADR-001 for constructor API and config model decisions. All implementation choices follow
established patterns:

- `RwLock<HashSet<String>>` — ADR-003 (entry #86); second independent lock, no hot-path contention.
- `#[serde(default = "fn")]` — established by `boosted_categories`; `adaptive_categories` mirrors it.
- `ConfigError` variant pattern — established by `BoostedCategoryNotInAllowlist`.
- Poison recovery via `.unwrap_or_else(|e| e.into_inner())` — every existing `CategoryAllowlist` method.
- `StatusService` new field pattern — established by `observation_registry` (col-023).
- `default_boosted_categories_set()` public helper — single source of truth for default, importable
  from all 7 literal sites without circular dependency.

---

## Integration Surface

| Integration Point | Type / Signature | Source |
|---|---|---|
| `CategoryAllowlist::from_categories_with_policy` | `fn(Vec<String>, Vec<String>) -> Self` | `infra/categories/mod.rs` |
| `CategoryAllowlist::is_adaptive` | `fn(&self, &str) -> bool` | `infra/categories/mod.rs` |
| `CategoryAllowlist::list_adaptive` | `fn(&self) -> Vec<String>` | `infra/categories/mod.rs` |
| `KnowledgeConfig::adaptive_categories` | `Vec<String>`, serde default `["lesson-learned"]` | `infra/config.rs` |
| `KnowledgeConfig::Default::boosted_categories` | `vec![]` (changed from `vec!["lesson-learned"]`) | `infra/config.rs` |
| `KnowledgeConfig::Default::adaptive_categories` | `vec![]` | `infra/config.rs` |
| `default_boosted_categories_set` | `pub fn() -> HashSet<String>` | `infra/config.rs` |
| `ConfigError::AdaptiveCategoryNotInAllowlist` | `{ path: PathBuf, category: String }` | `infra/config.rs` |
| `StatusReport::category_lifecycle` | `Vec<(String, String)>`, Default: `vec![]` | `mcp/response/status.rs` |
| `StatusService::new` new param | `category_allowlist: Arc<CategoryAllowlist>` | `services/status.rs` |
| `ServiceLayer::new` new param | `category_allowlist: Arc<CategoryAllowlist>` | `services/mod.rs` |
| `spawn_background_tick` new param | `category_allowlist: Arc<CategoryAllowlist>` | `background.rs` |
| `background_tick_loop` new param | `category_allowlist: Arc<CategoryAllowlist>` | `background.rs` |
| `maintenance_tick` new param | `category_allowlist: Arc<CategoryAllowlist>` | `background.rs` |
| `layer.rs` Step 12 replacement | `profile.config_overrides.knowledge.boosted_categories.iter().cloned().collect()` | `eval/profile/layer.rs` |

---

## SR-03: Parallel-List Default Collision — Mandatory Fixture Rewrite Pattern

Every `validate_config` test that constructs `KnowledgeConfig` with a non-default `categories`
list MUST explicitly zero ALL parallel list fields together. This is a hard requirement, not a
suggestion. After Goal 8, `KnowledgeConfig::default()` returns `vec![]` for both `boosted_categories`
and `adaptive_categories`, so the `..KnowledgeConfig::default()` spread is safe in new tests.

**Required pattern for any test with a custom `categories` list:**
```rust
KnowledgeConfig {
    categories: vec![/* test-specific */],
    boosted_categories: vec![],     // must be zeroed — suppress boosted cross-check
    adaptive_categories: vec![],    // must be zeroed — suppress adaptive cross-check
    freshness_half_life_hours: None,
}
```

Existing tests that already set `boosted_categories: vec![]` gain an `adaptive_categories:
vec![]` alongside it. The `test_empty_categories_documented_behavior` test removes its
explicit `boosted_categories: vec![]` override (no longer needed since Default returns `[]`)
and gains `adaptive_categories: vec![]` only if the struct spread is not used.

Helper constructors such as `config_with_categories` must be updated to set both parallel
fields to `vec![]`.

---

## SR-09: KnowledgeConfig::default() Callers — Enumerated

The following sites will observe `boosted_categories == []` after the `Default` impl change
and must be updated:

1. **`main_tests.rs` lines 393–404** (`test_default_config_boosted_categories_is_lesson_learned`):
   Must be rewritten to parse an empty TOML string `""` into `UnimatrixConfig` and assert
   `knowledge.boosted_categories == ["lesson-learned"]`. A companion assertion on
   `UnimatrixConfig::default().knowledge.boosted_categories == []` covers AC-17.
   Test name and comment must be updated.

2. **`config.rs` test `test_empty_categories_documented_behavior`** (~lines 3033–3041):
   Remove the explicit `boosted_categories: vec![]` override and its workaround comment —
   `Default` now produces `[]` naturally. Add `adaptive_categories: vec![]` to match the
   custom `categories` list.

3. **Implementer must grep** `KnowledgeConfig::default()` and `UnimatrixConfig::default()`
   across the full test suite before starting. Any test that constructs via `Default` and
   then asserts `boosted_categories == ["lesson-learned"]` will fail silently until found
   (entry #3774 pattern). The spec identifies items 1 and 2 as the known cases; the grep
   confirms there are no others.

---

## Open Questions

None. All prior open questions from the draft architecture are resolved:

- **OQ-1 (constructor API)**: Resolved. ADR-001.
- **OQ-2 (status format)**: Resolved. Summary adaptive-only; JSON all. ADR-001.
- **OQ-3 (`add_category`)**: Resolved. Domain packs default to pinned.
- **OQ-4 (test count)**: Not a gate requirement.
- **OQ-5 (eval harness)**: Resolved above. One-line fix. In-scope.
- **OQ-6 (Default vs serde)**: Resolved. ADR-001 decision 2; SR-09 enumerated above.
- **StatusService wiring**: Resolved. New `Arc<CategoryAllowlist>` field on `StatusService`,
  passed via `ServiceLayer::new()` from `main.rs`.
- **`default_boosted_categories_set` importability**: Confirmed. `infra/config.rs` has no
  upward dependency on any of the seven test infrastructure files.
- **`BackgroundTickConfig` composite (SR-02)**: Deferred. Out of scope for crt-031. One
  additional parameter is acceptable given existing `#[allow(clippy::too_many_arguments)]`.
