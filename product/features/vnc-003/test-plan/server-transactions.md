# Test Plan: C6 Server Combined Transaction Methods

## File: `crates/unimatrix-server/src/server.rs`

### Tests: Fixed insert_with_audit (GH #14)

1. **test_insert_with_audit_vector_map_present** (NEW, async)
   - Call insert_with_audit via context_store tool flow
   - Verify VECTOR_MAP contains the entry_id -> data_id mapping
   - Covers AC-30, R-02 scenario 1

2. **test_insert_with_audit_hnsw_point_added** (NEW, async)
   - After insert_with_audit, verify HNSW point count increased
   - Covers AC-32

### Tests: correct_with_audit

3. **test_correct_with_audit_both_entries_written** (NEW, async)
   - Call correct_with_audit
   - Verify original entry: status=Deprecated, superseded_by=new_id, correction_count=1
   - Verify correction entry: supersedes=original_id, status=Active
   - Covers R-01 scenarios 1-3, AC-02, AC-03

4. **test_correct_with_audit_vector_map_for_correction** (NEW, async)
   - After correction, verify VECTOR_MAP contains the correction's entry_id
   - Covers AC-33, R-02 scenario 2

5. **test_correct_with_audit_counters_consistent** (NEW, async)
   - Insert 3 entries (3 active), correct one
   - Verify: total_active=3 (2 original + 1 correction), total_deprecated=1
   - Covers R-03 scenario 1

6. **test_correct_with_audit_chain** (NEW, async)
   - Correct A -> B, then correct B -> C
   - Verify A: deprecated, superseded_by=B
   - Verify B: deprecated, supersedes=A, superseded_by=C
   - Verify C: active, supersedes=B
   - Covers R-01 scenario 5

7. **test_correct_with_audit_nonexistent_id** (NEW, async)
   - correct_with_audit with non-existent original_id
   - Verify EntryNotFound error
   - Covers R-01 scenario 6, AC-08

8. **test_correct_with_audit_deprecated_entry** (NEW, async)
   - Deprecate entry, then attempt correct_with_audit
   - Verify error (cannot correct deprecated)
   - Covers R-04 scenario 1, AC-09

9. **test_correct_with_audit_audit_event** (NEW, async)
   - After correction, read audit log
   - Verify target_ids contains both original and new IDs
   - Covers AC-44

### Tests: deprecate_with_audit

10. **test_deprecate_with_audit_status_change** (NEW, async)
    - deprecate_with_audit on an active entry
    - Verify entry status=Deprecated
    - Covers AC-12

11. **test_deprecate_with_audit_counters_updated** (NEW, async)
    - Insert 3 active entries, deprecate one
    - Verify total_active=2, total_deprecated=1
    - Covers R-03

12. **test_deprecate_with_audit_idempotent** (NEW, async)
    - Deprecate entry, then deprecate again
    - Verify no error, entry still deprecated
    - Covers AC-13, R-11 scenario 1-2

13. **test_deprecate_with_audit_nonexistent_id** (NEW, async)
    - deprecate_with_audit with non-existent ID
    - Verify EntryNotFound error
    - Covers AC-14

14. **test_deprecate_with_audit_audit_event** (NEW, async)
    - After deprecation, verify audit event logged with reason
    - Covers AC-16

15. **test_deprecate_with_audit_no_audit_on_noop** (NEW, async)
    - Deprecate twice, verify only ONE audit event (not two)
    - Covers R-11 scenario 2

### Tests: Counter Cross-Validation

16. **test_mixed_operations_counters** (NEW, async)
    - Insert 5, deprecate 1, correct 1 -> total_active=4, total_deprecated=2
    - Covers R-03 scenario 2

17. **test_audit_ids_sequential_across_operations** (NEW, async)
    - Perform insert, deprecate, correct in sequence
    - Read all audit events, verify monotonic IDs
    - Covers AC-45

### AC Coverage

| AC | Test |
|----|------|
| AC-02 | test 3 (original entry fields after correction) |
| AC-03 | test 3 (correction entry fields) |
| AC-07 | test 3 (atomicity of both entries) |
| AC-08 | test 7 (EntryNotFound) |
| AC-09 | test 8 (deprecated entry rejection) |
| AC-12 | test 10 (status change) |
| AC-13 | test 12 (idempotent deprecation) |
| AC-14 | test 13 (EntryNotFound) |
| AC-16 | test 14 (audit event) |
| AC-30 | test 1 (VECTOR_MAP in txn) |
| AC-32 | test 2 (HNSW after commit) |
| AC-33 | test 4 (both paths) |
| AC-44 | test 9 (target_ids) |
| AC-45 | test 17 (sequential audit IDs) |
