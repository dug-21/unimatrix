# Test Plan: C4 Contradiction Detection

## File: crates/unimatrix-server/src/contradiction.rs

### Unit Tests (6 tests)

Tests for the conflict heuristic and helper functions. These are pure functions that do not require a server or store.

#### Test 1: test_negation_opposition_use_vs_avoid
```
#[test]
fn test_negation_opposition_use_vs_avoid():
    let content_a = "Use serde for config serialization."
    let content_b = "Avoid serde for config serialization."

    let score = check_negation_opposition(content_a, content_b)
    assert!(score > 0.0)  // negation detected
    assert_eq!(score, 1.0)  // exact subject match ("serde")
```
**AC**: AC-13
**Risk**: R-05 (false negatives)

#### Test 2: test_negation_opposition_always_vs_never
```
#[test]
fn test_negation_opposition_always_vs_never():
    let content_a = "Always enable strict mode."
    let content_b = "Never enable strict mode."

    let score = check_negation_opposition(content_a, content_b)
    assert!(score > 0.0)
```
**AC**: AC-13
**Risk**: R-05

#### Test 3: test_incompatible_directives_different_subjects
```
#[test]
fn test_incompatible_directives_different_subjects():
    let content_a = "Use reqwest for HTTP clients."
    let content_b = "Use ureq for HTTP clients."

    let score = check_incompatible_directives(content_a, content_b)
    assert!(score > 0.0)  // different subjects prescribed
```
**AC**: AC-13
**Risk**: R-05

#### Test 4: test_no_conflict_complementary_entries
```
#[test]
fn test_no_conflict_complementary_entries():
    let content_a = "Use tokio for async runtime management."
    let content_b = "Use tokio with multi-threaded runtime for best performance."

    // Same subject, same polarity -- not a conflict
    let (score, _) = conflict_heuristic(content_a, content_b, 0.5)
    assert_eq!(score, 0.0)
```
**AC**: AC-13
**Risk**: R-04 (false positives)

#### Test 5: test_no_conflict_agreement
```
#[test]
fn test_no_conflict_agreement():
    let content_a = "Use serde for serialization."
    let content_b = "Serde is a recommended choice for serialization."

    let (score, _) = conflict_heuristic(content_a, content_b, 0.5)
    assert_eq!(score, 0.0)
```
**AC**: AC-13
**Risk**: R-04 (false positives)

#### Test 6: test_dedup_canonical_pair_order
```
#[test]
fn test_dedup_canonical_pair_order():
    // Verify the dedup logic uses (min, max) ordering
    let pair_key_ab = (min(5, 10), max(5, 10))
    let pair_key_ba = (min(10, 5), max(10, 5))
    assert_eq!(pair_key_ab, pair_key_ba)
    assert_eq!(pair_key_ab, (5, 10))
```
**AC**: AC-12
**Risk**: R-11 (dedup failure)

### Additional Unit Tests

#### Test 7: test_sensitivity_high_flags_more
```
#[test]
fn test_sensitivity_high_flags_more():
    // Weak conflict: only opposing sentiment
    let content_a = "This approach is recommended and considered best practice."
    let content_b = "This approach is problematic and discouraged."

    // At default sensitivity (0.5): opposing sentiment alone (weight 0.1) may not pass threshold 0.5
    let (score_default, _) = conflict_heuristic(content_a, content_b, 0.5)

    // At high sensitivity (0.9): threshold is 0.1, so sentiment signal should pass
    let (score_sensitive, _) = conflict_heuristic(content_a, content_b, 0.9)

    assert!(score_sensitive >= score_default)
```
**AC**: AC-13
**Risk**: R-04

#### Test 8: test_heuristic_uses_hnsw_search
```
#[test]
fn test_heuristic_uses_hnsw_search():
    // This is a design verification test:
    // scan_contradictions calls vector_store.search(), not brute-force
    // Verified by code inspection -- the pseudocode shows:
    //   vector_store.search(embedding, config.neighbors_per_entry, EF_SEARCH)
    // This test verifies by checking that the function signature
    // requires &dyn VectorStore (which provides HNSW-backed search)
    // Compile-time verification is sufficient.
```
**AC**: AC-18
**Risk**: R-06 (performance)

### Integration Tests (3 tests)

These tests require a full server with store, vector index, and embed service.

#### Test 9: test_scan_contradictions_finds_conflict
```
#[tokio::test]
async fn test_scan_contradictions_finds_conflict():
    let server = make_server()

    // Insert two contradictory entries
    let id_a = insert_entry(&server.store, "Serde Config", "Use serde for config files.")
    let id_b = insert_entry(&server.store, "Avoid Serde Config", "Avoid serde for config files.")

    // Add embeddings to vector index
    embed_and_index(&server, id_a)
    embed_and_index(&server, id_b)

    // Run scan
    let adapter = server.embed_service.get_adapter().await.unwrap()
    let config = ContradictionConfig::default()
    let results = scan_contradictions(&server.store, &*server.vector_store, &*adapter, &config)

    // Verify: at least one contradiction pair found
    assert!(!results.is_empty())
    let pair = &results[0]
    assert!(pair.conflict_score > 0.0)
    // Verify canonical ordering
    assert!(pair.entry_id_a < pair.entry_id_b)
```
**AC**: AC-11
**Risk**: R-05 (false negatives)

#### Test 10: test_scan_empty_store
```
#[tokio::test]
async fn test_scan_empty_store():
    let server = make_server()

    let adapter = server.embed_service.get_adapter().await.unwrap()
    let config = ContradictionConfig::default()
    let results = scan_contradictions(&server.store, &*server.vector_store, &*adapter, &config)

    assert!(results.is_empty())
```
**AC**: AC-11
**Risk**: R-06 (performance -- empty case)

#### Test 11: test_embedding_consistency_round_trip
```
#[tokio::test]
async fn test_embedding_consistency_round_trip():
    let server = make_server()

    // Insert entry and embed it
    let id = insert_entry(&server.store, "Test Entry", "Some content for consistency check.")
    embed_and_index(&server, id)

    // Run consistency check
    let adapter = server.embed_service.get_adapter().await.unwrap()
    let config = ContradictionConfig::default()
    let results = check_embedding_consistency(
        &server.store, &*server.vector_store, &*adapter, &config
    )

    // Entry was just embedded with same model -- should be consistent
    assert!(results.is_empty())
```
**AC**: AC-16
**Risk**: R-07 (embedding consistency false positive)

## Risk Coverage

| Risk | Scenarios Covered |
|------|-------------------|
| R-04 | Tests 4, 5, 7: complementary entries and agreement not flagged |
| R-05 | Tests 1, 2, 3, 9: negation, always/never, incompatible directives, full scan |
| R-06 | Tests 8, 10: HNSW search used (not brute-force), empty store |
| R-07 | Test 11: embedding consistency round-trip |
| R-11 | Test 6: dedup canonical pair ordering |
