# Component: Status Penalty Validation

## Purpose

Prove crt-010 status penalties produce correct ranking outcomes via behavior-based integration tests. Tests assert relative ranking, not score constants (ADR-003).

## Design

All tests go in a new test module in the server crate. Tests use the existing `make_test_store()` and search service construction patterns from `briefing.rs` tests.

**Test file**: `crates/unimatrix-server/src/services/search_penalty_tests.rs`

This is a new file included as a `#[cfg(test)]` module from `search.rs` or as a standalone test file in the services directory.

### Test Infrastructure

Each test:
1. Creates a fresh Store + VectorIndex + SearchService
2. Inserts entries with controlled embeddings (pre-computed vectors with known cosine similarity)
3. Inserts vector embeddings directly via VectorIndex
4. Executes search via SearchService
5. Asserts relative ranking of results

**Embedding injection pattern**:
- Use simple f32 vectors (384-dim or whatever the test embed dimension is)
- Control cosine similarity by vector construction (e.g., shared direction with magnitude variation)
- Bypass ONNX pipeline entirely by inserting vectors directly into VectorIndex

**Key helper function**:
```
fn setup_penalty_test() -> (TempDir, Arc<Store>, SearchService, ...)
  Create store, vector index, search service (same pattern as briefing tests)
  Return handles for direct entry/vector insertion

fn insert_entry_with_vector(store, vector_index, title, content, status, embedding, confidence)
  Insert entry via store.insert()
  Set status and confidence on the entry (may need store.update_status() or direct SQL)
  Insert embedding via vector_index.insert(id, embedding)
```

### Test Cases

#### T-SP-01: Deprecated below active (Flexible mode)
```
ARRANGE:
  active_entry = insert(status=Active, embedding=vec_a, confidence=0.65)
  deprecated_entry = insert(status=Deprecated, embedding=vec_b, confidence=0.65)
  // vec_b has HIGHER similarity to query than vec_a
  // But after 0.7 penalty, deprecated should rank lower

ACT:
  results = search(query_embedding, mode=Flexible, k=10)

ASSERT:
  results contains both entries
  active_entry appears before deprecated_entry in results
  // NO assertion on specific scores or constants
```

#### T-SP-02: Superseded below active (Flexible mode)
```
ARRANGE:
  active_entry = insert(status=Active, embedding=vec_a, confidence=0.65)
  superseded_entry = insert(status=Deprecated, superseded_by=Some(active_id), embedding=vec_b, confidence=0.65)

ACT:
  results = search(query_embedding, mode=Flexible, k=10)

ASSERT:
  active_entry appears before superseded_entry
```

#### T-SP-03: Strict mode exclusion
```
ARRANGE:
  active_entry = insert(status=Active, ...)
  deprecated_entry = insert(status=Deprecated, ...)
  superseded_entry = insert(status=Active, superseded_by=Some(other_id), ...)

ACT:
  results = search(query_embedding, mode=Strict, k=10)

ASSERT:
  results contain active_entry
  results do NOT contain deprecated_entry
  results do NOT contain superseded_entry
```

#### T-SP-04: Co-access exclusion for deprecated
```
ARRANGE:
  active_entry_1 = insert(status=Active, ...)
  active_entry_2 = insert(status=Active, ...)
  deprecated_entry = insert(status=Deprecated, ...)
  // Create co-access pairs: (active_1, active_2) and (active_1, deprecated)
  store.record_co_access(active_1, active_2, ...)
  store.record_co_access(active_1, deprecated, ...)

ACT:
  results = search(query, mode=Flexible, co_access_anchors=[active_1])

ASSERT:
  active_entry_2 may receive co-access boost
  deprecated_entry does NOT receive co-access boost
  // Verify by checking deprecated_entry does not rank higher than expected
```

#### T-SP-05: Deprecated-only query returns results (Flexible)
```
ARRANGE:
  deprecated_entry = insert(status=Deprecated, embedding matching query)
  // No active entries matching the query

ACT:
  results = search(query, mode=Flexible, k=10)

ASSERT:
  results is NOT empty
  results contain deprecated_entry
```

#### T-SP-06: Superseded with successor injection
```
ARRANGE:
  successor_entry = insert(status=Active, ...)
  superseded_entry = insert(status=Deprecated, superseded_by=Some(successor_id), ...)
  // Only superseded matches query well

ACT:
  results = search(query, mode=Flexible, k=10)

ASSERT:
  results contain successor_entry (injected)
  successor_entry ranks above superseded_entry
```

## Error Handling

Tests should not unwrap blindly except in test setup. Use `.expect("description")` pattern.

## Key Test Scenarios

See T-SP-01 through T-SP-06 above. All are integration-level tests exercising the full search pipeline.
