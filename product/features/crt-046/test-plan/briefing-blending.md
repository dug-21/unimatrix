# Test Plan: briefing-blending

## Component Summary

`briefing-blending` covers the goal-conditioned blending path added to the
`context_briefing` handler. Two levels of short-circuit guard (ADR-004) ensure
cold-start correctness:

- Level 1 guard: `session_state.feature.is_none() OR current_goal.is_empty()` →
  skip all blending (no DB calls for cluster work).
- Level 2 guard: `get_cycle_start_goal_embedding` returns `None` → cold-start
  path (pure semantic result, unchanged).

When both guards pass and `query_goal_clusters_by_embedding` returns matching
rows, `blend_cluster_entries` merges cluster entries into the semantic result
using Option A score-based interleaving (ADR-005).

Non-negotiable gate tests: AC-11 (R-07, recency cap), AC-16 (R-08, NULL guard).

Risks addressed: R-07, R-08, R-11, R-12.

---

## Unit Tests — Guard Logic (AC-16, R-08, I-04)

### Guard A: feature=None → no embedding lookup (AC-16)

```rust
fn test_briefing_guard_a_feature_none_skips_embedding_lookup()
```
- This test validates Resolution 3 and ADR-004 Guard A.
- Use a mock or spy on `store.get_cycle_start_goal_embedding`.
- Arrange: `session_state.feature = None`, `current_goal = Some("some goal")`.
- Assert: `get_cycle_start_goal_embedding` is NOT called (call count == 0).
- Assert: `query_goal_clusters_by_embedding` is NOT called.
- Note: if mocking is difficult in Rust, test the guard condition logic directly
  via a unit-testable function extracted from the handler.

### Guard A: empty current_goal → no embedding lookup (I-04, Resolution 3)

```rust
fn test_briefing_guard_a_empty_goal_skips_embedding_lookup()
```
- Arrange: `session_state.feature = Some("crt-046")`, `current_goal = ""`.
- Assert: `get_cycle_start_goal_embedding` is NOT called.
- Assert: the cold-start path is activated (no cluster query issued).
- Critical: validates Resolution 3 (empty string treated identical to absent goal).

### Guard B: embedding=None → no cluster query (AC-16)

```rust
fn test_briefing_guard_b_null_embedding_skips_cluster_query()
```
- Arrange: `session_state.feature = Some("crt-046")`, `current_goal = "test goal"`.
  `get_cycle_start_goal_embedding` returns `Ok(None)`.
- Assert: `query_goal_clusters_by_embedding` is NOT called (call count == 0).
- Note: this test must distinguish between Guard A (no embedding call) and
  Guard B (embedding call made but returned None → no cluster call).

### Guard B: embedding DB error → cold-start, no propagation (F-02)

```rust
fn test_briefing_guard_b_embedding_error_cold_start()
```
- Arrange: `get_cycle_start_goal_embedding` returns `Err(...)`.
- Assert: briefing handler returns `Ok(semantic_results)` — no error propagated.
- Assert: result is identical to pure-semantic output (cold-start path taken).

---

## Unit Tests — `blend_cluster_entries` (covered in behavioral-signals.md)

The following tests are defined in `behavioral-signals.md` but primarily serve
the briefing-blending correctness requirements. Test plan references only:

- `test_blend_cluster_entries_displaces_weakest_semantic` → AC-07
- `test_blend_cluster_entries_low_cluster_score_excluded` → R-13-doc
- `test_blend_cluster_entries_deduplicates_by_entry_id` → dedup semantics
- `test_blend_cluster_entries_empty_cluster_returns_semantic` → AC-08/AC-09 unit form

---

## Store-Layer Tests — `query_goal_clusters_by_embedding` Recency Cap (AC-11, R-07)

The primary recency cap test is in `store-v22.md` as a store-layer test. The
integration form below exercises the end-to-end path through the MCP interface.

---

## Integration Tests — infra-001 `test_tools.py` and `test_lifecycle.py`

All integration tests below must NOT flush the drain before asserting briefing
results (briefing queries `goal_clusters` not `graph_edges`). The drain flush
rule applies only to `graph_edges` assertions.

### Cold-start: NULL goal embedding → pure-semantic result (AC-08, R-11)

```python
def test_briefing_null_embedding_cold_start(server):
```
- Arrange:
  1. Store several entries with known content.
  2. Call `context_briefing` with a known query to establish the baseline
     pure-semantic result (record result set A).
  3. Start a cycle with a goal text; do NOT call `context_cycle` with a
     goal embedding (or seed `cycle_events` with NULL `goal_embedding`).
- Act: call `context_briefing` with the same query and a feature attributed.
- Assert: result set is identical to baseline A (same entry IDs in same order).
- Assert: no `query_goal_clusters_by_embedding` DB call was issued (verify via
  server logs or zero rows in goal_clusters as proxy).

### Cold-start: empty goal_clusters table → pure-semantic result (AC-09, R-11)

```python
def test_briefing_empty_goal_clusters_cold_start(server):
```
- Arrange: store entries; verify `goal_clusters` is empty; get baseline result B.
- Act: call `context_briefing` with a feature and a goal that has an embedding.
- Assert: result is identical to baseline B.
- Note: the goal_clusters table being empty triggers the cold-start branch even
  if the embedding is present and the goal_cluster query returns no rows.

### Cold-start: feature=None → pure-semantic result, no cluster query (AC-16, R-08)

```python
def test_briefing_feature_none_cold_start(server):
```
- Arrange: store entries; get baseline result.
- Act: call `context_briefing` without feature attribution (no cycle active, or
  session_state.feature=None).
- Assert: result is identical to pure-semantic baseline.

### Cluster entry displaces weakest semantic result (AC-07)

```python
def test_briefing_cluster_displaces_weak_semantic(populated_server):
```
- Arrange:
  1. Start a cycle with a goal. Store the embedding via cycle_start.
  2. Seed a `goal_clusters` row for that feature_cycle with:
     - `goal_embedding` = embedding near the current goal (cosine ≥ 0.80)
     - `entry_ids_json` = "[<high-confidence-entry-id>]"
     - The high-confidence entry is an existing Active entry.
  3. Verify that 20 semantic results all have scores < `cluster_score` of the
     cluster entry. (Use a low-content-similarity query vs the cluster entry
     to ensure the cluster entry would not appear semantically.)
- Act: call `context_briefing` with the same goal/query.
- Assert: the cluster-derived entry appears in the top-20 results.
- Assert: the entry that would have been 20th in pure-semantic results is absent.

### Inactive entries excluded from briefing results (AC-10, R-12)

```python
def test_briefing_inactive_entries_excluded(server):
```
- Arrange:
  1. Store an Active entry (ID=A) and deprecate it.
  2. Store a Quarantined entry (ID=B).
  3. Store an Active entry (ID=C).
  4. Seed a `goal_clusters` row with `entry_ids_json = "[A, B, C]"` and a goal
     embedding that matches the current goal (cosine ≥ threshold).
- Act: call `context_briefing` with matching goal.
- Assert: entry A (deprecated) does NOT appear in results.
- Assert: entry B (quarantined) does NOT appear in results.
- Assert: entry C (Active) DOES appear in results (positive case).

### Recency cap: 101st row excluded even with best cosine (AC-11, R-07) — integration form

```python
def test_briefing_recency_cap_101_rows(server):
```
- Arrange:
  1. Insert 101 `goal_clusters` rows directly via raw SQL or via repeated cycle
     reviews. The 101st row (oldest `created_at`) has an entry ID of a
     known Active entry (ID=Z). The other 100 rows have entry IDs NOT equal to Z.
  2. Set the goal_embedding of the 101st row to be cosine 1.0 to the current
     goal (best possible match).
  3. The 100 newest rows have goal_embeddings with cosine 0.0 to the current goal.
- Act: call `context_briefing` with the matching goal (has stored embedding).
- Assert: entry Z (from the 101st row) does NOT appear in the top-20 results.
- Assert: recency cap excluded it before cosine comparison.
- Note: This test specifically verifies the `LIMIT 100` clause in
  `query_goal_clusters_by_embedding`. If Z appears, the cap is not enforced.

### Below-threshold cosine → cold-start (R-11)

```python
def test_briefing_below_threshold_no_cluster_injection(server):
```
- Arrange: seed a goal_clusters row with goal_embedding orthogonal to the
  current goal (cosine ≈ 0.0, well below 0.80 threshold).
- Act: call `context_briefing` with the current goal.
- Assert: result is identical to pure-semantic baseline (no cluster entries
  injected despite goal_clusters having rows).

### R-13-doc: low cluster_score → not in top-k (infra-001 form)

```python
def test_briefing_cluster_score_below_semantic_no_displacement(populated_server):
# Comment: FR-21 / ADR-005 — score-based interleaving; low cluster_score does
# not displace high-scoring semantic results. This is correct per spec, not a bug.
```
- Arrange:
  1. Seed a goal_clusters row with entry IDs not in the semantic top-20.
  2. Ensure the cluster entry's `cluster_score` (confidence * 0.35 + cosine * 0.25)
     is below the lowest semantic result's score.
- Assert: cluster entry absent from top-20 output.
- Assert: no error or warning emitted.
- Assert: test has comment citing FR-21 and ADR-005.

### Full chain: cycle review → goal_clusters → briefing blending (lifecycle)

```python
def test_cycle_review_to_briefing_blending_chain(server):
```
- Located in `test_lifecycle.py`.
- Arrange:
  1. Start a cycle with a goal and feature attribution.
  2. Perform two `context_get` calls for entries A and B (Active entries).
  3. Call `context_cycle_review` — step 8b runs, inserts goal_clusters row.
  4. Start a new cycle with a similar goal (same embedding or near-equivalent).
- Act: call `context_briefing` with the new cycle's goal and feature.
- Assert: at least one of entries A or B appears in the briefing result (cluster
  entry injected from the prior cycle's goal_clusters row).
- Assert: result count ≤ 20.

### E-08: feature cycle with no cycle_start event → cold-start (E-08)

```python
def test_briefing_feature_no_cycle_start_cold_start(server):
```
- Arrange: start a session attributed to feature "crt-046/no-start" but do NOT
  call `context_cycle` with a start event (so `cycle_events` has no cycle_start
  row for this feature_cycle).
- Act: call `context_briefing` with `current_goal = "some goal text"` and
  feature="crt-046/no-start".
- Assert: result is identical to pure-semantic baseline.
- Assert: no error returned.

---

## Naming Collision Verification

Before Stage 3c testing begins, inspect the `context_briefing` handler
implementation and verify that the `cluster_score` formula uses
`entry_record.confidence` (from `store.get_by_ids()`, Wilson-score [0,1]) and
NOT `index_entry.confidence` (from `briefing.index()`, raw cosine similarity).

Document in RISK-COVERAGE-REPORT.md: "Naming collision verified — cluster_score
formula uses EntryRecord.confidence (Wilson-score) at [file:line]."

---

## Assertions Summary

| Test | Guard/Path | Key Assertion |
|------|-----------|---------------|
| `test_briefing_guard_a_feature_none` | Level 1 guard | No embedding DB call |
| `test_briefing_guard_a_empty_goal` | Level 1 guard (Resolution 3) | No embedding DB call |
| `test_briefing_guard_b_null_embedding` | Level 2 guard | No cluster query call |
| `test_briefing_null_embedding_cold_start` | AC-08 | Result identical to baseline |
| `test_briefing_empty_goal_clusters_cold_start` | AC-09 | Result identical to baseline |
| `test_briefing_cluster_displaces_weak_semantic` | AC-07 | Cluster entry in top-20 |
| `test_briefing_inactive_entries_excluded` | AC-10 | Deprecated/quarantined absent |
| `test_briefing_recency_cap_101_rows` | AC-11, R-07 | 101st row's entry absent |
| `test_briefing_below_threshold_no_cluster_injection` | R-11 | No injection below 0.80 |
| `test_cycle_review_to_briefing_blending_chain` | AC-05+AC-07 lifecycle | End-to-end blending works |
