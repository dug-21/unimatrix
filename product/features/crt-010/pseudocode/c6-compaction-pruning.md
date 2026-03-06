# C6: Vector Index Pruning During Compaction — Pseudocode

## Status: ALREADY SATISFIED

No code changes required. Verification test only.

## Existing Behavior (col-013)

```
// background.rs:234-257 — Background tick fires periodically
// StatusService::compute_report() collects active_entries:
//   status.rs:175-181 — filtered to Status::Active
// StatusService::run_maintenance() passes only Active entries to compact():
//   status.rs:608-637 — compaction with Active-only entries
```

The compaction path already excludes deprecated and quarantined entries from HNSW rebuilds. After compaction:
- Deprecated entry embeddings are gone from HNSW
- Active successor entries remain findable via their own embeddings
- VECTOR_MAP entries for deprecated IDs are removed

## Verification Test Plan

A single integration test confirms:
1. Insert Active + Deprecated entries with embeddings
2. Run compaction (simulating background tick)
3. Verify deprecated entry IDs absent from VECTOR_MAP
4. Verify Active entries still present and searchable
5. Verify Active successor of deprecated entry is directly findable

This test does NOT exercise crt-010 code — it validates the col-013 foundation that crt-010 relies on.
