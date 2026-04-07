# Test Plan: compute_knowledge_reuse (knowledge_reuse.rs)

**File**: `crates/unimatrix-server/src/mcp/knowledge_reuse.rs`
**Function**: `pub fn compute_knowledge_reuse<F, G>(..., explicit_read_ids: &HashSet<u64>, explicit_read_meta: &HashMap<u64, EntryMeta>) -> FeatureKnowledgeReuse`
**Test module**: existing `#[cfg(test)] mod tests` block

---

## Risks Covered

| Risk | AC | Priority |
|------|----|----------|
| R-03: total_served semantics change | AC-14 [GATE], AC-15 [GATE] | High |
| R-05: Early-return guard retains old condition | AC-09, AC-17 (partial) | Medium |
| R-09: explicit_read_by_category contract | AC-13 [GATE] | High |
| R-12: total_served deduplication not applied | AC-14 [GATE], AC-15 [GATE] | Medium |

---

## Existing Test Caller Updates Required

All existing tests in the `knowledge_reuse.rs` test module call `compute_knowledge_reuse`
directly. After the signature extension, every call site must pass two new trailing params:

```rust
// Before (existing tests):
compute_knowledge_reuse(query_logs, injection_logs, active_cats, "cycle", cat_lookup, meta_lookup)

// After (crt-049):
compute_knowledge_reuse(
    query_logs, injection_logs, active_cats, "cycle",
    cat_lookup, meta_lookup,
    &HashSet::new(),           // explicit_read_ids (empty for existing tests)
    &HashMap::new(),           // explicit_read_meta (empty for existing tests)
)
```

Passing empty sets for both new params must NOT change the behavior of existing tests. All
existing assertions (`delivery_count`, `cross_session_count`, `by_category`) must continue
to pass exactly as before (AC-11 regression guard).

Existing tests that currently assert `delivery_count` must be updated to assert
`search_exposure_count` after the field rename (compile-time catch via R-13).

---

## AC-13 [GATE]: explicit_read_by_category Category Map Contract

**Test: `test_compute_knowledge_reuse_explicit_read_by_category_populated`**
```
Arrange:
  explicit_read_ids = HashSet::from([10u64, 11u64, 12u64])
  explicit_read_meta = HashMap::from([
      (10, EntryMeta { category: "decision".to_string(), ... }),
      (11, EntryMeta { category: "decision".to_string(), ... }),
      (12, EntryMeta { category: "pattern".to_string(), ... }),
  ])
  query_logs = []  (search exposure path not exercised here)
  injection_logs = []

Act:
  compute_knowledge_reuse([], [], {}, "cycle", ..., &explicit_read_ids, &explicit_read_meta)

Assert:
  result.explicit_read_by_category == {"decision": 2, "pattern": 1}
  result.explicit_read_count == 3
```

**Test: `test_compute_knowledge_reuse_explicit_read_by_category_empty_when_no_reads`**
```
Arrange: explicit_read_ids = HashSet::new(), explicit_read_meta = HashMap::new()
Assert:  result.explicit_read_by_category.is_empty()
         result.explicit_read_count == 0
```
The field must be an empty map (not absent or null) when there are no explicit reads.
This validates the `#[serde(default)]` contract for Group 10 consumers.

**Test: `test_compute_knowledge_reuse_explicit_read_meta_lookup_skipped_on_empty_ids`**
```
Arrange: explicit_read_ids = HashSet::new()
         explicit_read_meta = HashMap::new()  (empty — correct for empty IDs)
Assert:  result.explicit_read_by_category.is_empty()  (no panic, no error)
```
Validates E-01 / F-02: batch lookup skipped when ID set is empty, no accidental skip guard
on single-entry set (E-04 is the inverse — one ID must still produce a result).

---

## AC-14 [GATE] + AC-15 [GATE]: total_served Redefinition

**Test: `test_compute_knowledge_reuse_total_served_union_of_reads_and_injections`**
```
Arrange:
  explicit_read_ids = HashSet::from([1u64, 2u64])
  injection_log entry IDs = [2, 3]    (entry 2 appears in both)
  query_log entry IDs = [4, 5, 6]    (search exposures — must NOT contribute to total_served)

Act:
  compute_knowledge_reuse with the above inputs

Assert:
  result.total_served == 3            (|{1,2} ∪ {2,3}| = 3, not 4, not 6)
  result.search_exposure_count == 3   (distinct query result entries)
  (or search_exposure_count = the actual count from query_log entries)
```

**Test: `test_compute_knowledge_reuse_total_served_excludes_search_exposures`**
```
Arrange:
  explicit_read_ids = HashSet::new()      (no explicit reads)
  injection_logs = []                      (no injections)
  query_logs with entry IDs [1, 2, 3]     (search exposures only)

Act:
  compute_knowledge_reuse with above inputs

Assert:
  result.total_served == 0
  result.search_exposure_count == 3  (exposures still counted separately)
```
This directly tests AC-15: an entry appearing only in search results does NOT increase
`total_served`.

**Test: `test_compute_knowledge_reuse_total_served_deduplication_overlap`**
```
Arrange:
  explicit_read_ids = HashSet::from([1u64])
  injection_log entry IDs = [1]       (same entry in both — dedup to 1)

Assert:
  result.total_served == 1            (not 2)
```

**Test: `test_compute_knowledge_reuse_total_served_disjoint_sets`**
```
Arrange:
  explicit_read_ids = HashSet::from([1u64, 2u64])
  injection_log entry IDs = [3]

Assert:
  result.total_served == 3
```

---

## AC-09: Early-Return Guard — Zero Search Exposures, Non-Zero Explicit Reads

**Test: `test_compute_knowledge_reuse_no_early_return_for_explicit_read_only_cycle`**
```
Arrange:
  query_logs = []                          (search_exposure_count = 0)
  injection_logs = []                      (injection_count = 0)
  explicit_read_ids = HashSet::from([5u64])
  explicit_read_meta = HashMap::from([(5, EntryMeta { category: "pattern", ... })])

Act:
  compute_knowledge_reuse with above inputs

Assert:
  result.explicit_read_count == 1
  result.explicit_read_by_category == {"pattern": 1}
  result.total_served == 1
  result is NOT the default/empty struct returned by the early-return path
```
This tests that the early-return guard `total_served == 0 && search_exposure_count == 0`
does NOT fire when `total_served > 0`. The old three-condition guard
(`search_exposure_count == 0 && explicit_read_count == 0 && injection_count == 0`)
would also allow this test to pass — the critical difference is verified by AC-17 via
`render_knowledge_reuse` (injection-only cycle).

---

## AC-17 (partial): Injection-Only Cycle — Guard Updated

**Test: `test_compute_knowledge_reuse_injection_only_cycle_has_nonzero_total_served`**
```
Arrange:
  query_logs = []                          (search_exposure_count = 0)
  injection_logs with entry IDs = [7, 8]   (injection_count = 2)
  explicit_read_ids = HashSet::new()       (explicit_read_count = 0)

Act:
  compute_knowledge_reuse with above inputs

Assert:
  result.total_served == 2                 (injection IDs count)
  result.search_exposure_count == 0
  result.explicit_read_count == 0
```
Combined with the AC-17 render test in `retrospective.rs`, this validates that a cycle
with only injection signal has `total_served > 0`, so the render guard does not suppress
the section.

---

## Regression Guard: Existing Tests

All existing tests previously asserting `result.delivery_count` must now assert
`result.search_exposure_count` (compile-time rename catch). The semantic values must be
unchanged — these tests cover R-13 fixture update completeness.

Specifically, tests asserting counts like `delivery_count == 1`, `delivery_count == 3`,
etc. must be updated to `search_exposure_count == 1`, `search_exposure_count == 3`.

---

## Expected Test Count Delta

- 7 new tests (AC-13 ×2, AC-14/AC-15 ×4, AC-09 ×1)
- N existing tests updated for `search_exposure_count` field rename (compile-time catch,
  not new tests)
- Total: +7 new unit tests in `crates/unimatrix-server/src/mcp/knowledge_reuse.rs` test module
