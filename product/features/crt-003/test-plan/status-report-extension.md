# Test Plan: C5 Status Report Extension

## File: crates/unimatrix-server/src/tools.rs (and response.rs)

### Unit Tests (2 tests)

#### Test 1: test_status_report_has_new_fields
```
#[test]
fn test_status_report_has_new_fields():
    let report = StatusReport {
        total_active: 10,
        total_deprecated: 2,
        total_proposed: 1,
        total_quarantined: 3,                          // NEW
        category_distribution: vec![],
        topic_distribution: vec![],
        entries_with_supersedes: 0,
        entries_with_superseded_by: 0,
        total_correction_count: 0,
        trust_source_distribution: vec![],
        entries_without_attribution: 0,
        contradictions: vec![],                         // NEW
        contradiction_count: 0,                         // NEW
        embedding_inconsistencies: vec![],              // NEW
        contradiction_scan_performed: false,             // NEW
        embedding_check_performed: false,                // NEW
    }
    assert_eq!(report.total_quarantined, 3)
    assert_eq!(report.contradiction_count, 0)
    assert!(!report.contradiction_scan_performed)
    assert!(!report.embedding_check_performed)
```
**AC**: AC-19
**Risk**: --

#### Test 2: test_status_report_with_contradictions
```
#[test]
fn test_status_report_with_contradictions():
    let pair = ContradictionPair {
        entry_id_a: 1,
        entry_id_b: 5,
        title_a: "Entry A".into(),
        title_b: "Entry B".into(),
        similarity: 0.92,
        conflict_score: 0.6,
        explanation: "negation opposition".into(),
    }
    let report = StatusReport {
        // ... standard fields ...
        contradictions: vec![pair],
        contradiction_count: 1,
        contradiction_scan_performed: true,
        // ... other new fields ...
    }
    assert_eq!(report.contradiction_count, 1)
    assert!(report.contradiction_scan_performed)
    assert_eq!(report.contradictions[0].entry_id_a, 1)
```
**AC**: AC-19
**Risk**: --

### Integration Tests (4 tests)

#### Test 3: test_context_status_includes_quarantined_count
```
#[tokio::test]
async fn test_context_status_includes_quarantined_count():
    let server = make_server()
    let id = insert_test_entry(&server.store)

    // Quarantine the entry
    server.store.update_status(id, Status::Quarantined)
    // Update counter manually (or use quarantine_with_audit)
    // ... ensure total_quarantined = 1

    let result = server.context_status(params { format: "json" }).await
    // Verify: JSON includes "quarantined" field with value > 0
    let json_text = extract_text(result)
    assert json_text contains "\"quarantined\""
```
**AC**: AC-14
**Risk**: R-03 (counter desync -- verified through status report)

#### Test 4: test_context_status_includes_contradictions
```
#[tokio::test]
async fn test_context_status_includes_contradictions():
    let server = make_server()

    // Insert two contradictory entries
    let id_a = insert_entry(&server.store, "Use Serde", "Use serde for config.")
    let id_b = insert_entry(&server.store, "Avoid Serde", "Avoid serde for config.")
    embed_and_index(&server, id_a)
    embed_and_index(&server, id_b)

    // Run context_status (contradiction scan is default ON)
    let result = server.context_status(params { format: "markdown" }).await
    let text = extract_text(result)

    // Verify: contradictions section present
    assert text contains "## Contradictions"
    // Note: if embed service not ready, section may be absent (graceful degradation)
```
**AC**: AC-15
**Risk**: --

#### Test 5: test_context_status_embedding_check_opt_in
```
#[tokio::test]
async fn test_context_status_embedding_check_opt_in():
    let server = make_server()

    // Without check_embeddings: no embedding integrity section
    let result = server.context_status(params { format: "markdown" }).await
    let text = extract_text(result)
    assert text does NOT contain "## Embedding Integrity"

    // With check_embeddings=true: embedding integrity section present
    let result = server.context_status(params {
        format: "markdown", check_embeddings: true
    }).await
    let text = extract_text(result)
    assert text contains "## Embedding Integrity"
```
**AC**: AC-17
**Risk**: --

#### Test 6: test_context_status_all_formats
```
#[tokio::test]
async fn test_context_status_all_formats():
    let server = make_server()
    insert_test_entry(&server.store)

    // Summary format
    let result = server.context_status(params { format: "summary" }).await
    let text = extract_text(result)
    assert text contains "Quarantined:"

    // Markdown format
    let result = server.context_status(params { format: "markdown" }).await
    let text = extract_text(result)
    assert text contains "Quarantined"

    // JSON format
    let result = server.context_status(params { format: "json" }).await
    let text = extract_text(result)
    assert text contains "\"quarantined\""
```
**AC**: AC-24
**Risk**: --

## Risk Coverage

| Risk | Scenarios Covered |
|------|-------------------|
| R-03 | Test 3: quarantined counter visible in status report |
