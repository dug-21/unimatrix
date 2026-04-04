# Test Plan: behavioral-signals

## Component Summary

`behavioral_signals.rs` is a new `pub(crate)` module in `unimatrix-server/src/services/`
containing all behavioral signal logic. Every function is tested at the unit
level without requiring a server stack or full SQLite store. The module has no
async functions except `emit_behavioral_edges` and `populate_goal_cluster`, which
require a `SqlxStore` handle.

Functions under test:
- `collect_coaccess_entry_ids(obs: &[ObservationRow]) -> (HashMap<String, Vec<(u64, i64)>>, usize)`
- `build_coaccess_pairs(by_session: HashMap<String, Vec<(u64, i64)>>) -> (Vec<(u64, u64)>, bool)`
- `outcome_to_weight(outcome: Option<&str>) -> f32`
- `emit_behavioral_edges(store, pairs, weight) -> (usize, usize)`
- `populate_goal_cluster(store, ...) -> Result<bool>`
- `blend_cluster_entries(semantic, cluster_entries_with_scores, k) -> Vec<IndexEntry>`

Risks addressed: R-02, R-03, R-06, R-09, R-10, R-16, E-02.

---

## Unit Tests — `collect_coaccess_entry_ids`

All tests in `#[cfg(test)] mod tests` in `behavioral_signals.rs`.

### Happy path: extracts IDs from context_get observations (I-03)

```rust
fn test_collect_coaccess_entry_ids_extracts_context_get_ids()
```
- Arrange: 3 observations with `tool = "context_get"` and `input = r#"{"id": 42}"#`.
  1 observation with `tool = "context_search"` (not context_get).
- Assert: returned HashMap contains only entries for context_get observations.
- Assert: parse_failure_count == 0.

### Malformed JSON → parse_failure_count incremented (AC-13, R-04)

```rust
fn test_collect_coaccess_entry_ids_malformed_json_counted()
```
- Arrange: 2 valid observations + 1 with `input = "not-json"`.
- Assert: parse_failure_count == 1.
- Assert: the two valid IDs are present in the HashMap.

### Missing `id` field → parse_failure_count incremented (AC-13)

```rust
fn test_collect_coaccess_entry_ids_missing_id_field_counted()
```
- Arrange: `input = r#"{"tool": "context_get"}"#` (has JSON but no `id` key).
- Assert: parse_failure_count == 1.

### None input → treated as parse failure

```rust
fn test_collect_coaccess_entry_ids_none_input_counted()
```
- Arrange: `input = None` on a context_get observation.
- Assert: parse_failure_count == 1 (not a panic).

### Non-context_get observations ignored (I-03)

```rust
fn test_collect_coaccess_entry_ids_ignores_non_context_get()
```
- Arrange: only observations with `tool = "context_search"` and `tool = "context_store"`.
- Assert: returned HashMap is empty; parse_failure_count == 0.

### Duplicate entry ID in same session (E-04)

```rust
fn test_collect_coaccess_entry_ids_deduplicates_same_id_same_session()
```
- Arrange: two context_get observations in the same session with `id = 42`.
- Assert: session bucket contains only one entry for ID 42 (deduplication before
  pair building).
  Note: deduplication of IDs within a session prevents self-pair from appearing.

---

## Unit Tests — `build_coaccess_pairs`

### Happy path: 3 IDs in one session → 3 canonical pairs

```rust
fn test_build_coaccess_pairs_three_ids_three_pairs()
```
- Arrange: session "s1" with IDs [1, 2, 3] at distinct ts_millis.
- Assert: pairs contains exactly {(1,2), (1,3), (2,3)} (canonical form min < max).
- Assert: cap_hit == false.

### Self-pair exclusion: all same ID → empty pairs (E-02, Resolution 4)

```rust
fn test_build_coaccess_pairs_self_pairs_excluded()
```
- Arrange: session "s1" with IDs [5, 5, 5] (all same entry ID).
- Assert: returned pairs is empty.
- Assert: cap_hit == false.
- This verifies the `filter(|(a, b)| a != b)` is applied before dedup.

### Pair cap enforced at enumeration time (AC-14, R-09)

```rust
fn test_build_coaccess_pairs_cap_enforced_at_200()
```
- Arrange: session "s1" with 25 distinct entry IDs (would produce 300 pairs without cap).
- Assert: returned pairs length ≤ 200.
- Assert: cap_hit == true.
- Assert: pairs generation halted at 200 (not all 300 generated then truncated —
  verify by observing that total pairs == exactly 200).

### Cap not hit when under 200

```rust
fn test_build_coaccess_pairs_no_cap_under_200()
```
- Arrange: 19 distinct IDs (produces 171 pairs).
- Assert: pairs length == 171.
- Assert: cap_hit == false.

### Multi-session: pairs scoped within sessions

```rust
fn test_build_coaccess_pairs_multi_session_no_cross_session_pairs()
```
- Arrange: session "s1" with IDs [1, 2]; session "s2" with IDs [3, 4].
- Assert: pairs contains (1,2) and (3,4) but NOT (1,3), (1,4), (2,3), (2,4).
  (Cross-session pairs are not co-access within the same request context.)
  Note: confirm with SPEC that pairs are per-session, not cross-session.

### Empty input → empty pairs

```rust
fn test_build_coaccess_pairs_empty_input_empty_pairs()
```
- Assert: empty HashMap → empty pairs, cap_hit == false.

### Single ID in session → no pair

```rust
fn test_build_coaccess_pairs_single_id_no_pair()
```
- Arrange: session "s1" with [42].
- Assert: pairs is empty (AC-04 unit form).

---

## Unit Tests — `outcome_to_weight` (R-16)

```rust
fn test_outcome_to_weight_success_returns_1_0()
```
- Assert: `outcome_to_weight(Some("success")) == 1.0f32`.

```rust
fn test_outcome_to_weight_none_returns_0_5()
```
- Assert: `outcome_to_weight(None) == 0.5f32`.

```rust
fn test_outcome_to_weight_rework_returns_0_5()
```
- Assert: `outcome_to_weight(Some("rework")) == 0.5f32`.

```rust
fn test_outcome_to_weight_unknown_returns_0_5()
```
- Assert: `outcome_to_weight(Some("some-future-outcome-string")) == 0.5f32`.

All four cases in one table-driven test is acceptable; separate tests shown for
clarity. At minimum, the four cases must be explicitly asserted.

---

## Unit Tests — `emit_behavioral_edges` (R-02, R-03, R-10)

These tests require a `SqlxStore`. Use `open_test_store()` from `test_helpers`.
The drain flush is `store.close().await` — this triggers drain shutdown which
completes all pending writes before returning.

### UNIQUE-conflict path does NOT increment edges_enqueued (R-02-contract)

```rust
#[tokio::test]
async fn test_emit_behavioral_edges_unique_conflict_not_counted()
```
- Arrange: use `open_test_store()`. Pre-insert an NLI `Informs` edge for pair
  (A=1, B=2) and (A=2, B=1) directly into `graph_edges` via write_pool.
- Act: call `emit_behavioral_edges(store, &[(1, 2)], 1.0)`.
- Flush: `store.close().await`.
- Assert: returned tuple is `(0, 1)` — edges_enqueued == 0, pairs_skipped == 1.
  Both directions conflicted with the pre-seeded NLI edges.

### New pair produces two edges (R-10)

```rust
#[tokio::test]
async fn test_emit_behavioral_edges_new_pair_emits_both_directions()
```
- Arrange: empty store; call `emit_behavioral_edges(store, &[(1, 2)], 0.5)`.
- Flush: close store.
- Assert: returned tuple is `(2, 0)` — both directions inserted.
- Assert: `graph_edges WHERE source='behavioral' AND relation_type='Informs'`
  has exactly 2 rows: (source=1, target=2) and (source=2, target=1).

### N pairs → 2N enqueue calls (R-10)

```rust
#[tokio::test]
async fn test_emit_behavioral_edges_n_pairs_2n_edges()
```
- Arrange: 3 pairs → expect 6 edges (no conflicts).
- Assert: edges_enqueued == 6; graph_edges count == 6.

### Weight stored correctly (AC-03)

```rust
#[tokio::test]
async fn test_emit_behavioral_edges_weight_stored_in_graph_edge()
```
- Arrange: emit with weight=1.0.
- Assert: `graph_edges.weight` column for the new rows is 1.0.

### Empty pairs input → zero enqueues

```rust
#[tokio::test]
async fn test_emit_behavioral_edges_empty_pairs_zero_edges()
```
- Assert: `emit_behavioral_edges(store, &[], 1.0)` returns `(0, 0)`.

### bootstrap_only=false in behavioral edges (R-03)

```rust
fn test_emit_behavioral_edges_bootstrap_only_is_false()
```
- This is a code inspection test — verify in the implementation that
  `AnalyticsWrite::GraphEdge` emitted by `emit_behavioral_edges` always has
  `bootstrap_only = false`.
- A runtime integration test (R-03 scenario 3 in OVERVIEW.md) provides
  complementary coverage.

---

## Unit Tests — `populate_goal_cluster`

### Happy path: inserts row and returns Ok(true)

```rust
#[tokio::test]
async fn test_populate_goal_cluster_new_cycle_returns_true()
```
- Arrange: empty store; valid goal_embedding; entry_ids = &[1, 2, 3].
- Act: call `populate_goal_cluster(store, "fc-001", embedding, &[1,2,3], Some("impl"), Some("success"))`.
- Assert: returns `Ok(true)`.
- Assert: `SELECT COUNT(*) FROM goal_clusters WHERE feature_cycle='fc-001'` == 1.
- Assert: `entry_ids_json` in DB parses as a JSON array containing 1, 2, 3.

### Duplicate feature_cycle returns Ok(false) without error (R-06)

```rust
#[tokio::test]
async fn test_populate_goal_cluster_duplicate_returns_false()
```
- Arrange: pre-insert goal_clusters row for "fc-001".
- Act: call `populate_goal_cluster` again for "fc-001".
- Assert: returns `Ok(false)` — INSERT OR IGNORE, no error.
- Assert: still only one row in goal_clusters.

### Called AFTER entry_ids assembled, not speculatively (R-06)

This is a code review / structural test. Verify in pseudocode and implementation
that `populate_goal_cluster` is the final call in step 8b, invoked only after
`entry_ids` is fully assembled from `collect_coaccess_entry_ids`. No partial-write
path exists where `insert_goal_cluster` is called before all entry IDs are known.

---

## Unit Tests — `blend_cluster_entries` (AC-07, R-11, R-12, R-13-doc)

`blend_cluster_entries` is a pure function. No store required.

### Cluster entry displaces weakest semantic result (AC-07)

```rust
fn test_blend_cluster_entries_displaces_weakest_semantic()
```
- Arrange:
  - semantic: 20 `IndexEntry` with scores [1.0, 0.9, ..., 0.05] (descending)
  - cluster_entries_with_scores: 1 entry not in semantic, cluster_score=0.5
  - k = 20
- Assert: result has exactly 20 entries.
- Assert: the cluster entry appears (its score 0.5 > weakest semantic score 0.05).
- Assert: the weakest semantic entry (score=0.05) is absent.

### Cluster entry below all semantic scores → not in top-k (R-13-doc)

```rust
fn test_blend_cluster_entries_low_cluster_score_excluded()
// Comment must cite FR-21 and ADR-005 per R-13-doc requirement.
```
- Arrange:
  - semantic: 20 entries with scores [1.0, 0.9, ..., 0.45] (all > 0.40)
  - cluster entry: cluster_score = 0.10 (below all semantic scores)
- Assert: cluster entry absent from top-20.
- Assert: no error, no warning.

### Deduplication: cluster entry already in semantic → first occurrence wins

```rust
fn test_blend_cluster_entries_deduplicates_by_entry_id()
```
- Arrange:
  - semantic entry ID=99 with score=0.3
  - cluster entry ID=99 with cluster_score=0.8
  - k = 5
- Assert: result contains exactly one entry with ID=99.
- Assert: the semantic entry (score=0.3) is the surviving one if it appeared
  first in the merge order — or the cluster entry if it scored higher and
  appeared first after sort. Clarify: "first occurrence wins" after sort-by-score
  descending means the one with the higher score survives. Assert that the
  entry in the output has the expected score (whichever score is higher).
  Note: this test reveals the exact deduplication semantics (first-by-score vs
  first-by-insertion-order); the assertion must match the implementation.

### Cold-start: empty cluster list → semantic results unchanged (R-11)

```rust
fn test_blend_cluster_entries_empty_cluster_returns_semantic()
```
- Arrange: semantic = 20 entries; cluster_entries_with_scores = vec![].
- Assert: result is identical to semantic (same IDs, same order).

### Return top-k only

```rust
fn test_blend_cluster_entries_returns_top_k()
```
- Arrange: semantic = 10 entries; cluster = 5 entries (all new IDs, high scores).
  k = 7.
- Assert: result has exactly 7 entries.

### Naming collision guard (ARCHITECTURE §Component 4 step 6)

This is not a runtime test but a code review requirement. The test plan author
must verify that the `cluster_score` formula in the `context_briefing` handler
uses `EntryRecord.confidence` (Wilson-score from `store.get_by_ids()`) and NOT
`IndexEntry.confidence` (raw cosine from `briefing.index()`). The implementation
must include a comment at the cluster_score computation line referencing ADR-005
naming-collision warning.
