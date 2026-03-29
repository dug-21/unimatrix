# crt-031: Category Lifecycle Policy — Architecture

## System Overview

Unimatrix currently treats all knowledge categories identically: every category is implicitly
"pinned," meaning entries in it can only be superseded by explicit operator action. This is
correct for categories like `decision` and `convention` but wrong for `lesson-learned`, where
entries should eventually age out via automated retention logic.

crt-031 introduces a categorical policy layer that classifies each category as either
**adaptive** (eligible for automated management) or **pinned** (requires explicit human
supersession). This layer is config-only — no database schema changes, no new MCP tools, no
runtime mutation. It is an infrastructure prerequisite for #409 (signal-driven auto-deprecation),
which needs a tested guard point before any automated deprecation pass can safely fire.

The feature touches five files across two subsystems:

| Subsystem | Files |
|-----------|-------|
| Config & validation | `infra/config.rs` |
| Category allowlist | `infra/categories.rs` (possible split: `infra/categories/`) |
| Status reporting | `mcp/response/status.rs`, `services/status.rs` |
| Background maintenance | `background.rs` |
| Server wiring | `main.rs` (two call sites) |

---

## Component Breakdown

### 1. KnowledgeConfig (config.rs)

**Responsibility:** Deserialize and expose operator configuration for category lifecycle policy.

Adds one new field:

```rust
pub adaptive_categories: Vec<String>
```

- Tagged `#[serde(default = "default_adaptive_categories")]`.
- `default_adaptive_categories()` returns `vec!["lesson-learned".to_string()]`.
- Existing configs omitting the field silently adopt the default (backward compatible).
- Placed after `boosted_categories` in the struct declaration and `Default` impl.

**Validation** (in `validate_config`): After the existing `boosted_categories` cross-check,
add an analogous block. Iterate `config.knowledge.adaptive_categories`; for each entry not
present in the `category_set` HashSet (already built for the boosted check), return
`ConfigError::AdaptiveCategoryNotInAllowlist { path, category }`. An empty
`adaptive_categories` list is valid — it disables automated management entirely.

### 2. CategoryAllowlist (categories.rs / lifecycle submodule)

**Responsibility:** Runtime enforcement of category validity and lifecycle classification.

**Module split decision (SR-01):** `categories.rs` is currently 454 lines. Adding a second
`RwLock<HashSet<String>>` field, `is_adaptive()`, updated constructors, and tests will exceed
the 500-line limit. The production code portion (struct + impls) will be extracted to a
`lifecycle.rs` submodule within an `infra/categories/` directory. Tests remain co-located with
the type they test (`mod.rs` or `lifecycle.rs`). The public module path
`crate::infra::categories::CategoryAllowlist` is unchanged.

**New field:**

```rust
pub struct CategoryAllowlist {
    categories: RwLock<HashSet<String>>,
    adaptive: RwLock<HashSet<String>>,
}
```

Two separate `RwLock` guards (rather than a single `RwLock<(HashSet, HashSet)>`) keeps reads
of the category set and the adaptive set independent, preserving the existing fine-grained
locking pattern from ADR-003 (entry #86).

**Constructor hierarchy:**

```
new()
  └─ from_categories(cats: Vec<String>)
       └─ from_categories_with_policy(cats: Vec<String>, adaptive: Vec<String>) -> Self
```

- `from_categories_with_policy` is the canonical constructor. All struct initialization goes
  here.
- `from_categories(cats)` delegates: `from_categories_with_policy(cats, vec!["lesson-learned"])`.
- `new()` delegates: `from_categories(INITIAL_CATEGORIES...)` — no change to its signature.
- No call site breakage: both `main.rs` sites and `server.rs` continue to compile. The two
  `main.rs` sites are updated to call `from_categories_with_policy` to wire operator config.

**New method:**

```rust
pub fn is_adaptive(&self, category: &str) -> bool {
    self.adaptive.read().unwrap_or_else(|e| e.into_inner()).contains(category)
}
```

Poison recovery follows the identical `.unwrap_or_else(|e| e.into_inner())` pattern used
throughout the file (ADR-003 pattern, entry #86).

**`add_category` (domain pack runtime path):** Silently defaults to pinned — the existing
signature `add_category(&self, category: String)` gains no lifecycle parameter. A domain pack
operator who wants an adaptive custom category must add it to `adaptive_categories` in
`config.toml` before startup (SR-06: locked decision from SCOPE).

### 3. StatusReport and format_status_report (mcp/response/status.rs, services/status.rs)

**Responsibility:** Expose per-category lifecycle to operators at runtime.

**New field on StatusReport:**

```rust
pub category_lifecycle: Vec<(String, String)>
```

- Type `String` (not `&'static str`) to avoid lifetime parameters on the struct.
- Default: empty `Vec`.
- Populated in `StatusService::compute_report()` by iterating
  `category_allowlist.list_categories()` and tagging each with
  `if allowlist.is_adaptive(cat) { "adaptive" } else { "pinned" }`.

**Status output asymmetry (locked design — SR-04 documented intent):**

- Summary text format: lists only adaptive categories (pinned is the default; showing all
  pinned categories adds noise for operators scanning text output). Example line:
  `Adaptive categories: lesson-learned`.
- JSON format (`StatusReportJson`): includes all categories with lifecycle label via a new
  `category_lifecycle` field of type `HashMap<String, String>`.

This asymmetry is intentional and must be tested with a golden-output assertion so future
formatters cannot accidentally violate either direction.

`StatusService` already receives `Arc<CategoryAllowlist>` (it is stored on `UnimatrixServer`
as `Arc<CategoryAllowlist>`). No new parameter threading is needed for `compute_report`.

### 4. Maintenance tick guard stub (background.rs)

**Responsibility:** Provide a tested insertion point for #409's auto-deprecation logic.

**Placement:** After Step 10 (`run_maintenance`) and before Step 11 (dead-knowledge migration)
in `maintenance_tick()`.

**Stub form:**

```rust
// --- Lifecycle guard stub (crt-031) — insertion point for #409 auto-deprecation ---
// Log which categories are configured as adaptive at tick start (once per tick, not per entry).
let adaptive_cats: Vec<String> = category_allowlist
    .list_categories()
    .into_iter()
    .filter(|c| category_allowlist.is_adaptive(c))
    .collect();
if !adaptive_cats.is_empty() {
    tracing::debug!(
        categories = ?adaptive_cats,
        "lifecycle guard: adaptive categories eligible for auto-deprecation"
    );
}
// TODO(#409): call auto-deprecation logic here for each adaptive category.
// The outer guard is already in place; #409 fills in the loop body.
```

The `if !adaptive_cats.is_empty()` guard ensures the log does not fire when
`adaptive_categories = []` (AC-10).

**Parameter threading (SR-02):** `maintenance_tick` currently has 11 parameters;
`spawn_background_tick` has 22. Adding `Arc<CategoryAllowlist>` to `maintenance_tick` is the
minimal correct approach. The `BackgroundTickConfig` composite struct proposed in SR-02 is an
architectural option but out of scope for crt-031 — it would require touching all 22
`spawn_background_tick` call sites. The parameter addition is explicitly justified with a
comment citing crt-031 and #409. `maintenance_tick` already has `#[allow(clippy::too_many_arguments)]`.

`spawn_background_tick` and `background_tick_loop` also each receive `Arc<CategoryAllowlist>`.
The `Arc` is already held on `UnimatrixServer` as `categories: Arc<CategoryAllowlist>`, so
wiring is a single `Arc::clone` at the call site in `main.rs`.

### 5. main.rs call site updates

Both `from_categories` call sites (lines ~550 and ~940) are updated to
`from_categories_with_policy`:

```rust
let categories = Arc::new(CategoryAllowlist::from_categories_with_policy(
    knowledge_categories,
    config.knowledge.adaptive_categories.clone(),
));
```

A compile-level wiring test (SR-05 mandate) verifies that `CategoryAllowlist` as constructed
in the server path carries the policy: instantiate with a known `adaptive_categories` list,
assert `is_adaptive` returns correct values. This is analogous to the R-14 `PhaseFreqTableHandle`
wiring test in `background.rs`.

---

## Component Interactions

```
config.toml
    │
    ▼
validate_config()           ← rejects adaptive_categories not in categories
    │
    ▼
KnowledgeConfig
  .categories
  .adaptive_categories
    │
    ▼
CategoryAllowlist::from_categories_with_policy(categories, adaptive_categories)
    │
    ├─── validate(category)      ← unchanged hot path
    ├─── is_adaptive(category)   ← new; used by StatusService and maintenance_tick
    ├─── list_categories()       ← unchanged; drives status population
    └─── add_category(cat)       ← unchanged; domain packs → pinned by default
         │
         ├─── StatusService::compute_report()
         │      └─ category_lifecycle: Vec<(String, String)>
         │           └─ format_status_report()
         │                ├─ summary: adaptive only
         │                └─ JSON: all categories with label
         │
         └─── maintenance_tick()
                └─ lifecycle guard stub
                     └─ tracing::debug! (adaptive list, once per tick)
                          └─ TODO(#409): auto-deprecation loop body
```

---

## Technology Decisions

See ADR-001 for the constructor API and config model decision.

All other choices follow existing patterns:
- `RwLock<HashSet<String>>` — established by ADR-003 (entry #86); this feature adds a second
  independent lock rather than widening the existing one.
- `#[serde(default)]` with named fn — established by `boosted_categories`; `adaptive_categories`
  mirrors it exactly (entry #3770).
- `ConfigError` variant with path + category in Display — established by
  `BoostedCategoryNotInAllowlist`; `AdaptiveCategoryNotInAllowlist` mirrors it.
- Poison recovery via `.unwrap_or_else(|e| e.into_inner())` — established in every existing
  `CategoryAllowlist` method; `is_adaptive` follows the same pattern.

---

## Integration Points

- **`main.rs`** (two call sites): Update `from_categories` → `from_categories_with_policy`;
  pass `config.knowledge.adaptive_categories`.
- **`server.rs`** field init: `CategoryAllowlist::new()` continues to work (delegates to
  `from_categories_with_policy` with `["lesson-learned"]` default). No source change needed
  for `server.rs` itself.
- **`StatusService`**: Already holds `Arc<CategoryAllowlist>` — no new wiring, just call
  `is_adaptive` in `compute_report`.
- **`background.rs`**: Receives new `Arc<CategoryAllowlist>` parameter in
  `spawn_background_tick`, `background_tick_loop`, and `maintenance_tick`.
- **`unimatrix-observe`**: Not touched — the lockstep 5-location rule (entry #3721) applies to
  category additions/removals, not to lifecycle policy metadata. This feature adds no new
  category.

---

## Integration Surface

| Integration Point | Type / Signature | Source |
|---|---|---|
| `from_categories_with_policy` | `fn(cats: Vec<String>, adaptive: Vec<String>) -> CategoryAllowlist` | `infra/categories.rs` (or `infra/categories/lifecycle.rs`) |
| `from_categories` | `fn(cats: Vec<String>) -> CategoryAllowlist` — delegates to above with `["lesson-learned"]` | same |
| `new` | `fn() -> CategoryAllowlist` — unchanged, delegates to `from_categories` | same |
| `is_adaptive` | `fn(&self, category: &str) -> bool` | same |
| `KnowledgeConfig.adaptive_categories` | `Vec<String>` — `#[serde(default = "default_adaptive_categories")]` | `infra/config.rs` |
| `ConfigError::AdaptiveCategoryNotInAllowlist` | `{ path: PathBuf, category: String }` | `infra/config.rs` |
| `StatusReport.category_lifecycle` | `Vec<(String, String)>` — default empty `Vec` | `mcp/response/status.rs` |
| `StatusReportJson.category_lifecycle` | `HashMap<String, String>` — `#[serde(skip_serializing_if = "HashMap::is_empty")]` | `mcp/response/status.rs` |
| `maintenance_tick` new param | `category_allowlist: &Arc<CategoryAllowlist>` | `background.rs` |
| `spawn_background_tick` new param | `category_allowlist: Arc<CategoryAllowlist>` | `background.rs` |

---

## Test Construction Pattern (SR-03 Mandatory)

Both `adaptive_categories` and `boosted_categories` default to `["lesson-learned"]`. Any
`validate_config` test that constructs a partial `KnowledgeConfig` with an empty or custom
`categories` list must explicitly set **both** to `vec![]` (or a valid subset) to avoid
cross-check failures with confusing error attribution.

Pattern to follow in every new `validate_config` test with custom `categories`:

```rust
KnowledgeConfig {
    categories: vec!["custom-cat".into()],
    boosted_categories: vec![],     // suppress boosted cross-check
    adaptive_categories: vec![],    // suppress adaptive cross-check
    freshness_half_life_hours: None,
}
```

Existing tests that already set `boosted_categories: vec![]` must also add
`adaptive_categories: vec![]`. The spec must enumerate all such test cases.

---

## Module Split Plan (SR-01)

Current state: `categories.rs` is 454 lines, all in one file.

After crt-031 the production code adds approximately:
- 1 field declaration (~2 lines)
- `from_categories_with_policy` constructor (~10 lines)
- Updated `from_categories` and `new` (~4 lines changed)
- `is_adaptive` method (~5 lines)
- Updated `add_category` and `list_categories` (unchanged)

The test block adds approximately:
- `is_adaptive` default and custom policy tests (~40 lines, 6–8 tests)
- Updated `poison_recovery` for `adaptive` lock (~30 lines, 2 tests)
- Wiring / constructor equivalence tests (~20 lines, 2 tests)

Estimated post-addition line count: 454 + ~111 = ~565 lines — exceeds the 500-line limit.

**Split plan:**

```
crates/unimatrix-server/src/infra/categories/
    mod.rs           — re-exports (pub use lifecycle::CategoryAllowlist, INITIAL_CATEGORIES)
    lifecycle.rs     — CategoryAllowlist struct + all impls + tests
```

The `mod.rs` approach keeps the public import path `crate::infra::categories::CategoryAllowlist`
unchanged. No other file needs updating. The split is a pure internal reorganization.

---

## Open Questions

1. **`StatusService` and `CategoryAllowlist` wiring**: `StatusService` is constructed in
   `main.rs`. It does not currently hold an `Arc<CategoryAllowlist>`. Confirm whether the
   allowlist should be added as a `StatusService` field, or whether `compute_report` receives
   it as a parameter. Check `StatusService::new()` signature before speccing.

2. **`default_adaptive_categories` naming**: The naming convention for default-value functions
   in `config.rs` should be confirmed against existing examples (e.g., the `boosted_categories`
   default fn name) before speccing to avoid a trivial mismatch.

3. **Test count estimate for gate**: Rough estimate is 22–30 new unit tests. The spec should
   confirm whether the implementation brief requires a precise count or accepts a range.

4. **`BackgroundTickConfig` composite (SR-02)**: Deferring this refactor is explicitly an
   open issue. If a follow-up wants to reduce `spawn_background_tick` parameter count, the
   composite struct pattern should be captured in Unimatrix as a candidate procedure after
   crt-031 ships.
