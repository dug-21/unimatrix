# Component Pseudocode: services/status + mcp/response/status

## Purpose

Wire `Arc<CategoryAllowlist>` as a new field on `StatusService`, updating all four
`StatusService::new()` construction sites. Add `category_lifecycle: Vec<(String, String)>`
to `StatusReport`, populate it in `compute_report()`, and update both formatters
(summary and JSON) with the intentional asymmetry documented in ADR-001 decision 2.

This pseudocode covers two files:
- `services/status.rs` — `StatusService` struct + `StatusService::new()` + `compute_report()`
- `mcp/response/status.rs` — `StatusReport` struct + `Default` + `format_status_report()`

It also covers `services/mod.rs` — `ServiceLayer::new()` signature extension.

---

## File: `services/status.rs`

### Modified: `StatusService` struct

#### Current fields (relevant excerpt)
```
pub(crate) struct StatusService {
    store: Arc<Store>,
    vector_index: Arc<VectorIndex>,
    embed_service: Arc<EmbedServiceHandle>,
    adapt_service: Arc<AdaptationService>,
    confidence_state: ConfidenceStateHandle,
    confidence_params: Arc<ConfidenceParams>,
    contradiction_cache: ContradictionScanCacheHandle,
    rayon_pool: Arc<RayonPool>,
    observation_registry: Arc<DomainPackRegistry>,
}
```

#### New field added (after observation_registry)
```
    /// crt-031: operator-configured lifecycle policy for per-category adaptive/pinned labeling.
    ///
    /// Threaded from startup wiring via ServiceLayer::new() and run_single_tick.
    /// All four StatusService::new() construction sites must supply the operator-loaded Arc
    /// (never a freshly constructed CategoryAllowlist::new() which ignores operator config).
    category_allowlist: Arc<CategoryAllowlist>,
```

Import addition at the top of `status.rs`:
```
use crate::infra::categories::CategoryAllowlist;
```

### Modified: `StatusService::new()` — all four construction sites

#### Signature change
```
pub(crate) fn new(
    store: Arc<Store>,
    vector_index: Arc<VectorIndex>,
    embed_service: Arc<EmbedServiceHandle>,
    adapt_service: Arc<AdaptationService>,
    confidence_state: ConfidenceStateHandle,
    confidence_params: Arc<ConfidenceParams>,
    contradiction_cache: ContradictionScanCacheHandle,
    rayon_pool: Arc<RayonPool>,
    observation_registry: Arc<DomainPackRegistry>,
    category_allowlist: Arc<CategoryAllowlist>,    // NEW final parameter
) -> Self {
    StatusService {
        store,
        vector_index,
        embed_service,
        adapt_service,
        confidence_state,
        confidence_params,
        contradiction_cache,
        rayon_pool,
        observation_registry,
        category_allowlist,    // NEW
    }
}
```

#### Site 1: `ServiceLayer::new()` in `services/mod.rs`
This site is updated as part of the ServiceLayer change (see below). Passes the `category_allowlist`
parameter that ServiceLayer::new() receives.

#### Site 2: `run_single_tick` in `background.rs` (~line 446)
See background pseudocode for full context. This site must pass `Arc::clone(category_allowlist)`
where `category_allowlist` is the `&Arc<CategoryAllowlist>` parameter threaded to `run_single_tick`.
Must NOT construct `CategoryAllowlist::new()` inline (I-04 / FM-06 / R-02 critical risk).

#### Site 3 + 4: Test helpers in `services/status.rs` (~lines 1886 and 2038)
Both test helpers that call `StatusService::new()` will fail to compile after the signature
change (compile-time catch). They must be updated to pass an `Arc<CategoryAllowlist>`.

The correct value for test helpers is `Arc::new(CategoryAllowlist::new())` — the default
`new()` constructor gives `["lesson-learned"]` as the adaptive default, which is correct
for all test scenarios that do not specifically test lifecycle policy. If a test helper
specifically tests lifecycle, it may construct `Arc::new(CategoryAllowlist::from_categories_with_policy(...))`.

```
// Test helper update pattern (apply to both ~line 1886 and ~line 2038):

fn build_status_service_for_test(
    store: Arc<Store>,
    /* ... existing params ... */
) -> StatusService {
    StatusService::new(
        store,
        /* ... existing args unchanged ... */
        Arc::new(CategoryAllowlist::new()),    // NEW: default lifecycle policy for tests
    )
}
```

### Modified: `StatusService::compute_report()`

#### Insertion Point
`compute_report()` builds the `StatusReport` incrementally. The `category_lifecycle` field
must be populated before the report is returned. The most natural insertion point is after the
`category_distribution` block (which also reads category data) and before the report is finalized.

#### Pseudocode for population
```
// Inside compute_report(), after category_distribution is populated:

// --- crt-031: populate category_lifecycle ---
// Call list_categories() once to get all categories (sorted alphabetically — list_categories
// already returns sorted output).
let all_categories: Vec<String> = self.category_allowlist.list_categories();

// Tag each category with its lifecycle label.
// is_adaptive() reads only the adaptive lock — no contention on categories lock.
let mut lifecycle: Vec<(String, String)> = all_categories
    .into_iter()
    .map(|cat| {
        let label = if self.category_allowlist.is_adaptive(&cat) {
            "adaptive".to_string()
        } else {
            "pinned".to_string()
        };
        (cat, label)
    })
    .collect();

// Alphabetical sort by category name (R-08: list_categories already returns sorted,
// but sort here defensively since we iterate in order).
// If list_categories returns sorted, this is a no-op — O(n) confirmation pass.
lifecycle.sort_by(|a, b| a.0.cmp(&b.0));

// Assign to report.
report.category_lifecycle = lifecycle;
// --- end crt-031 ---
```

Note: `list_categories()` already returns a sorted `Vec<String>` (see categories.rs line 73–75).
The `.map()` preserves order. The final `.sort_by` is a defensive guard against any future change
to `list_categories()` ordering. Cost: O(n log n) with n = 5–64 categories (negligible).

#### Output format asymmetry comment
```
// Note (crt-031, ADR-001 decision 2): category_lifecycle contains all categories.
// The formatter in mcp/response/status.rs uses this vec differently per format:
//   Summary: lists only adaptive categories (pinned is the silent default — avoids noise).
//   JSON:    lists all categories with their lifecycle label.
// This asymmetry is intentional and locked by golden-output tests (AC-09).
```

---

## File: `services/mod.rs`

### Modified: `ServiceLayer::new()` signature

#### Current signature (relevant excerpt)
```
pub fn new(
    store: Arc<Store>,
    /* ... many params ... */
    observation_registry: Arc<DomainPackRegistry>,
    confidence_params: Arc<ConfidenceParams>,
) -> Self {
    Self::with_rate_config(
        /* ... */
        observation_registry,
        confidence_params,
    )
}
```

#### New signature
```
pub fn new(
    store: Arc<Store>,
    /* ... existing params unchanged ... */
    observation_registry: Arc<DomainPackRegistry>,
    confidence_params: Arc<ConfidenceParams>,
    category_allowlist: Arc<CategoryAllowlist>,    // NEW
) -> Self {
    Self::with_rate_config(
        /* ... existing args unchanged ... */
        observation_registry,
        confidence_params,
        category_allowlist,    // NEW: forwarded to StatusService::new()
    )
}
```

`with_rate_config` also gains the parameter and forwards it to `StatusService::new()`.

Import addition to `services/mod.rs`:
```
use crate::infra::categories::CategoryAllowlist;
```

---

## File: `mcp/response/status.rs`

### Modified: `StatusReport` struct — new field

```
// Add to StatusReport struct after the `effectiveness` field:

/// Per-category lifecycle label (crt-031).
///
/// Populated by compute_report() via category_allowlist.list_categories() + is_adaptive().
/// Sorted alphabetically by category name before storing (R-08: deterministic golden tests).
/// Empty vec when StatusReport is constructed via Default (e.g. maintenance_tick thin shell).
///
/// Output format asymmetry (ADR-001 decision 2):
/// - Summary formatter: lists only adaptive categories (pinned is the silent default)
/// - JSON formatter: includes all categories with their lifecycle label
pub category_lifecycle: Vec<(String, String)>,   // (category_name, "adaptive" | "pinned")
```

### Modified: `Default for StatusReport` — new field initialization

```
// In the existing Default impl, add to the struct literal:
category_lifecycle: Vec::new(),    // empty vec — no lifecycle data until compute_report runs
```

### Modified: `format_status_report` — Summary formatter

#### Insertion Point
After the `effectiveness` section, before `CallToolResult::success`.

#### New section in summary
```
// crt-031: lifecycle summary — show only adaptive categories (pinned is the silent default).
// If no adaptive categories are configured, this block is silent (E-01: empty adaptive list).
let adaptive_categories: Vec<&str> = report.category_lifecycle
    .iter()
    .filter(|(_, label)| label == "adaptive")
    .map(|(cat, _)| cat.as_str())
    .collect();

if !adaptive_categories.is_empty() {
    text.push_str(&format!(
        "\nAdaptive categories: {}",
        adaptive_categories.join(", ")
    ));
}
// Note: when adaptive_categories is empty, no line is added (operator disabled adaptive management).
// Rationale: showing all pinned categories adds noise for standard configurations.
```

### Modified: `format_status_report` — JSON formatter

#### Context
The JSON formatter uses a `HashMap`-based or struct-based serialization. The `category_lifecycle`
field goes into the JSON output with all categories labeled (not just adaptive).

#### New JSON field approach
The JSON formatter constructs a map or serializable object. The `category_lifecycle` Vec should
appear as a JSON object mapping category name to lifecycle label, not as a raw Vec of tuples
(which serializes to `[[name, label], ...]` which is hard to consume).

```
// In the JSON formatter section, build a serde_json::Value for category_lifecycle:
let lifecycle_map: serde_json::Map<String, serde_json::Value> = report.category_lifecycle
    .iter()
    .map(|(cat, label)| (cat.clone(), serde_json::Value::String(label.clone())))
    .collect();

// Insert into the JSON object:
// "category_lifecycle": { "lesson-learned": "adaptive", "decision": "pinned", ... }
```

Note: The `category_lifecycle` Vec is already sorted alphabetically (R-08). The `serde_json::Map`
preserves insertion order in serde_json v1.x (BTreeMap-backed). This ensures deterministic JSON
output for golden tests.

If the JSON formatter uses a `#[derive(Serialize)]` struct rather than manual construction,
add `category_lifecycle` as a `HashMap<String, String>` field with a custom serializer or
convert to `BTreeMap<String, String>` before serialization to ensure deterministic order.

Preferred approach (matches existing formatter style): build a `serde_json::Value::Object`
for the category_lifecycle entry. Implementation agent should follow whatever serialization
approach is already used in the JSON path for `category_distribution`.

---

## Error Handling

`compute_report()` is `async fn ... -> Result<StatusReport, ServiceError>`. The new
`category_lifecycle` population uses `list_categories()` (infallible) and `is_adaptive()`
(infallible). No new error propagation is added.

`format_status_report` is infallible — returns `CallToolResult` directly. No error handling.

---

## Key Test Scenarios

### AC-09: context_status lifecycle output
```
test_status_report_category_lifecycle_populated:
  // Construct StatusService with a known CategoryAllowlist
  allowlist = Arc::new(CategoryAllowlist::from_categories_with_policy(
      INITIAL_CATEGORIES...,
      vec!["lesson-learned"],
  ))
  // Call compute_report (or build StatusReport directly for unit test)
  report = StatusReport { category_lifecycle: vec![
      ("convention".to_string(), "pinned".to_string()),
      ("decision".to_string(), "pinned".to_string()),
      ("lesson-learned".to_string(), "adaptive".to_string()),
      ("pattern".to_string(), "pinned".to_string()),
      ("procedure".to_string(), "pinned".to_string()),
  ], ..Default::default() }

  summary = format_status_report(&report, ResponseFormat::Summary)
  // Summary MUST contain "Adaptive categories: lesson-learned"
  // Summary MUST NOT contain "pinned"

  json = format_status_report(&report, ResponseFormat::Json)
  // JSON MUST contain both "adaptive" and "pinned" labels for all 5 categories
```

### Summary: adaptive-only output
```
test_summary_shows_only_adaptive_categories:
  report = StatusReport {
      category_lifecycle: vec![
          ("lesson-learned".to_string(), "adaptive".to_string()),
          ("decision".to_string(), "pinned".to_string()),
      ],
      ..Default::default()
  }
  text = format_status_report_summary(&report)
  assert text.contains("Adaptive categories: lesson-learned")
  assert !text.contains("decision")     // pinned categories not shown in summary
  assert !text.contains("pinned")
```

### Summary: empty adaptive list is silent (E-01)
```
test_summary_silent_when_no_adaptive_categories:
  report = StatusReport {
      category_lifecycle: vec![
          ("decision".to_string(), "pinned".to_string()),
          ("lesson-learned".to_string(), "pinned".to_string()),
      ],
      ..Default::default()
  }
  text = format_status_report_summary(&report)
  assert !text.contains("Adaptive categories")
  assert !text.contains("adaptive")
```

### R-08: category_lifecycle sorted alphabetically
```
test_category_lifecycle_is_sorted:
  // Build report via compute_report() or populate manually with unsorted input
  // Assert report.category_lifecycle is sorted by category name
  for i in 1..lifecycle.len() {
      assert lifecycle[i].0 >= lifecycle[i-1].0
  }
```

### I-02: Default impl produces empty vec
```
test_status_report_default_category_lifecycle_empty:
  report = StatusReport::default()
  assert report.category_lifecycle.is_empty()
```

### I-03: JSON output deterministic (deserialized comparison)
```
test_json_category_lifecycle_deterministic:
  report = // same report constructed twice with identical input
  json1 = format_status_report(&report, ResponseFormat::Json)
  json2 = format_status_report(&report, ResponseFormat::Json)
  // Compare as parsed serde_json::Value, not as raw string
  parsed1: serde_json::Value = serde_json::from_str(json1.text()).unwrap()
  parsed2: serde_json::Value = serde_json::from_str(json2.text()).unwrap()
  assert parsed1 == parsed2
```

### E-02: all categories adaptive
```
test_all_categories_adaptive:
  allowlist = CategoryAllowlist::from_categories_with_policy(
      INITIAL_CATEGORIES...,
      INITIAL_CATEGORIES...,  // all adaptive
  )
  // All 5 labels should be "adaptive"
  for (_, label) in &lifecycle {
      assert_eq!(label, "adaptive")
  }
```
