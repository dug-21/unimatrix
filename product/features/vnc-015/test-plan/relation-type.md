# Test Plan: RelationType Enum Extension (graph.rs)

**Component**: `crates/unimatrix-engine/src/graph.rs`
**Architecture ref**: Component 3
**Risk coverage**: R-01 (Critical), R-11 (partial)
**AC coverage**: AC-03, AC-04, AC-14

---

## SR-01 Grep Verification (ADR-007 Gate-3a Requirement)

Before any unit tests run, Gate-3a must verify the 10×4 variant×site compliance table.
These are grep checks against the source file, not test executions.

| Variant | graph.rs enum | graph.rs as_str() | graph.rs from_str() | Required in PPR | Required in graph_expand |
|---------|:---:|:---:|:---:|:---:|:---:|
| `Advances` | grep required | grep required | grep required | ABSENT (Phase 2) | ABSENT (Phase 2) |
| `Cites` | grep required | grep required | grep required | ABSENT | ABSENT |
| `Asserts` | grep required | grep required | grep required | ABSENT | ABSENT |
| `Mentions` | grep required | grep required | grep required | ABSENT | ABSENT |
| `Refutes` | grep required | grep required | grep required | ABSENT | ABSENT |
| `Tests` | grep required | grep required | grep required | ABSENT | ABSENT |
| `DerivedFrom` | grep required | grep required | grep required | ABSENT | ABSENT |
| `Motivates` | grep required | grep required | grep required | ABSENT (Phase 2) | ABSENT (Phase 2) |
| `About` | grep required | grep required | grep required | ABSENT | ABSENT |
| `RelatedTo` | grep required | grep required | grep required | REQUIRED | REQUIRED |

### Grep Commands for Stage 3c Verification

```bash
# Enum body — each variant must appear as a standalone variant arm
for v in Advances Cites Asserts Mentions Refutes Tests DerivedFrom Motivates About RelatedTo; do
    grep -n "    $v," crates/unimatrix-engine/src/graph.rs || echo "MISSING: $v in enum body"
done

# as_str() — each variant must appear in the match
for v in Advances Cites Asserts Mentions Refutes Tests DerivedFrom Motivates About RelatedTo; do
    grep -n "Self::$v =>" crates/unimatrix-engine/src/graph.rs || echo "MISSING: $v in as_str()"
done

# from_str() — each variant must appear in the match
for v in Advances Cites Asserts Mentions Refutes Tests DerivedFrom Motivates About RelatedTo; do
    grep -n "\"$v\" =>" crates/unimatrix-engine/src/graph.rs || echo "MISSING: $v in from_str()"
done

# RelatedTo MUST appear in PPR positive set
grep -n "RelatedTo" crates/unimatrix-engine/src/graph_ppr.rs || echo "MISSING: RelatedTo in graph_ppr.rs"

# RelatedTo MUST appear in graph_expand positive set
grep -n "RelatedTo" crates/unimatrix-engine/src/graph_expand.rs || echo "MISSING: RelatedTo in graph_expand.rs"

# NEGATIVE: Advances and Motivates must NOT appear in PPR or graph_expand
grep -n "Advances\|Motivates" crates/unimatrix-engine/src/graph_ppr.rs && echo "ERROR: Advances/Motivates in graph_ppr.rs"
grep -n "Advances\|Motivates" crates/unimatrix-engine/src/graph_expand.rs && echo "ERROR: Advances/Motivates in graph_expand.rs"
```

---

## Unit Test Expectations

### Location: `crates/unimatrix-engine/src/graph.rs` (inline tests)

#### Per-Variant from_str() Round-Trip Tests (10 tests — R-01, AC-03)

One test per new variant. Each must be individually named and assertable — no omnibus test.

```
test_relation_type_advances_roundtrip
test_relation_type_cites_roundtrip
test_relation_type_asserts_roundtrip
test_relation_type_mentions_roundtrip
test_relation_type_refutes_roundtrip
test_relation_type_tests_roundtrip
test_relation_type_derived_from_roundtrip
test_relation_type_motivates_roundtrip
test_relation_type_about_roundtrip
test_relation_type_related_to_roundtrip
```

Pattern for each:
```rust
#[test]
fn test_relation_type_<variant>_roundtrip() {
    let v = RelationType::<Variant>;
    assert_eq!(RelationType::from_str(v.as_str()), Some(v));
}
```

#### test_relation_type_total_variant_count
- Assert: `RelationType::all_variants().len() == 16` (6 existing + 10 new)
- If no `all_variants()` helper exists, count via exhaustive match or enum iteration

#### test_relation_type_existing_variants_unchanged (AC-04)
- Assert: all 6 existing variants still parse correctly:
  ```rust
  assert_eq!(RelationType::from_str("Supersedes"), Some(RelationType::Supersedes));
  assert_eq!(RelationType::from_str("Contradicts"), Some(RelationType::Contradicts));
  assert_eq!(RelationType::from_str("Supports"), Some(RelationType::Supports));
  assert_eq!(RelationType::from_str("CoAccess"), Some(RelationType::CoAccess));
  assert_eq!(RelationType::from_str("Prerequisite"), Some(RelationType::Prerequisite));
  assert_eq!(RelationType::from_str("Informs"), Some(RelationType::Informs));
  ```

#### test_relation_type_unknown_string_returns_none
- Assert: `RelationType::from_str("UnknownType") == None`
- Assert: `RelationType::from_str("") == None`
- Assert: `RelationType::from_str("relatedto") == None` (case-sensitive — case-insensitive is NOT the contract)
- Note: this confirms no wildcard fallback arm (security risk: `_ => Some(RelatedTo)`)

#### test_relation_type_as_str_case_preserved
- For all 10 new variants: `v.as_str()` must return the exact string declared in the 10×4 table
- Assert: `RelationType::RelatedTo.as_str() == "RelatedTo"` (not "relatedTo", "RELATEDTO", etc.)
- Same for all others.

---

## Integration Test Expectations (Pass 2b Survival — R-01, AC-14)

### Location: inline in `crates/unimatrix-engine/src/graph.rs` (or `graph_tests.rs`)

Per-variant Pass 2b survival tests (10 tests — each must be individually named).
These verify that `build_typed_relation_graph` does NOT silently drop rows for the new variants.

```
test_build_typed_graph_advances_survives_pass2b
test_build_typed_graph_cites_survives_pass2b
test_build_typed_graph_asserts_survives_pass2b
test_build_typed_graph_mentions_survives_pass2b
test_build_typed_graph_refutes_survives_pass2b
test_build_typed_graph_tests_survives_pass2b
test_build_typed_graph_derived_from_survives_pass2b
test_build_typed_graph_motivates_survives_pass2b
test_build_typed_graph_about_survives_pass2b
test_build_typed_graph_related_to_survives_pass2b
```

Pattern for each:
```rust
#[tokio::test]
async fn test_build_typed_graph_<variant>_survives_pass2b() {
    // Arrange: insert a GRAPH_EDGES row with relation_type = "<Variant>"
    //          directly into a test DB (bypassing write path validation)
    let store = test_store().await;
    insert_raw_graph_edge(&store, source_id, target_id, "<Variant>").await;

    // Act: call build_typed_relation_graph
    let graph = build_typed_relation_graph(&store).await.unwrap();

    // Assert: edge count for this (source, target, Variant) pair is 1, not 0
    let edges = graph.edges_between(source_id, target_id);
    assert_eq!(edges.len(), 1, "Variant <Variant> was silently dropped by Pass 2b");
}
```

Note: The R-10 guard in `build_typed_relation_graph` Pass 2b drops rows where `from_str()` returns
`None`. A missing `from_str()` arm causes the edge to be silently discarded. These 10 per-variant
tests each independently confirm that from_str() is present and correctly handles the variant string.

### test_build_typed_graph_existing_variants_unaffected (AC-04 regression)
- Insert GRAPH_EDGES rows for all 6 existing variants
- Build typed graph
- Assert: all 6 edges survive Pass 2b; edge count per variant == 1

### test_build_typed_graph_unknown_string_dropped
- Insert a GRAPH_EDGES row with `relation_type = "BogusType"`
- Build typed graph
- Assert: edge count == 0 (the R-10 guard correctly drops unknown types)
- Note: confirms the R-10 guard is working correctly for truly unknown strings
