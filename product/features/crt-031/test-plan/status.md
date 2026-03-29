# Test Plan: services/status.rs + mcp/response/status.rs

Component from IMPLEMENTATION-BRIEF.md §Component Map rows 4 (StatusService) and shared
StatusReport.

---

## Risks Addressed

- **R-02** (Critical): All 4 `StatusService::new()` construction sites must receive
  `Arc<CategoryAllowlist>` — two test helpers in `services/status.rs` are compile-time
  catches; `run_single_tick` in `background.rs` is a silent failure risk.
- **R-08** (Medium): `category_lifecycle` Vec must be sorted alphabetically before storing.
- **R-10** (High): Test modules for these files must exist before gate 3b.
- **I-02** (Medium): `StatusReport::default()` must include `category_lifecycle: vec![]`.
- **I-03** (Medium): JSON comparison must use deserialized value, not raw string equality.

---

## Pre-Implementation Step: StatusService::new() Site Enumeration (R-02)

Before writing any code, run:

```bash
grep -rn "StatusService::new" crates/
```

Expected output: exactly 4 hits:
1. `services/mod.rs` — `ServiceLayer::new()` call site
2. `background.rs` — `run_single_tick` at ~line 446
3. `services/status.rs` — test helper 1 at ~line 1886
4. `services/status.rs` — test helper 2 at ~line 2038

Document all 4 locations before writing any code. The test plan verification step (R-02
scenario 1) is satisfied when this grep output is recorded.

After adding `category_allowlist: Arc<CategoryAllowlist>` to `StatusService::new()`:
- Sites 3 and 4 (test helpers) will fail to compile — compile-time catch.
- Site 2 (`run_single_tick`) will compile if `CategoryAllowlist::new()` is inserted inline —
  that is the silent failure. Test scenario R-02/4 guards against it.

---

## Unit Test Expectations: mcp/response/status.rs (StatusReport)

### StatusReport Default Impl (I-02)

**`test_status_report_default_category_lifecycle_is_empty`** (I-02)
- Assert: `StatusReport::default().category_lifecycle == vec![]`
- This verifies the Default impl includes the new field

### StatusReport Alphabetic Sorting (R-08, FR-11)

**`test_category_lifecycle_sorted_alphabetically`** (R-08 scenario 1)
- Arrange: populate `category_lifecycle` by building a `StatusService` with an allowlist
  containing 3 categories in non-alphabetical input order
- Assert: `report.category_lifecycle` is sorted by category name (field 0 of each tuple)
- Assertion form:
  ```rust
  for i in 1..report.category_lifecycle.len() {
      assert!(report.category_lifecycle[i].0 >= report.category_lifecycle[i-1].0,
              "category_lifecycle must be sorted alphabetically");
  }
  ```

**`test_category_lifecycle_labels_correct`**
- Arrange: `StatusService` with an allowlist where `lesson-learned` is adaptive, others pinned
- Assert: the tuple for `lesson-learned` is `("lesson-learned", "adaptive")`
- Assert: the tuple for `decision` is `("decision", "pinned")`

### StatusReport JSON Output (R-08, I-03, AC-09)

**`test_category_lifecycle_json_sorted`** (R-08 scenario 2)
- Arrange: same setup as sorting test above
- Assert: deserialize the JSON output with `serde_json::from_str`, NOT raw string comparison
- Assert: `category_lifecycle` entries appear in alphabetical order in the deserialized value

**`test_status_report_json_includes_all_categories`** (AC-09 JSON path)
- Arrange: `StatusService` with 5 default categories, `lesson-learned` adaptive
- Act: call `compute_report()`, format as JSON
- Assert: JSON contains all 5 categories, each with a lifecycle label
- Assert: `lesson-learned` labeled `"adaptive"`, all others labeled `"pinned"`

### StatusReport Summary Text (AC-09 summary path)

**`test_status_report_summary_lists_only_adaptive`** (AC-09 summary path)
- Arrange: `StatusService` with `lesson-learned` adaptive, `decision` and `convention` pinned
- Act: format as summary text
- Assert: summary text contains `"lesson-learned"` in the lifecycle section
- Assert: summary text does NOT contain `"decision"` or `"convention"` in the lifecycle section
- Note: the summary shows adaptive categories only (SR-04 intentional asymmetry)

**`test_status_report_summary_no_adaptive_section_when_empty`**
- Arrange: `StatusService` with no adaptive categories (`adaptive_categories: vec![]`)
- Act: format as summary text
- Assert: summary text does NOT contain an "Adaptive categories" line
- Covers: E-01 (empty adaptive list — summary section absent)

---

## Unit Test Expectations: services/status.rs (StatusService)

### compute_report() populates category_lifecycle (AC-09, R-02)

**`test_status_service_compute_report_has_lifecycle`** (R-02 scenario 3, AC-09)
- Arrange: construct `StatusService::new(...)` with a `CategoryAllowlist` that has
  `lesson-learned` adaptive and 4 others pinned
- Act: `service.compute_report()` (or the relevant async wrapper)
- Assert: `report.category_lifecycle.len() == 5`
- Assert: the entry for `lesson-learned` has label `"adaptive"`
- Assert: the entry for `decision` has label `"pinned"`

**`test_status_service_compute_report_sorted_lifecycle`**
- Arrange: same as above with categories in shuffled order
- Assert: `category_lifecycle` is sorted alphabetically

### Test Helper Updates (R-02 — compile-time catch)

The two test helpers in `services/status.rs` at ~lines 1886 and ~2038 will fail to compile
after the `StatusService::new()` signature change. The implementer must update them.

Both helpers must pass an `Arc<CategoryAllowlist>` that reflects the correct policy for the
helper's context. Acceptable constructions:

```rust
// Test helper construction — uses from_categories_with_policy for clarity
let allowlist = Arc::new(CategoryAllowlist::from_categories_with_policy(
    crate::infra::config::INITIAL_CATEGORIES.iter().map(|s| s.to_string()).collect(),
    vec!["lesson-learned".to_string()],
));
```

Or, if the test helper does not need a specific lifecycle policy:
```rust
let allowlist = Arc::new(CategoryAllowlist::new());
```

Both are valid. What is NOT valid: omitting the parameter and letting the test helper
compile only after removing the field from `StatusService`.

**Verification**: `cargo test -p unimatrix-server -- status` must show > 0 passing tests,
including tests using both helpers.

---

## Assertions Summary

```rust
// Default impl
assert_eq!(StatusReport::default().category_lifecycle, vec![]);

// Sorted
let lifecycle = &report.category_lifecycle;
for i in 1..lifecycle.len() {
    assert!(lifecycle[i].0 >= lifecycle[i-1].0);
}

// Labels
assert_eq!(report.category_lifecycle[idx_lesson_learned].1, "adaptive");
assert_eq!(report.category_lifecycle[idx_decision].1, "pinned");

// JSON: use deserialized comparison
let json_val: serde_json::Value = serde_json::from_str(&json_str).unwrap();
let lifecycle_json = &json_val["category_lifecycle"];
// ... assert on deserialized structure, not raw string
```

---

## Integration Test Expectations

The `test_status_category_lifecycle_field_present` integration test planned in OVERVIEW.md
validates AC-09 through the MCP interface. This test:
- Calls `context_status(format="json")` on a default-config server
- Asserts `category_lifecycle` is present in the response
- Asserts it contains at least 5 entries
- Asserts `lesson-learned` is labeled `"adaptive"` (when default config is used)

Unit tests in this component verify the sorting and labeling logic; the integration test
confirms it reaches the MCP response layer.
