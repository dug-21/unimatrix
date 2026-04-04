# Test Plan: cycle-review-step-8b

## Component Summary

`cycle-review-step-8b` covers the insertion point of the new step 8b block
within the `context_cycle_review` handler in `mcp/tools.rs`. Step 8b:
1. Runs on EVERY call — cache-hit (force=false) or cache-miss (force=true).
2. The memoisation early-return appears AFTER the step 8b call site (Resolution 2).
3. Returns `parse_failure_count: u32` as a top-level field in the JSON response
   (outside `CycleReviewRecord`, Resolution 1).
4. All errors in step 8b are non-fatal — the handler returns successfully even
   if step 8b fails entirely.

Non-negotiable gate tests for this component: AC-13 (R-04), AC-15 (R-01).

Risks addressed: R-01, R-04, and structurally I-01 (step ordering).

---

## Unit Tests — `mcp/tools.rs` or `services/behavioral_signals.rs`

### Step 8b position: memoisation return AFTER step 8b (R-01, AC-15)

This is a **structural test** — verifiable via code inspection at Gate 3a.

**Gate 3a checklist item**: grep the implementation of `context_cycle_review` and
confirm that the memoisation early-return branch (the `if force == false AND
memoised` path) appears in the source code AFTER the `run_step_8b(...)` call.
The test plan mandates that no early-return branch precedes the step 8b call
site.

A runtime integration test (AC-15 below) provides the behavioral confirmation.

### Step 8b non-fatal on store error (F-01)

```rust
#[tokio::test]
async fn test_cycle_review_step8b_store_error_returns_success()
```
- Arrange: use a mock or test double that returns `Err` from
  `load_sessions_for_feature`. Alternatively: seed a valid store state but
  pass an invalid feature_cycle that returns no sessions.
- Act: call the step 8b orchestration function (or call `context_cycle_review`
  via the handler in a test harness that permits injection).
- Assert: the handler returns `Ok(review_record)` — no error propagation.
- Assert: `parse_failure_count` in response is either 0 (no parse attempted) or
  a valid u32 (not absent from the response).

Note: if step 8b is extracted into `run_step_8b(store, feature_cycle, outcome)`
as suggested by the architecture, the non-fatal behavior can be tested directly
on that function.

### parse_failure_count is u32, not nested inside CycleReviewRecord (R-04, Resolution 1)

This is a **structural test** (code review at Gate 3a):
- Verify that `CycleReviewRecord` struct in `cycle_review_index.rs` does NOT
  have a `parse_failure_count` field added.
- Verify that `parse_failure_count` is serialized as a top-level field in the
  JSON response alongside the serialized `CycleReviewRecord`.
- Verify that no `SUMMARY_SCHEMA_VERSION` bump is present.

---

## Integration Tests — infra-001 `test_tools.py`

All integration tests querying `graph_edges` must flush the analytics drain
before asserting. In the infra-001 harness the drain flush method is:
1. Server restart (preferred — terminates drain task, flushes on shutdown).
2. A dedicated force-flush call if the harness exposes one.
3. `time.sleep(0.7)` as a fallback (>500ms DRAIN_FLUSH_INTERVAL), documented.

### AC-13 (NON-NEGOTIABLE): parse_failure_count in MCP response (R-04)

```python
def test_cycle_review_parse_failure_count_in_response(server):
```
- Arrange:
  1. Start a cycle via `context_cycle` (start event) for a feature.
  2. Seed one malformed observation directly via raw SQL:
     `INSERT INTO observations (session_id, ts_millis, hook, tool, input) VALUES (session_id, ..., 'tool_call', 'context_get', 'not-json')`.
     Note: if the harness does not support raw SQL seeding, use `context_get`
     with a crafted input that will parse successfully at the MCP level but
     produce a non-integer `id` field.
  3. Seed two valid `context_get` observations via `server.context_get(id=A)` and
     `server.context_get(id=B)`.
- Act: call `server.context_cycle_review(feature=..., format="json")`.
- Assert:
  - Response JSON has top-level field `parse_failure_count` with value ≥ 1.
  - Valid edges still produced: flush drain, assert `graph_edges WHERE source='behavioral'`
    has ≥ 2 rows (both directions for the valid pair).
- Note: This test MUST inspect the actual returned JSON payload for
  `parse_failure_count`, not only side-effect assertions.

### All-valid observations: parse_failure_count == 0 in response (R-04)

```python
def test_cycle_review_parse_failure_count_zero_clean(server):
```
- Arrange: valid context_get observations only.
- Act: call `context_cycle_review`.
- Assert: `parse_failure_count == 0` in response (field present and zero, not absent).

### All-malformed observations: parse_failure_count == N, zero edges (R-04)

```python
def test_cycle_review_all_malformed_zero_edges(server):
```
- Arrange: seed N malformed observations, no valid context_get observations.
- Act: call `context_cycle_review`.
- Assert: `parse_failure_count == N` (or ≥ N if count not deterministic).
- Flush drain; assert 0 behavioral edges in `graph_edges`.
- Assert: handler returned success (not an error).

### AC-15 (NON-NEGOTIABLE): force=false step 8b re-emission (R-01)

```python
def test_cycle_review_force_false_reruns_step8b(server):
```
- Arrange:
  1. Start a cycle with a feature and a goal.
  2. Seed two valid `context_get` observations in the same session (IDs A and B).
- Act first call: `server.context_cycle_review(feature=..., force=True)` (or
  first call without force for a cache miss).
- Flush drain.
- Record: `count_after_first = COUNT(*) FROM graph_edges WHERE source='behavioral'`.
- Act second call: `server.context_cycle_review(feature=..., force=False)`.
- Flush drain.
- Assert: `COUNT(*) FROM graph_edges WHERE source='behavioral'` is identical to
  `count_after_first` — step 8b ran, INSERT OR IGNORE deduplicated, count stable.
- Assert: count is NOT 0 (step 8b ran on both calls, not bypassed).
- Note: this test fails if the early-return is placed before step 8b (R-01).

### I-01: step 8b does not run if step 8a fails

```python
def test_cycle_review_step8b_skipped_if_step8a_fails(server):
```
- This test verifies that if `store_cycle_review` (step 8a) fails, step 8b does
  not emit edges for an uncommitted review record.
- Arrange: force a step 8a failure (e.g., invalid feature_cycle that triggers a
  constraint violation in store_cycle_review).
- Assert: `graph_edges WHERE source='behavioral'` count is 0.
- Note: if step 8a failure is hard to trigger via MCP, this can be a unit test
  using a mock store.

### Bidirectional edge emission (AC-01, R-10)

```python
def test_cycle_review_bidirectional_edges(server):
```
- Arrange: cycle with two context_get observations for entries A and B.
- Act: call review; flush drain.
- Assert: `graph_edges WHERE source_id=A AND target_id=B AND source='behavioral'`
  has ≥ 1 row.
- Assert: `graph_edges WHERE source_id=B AND target_id=A AND source='behavioral'`
  has ≥ 1 row.
- Fail condition: only one direction is present.

### Edge idempotency — duplicate call (AC-02)

```python
def test_cycle_review_edge_idempotency(server):
```
- Arrange: same observations seeded once.
- Act: call review twice; flush drain after each call.
- Assert: `COUNT(*) WHERE source='behavioral'` is identical after both calls.

### Pair cap produces ≤ 400 edges (AC-14, R-09)

```python
def test_cycle_review_pair_cap_200(server):
```
- Arrange: seed a session with 21 distinct `context_get` observations (IDs
  1..21). 21 IDs produce 210 pairs; cap should truncate at 200.
- Act: call review; flush drain.
- Assert: `COUNT(*) WHERE source='behavioral'` ≤ 400 (200 pairs × 2 directions).
- Assert: server log contains "pair cap" or equivalent warning.

### Zero context_get observations → zero edges (AC-04)

```python
def test_cycle_review_zero_get_obs_zero_edges(server):
```
- Arrange: cycle with only `context_store` and `context_search` observations
  (no context_get).
- Act: call review; flush drain.
- Assert: `COUNT(*) WHERE source='behavioral'` == 0.

### goal_clusters row created when goal embedding present (AC-05)

```python
def test_cycle_review_goal_cluster_created(server):
```
- Arrange: start cycle with non-empty goal text (triggers goal embedding write
  in cycle_events); seed two context_get observations.
- Act: call review.
- Assert: `SELECT COUNT(*) FROM goal_clusters WHERE feature_cycle=?` == 1.
- Assert: `goal_embedding` column is non-NULL.
- Assert: `entry_ids_json` is a JSON array containing the observed entry IDs.
- Assert: `outcome` column matches the review outcome.

### No goal → no goal_clusters row (AC-06)

```python
def test_cycle_review_no_goal_no_cluster(server):
```
- Arrange: start cycle without a goal (no goal text in cycle_start event,
  so `goal_embedding IS NULL` in cycle_events).
- Act: call review.
- Assert: `SELECT COUNT(*) FROM goal_clusters WHERE feature_cycle=?` == 0.
