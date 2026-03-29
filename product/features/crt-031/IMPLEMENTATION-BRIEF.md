# crt-031: Category Lifecycle Policy (Pinned vs Adaptive) — Implementation Brief

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

Introduce a two-tier categorical lifecycle policy (`pinned` vs `adaptive`) that distinguishes
knowledge categories eligible for automated management from those requiring explicit operator
action. The policy is config-driven, validated at startup, exposed in `context_status`, and
establishes a tested insertion point in the maintenance tick for the future auto-deprecation
logic in #409.

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| KnowledgeConfig + validate_config + merge_configs | pseudocode/config.md | test-plan/config.md |
| CategoryAllowlist (categories/lifecycle module) | pseudocode/categories.md | test-plan/categories.md |
| StatusReport + format_status_report + StatusService | pseudocode/status.md | test-plan/status.md |
| maintenance_tick guard stub | pseudocode/background.md | test-plan/background.md |
| main.rs call site updates | pseudocode/main.md | test-plan/main.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|------------|--------|----------|
| Constructor API for lifecycle policy injection | Add `from_categories_with_policy(cats, adaptive)` as canonical constructor; `from_categories` and `new()` delegate to it — no call-site breakage | ADR-001 | product/features/crt-031/architecture/ADR-001-lifecycle-policy-config-model.md |
| Config model for `adaptive_categories` | Parallel `Vec<String>` field on `KnowledgeConfig` under `[knowledge]`, following `boosted_categories` pattern exactly | ADR-001 | product/features/crt-031/architecture/ADR-001-lifecycle-policy-config-model.md |
| Internal struct layout for adaptive set | Two independent `RwLock<HashSet<String>>` fields (not a single wide `RwLock<(HashSet, HashSet)>`) to preserve fine-grained locking from ADR-003 | ADR-001 | product/features/crt-031/architecture/ADR-001-lifecycle-policy-config-model.md |
| `add_category` lifecycle default | Runtime-added categories (domain packs) silently default to `pinned`; no API change | ADR-001 | product/features/crt-031/architecture/ADR-001-lifecycle-policy-config-model.md |
| `context_status` output format asymmetry | Summary text shows only adaptive categories; JSON includes all categories with labels | ADR-001 / SPEC FR-12/FR-13 | product/features/crt-031/architecture/ADR-001-lifecycle-policy-config-model.md |
| `categories.rs` module split | File is 454 lines; adding ~111 lines exceeds 500-line limit; split to `infra/categories/mod.rs` + `infra/categories/lifecycle.rs` before implementation | ARCH SR-01 | product/features/crt-031/architecture/ADR-001-lifecycle-policy-config-model.md |
| `BackgroundTickConfig` composite struct | Deferred out of scope for crt-031; `spawn_background_tick` accepts the 23rd parameter with explicit `#[allow(clippy::too_many_arguments)]` justification | ARCH OQ-05 / SR-02 | product/features/crt-031/architecture/ADR-001-lifecycle-policy-config-model.md |
| FR-17 `merge_configs` field (WARN-01) | Accepted: necessary to prevent silent config drop (FM-04); follows `boosted_categories` pattern; zero product-direction risk | ALIGNMENT WARN-01 | product/features/crt-031/architecture/ADR-001-lifecycle-policy-config-model.md |

---

## Files to Create / Modify

| File | Action | Summary |
|------|--------|---------|
| `crates/unimatrix-server/src/infra/categories/mod.rs` | Create | Module root; re-exports `CategoryAllowlist` and `INITIAL_CATEGORIES` from `lifecycle.rs`; preserves public import path |
| `crates/unimatrix-server/src/infra/categories/lifecycle.rs` | Create | All `CategoryAllowlist` struct definition, impls, and tests (extracted from `categories.rs`) plus new `adaptive` field, `from_categories_with_policy`, and `is_adaptive` |
| `crates/unimatrix-server/src/infra/categories.rs` | Delete / Replace | Replaced by the `infra/categories/` module directory |
| `crates/unimatrix-server/src/infra/config.rs` | Modify | Add `adaptive_categories` field to `KnowledgeConfig`; add `default_adaptive_categories()` fn; add `ConfigError::AdaptiveCategoryNotInAllowlist` variant; extend `validate_config` cross-check; extend `merge_configs` merge block (FR-17) |
| `crates/unimatrix-server/src/mcp/response/status.rs` | Modify | Add `category_lifecycle: Vec<(String, String)>` to `StatusReport`; add `category_lifecycle: HashMap<String, String>` to `StatusReportJson`; update `format_status_report` for both summary and JSON paths |
| `crates/unimatrix-server/src/services/status.rs` | Modify | Populate `category_lifecycle` in `StatusService::compute_report()` via `is_adaptive`; confirm or wire `Arc<CategoryAllowlist>` into `StatusService` |
| `crates/unimatrix-server/src/background.rs` | Modify | Add `Arc<CategoryAllowlist>` parameter to `spawn_background_tick`, `background_tick_loop`, and `maintenance_tick`; add lifecycle guard stub after Step 10 |
| `crates/unimatrix-server/src/main.rs` | Modify | Update both `from_categories` call sites (~lines 550 and ~940) to `from_categories_with_policy(knowledge_categories, config.knowledge.adaptive_categories.clone())` |

---

## Data Structures

### KnowledgeConfig (extended)

```rust
pub struct KnowledgeConfig {
    pub categories:               Vec<String>,          // existing
    pub boosted_categories:       Vec<String>,          // existing
    #[serde(default = "default_adaptive_categories")]
    pub adaptive_categories:      Vec<String>,          // NEW — default ["lesson-learned"]
    pub freshness_half_life_hours: Option<f64>,         // existing
}

fn default_adaptive_categories() -> Vec<String> {
    vec!["lesson-learned".to_string()]
}
```

### CategoryAllowlist (extended)

```rust
pub struct CategoryAllowlist {
    categories: RwLock<HashSet<String>>,    // existing: presence validation
    adaptive:   RwLock<HashSet<String>>,    // NEW: lifecycle policy subset
}
```

### ConfigError (extended)

```rust
ConfigError::AdaptiveCategoryNotInAllowlist { path: PathBuf, category: String }
// Display: "config error in {path}: [knowledge] adaptive_categories contains {category:?}
//           which is not present in the categories list; add it to [knowledge] categories first"
```

### StatusReport (extended)

```rust
pub struct StatusReport {
    // ...existing fields...
    pub category_lifecycle: Vec<(String, String)>,  // NEW; (name, "pinned"|"adaptive"), sorted alpha
}
// Default: vec![]
```

### StatusReportJson (extended)

```rust
pub struct StatusReportJson {
    // ...existing fields...
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub category_lifecycle: HashMap<String, String>, // NEW; all categories with lifecycle labels
}
```

---

## Function Signatures

```rust
// CategoryAllowlist — new canonical constructor
pub fn from_categories_with_policy(cats: Vec<String>, adaptive: Vec<String>) -> Self

// CategoryAllowlist — existing, now delegates to above with ["lesson-learned"] default
pub fn from_categories(cats: Vec<String>) -> Self  // no signature change

// CategoryAllowlist — new lifecycle query
pub fn is_adaptive(&self, category: &str) -> bool

// maintenance_tick — new parameter added
async fn maintenance_tick(
    // ...existing 11 params...
    category_allowlist: &Arc<CategoryAllowlist>,
) -> ...

// spawn_background_tick / background_tick_loop — new parameter threaded through
// (same Arc<CategoryAllowlist>, exact position TBD by implementer)
```

---

## Lifecycle Guard Stub (background.rs)

The stub is fully specified. Place after Step 10 (`run_maintenance`), before Step 11
(dead-knowledge migration):

```rust
// --- Lifecycle guard stub (crt-031) — insertion point for #409 auto-deprecation ---
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

Key invariants: (1) lock is dropped before any `.await`; (2) log does not fire when adaptive
set is empty; (3) stub body is a complete no-op — no entries are modified.

---

## Constraints

1. `CategoryAllowlist` is `pub`. `new()` and `from_categories()` signatures are frozen — no
   breaking changes to existing call sites. Only additive constructor and method additions.
2. The `adaptive` `RwLock` must use `.unwrap_or_else(|e| e.into_inner())` poison recovery
   on every read — same pattern as the existing `categories` field throughout.
3. `StatusReport` has a `Default` impl used in `maintenance_tick` (line ~816). The new
   `category_lifecycle` field must default to `vec![]`.
4. `spawn_background_tick` already has `#[allow(clippy::too_many_arguments)]`. The 23rd
   parameter is explicitly justified citing crt-031 and #409; the `BackgroundTickConfig`
   composite refactor is deferred.
5. `categories.rs` must be split to `infra/categories/mod.rs` + `infra/categories/lifecycle.rs`
   before adding new code. The `mod.rs` must re-export `CategoryAllowlist` and
   `INITIAL_CATEGORIES` so all existing import paths resolve without changes to downstream files.
6. No database schema changes. No MCP tool surface changes. `context_status` output changes
   are additive only.
7. `add_category(&self, category: String)` behavior is unchanged — runtime-added categories
   are always pinned (the `adaptive` set is frozen post-construction).
8. `is_adaptive` MUST NOT be called on MCP request hot paths (search, lookup, briefing).
   Call sites are limited to `maintenance_tick` (once per tick) and `compute_report` (once per
   `context_status` invocation).

---

## Critical Test Construction Invariant (R-01 / SR-03)

Both `adaptive_categories` and `boosted_categories` default to `["lesson-learned"]`. Any
`validate_config` test that sets `categories` to a non-default value must explicitly zero
**both** parallel lists to avoid cross-check false failures:

```rust
KnowledgeConfig {
    categories: vec!["custom-cat".into()],
    boosted_categories: vec![],     // suppress boosted cross-check
    adaptive_categories: vec![],    // suppress adaptive cross-check
    freshness_half_life_hours: None,
}
```

Existing test helpers (`config_with_categories`, `config_with_half_life`, etc.) that already
set `boosted_categories: vec![]` must also add `adaptive_categories: vec![]`.

---

## Dependencies

### Crates (no new dependencies)

All changes are within `unimatrix-server`. No new crate or workspace dependencies required.

| Dependency | Usage |
|------------|-------|
| `std::collections::HashSet` | adaptive set storage — existing |
| `std::sync::RwLock` | second lock field — existing |
| `serde` | `#[serde(default)]` on new field — existing |
| `tracing` | `tracing::debug!` in guard stub — existing |

### Existing Components Touched

| Component | Location | Change |
|-----------|----------|--------|
| `CategoryAllowlist` | `infra/categories.rs` → `infra/categories/lifecycle.rs` | Module split + new field + new method |
| `KnowledgeConfig` | `infra/config.rs` | New `adaptive_categories` field |
| `validate_config` | `infra/config.rs` | New cross-check block |
| `merge_configs` | `infra/config.rs` | New `adaptive_categories` merge line (FR-17) |
| `ConfigError` | `infra/config.rs` | New `AdaptiveCategoryNotInAllowlist` variant |
| `StatusReport` | `mcp/response/status.rs` | New `category_lifecycle` field |
| `StatusReportJson` | `mcp/response/status.rs` | New `category_lifecycle` HashMap field |
| `format_status_report` | `mcp/response/status.rs` | Summary + JSON format paths updated |
| `StatusService::compute_report` | `services/status.rs` | Populate new field; wire `Arc<CategoryAllowlist>` if needed |
| `maintenance_tick` | `background.rs` | New parameter + lifecycle guard stub |
| `spawn_background_tick` | `background.rs` | New `Arc<CategoryAllowlist>` parameter |
| `background_tick_loop` | `background.rs` | New `Arc<CategoryAllowlist>` parameter |
| `main.rs` | `src/main.rs` | Two call sites updated to `from_categories_with_policy` |

### #409 Dependency Contract

crt-031 guarantees that #409 can rely on the following stable interface:

1. `CategoryAllowlist::is_adaptive(&self, category: &str) -> bool` — signature is frozen.
2. Lifecycle guard stub exists in `maintenance_tick` between Step 10 and dead-knowledge
   migration. #409 replaces the stub body only.
3. `Arc<CategoryAllowlist>` is wired into `maintenance_tick` as a parameter.
4. `KnowledgeConfig::adaptive_categories: Vec<String>` is serde-deserialized at startup.
5. #409 MUST NOT add decay schedules or signal mechanics to `CategoryAllowlist` — those belong
   in a separate service layer.

---

## NOT in Scope

- Entry auto-deprecation logic — #409's responsibility.
- PPR weighting, co-access scoring, or any search/ranking signal changes.
- Wiring lifecycle policy to the existing effectiveness-based auto-quarantine path.
- Database schema changes or migration files.
- Runtime MCP tool for changing lifecycle policy.
- Decay schedules, score thresholds, or signal mechanics.
- `adaptive_categories` support in `DomainPackConfig` (domain pack categories always default
  to pinned via `add_category`).
- Changes to any `INITIAL_CATEGORIES` entries or their set membership.
- `BackgroundTickConfig` composite struct refactor (deferred, SR-02 / OQ-05).

---

## Test Count Estimate

Estimated 20–28 new unit tests distributed across four modules:

| Module | Estimated Tests | Key Coverage |
|--------|----------------|--------------|
| `infra/categories/lifecycle.rs` | ~12 | `is_adaptive` default/custom, `from_categories_with_policy`, poison recovery on adaptive lock, `add_category` pinned default, constructor equivalence (AC-13), wiring (AC-17) |
| `infra/config.rs` | ~8 | `AdaptiveCategoryNotInAllowlist` validation, empty list, multi-value, default deserialization, merge behavior (R-07), cross-check isolation (R-01 / AC-16) |
| `mcp/response/status.rs` | ~5 | `category_lifecycle` field in summary and JSON, empty adaptive suppression, golden-output alphabetic sort, asymmetry documentation |
| `background.rs` | ~3 | Guard stub invocation, debug log gate (AC-10), wiring test (AC-17) |

Gate 3b requires at least one passing test in each of the four modules above before sign-off
(R-10 mitigation). The exact count is non-binding; all tests must pass.

---

## Pre-Coding Verification Steps

Before writing code, the implementer must confirm:

1. **R-02**: Read `StatusService::new()` to determine whether `Arc<CategoryAllowlist>` is
   already a field. If not, add it as a field and update all construction call sites.
2. **R-04**: Verify no file outside `infra/categories/` imports below
   `crate::infra::categories` — only the top-level `CategoryAllowlist` and
   `INITIAL_CATEGORIES` symbols need re-exporting from `mod.rs`.
3. **I-01**: Confirm both `from_categories` call sites in `main.rs` are found (approximately
   lines 550 and 940). Both must be updated.

---

## Alignment Status

**Overall: PASS with one accepted WARN.**

| Finding | Status | Detail |
|---------|--------|--------|
| Vision alignment | PASS | Advances W0-3 domain-agnosticism; `adaptive_categories` is operator-configurable, not hardcoded |
| Milestone fit | PASS | Correct Cortical phase prerequisite; Wave 1A (#409) insertion point established without over-building |
| Scope coverage | PASS | All 5 SCOPE goals covered; all 15 original ACs plus 2 defensive additions (AC-16, AC-17) |
| WARN-01: FR-17 `merge_configs` | ACCEPTED | Not in SCOPE.md but necessary to prevent silent FM-04 config drop; follows `boosted_categories` pattern; zero product-direction risk |
| Architecture consistency | PASS | All SCOPE open questions resolved; ADR-001 locks all three constructor/status/domain-pack decisions |
| Risk completeness | PASS | 10 risks registered; all 6 scope risks traced; SR-03 R-01 elevated to Critical; security and failure modes covered |
