# Test Plan: Log Downgrade in `run_cosine_supports_path` (Item 2) — AC-04 / AC-05

## Component

`run_cosine_supports_path` in `crates/unimatrix-server/src/services/nli_detection_tick.rs`

## Behavioral-Only Coverage — Mandatory Gate Acknowledgment

**Per ADR-001(c) (Unimatrix entry #4143), log level is NOT asserted in tests for this item.**

The gate report at Stage 3c MUST state verbatim:

> "AC-04 and AC-05 log-level assertions are behavioral-only per ADR-001(c) (Unimatrix entry
> #4143). Log level verified by code review. No `tracing-test` harness used."

Any gate feedback requesting log-level assertions must be escalated to the Bugfix Leader.
This is the authoritative decision. The `tracing-test` harness is not to be added.

---

## Risks Covered

| Risk | Priority | AC |
|------|----------|----|
| R-05: Wrong `warn!` site downgraded | High | AC-04 (behavioral), AC-05 (behavioral + code review) |
| R-11: Behavioral-only coverage unacknowledged | Med | Gate report statement |

---

## Test Functions

### T-05: `test_cosine_supports_path_skips_missing_category_map_src` (AC-04, src branch)

**File**: `services/nli_detection_tick.rs` `#[cfg(test)]`
**Type**: `#[tokio::test]` async

**Arrange**:
- Open in-memory test store.
- Insert two test entries (IDs 1 and 2) with categories.
- Build `InferenceConfig::default()` (cosine threshold values are not critical — the path
  should skip before reaching the cosine calculation if src_id is absent from category_map).
- `candidate_pairs: Vec<(u64, u64, f32)> = vec![(1, 2, 0.80_f32)]`
- `existing: HashSet<(u64, u64)> = HashSet::new()`
- `category_map: HashMap<u64, &str>` — intentionally **omit entry ID 1** (src_id). Provide
  only `{ 2: "decision" }`.
- `ts = current_timestamp_secs()`

**Act**: Call `run_cosine_supports_path(&store, &config, &candidate_pairs, &existing, &category_map, ts).await`.

**Assert**:
- Function returns without panic.
- No Supports edges written to the store (pair is skipped).
- `store.query_graph_edges().await.unwrap()` returns empty — the pair was not processed.

**What is NOT asserted**: Log level. The test does not assert that a `debug!` was emitted
(as opposed to `warn!`). That is verified by code review.

**Coverage note**: Tests the `category_map.get(src_id)` None arm in Gate 3 of the function.
The pair should be skipped at the first lookup (src_id miss), so tgt_id is never reached.

---

### T-06: `test_cosine_supports_path_skips_missing_category_map_tgt` (AC-04, tgt branch)

**File**: `services/nli_detection_tick.rs` `#[cfg(test)]`
**Type**: `#[tokio::test]` async

**Arrange**:
- Same store setup as T-05.
- `candidate_pairs = vec![(1, 2, 0.80_f32)]`
- `category_map: HashMap<u64, &str>` — provide `{ 1: "lesson-learned" }` but **omit entry
  ID 2** (tgt_id).

**Act**: Call `run_cosine_supports_path` with the above inputs.

**Assert**:
- Function returns without panic.
- No Supports edges written.

**Coverage note**: Tests the `category_map.get(tgt_id)` None arm independently from T-05.
The edge case where both src_id and tgt_id are absent is handled by T-05 (src_id is the
first lookup; if it returns None, tgt_id is never reached).

---

### T-07: `test_cosine_supports_path_nonfinite_cosine_handled` (AC-05)

**File**: `services/nli_detection_tick.rs` `#[cfg(test)]`
**Type**: `#[tokio::test]` async

**Arrange**:
- Open in-memory test store.
- Insert two entries.
- Build `InferenceConfig::default()`.
- `candidate_pairs = vec![(1, 2, f32::NAN)]` — NaN cosine similarity value.
- `existing: HashSet<(u64, u64)> = HashSet::new()`
- `category_map: HashMap<u64, &str> = [(1, "lesson-learned"), (2, "decision")].into_iter().collect()`
  (both entries present so the non-finite cosine branch is reached, not the category_map miss).

**Act**: Call `run_cosine_supports_path` with the above inputs.

**Assert**:
- Function returns without panic.
- No Supports edges written (pair is skipped by the `!cosine.is_finite()` guard).

**What is NOT asserted**: Whether a `warn!` or `debug!` was emitted. Log level (must remain
`warn!` at this site) is verified by code review only per ADR-001(c).

**Code review requirement** (documented here for Stage 3c): The tester must verify by
inspection that the non-finite cosine guard site in `run_cosine_supports_path` (currently
line 765–771) uses `tracing::warn!`, not `tracing::debug!`, after the Item 2 change. This
site must NOT be changed. Code review confirmation must appear in the gate report.

---

## Code Review Checklist (required at Stage 3c)

The tester must inspect the diff for `nli_detection_tick.rs` and confirm:

1. Exactly two `warn!`→`debug!` changes exist in `run_cosine_supports_path`:
   - The `category_map.get(src_id)` None arm (message: `"Path C: source entry not found
     in category_map (deprecated mid-tick?) — skipping"`).
   - The `category_map.get(tgt_id)` None arm (message: `"Path C: target entry not found
     in category_map (deprecated mid-tick?) — skipping"`).

2. The non-finite cosine guard site (message: `"Path C: non-finite cosine for candidate
   pair — skipping"`) remains `tracing::warn!` unchanged.

3. No other sites in `run_cosine_supports_path` are modified.

The gate report must document: "Non-finite cosine `warn!` site verified by code review
to be unchanged. Exactly two `warn!`→`debug!` changes in `run_cosine_supports_path`."

---

## Shared-File Integration Note (SR-06)

Items 1 and 2 both modify `nli_detection_tick.rs`. Stage 3c tester must verify:
- The diff contains both the gate insertion (Item 1) and both warn→debug changes (Item 2).
- No extraneous changes are present in `nli_detection_tick.rs`.
- Tests for both items compile and pass together.
