# Agent Report: 528-agent-1-fix

**Feature:** bugfix-528
**Branch:** bugfix/528-promotion-tick-allowlist

## Files Modified

- `crates/unimatrix-server/src/services/co_access_promotion_tick.rs`
- `crates/unimatrix-server/src/services/co_access_promotion_tick_tests.rs`
- `crates/unimatrix-server/src/services/typed_graph.rs`

## Changes Made

### Fix 1 — co_access_promotion_tick.rs

Changed all four JOIN conditions from denylist (`status != ?3` / `Status::Quarantined`) to allowlist (`status = ?3` / `Status::Active`):
- Lines in outer query: `ea.status` and `eb.status`
- Lines in subquery: `ea2.status` and `eb2.status`

Updated comments to reflect:
- Filter description: allowlist excluding all non-Active statuses (Deprecated, Proposed, Quarantined, and any future non-Active status by construction)
- Bind description: `?3 = Status::Active`
- NULL failure mode: `status = NULL` (operator changed from `!=` to `=`; same outcome — always NULL, silently promotes nothing)

Updated bind from `Status::Quarantined as u8 as i64` to `Status::Active as u8 as i64`.

### Fix 2 — co_access_promotion_tick_tests.rs

Added Group K test `test_deprecated_endpoint_pair_not_promoted`:
- Seeds D (deprecated, status=1) BEFORE calling `seed_co_access` for the D-endpoint pair, so INSERT OR IGNORE preserves the deprecated status
- Deprecated pair (A↔D) has count=10, active pair (A↔B) has count=5 — higher deprecated count is the primary correctness signal
- PRIMARY assertion: A↔B weight = 1.0 (5/5). A broken subquery filter would yield max_count=10 and weight=0.5
- SECONDARY assertion: only A↔B edges present; A↔D and D↔A absent

### Fix 3 — typed_graph.rs

Expanded comment at the Quarantined filter in `TypedGraph::rebuild` to document:
- Deprecated nodes are intentionally retained for SR-01 Supersedes-chain traversal
- After compaction removes deprecated-endpoint edges, deprecated nodes appear in `all_entries` with no outgoing CoAccess edges — this is EXPECTED and CORRECT
- Future maintainers must NOT add a filter to exclude deprecated nodes from the snapshot

## New Tests

- `test_deprecated_endpoint_pair_not_promoted` (Group K, GH #528)

## Test Results

```
cargo test -p unimatrix-server co_access_promotion
test result: ok. 35 passed; 0 failed; 0 ignored
```

All 35 co_access_promotion tests pass (34 pre-existing + 1 new).

## Issues

None.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — found entry #4161 (pre-created lesson for this exact bug, already tagged bugfix-528) and entry #3980 (pattern: tick batch SELECT must JOIN entries on both endpoints)
- Stored: entry #4162 via `context_correct` superseding #4161 — added: (1) the subquery alias ea2/eb2 being the critical missed case that silently deflates weights, (2) test design rule requiring the deprecated pair to have a higher count than the active pair to make a subquery-side miss detectable
