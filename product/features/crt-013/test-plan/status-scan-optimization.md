# Test Plan: Status Scan Optimization

## Unit Tests

### Store Method Tests (R-06, R-08, R-11)

| Test | Assertion | Risk |
|------|-----------|------|
| compute_status_aggregates_empty_db | All counts 0, empty distribution | R-06 |
| compute_status_aggregates_basic | Insert known entries, verify counts match | R-06 |
| compute_status_aggregates_trust_source_grouping | Multiple trust sources → correct distribution | R-06 |
| compute_status_aggregates_empty_trust_source | trust_source="" mapped to "(none)" | R-06 |
| compute_status_aggregates_empty_created_by | created_by="" counted as unattributed | R-06 |
| compute_status_aggregates_extreme_correction_count | correction_count=u32::MAX → no overflow | R-11 |
| load_active_entries_with_tags_only_active | Returns Active entries only, not Deprecated etc | R-08 |
| load_active_entries_with_tags_includes_tags | Tags correctly loaded for active entries | R-08 |
| load_active_entries_with_tags_empty_db | Returns empty vec | R-08 |

### Comparison Test (AC-10, R-06)

| Test | Assertion | AC |
|------|-----------|-----|
| status_aggregates_comparison | Both paths produce field-by-field identical results | AC-10 |

**Comparison test design**:
1. Create dataset with 10+ entries covering all edge cases:
   - Entry with supersedes=Some, superseded_by=None
   - Entry with supersedes=None, superseded_by=Some
   - Entry with both supersedes and superseded_by
   - Entry with correction_count=0, 1, 100
   - Entry with trust_source="human", "agent", ""
   - Entry with created_by="someone", ""
   - Active, Deprecated, Proposed entries
2. Run old path: SELECT * FROM entries → Rust iteration
3. Run new path: compute_status_aggregates()
4. Assert field-by-field equality:
   - supersedes_count matches
   - superseded_by_count matches
   - total_correction_count matches
   - trust_source_distribution matches exactly
   - unattributed_count matches

## Integration Test

### StatusService produces equivalent output (AC-09)

| Test | Assertion | AC |
|------|-----------|-----|
| status_no_full_scan | Code review confirms no `SELECT {ENTRY_COLUMNS} FROM entries` without WHERE clause in status.rs | AC-09 |

## Edge Cases

| Edge | Test | Expected |
|------|------|----------|
| EC-06: All entries empty trust_source | compute_status_aggregates_empty_trust_source | `{"(none)": N}` |
| EC-07: Zero entries | compute_status_aggregates_empty_db | All zeros |
| EC-08: supersedes → non-existent entry | Counted in supersedes_count (just counts non-NULL) | Correct |
| R-11: Large correction_count | compute_status_aggregates_extreme_correction_count | No overflow |
