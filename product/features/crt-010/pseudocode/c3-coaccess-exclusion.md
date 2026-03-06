# C3: Co-Access Deprecated Exclusion — Pseudocode

## Location
`crates/unimatrix-engine/src/coaccess.rs`

## Changes

### Modified: `compute_search_boost`

```
pub fn compute_search_boost(
    anchor_ids: &[u64],
    result_ids: &[u64],
    store: &Store,
    staleness_cutoff: u64,
    deprecated_ids: &HashSet<u64>,   // NEW parameter
) -> HashMap<u64, f64>:
    return compute_boost_internal(anchor_ids, result_ids, store, staleness_cutoff, MAX_CO_ACCESS_BOOST, deprecated_ids)
```

### Modified: `compute_briefing_boost`

```
pub fn compute_briefing_boost(
    anchor_ids: &[u64],
    result_ids: &[u64],
    store: &Store,
    staleness_cutoff: u64,
    deprecated_ids: &HashSet<u64>,   // NEW parameter
) -> HashMap<u64, f64>:
    return compute_boost_internal(anchor_ids, result_ids, store, staleness_cutoff, MAX_BRIEFING_CO_ACCESS_BOOST, deprecated_ids)
```

### Modified: `compute_boost_internal`

```
fn compute_boost_internal(
    anchor_ids: &[u64],
    result_ids: &[u64],
    store: &Store,
    staleness_cutoff: u64,
    max_boost: f64,
    deprecated_ids: &HashSet<u64>,   // NEW parameter
) -> HashMap<u64, f64>:
    boost_map = HashMap::new()
    result_set = HashSet from result_ids

    for anchor_id in anchor_ids:
        // NEW: skip deprecated anchors
        if deprecated_ids.contains(&anchor_id):
            continue

        partners = store.get_co_access_partners(anchor_id, staleness_cutoff)
        if partners.is_err():
            log warning, continue

        for (partner_id, record) in partners:
            if partner_id not in result_set:
                continue
            if partner_id == anchor_id:
                continue
            // NEW: skip deprecated partners
            if deprecated_ids.contains(&partner_id):
                continue

            boost = co_access_boost(record.count, max_boost)
            update boost_map: take max of existing and new boost

    return boost_map
```

## Key Design Points

- `deprecated_ids` is `&HashSet<u64>` — no server-crate types in engine (ADR-004)
- Both anchor AND partner are checked against deprecated set (AC-08)
- Passing an empty HashSet preserves existing behavior (backward compatible)
- Co-access pair STORAGE is unchanged — only boost COMPUTATION excludes deprecated (AC-09)
- All existing callers must be updated: `compute_search_boost` in search.rs, `compute_briefing_boost` in briefing.rs
