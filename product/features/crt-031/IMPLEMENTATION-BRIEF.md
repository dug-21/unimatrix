# crt-031 Implementation Brief: Category Lifecycle Policy + boosted_categories De-hardcoding

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/crt-031/SCOPE.md |
| Architecture | product/features/crt-031/architecture/ARCHITECTURE.md |
| Specification | product/features/crt-031/specification/SPECIFICATION.md |
| Risk Strategy | product/features/crt-031/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/crt-031/ALIGNMENT-REPORT.md |

---

## Goal

Introduce a two-tier lifecycle policy (`adaptive` vs `pinned`) into `CategoryAllowlist`,
driven by a new `adaptive_categories` field in `KnowledgeConfig`, so that future automated
retention logic (#409) has a categorical guard before touching any entry. Simultaneously,
consolidate the seven hardcoded `HashSet::from(["lesson-learned"...])` literals scattered
across test infrastructure and the eval harness into a single `default_boosted_categories_set()`
helper, expressing the operative default exclusively via a serde deserialization function rather
than the Rust `Default` impl. Zero net behavior change for any operator not adding
`adaptive_categories` to their config.

---

## CRITICAL RISKS — Read Before Writing Any Code

### R-02 (Critical): StatusService::new() has FOUR construction sites

The architecture specifies wiring `Arc<CategoryAllowlist>` through `ServiceLayer::new()`. The
risk strategy (R-02) confirmed three additional direct construction sites that bypass
`ServiceLayer` entirely:

| # | Site | File | Approx Line |
|---|------|------|-------------|
| 1 | `ServiceLayer::new()` | `services/mod.rs` | startup path |
| 2 | `run_single_tick` | `background.rs` | ~446 |
| 3 | Test helper 1 | `services/status.rs` | ~1886 |
| 4 | Test helper 2 | `services/status.rs` | ~2038 |

**Action**: Run `grep -rn "StatusService::new" crates/` as the first step and enumerate all
matches. Site 2 (`run_single_tick`) will NOT produce a compile error if the implementer inserts
`CategoryAllowlist::new()` inline — it silently carries the default `["lesson-learned"]` policy
rather than the operator-configured policy. Sites 3 and 4 will fail to compile after the
constructor signature changes (compile-time catch). All four sites must pass the operator-loaded
`Arc<CategoryAllowlist>`, not a freshly constructed default.

### R-11 (Critical): Pre-implementation grep for KnowledgeConfig::default() callers

FR-19 is a mandatory blocking first step before any code change. Run:

```
grep -rn "KnowledgeConfig::default()" crates/
grep -rn "UnimatrixConfig::default()" crates/
```

Changing `KnowledgeConfig::Default` to return `boosted_categories: vec![]` does NOT produce a
compile error. Any test that constructs via `Default` and implicitly expects
`boosted_categories == ["lesson-learned"]` will fail with an opaque assertion error that looks
unrelated to the Default change. Known affected site: `main_tests.rs` lines 393-404.
Document the grep output in the PR description (AC-26).

### R-01 (Critical): validate_config parallel-list collision

Both `boosted_categories` and `adaptive_categories` default (via serde) to `["lesson-learned"]`.
Every `validate_config` test fixture that constructs `KnowledgeConfig` with a custom `categories`
list MUST explicitly zero BOTH parallel lists or `validate_config` fires the wrong error first:

```rust
// Required pattern for all test fixtures with custom categories:
KnowledgeConfig {
    categories: vec![/* test-specific */],
    boosted_categories: vec![],    // zero — suppress boosted cross-check
    adaptive_categories: vec![],   // zero — suppress adaptive cross-check
    freshness_half_life_hours: None,
}
```

Grep `KnowledgeConfig {` across `crates/` and audit every existing fixture. Any fixture that
was already explicitly setting `boosted_categories: vec![]` for the workaround must also gain
`adaptive_categories: vec![]`. Fixtures using `..Default::default()` spread are safe after the
Default impl change (Default now returns `[]` for both fields).

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| infra/categories (module split + lifecycle policy) | pseudocode/categories.md | test-plan/categories.md |
| infra/config (KnowledgeConfig + validate_config + merge_configs + helper) | pseudocode/config.md | test-plan/config.md |
| main.rs startup wiring | pseudocode/main.md | test-plan/main.md |
| services/status + services/mod + mcp/response/status | pseudocode/status.md | test-plan/status.md |
| background (maintenance tick stub + run_single_tick) | pseudocode/background.md | test-plan/background.md |
| eval/profile/layer + test-infrastructure literal removal (6 sites) | pseudocode/eval-layer.md | test-plan/eval-layer.md |

### Cross-Cutting Artifacts

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | product/features/crt-031/pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | product/features/crt-031/test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Constructor API: break from_categories vs add new constructor | Add `from_categories_with_policy(cats, adaptive)` as canonical constructor; `from_categories` delegates with `["lesson-learned"]` default; `new()` unchanged | ADR-001 decision 1 | product/features/crt-031/architecture/ADR-001-lifecycle-policy-config-model.md |
| Config model: new `[lifecycle]` table vs parallel field on `[knowledge]` | Parallel `adaptive_categories: Vec<String>` field on `KnowledgeConfig`; same validation pattern as `boosted_categories` | ADR-001 decision 2 | product/features/crt-031/architecture/ADR-001-lifecycle-policy-config-model.md |
| Internal struct layout: single wide lock vs two independent locks | Two independent `RwLock<HashSet<String>>` fields (`categories` and `adaptive`); no hot-path contention on `validate` | ADR-001 decision 3 | product/features/crt-031/architecture/ADR-001-lifecycle-policy-config-model.md |
| KnowledgeConfig::Default for boosted_categories | `Default` returns `vec![]`; serde default fn `default_boosted_categories()` expresses `["lesson-learned"]` for production config deserialization | ADR-001 decision 4 | product/features/crt-031/architecture/ADR-001-lifecycle-policy-config-model.md |
| StatusService lifecycle wiring: parameter vs new field | `Arc<CategoryAllowlist>` added as new `StatusService` field; follows `observation_registry` pattern (col-023) | ADR-001 decision 5 | product/features/crt-031/architecture/ADR-001-lifecycle-policy-config-model.md |
| OQ-5 (eval harness path) | `profile.config_overrides.knowledge.boosted_categories` is in scope at Step 12 of `layer.rs::from_profile`; one-line fix, no threading change | ARCHITECTURE.md §OQ-5 | product/features/crt-031/architecture/ADR-001-lifecycle-policy-config-model.md |
| OQ-3 (BackgroundTickConfig composite struct) | Deferred out of scope; one additional parameter (22 to 23) accepted with existing `#[allow(clippy::too_many_arguments)]` | ARCHITECTURE.md §Open Questions | product/features/crt-031/architecture/ADR-001-lifecycle-policy-config-model.md |
| context_status output format asymmetry | Summary text: adaptive categories only. JSON output: all categories with lifecycle labels. Intentional; documented in code comments and locked by golden-output test | ADR-001 decision 2 | product/features/crt-031/architecture/ADR-001-lifecycle-policy-config-model.md |

---

## Files to Create / Modify

### New Files

| File | Summary |
|------|---------|
| `crates/unimatrix-server/src/infra/categories/mod.rs` | Replaces `categories.rs`: all existing content + `adaptive: RwLock<HashSet<String>>` field, `from_categories_with_policy`, `is_adaptive`, `list_adaptive`, and all tests |
| `crates/unimatrix-server/src/infra/categories/lifecycle.rs` | Reserved stub for future lifecycle extensions; initially minimal (one comment block) |

### Modified Files

| File | Summary |
|------|---------|
| `crates/unimatrix-server/src/infra/categories.rs` | Delete and replace with `infra/categories/` directory (module split) |
| `crates/unimatrix-server/src/infra/config.rs` | Add `adaptive_categories` to `KnowledgeConfig`; change `Default::boosted_categories` to `vec![]`; add both serde default fns; add `ConfigError::AdaptiveCategoryNotInAllowlist`; add `default_boosted_categories_set()` helper; extend `validate_config` and `merge_configs` |
| `crates/unimatrix-server/src/main.rs` | Update both `CategoryAllowlist` construction sites to `from_categories_with_policy`; update both `ServiceLayer::new()` sites; update both `spawn_background_tick` sites |
| `crates/unimatrix-server/src/services/status.rs` | Add `category_allowlist: Arc<CategoryAllowlist>` field; update `StatusService::new()`; update `compute_report()` to populate `category_lifecycle`; update test helpers at ~lines 1886 and ~2038 |
| `crates/unimatrix-server/src/services/mod.rs` | Update `ServiceLayer::new()` to accept and forward `Arc<CategoryAllowlist>` to `StatusService::new()` |
| `crates/unimatrix-server/src/mcp/response/status.rs` | Add `category_lifecycle: Vec<(String, String)>` to `StatusReport`; update `Default` impl; update summary formatter (adaptive only) and JSON formatter (all categories); sort alphabetically before storing |
| `crates/unimatrix-server/src/background.rs` | Add `Arc<CategoryAllowlist>` parameter to `maintenance_tick`, `background_tick_loop`, `spawn_background_tick`; add Step 10b lifecycle guard stub; update `run_single_tick` (~line 446) to pass operator-loaded Arc to `StatusService::new()` |
| `crates/unimatrix-server/src/eval/profile/layer.rs` | Replace `HashSet::from(["lesson-learned".to_string()])` at line ~277 with `profile.config_overrides.knowledge.boosted_categories.iter().cloned().collect()` |
| `crates/unimatrix-server/src/server.rs` | Replace `HashSet::from(["lesson-learned"...])` at line ~287 with `crate::infra::config::default_boosted_categories_set()` |
| `crates/unimatrix-server/src/infra/shutdown.rs` | Replace two `HashSet::from(["lesson-learned"...])` literals at lines ~308 and ~408 with `crate::infra::config::default_boosted_categories_set()` |
| `crates/unimatrix-server/src/test_support.rs` | Replace `HashSet::from(["lesson-learned"...])` at line ~129 with `crate::infra::config::default_boosted_categories_set()` |
| `crates/unimatrix-server/src/services/index_briefing.rs` | Replace `HashSet::from(["lesson-learned"...])` at line ~627 with `crate::infra::config::default_boosted_categories_set()` |
| `crates/unimatrix-server/src/uds/listener.rs` | Replace `HashSet::from(["lesson-learned"...])` at line ~2783 with `crate::infra::config::default_boosted_categories_set()` |
| `crates/unimatrix-server/src/main_tests.rs` | Rewrite `test_default_config_boosted_categories_is_lesson_learned` (lines 393-404) to parse empty TOML string; add `test_knowledge_config_default_boosted_is_empty` and `test_knowledge_config_default_adaptive_is_empty` |
| `README.md` | Update `[knowledge]` example block (~lines 239-248) to add `adaptive_categories` entry and update `boosted_categories` comment |

---

## Data Structures

### CategoryAllowlist (infra/categories/mod.rs)

```rust
pub struct CategoryAllowlist {
    categories: RwLock<HashSet<String>>,  // existing — hot path for validate()
    adaptive:   RwLock<HashSet<String>>,  // new — lifecycle policy set
}
```

Two independent locks. `is_adaptive` reads only `adaptive`; `validate` reads only `categories`.
No added contention on the hot path.

### KnowledgeConfig (infra/config.rs)

```rust
pub struct KnowledgeConfig {
    pub categories: Vec<String>,
    #[serde(default = "default_boosted_categories")]
    pub boosted_categories: Vec<String>,   // Default impl: vec![] / serde: ["lesson-learned"]
    #[serde(default = "default_adaptive_categories")]
    pub adaptive_categories: Vec<String>,  // NEW — Default impl: vec![] / serde: ["lesson-learned"]
    pub freshness_half_life_hours: Option<f64>,
}
// Default impl returns vec![] for both boosted and adaptive
// Serde default fns return vec!["lesson-learned"] for both
```

### StatusReport new field (mcp/response/status.rs)

```rust
pub category_lifecycle: Vec<(String, String)>,  // (category_name, "adaptive"|"pinned")
// Default: vec![]
// Sorted alphabetically by category name before storing
```

### ConfigError new variant

```rust
ConfigError::AdaptiveCategoryNotInAllowlist { path: PathBuf, category: String }
// Display: "config error in {path}: [knowledge] adaptive_categories contains {category:?}
//           which is not present in the categories list; add it to [knowledge] categories first"
```

---

## Function Signatures

```rust
// infra/categories/mod.rs — new canonical constructor
pub fn from_categories_with_policy(cats: Vec<String>, adaptive: Vec<String>) -> Self

// infra/categories/mod.rs — new query methods (both use .unwrap_or_else(|e| e.into_inner()))
pub fn is_adaptive(&self, category: &str) -> bool
pub fn list_adaptive(&self) -> Vec<String>

// infra/config.rs — single source of truth for default boosted set (replaces 6 literals)
pub fn default_boosted_categories_set() -> HashSet<String>

// services/status.rs — updated constructor signature
pub fn StatusService::new(
    /* existing params */,
    category_allowlist: Arc<CategoryAllowlist>,  // NEW final param
) -> Self

// background.rs — all three gain the same new param
async fn maintenance_tick(/* existing 11 params */, category_allowlist: Arc<CategoryAllowlist>)
async fn background_tick_loop(/* existing 22 params */, category_allowlist: Arc<CategoryAllowlist>)
pub fn spawn_background_tick(/* existing params */, category_allowlist: Arc<CategoryAllowlist>)
```

---

## Maintenance Tick Guard Stub (background.rs Step 10b)

```rust
// --- Step 10b: Lifecycle guard stub (crt-031) — #409 insertion point ---
{
    let adaptive = category_allowlist.list_adaptive();
    if !adaptive.is_empty() {
        tracing::debug!(
            categories = ?adaptive,
            "lifecycle guard: adaptive categories eligible for auto-deprecation (stub, #409)"
        );
        // TODO(#409): for each candidate entry in these categories, call
        // category_allowlist.is_adaptive(category) before any deprecation action.
        // If is_adaptive returns false, skip unconditionally.
    }
}
```

Placement: between Step 10 (`run_maintenance`) and Step 11 (dead-knowledge migration).
No entries are modified. The outer guard structure is in place; #409 fills the body.

---

## Constraints

1. `CategoryAllowlist` is `pub`. `from_categories` and `new()` signatures must NOT change.
   All existing call sites compile without modification; only the two `main.rs` sites are
   proactively updated to use `from_categories_with_policy`.

2. `categories.rs` is currently 454 lines with a 500-line ceiling. The module split to
   `infra/categories/mod.rs + lifecycle.rs` is mandatory before adding new code. The public
   import path `crate::infra::categories::CategoryAllowlist` must remain unchanged.

3. All `RwLock` accesses on the new `adaptive` field use `.unwrap_or_else(|e| e.into_inner())`
   poison recovery. Tests must cover the poison path for the new lock.

4. No database schema changes. No migrations. No schema version bump.

5. `StatusReport::default()` must return `category_lifecycle: vec![]`.

6. `category_lifecycle` Vec must be sorted alphabetically by category name before being stored
   in `StatusReport` (R-08: non-deterministic HashSet iteration causes flaky golden tests).

7. `maintenance_tick` already has `#[allow(clippy::too_many_arguments)]`. The 22 to 23 growth
   on `spawn_background_tick` is accepted. The `BackgroundTickConfig` composite struct refactor
   is explicitly deferred (OQ-05/SR-02). The PR description must reference this deferral.

8. The `run_single_tick` `StatusService::new()` call (~background.rs line 446) must receive
   the operator-loaded `Arc<CategoryAllowlist>`, NOT a freshly constructed `CategoryAllowlist::new()`.
   Operator-configured policy must be threaded through, not reconstructed inline.

9. `add_category` (domain pack runtime path) is unchanged and always defaults to pinned.
   A doc comment must state this invariant on the method.

10. The `lifecycle.rs` stub file is committed but initially minimal. Reserved for future use.

---

## Dependencies

| Dependency | Type | Notes |
|------------|------|-------|
| `std::sync::RwLock<HashSet<String>>` | stdlib | Second independent lock for adaptive set |
| `serde` | existing crate | `#[serde(default = "fn")]` on new config fields |
| `toml` | existing crate | Deserialization of `adaptive_categories` from config file |
| `tracing` | existing crate | `debug!` in maintenance tick lifecycle guard stub |
| `infra/categories.rs` | internal | Extended via module split; import path unchanged |
| `infra/config.rs` | internal | `KnowledgeConfig`, `validate_config`, `merge_configs`, `default_boosted_categories_set` |
| `services/status.rs` | internal | `StatusReport` + `StatusService` — new field and all 4 wiring sites |
| `mcp/response/status.rs` | internal | `StatusReport::category_lifecycle` field + formatters |
| `background.rs` | internal | `maintenance_tick`, `background_tick_loop`, `spawn_background_tick`, `run_single_tick` |
| `main.rs` | internal | 2 `CategoryAllowlist` sites + 2 `ServiceLayer` sites + 2 `spawn_background_tick` sites |
| `eval/profile/layer.rs` | internal | One-line boosted_categories fix at Step 12 |
| `README.md` | docs | `[knowledge]` example block update only |
| Issue #409 | downstream | Consumes `is_adaptive()` and the maintenance tick stub insertion point |

---

## NOT in Scope

- Entry auto-deprecation logic — #409.
- Changes to PPR weighting, co-access scoring, or any ranking signal.
- Wiring lifecycle policy to the effectiveness-based auto-quarantine path.
- `DomainPackConfig.adaptive_categories`.
- Modifying any operator-side `config.toml`.
- A default `config.toml` from `unimatrix init`.
- Decay schedules, score thresholds, or signal mechanics.
- A runtime MCP tool for changing lifecycle policy.
- New documentation files (README.md update only).
- `BackgroundTickConfig` composite struct refactor (deferred, OQ-05/SR-02).
- Any behavior change to `merge_configs` for `boosted_categories`.

---

## Alignment Status

Overall: **PASS with one accepted WARN.**

| Finding | Status | Detail |
|---------|--------|--------|
| Vision alignment | PASS | Closes W0-3 `"lesson-learned" hardcoded in scoring` gap; enables domain-agnostic lifecycle config |
| Milestone fit | PASS | Policy layer only; #409 mechanics not touched; BackgroundTickConfig deferral documented |
| Scope goals coverage | PASS | All 10 SCOPE.md goals present in SPECIFICATION FRs |
| AC coverage | PASS | All 23 original ACs in SPECIFICATION; AC-24 to AC-27 added for SR-03 and SR-09 |
| Architecture consistency | PASS | All 7 open questions resolved before spec; StatusService bypass gap caught and mitigated by R-02 |
| Risk completeness | PASS | 11 runtime risks + 4 integration risks + 7 edge cases; 3 Critical risks with full test plans |
| Scope additions | **WARN — accepted** | `list_adaptive()` public method and `lifecycle.rs` stub added beyond SCOPE.md; both are internal implementation details, no approval needed |

The accepted WARN does not block implementation. The vision guardian confirmed no approval is
required. `list_adaptive()` exists to satisfy R-06 (single lock acquisition in tick stub
instead of per-category `is_adaptive()` calls). `lifecycle.rs` is an initially-empty reserved
stub committed as part of the module split.

---

## Implementation Order

1. **Pre-implementation greps (mandatory, blocking before any code change):**
   - `grep -rn "StatusService::new" crates/` — enumerate all 4 sites (R-02)
   - `grep -rn "KnowledgeConfig::default()" crates/` — audit for boosted reliance (R-11, FR-19)
   - `grep -rn "UnimatrixConfig::default()" crates/` — same audit
   - `grep -rn 'KnowledgeConfig {' crates/` — audit test fixtures for parallel-list zeroing (R-01)

2. **Module split**: `categories.rs` to `infra/categories/mod.rs + lifecycle.rs`. Run
   `cargo check -p unimatrix-server` before adding any new code; confirm no import regressions.

3. **Config layer**: `KnowledgeConfig` extension + `Default` impl change + serde default fns
   + `ConfigError` variant + `validate_config` + `merge_configs` + `default_boosted_categories_set()`.

4. **CategoryAllowlist extension**: `from_categories_with_policy`, `is_adaptive`, `list_adaptive`.

5. **main.rs wiring**: Both `CategoryAllowlist` construction sites + both `ServiceLayer::new()`
   sites + both `spawn_background_tick` sites.

6. **StatusService wiring**: Add `category_allowlist` field + update all 4 `StatusService::new()`
   sites (including `run_single_tick` at background.rs ~line 446) + populate `category_lifecycle`
   in `compute_report()` with sorted output.

7. **StatusReport field**: Add `category_lifecycle` to `mcp/response/status.rs` + update summary
   and JSON formatters + alphabetic sort before storing.

8. **Background tick stub**: Add parameter to all 3 background functions + add Step 10b guard
   using `list_adaptive()`.

9. **Literal removal**: `eval/profile/layer.rs` + 6 test infrastructure sites.

10. **Test updates**: `main_tests.rs` rewrite + `config.rs` workaround removal + AC-17, AC-18,
    AC-24 through AC-27 test additions.

11. **README.md**: Update `[knowledge]` example block.
