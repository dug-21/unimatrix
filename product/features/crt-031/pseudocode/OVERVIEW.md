# crt-031 Pseudocode Overview: Category Lifecycle Policy + boosted_categories De-hardcoding

## Components Involved

| Component | File(s) | Change Type |
|-----------|---------|-------------|
| `infra/categories` | `categories.rs` → `infra/categories/mod.rs` + `lifecycle.rs` | Module split + new field + new methods |
| `infra/config` | `infra/config.rs` | New field, Default change, serde fns, new ConfigError variant, validate/merge, public helper |
| `main.rs` | `main.rs` | 2 CategoryAllowlist sites + 2 ServiceLayer sites + 2 spawn_background_tick sites |
| `services/status` | `services/status.rs` + `services/mod.rs` | New field on StatusService, all 4 construction sites, compute_report |
| `mcp/response/status` | `mcp/response/status.rs` | New StatusReport field, Default, summary formatter, JSON formatter |
| `background` | `background.rs` | 3 function signatures + Step 10b stub + run_single_tick StatusService::new |
| `eval-layer` | `eval/profile/layer.rs` + 6 test infrastructure sites | Literal removal (7 sites total) |

---

## Data Flow

```
config.toml (operator)
    │
    ▼ toml deserialization
KnowledgeConfig
  .categories          → CategoryAllowlist::from_categories_with_policy(cats, adaptive)
  .adaptive_categories ─────────────────────────────────────────────────────────────┘
  .boosted_categories  → main.rs: boosted_set: HashSet<String>
    │
    ▼ main.rs startup
Arc<CategoryAllowlist>
  ├── Arc::clone → ServiceLayer::new(…, category_allowlist)
  │                   └── StatusService::new(…, category_allowlist)
  │                         └── StatusService.category_allowlist field
  │                               └── compute_report()
  │                                     └── category_lifecycle: Vec<(String, String)>
  │                                           ├── summary: adaptive categories only
  │                                           └── JSON: all categories with label
  │
  └── Arc::clone → spawn_background_tick(…, category_allowlist)
                       └── background_tick_loop(…, category_allowlist)
                             └── run_single_tick(…, category_allowlist)
                                   ├── StatusService::new(…, Arc::clone(category_allowlist))
                                   └── maintenance_tick(…, category_allowlist)
                                         ├── Step 10: run_maintenance (unchanged)
                                         ├── Step 10b: lifecycle guard stub  ← NEW
                                         │     list_adaptive() → if non-empty → tracing::debug!
                                         │     // TODO(#409): auto-deprecation body
                                         └── Step 11: dead_knowledge_migration (unchanged)
```

---

## Shared Types Introduced or Modified

### New: `CategoryAllowlist.adaptive` field
```
RwLock<HashSet<String>>  -- second independent lock for lifecycle policy set
```
Added alongside the existing `categories: RwLock<HashSet<String>>`.

### New: `KnowledgeConfig.adaptive_categories`
```
Vec<String>
  serde default: vec!["lesson-learned"]
  Default impl: vec![]
```

### Changed: `KnowledgeConfig.boosted_categories` Default
```
Default impl: vec![]  (was vec!["lesson-learned"])
serde default fn: unchanged — still returns vec!["lesson-learned"]
```

### New: `ConfigError::AdaptiveCategoryNotInAllowlist`
```
{ path: PathBuf, category: String }
Display: "config error in {path}: [knowledge] adaptive_categories contains {category:?}
          which is not present in the categories list; add it to [knowledge] categories first"
```

### New: `default_boosted_categories_set() -> HashSet<String>`
```
pub fn in infra/config.rs
Returns: HashSet containing "lesson-learned"
Used by: 6 literal-replacement sites in test infrastructure
```

### New: `StatusReport.category_lifecycle`
```
Vec<(String, String)>  -- (category_name, "adaptive" | "pinned")
Default: vec![]
Sorted alphabetically by category name before storing
```

### Modified: `StatusService` struct
```
new field: category_allowlist: Arc<CategoryAllowlist>
```

### Modified: `StatusService::new()` signature
```
added final param: category_allowlist: Arc<CategoryAllowlist>
```

### Modified: `ServiceLayer::new()` signature
```
added param (position after observation_registry): category_allowlist: Arc<CategoryAllowlist>
```

### Modified: background function signatures
```
maintenance_tick:       gains category_allowlist: Arc<CategoryAllowlist>  (param 12 of 12)
background_tick_loop:   gains category_allowlist: Arc<CategoryAllowlist>  (param 23 of 23)
spawn_background_tick:  gains category_allowlist: Arc<CategoryAllowlist>  (param 23 of 23)
run_single_tick:        gains category_allowlist: &Arc<CategoryAllowlist>  (ref, final param)
```

---

## Build Wave Dependencies (Implementation Order)

Wave 1 — Config foundation (no dependents yet):
- `infra/categories` module split + new field + new methods
- `infra/config` KnowledgeConfig changes + ConfigError + validate + merge + helper

Wave 2 — Wiring (depends on Wave 1):
- `main.rs` — uses `from_categories_with_policy` and `config.knowledge.adaptive_categories`
- `services/mod.rs` — `ServiceLayer::new` gains parameter
- `services/status.rs` — `StatusService` field + all 4 construction sites + `compute_report`
- `mcp/response/status.rs` — `StatusReport` field + formatters

Wave 3 — Background + Literal removal (depends on Wave 1 + 2):
- `background.rs` — 3 function signatures + Step 10b stub + `run_single_tick` StatusService::new
- `eval/profile/layer.rs` + 6 test infrastructure sites — literal removal

Wave 4 — Test updates (depends on all above):
- `main_tests.rs` rewrite
- `config.rs` fixture workaround removal
- New tests for AC-05 through AC-18, AC-24 through AC-27

---

## Key Constraints (Traced from Architecture + ADR-001)

1. `CategoryAllowlist::new()` and `from_categories()` signatures are frozen — no callers change.
2. Public import path `crate::infra::categories::CategoryAllowlist` unchanged through module split.
3. Both RwLock fields use `.unwrap_or_else(|e| e.into_inner())` poison recovery — no exceptions.
4. `category_lifecycle` Vec sorted alphabetically before storing (R-08 — non-deterministic HashSet iteration).
5. `StatusReport::default()` returns `category_lifecycle: vec![]`.
6. `run_single_tick` must pass the operator-loaded Arc, never construct `CategoryAllowlist::new()` inline.
7. `maintenance_tick` Step 10b stub: calls `list_adaptive()` once (not per-category `is_adaptive()` — R-06).
8. `lifecycle.rs` is committed but initially minimal — reserved stub only.
9. `#[allow(clippy::too_many_arguments)]` already present on all three background functions.
