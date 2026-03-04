# Test Plan: status-serialize

## Risk Coverage

| Risk | Scenarios | Priority |
|------|----------|----------|
| R-03 | JSON backward compatibility | High |
| R-12 | Serde propagation | Low |

## Unit Tests (mcp/response/status.rs)

### R-03: JSON Backward Compatibility

1. **test_status_report_json_full_snapshot**
   - Build StatusReport with known data for ALL fields
   - Serialize via StatusReportJson
   - Compare output against golden JSON (hardcoded expected string)
   - Verify field names: total_active, total_deprecated, total_proposed, total_quarantined
   - Verify nested: correction_chains.entries_with_supersedes, security.trust_source_distribution
   - Verify nested: co_access.total_pairs, co_access.top_clusters[].entry_a.id
   - Covers: R-03 scenarios 1, 6, 7, 8, 10

2. **test_status_report_json_with_contradictions**
   - Build StatusReport with contradiction_scan_performed=true, 1 contradiction
   - Serialize, verify "contradictions" array present with correct field names
   - Verify contradiction_count present
   - Covers: R-03 scenario 2

3. **test_status_report_json_without_contradictions**
   - Build StatusReport with contradiction_scan_performed=false
   - Serialize, verify "contradictions" key absent from output
   - Covers: R-03 scenario 3

4. **test_status_report_json_with_embedding_inconsistencies**
   - Build StatusReport with embedding_check_performed=true, 1 inconsistency
   - Serialize, verify "embedding_inconsistencies" array present
   - Verify field name is "self_match_similarity" (not "expected_similarity")
   - Covers: R-03 scenario 4

5. **test_status_report_json_without_embedding_check**
   - Build StatusReport with embedding_check_performed=false
   - Serialize, verify "embedding_inconsistencies" key absent
   - Covers: R-03 scenario 5

6. **test_status_report_json_with_outcomes**
   - Build StatusReport with total_outcomes > 0
   - Serialize, verify "outcomes" section with total, by_type, by_result, top_feature_cycles
   - Covers: R-03 scenario 8

7. **test_status_report_json_category_distribution_is_object**
   - Build StatusReport with category_distribution: [("decision", 5), ("pattern", 3)]
   - Serialize, parse as serde_json::Value
   - Verify category_distribution is {"decision": 5, "pattern": 3} (object, not array)
   - Covers: R-03 scenario 6

### R-12: Serde Propagation

8. **test_contradiction_pair_serializable**
   - Create ContradictionPair, call serde_json::to_string
   - Verify compiles and produces valid JSON
   - Covers: R-12

9. **test_embedding_inconsistency_serializable**
   - Create EmbeddingInconsistency, call serde_json::to_string
   - Verify compiles and produces valid JSON

## Test Setup Pattern

```
fn make_test_status_report() -> StatusReport {
    StatusReport {
        total_active: 10,
        total_deprecated: 3,
        // ... all fields with known values ...
    }
}
```

Golden JSON comparison uses `serde_json::from_str::<serde_json::Value>` for
structural comparison (ignoring key ordering differences).
