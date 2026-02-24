# Test Plan: C6 Server Integration

## Risk Coverage: R-09 (Fire-and-forget masking)

### T-C6-01: Usage recording actually happens (R-09 scenario 1)
- Insert entry, call record_usage_for_entries
- Verify access_count > 0
- Verifies: AC-04, AC-05

### T-C6-02: Usage recording failure doesn't propagate (R-09 scenario 2)
- record_usage_for_entries with empty entry list
- Verify method returns without error

### T-C6-03: All 4 retrieval tools record usage (R-09 scenario 3)
- Verify record_usage_for_entries is called from context_search, context_lookup, context_get, context_briefing tool handlers

## Risk Coverage: R-10 (context_briefing double-counting)

### T-C6-04: Briefing dedup (R-10 scenario 1)
- Create entry matching both role lookup and task search
- Call context_briefing
- Verify access_count=1 (not 2)
- Verifies: AC-10

### T-C6-05: Briefing separate entries (R-10 scenario 2)
- Entries only in lookup + entries only in search
- Verify each has access_count=1

### T-C6-06: Briefing helpful dedup (R-10 scenario 3)
- Overlapping entry, call with helpful=true
- Verify helpful_count=1 (not 2)

## Risk Coverage: R-11 (Backward compatibility)

### T-C6-07: context_search without new params (R-11 scenario 1)
- Call context_search without feature or helpful
- Verify same results as before

### T-C6-08: context_lookup without new params (R-11 scenario 2)
- Verify same behavior

### T-C6-09: context_get without new params (R-11 scenario 3)
- Verify same behavior

### T-C6-10: context_briefing without new params (R-11 scenario 4)
- Verify same behavior

### T-C6-11: JSON schema includes optional params (R-11 scenario 5)
- Verify feature and helpful are optional in generated schema

## Risk Coverage: R-17 (FEATURE_ENTRIES trust bypass)

### T-C6-12: Restricted agent feature param ignored (R-17 scenario 1)
- Restricted agent retrieves with feature="test-feature"
- Verify FEATURE_ENTRIES empty for "test-feature"
- Verifies: AC-17

### T-C6-13: Internal agent feature param recorded (R-17 scenario 2)
- Internal agent retrieves with feature="test-feature"
- Verify FEATURE_ENTRIES has entries

### T-C6-14: Privileged agent feature param recorded (R-17 scenario 3)
- Privileged agent retrieves with feature="test-feature"
- Verify FEATURE_ENTRIES has entries

### T-C6-15: Restricted agent retrieval unchanged (R-17 scenario 4)
- Restricted agent's retrieval results are unchanged despite feature param being ignored

## Vote Correction Integration

### T-C6-16: End-to-end vote correction (AC-16)
- record_usage_for_entries with helpful=false
- record_usage_for_entries with helpful=true (same agent, same entries)
- Verify helpful_count=1, unhelpful_count=0

### T-C6-17: record_usage_for_entries with helpful=None
- Verify neither counter changes
- Verifies: AC-07 (partial)

## Edge Cases

### T-C6-18: Empty retrieval results (EC-01)
- record_usage_for_entries with empty entry_ids
- Verify no crash, no transaction

### T-C6-19: Single entry retrieval (EC-02)
- context_get returns 1 entry
- Verify record_usage works for 1-element batch

### T-C6-20: Vote after access-only retrieval (EC-06)
- First call: no helpful param (access only)
- Second call: helpful=true
- Verify vote registers (separate from access dedup)
