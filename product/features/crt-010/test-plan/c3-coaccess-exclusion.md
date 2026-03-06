# C3: Co-Access Deprecated Exclusion — Test Plan

## Location
`crates/unimatrix-engine/src/coaccess.rs` (unit tests in same file)

## Tests

### T-CA-01: Deprecated anchor produces zero boost (AC-08, R-06)
- Setup: co-access pair (anchor=1, partner=2), deprecated_ids = {1}
- Call compute_search_boost with anchor_ids=[1], result_ids=[2]
- Expected: empty boost_map (anchor skipped)

### T-CA-02: Deprecated partner produces zero boost (AC-08, R-06)
- Setup: co-access pair (anchor=1, partner=2), deprecated_ids = {2}
- Call compute_search_boost with anchor_ids=[1], result_ids=[2]
- Expected: empty boost_map (partner skipped)

### T-CA-03: Both deprecated — zero boost (AC-08)
- Setup: co-access pair (anchor=1, partner=2), deprecated_ids = {1, 2}
- Expected: empty boost_map

### T-CA-04: Empty deprecated_ids — backward compatible (R-06)
- Setup: co-access pair with count > 0, deprecated_ids = {} (empty)
- Expected: same behavior as before — boost computed normally
- This is the backward compatibility test

### T-CA-05: Mixed active/deprecated — only active get boost (AC-08)
- Setup: anchors=[1,2], results=[3,4], deprecated_ids={2,4}
- Co-access: (1,3) count=5, (1,4) count=5, (2,3) count=5
- Expected: boost for 3 (from anchor 1 only), no boost for 4 (deprecated partner)

### T-CA-06: Co-access storage unchanged for deprecated entries (AC-09)
- Setup: record co-access pair between Active and Deprecated entries
- Expected: pair stored in CO_ACCESS table
- Verify: storage write path does NOT check deprecated_ids

### T-CA-07: compute_briefing_boost also applies deprecated filtering (R-06)
- Same as T-CA-01 but via compute_briefing_boost
- Expected: empty boost_map with deprecated anchor

### T-CA-08: Existing co-access tests pass with empty deprecated_ids
- All existing tests in coaccess.rs must be updated to pass `&HashSet::new()`
- All must continue to pass (no behavioral change with empty set)

## Risk Coverage

| Risk | Scenarios | Tests |
|------|-----------|-------|
| R-06 (signature change) | All callers updated, backward compat | T-CA-04, T-CA-08 |
| AC-08 (deprecated exclusion) | Anchor, partner, both | T-CA-01..03, T-CA-05 |
| AC-09 (storage unchanged) | Write path unaffected | T-CA-06 |
