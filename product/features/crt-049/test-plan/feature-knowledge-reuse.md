# Test Plan: FeatureKnowledgeReuse (types.rs)

**File**: `crates/unimatrix-observe/src/types.rs`
**Test module**: existing `#[cfg(test)] mod tests` block

---

## Risks Covered

| Risk | AC | Priority |
|------|----|----------|
| R-01: Triple-alias serde chain silent zero | AC-02 [GATE] | Critical |
| R-09: explicit_read_by_category field contract break | AC-13 [GATE] (partial — field definition) | High |
| R-13: Fixture update completeness | AC-10 | Medium |

---

## Unit Test Expectations

### AC-02 [GATE]: Triple-Alias Serde Chain — 5 Non-Negotiable Assertions

All five assertions are required. No alias may be validated only implicitly. Omission of any
single assertion is a gate failure (lesson #885 — serde-heavy types cause gate failures when
alias tests are omitted).

**Test: `test_search_exposure_count_deserializes_from_canonical_key`**
```
Arrange: JSON string with key "search_exposure_count" and value 42
Act:     serde_json::from_str::<FeatureKnowledgeReuse>(&json)
Assert:  result.is_ok()
         result.unwrap().search_exposure_count == 42
```

**Test: `test_search_exposure_count_deserializes_from_delivery_count_alias`**
```
Arrange: JSON string with key "delivery_count" and value 42
         (simulates stored cycle_review_index rows from pre-crt-049 cycles)
Act:     serde_json::from_str::<FeatureKnowledgeReuse>(&json)
Assert:  result.is_ok() — no deserialization error
         result.unwrap().search_exposure_count == 42
         (NOT zero — the alias must resolve to the correct field)
```

**Test: `test_search_exposure_count_deserializes_from_tier1_reuse_count_alias`**
```
Arrange: JSON string with key "tier1_reuse_count" and value 42
         (simulates stored rows from pre-col-020b cycles)
Act:     serde_json::from_str::<FeatureKnowledgeReuse>(&json)
Assert:  result.is_ok()
         result.unwrap().search_exposure_count == 42
```

**Test: `test_search_exposure_count_serializes_to_canonical_key`**
```
Arrange: FeatureKnowledgeReuse { search_exposure_count: 42, ..defaults }
Act:     serde_json::to_string(&value)
Assert:  serialized string contains key "search_exposure_count"
         serialized string does NOT contain key "delivery_count"
         serialized string does NOT contain key "tier1_reuse_count"
```
This ensures the canonical serialization output uses the new name. A stored row written
after crt-049 merges will use "search_exposure_count" as the key.

**Test: `test_search_exposure_count_round_trip_all_alias_forms`**
```
Arrange: JSON with each of the three key names in turn
For each alias form:
  Act:   deserialize → serialize → deserialize
  Assert: final value matches original value (42)
         final key is "search_exposure_count" (canonical)
```

### AC-01 / AC-13 [GATE] (partial): New Field Definitions

**Test: `test_explicit_read_count_defaults_to_zero_when_absent`**
```
Arrange: Minimal JSON for FeatureKnowledgeReuse omitting "explicit_read_count"
Act:     serde_json::from_str::<FeatureKnowledgeReuse>(&json)
Assert:  result.is_ok()
         result.unwrap().explicit_read_count == 0
         (verifies #[serde(default)] is present and correct)
```

**Test: `test_explicit_read_by_category_defaults_to_empty_map_when_absent`**
```
Arrange: Minimal JSON for FeatureKnowledgeReuse omitting "explicit_read_by_category"
Act:     serde_json::from_str::<FeatureKnowledgeReuse>(&json)
Assert:  result.is_ok()
         result.unwrap().explicit_read_by_category.is_empty()
         (verifies #[serde(default)] and type HashMap<String,u64>)
```

**Test: `test_explicit_read_by_category_serde_round_trip`**
```
Arrange: FeatureKnowledgeReuse with explicit_read_by_category = {"decision": 2, "pattern": 1}
Act:     serialize → deserialize
Assert:  round-tripped map equals {"decision": 2, "pattern": 1}
```

---

## Structural Assertions (Code Review / Compilation)

These are verified by `cargo build --workspace` passing and by code inspection:

1. `FeatureKnowledgeReuse.search_exposure_count` field exists with exactly two stacked
   `#[serde(alias)]` lines: `"delivery_count"` and `"tier1_reuse_count"`.
2. `FeatureKnowledgeReuse.explicit_read_count: u64` exists with `#[serde(default)]`.
3. `FeatureKnowledgeReuse.explicit_read_by_category: HashMap<String, u64>` exists with
   `#[serde(default)]`.
4. No reference to field name `delivery_count` remains in the struct definition
   (except as an alias string literal).
5. All callers in `retrospective.rs` that construct `FeatureKnowledgeReuse` using
   `delivery_count: ...` struct field syntax are updated to `search_exposure_count: ...`.

---

## Edge Cases

- **E-02 (fixture update)**: Existing tests in `retrospective.rs` that construct
  `FeatureKnowledgeReuse` with `delivery_count` as a Rust field name fail to compile
  after the rename — caught at `cargo build`. The resulting compile error is expected
  and must be fixed before the test suite can run (R-13).
- **AC-10**: All previously-passing serde round-trip tests continue to pass unchanged.
  No existing alias test is weakened. Verified by `cargo test unimatrix-observe` green.

---

## Expected Test Count Delta

- 5 new tests for AC-02 (triple-alias chain — all required)
- 3 new tests for AC-01/AC-13 partial field contract
- Total: +8 unit tests in `crates/unimatrix-observe/src/types.rs` test module
