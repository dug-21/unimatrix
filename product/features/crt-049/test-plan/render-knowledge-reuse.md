# Test Plan: render_knowledge_reuse (retrospective.rs)

**File**: `crates/unimatrix-server/src/mcp/response/retrospective.rs`
**Function**: `fn render_knowledge_reuse(reuse: &FeatureKnowledgeReuse, feature_cycle: &str) -> String`
**Test module**: existing `#[cfg(test)] mod tests` block

---

## Risks Covered

| Risk | AC | Priority |
|------|----|----------|
| R-06: render_knowledge_reuse section-order regression | AC-07 | Medium |
| R-05/R-03: Early-return guard | AC-17 [GATE] | High |
| R-13: Fixture update completeness (legacy label) | AC-07 partial | Medium |

---

## Existing Fixture Updates Required

All test fixtures in `retrospective.rs` that construct `FeatureKnowledgeReuse` using the
Rust field name `delivery_count: ...` must be updated to `search_exposure_count: ...`.
These are compile-time catches (R-13). Additionally, any golden-output assertion that
checks for the literal string `"delivery_count"` in serialized JSON must be updated to
`"search_exposure_count"`.

Existing test fixtures that set `total_served: 0` remain valid — `total_served` is still
`u64`, semantics changed but field name did not.

---

## AC-07: Golden-Output Assertion (Five Required Assertions)

**Test: `test_render_knowledge_reuse_golden_output_all_sections`**

This test must cover all five assertions listed in the ACCEPTANCE-MAP for AC-07:

```
Arrange:
  reuse = FeatureKnowledgeReuse {
      search_exposure_count: 10,
      explicit_read_count: 3,
      total_served: 5,
      explicit_read_by_category: HashMap::from([
          ("decision".to_string(), 2u64),
          ("pattern".to_string(), 1u64),
      ]),
      cross_session_count: 0,
      by_category: HashMap::new(),
      category_gaps: vec![],
      total_stored: 20,
      cross_feature_reuse: 0,
      intra_cycle_reuse: 0,
      top_cross_feature_entries: vec![],
  }
  feature_cycle = "crt-049"

Act:
  let rendered = render_knowledge_reuse(&reuse, "crt-049");

Assert (a):
  rendered.contains("Entries served to agents (reads + injections)")
  — AND — the value 5 appears on that line (not 10, not 3)

Assert (b):
  rendered.contains("Search exposures (distinct)")
  — AND — the value 10 appears on or near that label

Assert (c):
  rendered.contains("Explicit reads (distinct)")
  — AND — the value 3 appears on or near that label

Assert (d):
  !rendered.contains("Distinct entries served")
  — legacy label must NOT appear anywhere in the rendered string

Assert (e):
  Section order: "Entries served" line appears BEFORE "Search exposures" and
  "Explicit reads" lines. Verify by checking character position or line order:
    let entries_pos = rendered.find("Entries served to agents").unwrap();
    let search_pos  = rendered.find("Search exposures (distinct)").unwrap();
    let reads_pos   = rendered.find("Explicit reads (distinct)").unwrap();
    assert!(entries_pos < search_pos);
    assert!(entries_pos < reads_pos);
```

**Test: `test_render_knowledge_reuse_explicit_read_categories_section`**
```
Arrange: Same as above (explicit_read_by_category = {"decision": 2, "pattern": 1})
Act:     render_knowledge_reuse(&reuse, "crt-049")
Assert:
  rendered.contains("Explicit read categories")
  rendered.contains("decision")
  rendered.contains("pattern")
```

**Test: `test_render_knowledge_reuse_no_explicit_read_categories_when_empty`**
```
Arrange:
  reuse = FeatureKnowledgeReuse {
      search_exposure_count: 5,
      explicit_read_count: 0,
      total_served: 0,
      explicit_read_by_category: HashMap::new(),
      ...
  }
Assert:
  !rendered.contains("Explicit read categories")
  (section is omitted when the map is empty — matches existing by_category rendering pattern)
```

---

## AC-17 [GATE]: Injection-Only Cycle Does NOT Trigger Early-Return Guard

**Test: `test_render_knowledge_reuse_injection_only_cycle_not_suppressed`**
```
Arrange:
  reuse = FeatureKnowledgeReuse {
      search_exposure_count: 0,    // guard check: search_exposure_count == 0
      explicit_read_count: 0,
      total_served: 3,             // guard check: total_served > 0, so guard must NOT fire
      explicit_read_by_category: HashMap::new(),
      cross_session_count: 0,
      by_category: HashMap::new(),
      category_gaps: vec![],
      total_stored: 10,
      cross_feature_reuse: 0,
      intra_cycle_reuse: 0,
      top_cross_feature_entries: vec![],
  }

Act:
  let rendered = render_knowledge_reuse(&reuse, "crt-049");

Assert:
  !rendered.trim_end().ends_with("No knowledge entries served.")  OR
  rendered.len() > "## Knowledge Reuse\n\nNo knowledge entries served.\n\n".len()
  (i.e., the early-return did NOT fire — output is non-trivial)

  rendered.contains("3")   (total_served value visible in output)
  rendered.contains("Entries served to agents (reads + injections)")
```

This verifies that the guard `total_served == 0 && search_exposure_count == 0` is used
rather than the old form that checked `search_exposure_count == 0` alone. An injection-only
cycle has `total_served = 3` so the guard condition is false — the section renders.

**Test: `test_render_knowledge_reuse_zero_guard_both_zero`**
```
Arrange:
  reuse = FeatureKnowledgeReuse {
      search_exposure_count: 0,
      total_served: 0,
      explicit_read_count: 0,
      ...
  }

Act:
  let rendered = render_knowledge_reuse(&reuse, "crt-049");

Assert:
  rendered.contains("No knowledge entries served.")
  (both conditions are zero — early-return fires correctly)
```

---

## Regression Guard: Legacy Label Absence

**Test: `test_render_knowledge_reuse_no_legacy_distinct_entries_served_label`**
```
Arrange: Any FeatureKnowledgeReuse with non-zero total_served or search_exposure_count
Act:     render_knowledge_reuse(&reuse, "crt-049")
Assert:  !rendered.contains("Distinct entries served")
         (legacy label from the pre-crt-049 renderer must not appear in any output)
```

---

## Search Exposure by_category Relabeling (R-06 Adjacent)

The spec (FR-09) and architecture mention `by_category` is "relabeled" in rendering.
The existing label in `render_knowledge_reuse` reads "By category (all N served)" — this
may need updating to "Search exposure categories" or similar. The exact label is defined
in the renderer implementation. The test must check that:

1. The existing `by_category` data still renders (regression guard — existing data path).
2. The label does not confusingly say "all N served" while referring only to search
   exposures (since `total_served` now means something different).

This is implementation-detail-level; the golden-output test above covers the primary labels.

---

## Expected Test Count Delta

- 1 updated existing test (`delivery_count` field rename in fixture — compile-time)
- 5 new tests:
  - `test_render_knowledge_reuse_golden_output_all_sections`
  - `test_render_knowledge_reuse_explicit_read_categories_section`
  - `test_render_knowledge_reuse_no_explicit_read_categories_when_empty`
  - `test_render_knowledge_reuse_injection_only_cycle_not_suppressed` (AC-17 [GATE])
  - `test_render_knowledge_reuse_zero_guard_both_zero`
  - `test_render_knowledge_reuse_no_legacy_distinct_entries_served_label`
- Total: +6 new unit tests in `crates/unimatrix-server/src/mcp/response/retrospective.rs`
  test module (one of these may subsume the legacy-label check)
