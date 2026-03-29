# Component Pseudocode: main.rs startup wiring

## Purpose

Update the two `CategoryAllowlist` construction call sites to use `from_categories_with_policy`,
passing the operator-configured `adaptive_categories` from `KnowledgeConfig`. Update both
`ServiceLayer::new()` call sites to pass `Arc::clone(&categories)`. Update both
`spawn_background_tick` call sites to pass `Arc::clone(&categories)`.

Six total changes in `main.rs`. No new imports are needed if `CategoryAllowlist` is already imported.

---

## Context: Two Startup Paths

`main.rs` has two startup paths depending on whether a project-level config is found:
1. **Global config path** (~lines 460 + 940): `CategoryAllowlist::from_categories(...)` called twice
2. **Project config path** (~lines 550 + 940): `CategoryAllowlist::from_categories(...)` called twice

The architecture says "both sites" are updated. From the IMPLEMENTATION-BRIEF: there are
two `CategoryAllowlist` construction sites and two `ServiceLayer::new()` sites. The pattern
at each site is identical.

---

## Change 1 + 2: CategoryAllowlist construction sites

### Current pattern (at each of the two sites)
```
let categories = Arc::new(CategoryAllowlist::from_categories(knowledge_categories));
```

### New pattern (at each of the two sites)
```
let adaptive_categories: Vec<String> = config.knowledge.adaptive_categories.clone();
let categories = Arc::new(CategoryAllowlist::from_categories_with_policy(
    knowledge_categories,
    adaptive_categories,
));
```

The `adaptive_categories` local follows the same extraction pattern as `knowledge_categories`
immediately above it. `config.knowledge.adaptive_categories` is the operator-configured value
after `validate_config` has already accepted it (startup validation is fail-fast, so if we
reach this line, the value is valid).

The `Arc::new` wrapper is unchanged. The result is `Arc<CategoryAllowlist>` as before.

---

## Change 3 + 4: ServiceLayer::new() call sites

### Context
`ServiceLayer::new()` gains a new final parameter `category_allowlist: Arc<CategoryAllowlist>`
(see services/mod.rs in the status pseudocode). Both `ServiceLayer::new()` call sites in
`main.rs` must pass `Arc::clone(&categories)`.

### Current pattern (each site)
```
let service_layer = ServiceLayer::new(
    Arc::clone(&store),
    /* ... existing params ... */
    Arc::clone(&observation_registry),
    Arc::clone(&confidence_params),
);
```

### New pattern (each site)
```
let service_layer = ServiceLayer::new(
    Arc::clone(&store),
    /* ... existing params unchanged ... */
    Arc::clone(&observation_registry),
    Arc::clone(&confidence_params),
    Arc::clone(&categories),    // NEW: operator-configured category allowlist with lifecycle policy
);
```

The `categories` Arc is constructed before `service_layer` in both paths (categories is built
from config, ServiceLayer is built from categories among other things). Ordering is safe.

---

## Change 5 + 6: spawn_background_tick call sites

### Context
`spawn_background_tick` gains a new final parameter `category_allowlist: Arc<CategoryAllowlist>`
(see background pseudocode). Both call sites in `main.rs` pass `Arc::clone(&categories)`.

### Current pattern (each site)
```
let tick_handle = spawn_background_tick(
    Arc::clone(&store),
    /* ... 21 existing params ... */
    phase_freq_table.clone(),
);
```

### New pattern (each site)
```
let tick_handle = spawn_background_tick(
    Arc::clone(&store),
    /* ... 21 existing params unchanged ... */
    phase_freq_table.clone(),
    Arc::clone(&categories),    // NEW: category_allowlist for lifecycle guard stub (crt-031)
);
```

---

## Ordering Constraint

At each startup path, the sequence must be:
1. Config loaded and validated (`validate_config` has run)
2. `knowledge_categories` extracted from `config.knowledge.categories`
3. `adaptive_categories` extracted from `config.knowledge.adaptive_categories`  ← NEW
4. `categories` Arc constructed via `from_categories_with_policy`  ← CHANGED
5. `service_layer` constructed (passes `Arc::clone(&categories)`)  ← UPDATED
6. `tick_handle` created (passes `Arc::clone(&categories)`)  ← UPDATED

This ordering already holds in both startup paths (service_layer and tick_handle are built
after categories). No reordering needed.

---

## Tests

### Integration: startup paths compile and wire correctly
These are verified by the compile test (R-04) and the wiring test (R-02). The primary
test signal is: `cargo check -p unimatrix-server` passes after all six changes are applied.

### Wiring assertion (from R-02 test scenarios)
```
test_startup_wiring_categories_arc_is_operator_configured:
  // Integration test: construct a CategoryAllowlist with a known policy,
  // pass it through ServiceLayer, extract StatusService, call compute_report().
  // Assert category_lifecycle has the expected labels.
  //
  // This is a compile-time-catch test (StatusService::new signature change forces update)
  // plus a runtime assertion on the output.
  //
  // Location: services/status.rs test helpers (test sites 3 and 4)
  // These test helpers must pass Arc::clone of a known allowlist.
```

### Pre-implementation verification
Before implementing, run:
```bash
grep -rn "StatusService::new" crates/
```
Expected output: exactly 4 sites.
- services/mod.rs (ServiceLayer construction)
- background.rs (~line 446 in run_single_tick)
- services/status.rs (~line 1886, test helper 1)
- services/status.rs (~line 2038, test helper 2)

Document grep output in PR description (AC-26).

---

## Error Handling

No error handling changes in `main.rs` for this component. `from_categories_with_policy`
is infallible. `validate_config` has already run before this code executes.

---

## Key Constraint

Both `CategoryAllowlist` construction sites must be updated. There is no compile error if one
site is missed — `from_categories` still compiles. The consequence is that one startup path
silently uses `["lesson-learned"]` as the adaptive policy regardless of operator config (I-01).
The pre-implementation grep for both sites is mandatory.
