# Test Plan: C6 -- Tool Integration

## Location

`crates/unimatrix-server/tests/` (integration tests)
`crates/unimatrix-server/src/response.rs` (unit tests for format)

## Risk Coverage

- R-08: Quarantined entry partners receive undeserved boost
- R-11: StatusReport extension breaks existing status response parsing
- R-13: Briefing boost changes which entries appear in orientation

## Test Scenarios

### T-C6-01: context_search -- co-access boost applied (integration)

```
Given entries E1, E2, E3 stored
And CO_ACCESS pairs: (E1.id, E3.id) count=10
When context_search returns [E1, E2, E3] (E1 is anchor)
Then E3 receives co-access boost
And E3's final score > E3's base rerank score
```

### T-C6-02: context_search -- no co-access data, identical to pre-crt-004

```
Given entries stored but CO_ACCESS table empty
When context_search is called
Then results are ranked by similarity+confidence only (no boost)
```

### T-C6-03: context_search -- quarantined partner excluded from boost (R-08)

```
Given:
  - Entry A (active), Entry B (quarantined), Entry C (active)
  - CO_ACCESS: (A.id, B.id) count=10, (A.id, C.id) count=1
When context_search with A as anchor
Then B excluded from partner list (quarantined)
And C gets boost from co-access with A
```

### T-C6-04: context_search -- deprecated partner excluded from boost (R-08)

```
Given:
  - Entry A (active), Entry D (deprecated), Entry C (active)
  - CO_ACCESS: (A.id, D.id) count=10, (A.id, C.id) count=1
When computing co-access partners for A
Then D excluded (deprecated status)
And C included
```

### T-C6-05: context_briefing -- small boost applied (R-13)

```
Given entries and CO_ACCESS pairs set up
When context_briefing called
Then relevant_context ordering may change slightly due to MAX_BRIEFING_CO_ACCESS_BOOST=0.01
But the effect is small (0.01 max)
```

### T-C6-06: context_briefing -- no co-access data, identical to pre-crt-004 (R-13)

```
Given CO_ACCESS table empty
When context_briefing called
Then briefing output is identical to behavior without crt-004
```

### T-C6-07: context_status -- co-access stats in summary format (R-11)

```
Given 3 CO_ACCESS pairs (2 active, 1 stale)
When context_status called with format=summary
Then response includes "Co-access: 2 active pairs (3 total)"
And stale_pairs_cleaned >= 1
```

### T-C6-08: context_status -- co-access stats in markdown format (R-11)

```
Given CO_ACCESS pairs with top clusters
When context_status called with format=markdown
Then response includes "## Co-Access Patterns" section
And top clusters table present
```

### T-C6-09: context_status -- co-access stats in JSON format (R-11)

```
Given CO_ACCESS pairs
When context_status called with format=json
Then JSON includes "co_access" object
With total_pairs, active_pairs, stale_pairs_cleaned, top_clusters fields
```

### T-C6-10: context_status -- empty co-access data (R-11)

```
Given empty CO_ACCESS table
When context_status called
Then co-access fields present with zero values (not omitted)
And no error
```

### T-C6-11: StatusReport -- new fields initialized to defaults

```
Given StatusReport constructed with existing field values
Then total_co_access_pairs = 0
And active_co_access_pairs = 0
And top_co_access_pairs = vec![]
And stale_pairs_cleaned = 0
```

### T-C6-12: context_status -- stale cleanup piggybacked (R-07 integration)

```
Given CO_ACCESS pairs including stale ones
When context_status called
Then stale_pairs_cleaned > 0
And subsequent co_access_stats shows reduced total
```
