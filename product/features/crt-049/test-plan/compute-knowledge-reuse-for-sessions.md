# Test Plan: compute_knowledge_reuse_for_sessions (tools.rs)

**File**: `crates/unimatrix-server/src/mcp/tools.rs`
**Function**: `async fn compute_knowledge_reuse_for_sessions(store: &Arc<SqlxStore>, session_records: &[SessionRecord], feature_cycle: &FeatureCycle, attributed: &[ObservationRecord]) -> Result<FeatureKnowledgeReuse>`
**Test module**: existing `#[cfg(test)] mod tests` block (around line 4737)

---

## Risks Covered

| Risk | AC | Priority |
|------|----|----------|
| R-07: attributed slice not threaded through | AC-05 | Medium |
| R-04: explicit_read_by_category silently partial at cap | — | Medium |
| R-11: N+1 query pattern | — (structural) | Medium |
| I-02: Two batch_entry_meta_lookup calls, connection safety | — | Medium |

---

## Existing Test Update: Signature Change

The existing test `test_compute_knowledge_reuse_for_sessions_no_block_on_panic` must be
updated to pass the new `attributed` parameter. The call site currently at line 4753 is:

```rust
// Before:
compute_knowledge_reuse_for_sessions(&store, &[], "test-cycle").await

// After (crt-049):
compute_knowledge_reuse_for_sessions(&store, &[], "test-cycle", &[]).await
```

The `&[]` for `attributed` is correct — an empty slice produces `explicit_read_count = 0`,
which is the expected behavior for the existing assertions:

```
assert_eq!(reuse.search_exposure_count, 0);  // was delivery_count
assert_eq!(reuse.cross_session_count, 0);
```

Note: the existing assertion `reuse.delivery_count` must be updated to
`reuse.search_exposure_count` after the field rename.

---

## AC-05: Integration Test — Attributed Slice Threads Through

**Test: `test_compute_knowledge_reuse_for_sessions_explicit_read_count_from_attributed`**

This is the only store-backed integration test for AC-05. It requires a real `SqlxStore`
and a synthetic `ObservationRecord` with a valid PreToolUse/context_get event.

```
Arrange:
  Open a test SqlxStore (same pattern as existing test — tempfile + SqlxStore::open)
  
  attributed = vec![
      ObservationRecord {
          event_type: "PreToolUse".to_string(),
          tool: Some("context_get".to_string()),
          input: Some(serde_json::json!({"id": 42})),
          session_id: "s1".to_string(),
          // ... other fields as defaults
      }
  ]
  
  sessions = &[]   (empty — no query_log or injection_log records needed;
                    explicit read extraction is in-memory from attributed slice)

Act:
  let result = compute_knowledge_reuse_for_sessions(
      &store, sessions, "test-cycle", &attributed
  ).await;

Assert:
  result.is_ok()
  let reuse = result.unwrap();
  reuse.explicit_read_count == 1
  reuse.search_exposure_count == 0   (no query_log records)
```

This test validates:
1. The `attributed` parameter is accepted by the new signature.
2. `extract_explicit_read_ids` is called with the attributed slice (R-07).
3. The returned `explicit_read_count` reflects in-memory observations, not DB queries.
4. `batch_entry_meta_lookup` is called for the explicit read IDs (entry `42` may not
   exist in the test DB — that's acceptable; `explicit_read_by_category` will be empty,
   but `explicit_read_count` is sourced from the ID set length, not the meta map).

---

## AC-05 Additional: Prefixed Tool Name in Attributed Slice

**Test: `test_compute_knowledge_reuse_for_sessions_prefixed_tool_name_normalized`**
```
Arrange:
  attributed = vec![
      ObservationRecord {
          event_type: "PreToolUse".to_string(),
          tool: Some("mcp__unimatrix__context_get".to_string()),
          input: Some(serde_json::Value::String(r#"{"id": 99}"#.to_string())),
          session_id: "s1".to_string(),
          ...
      }
  ]
  sessions = &[]

Act:
  compute_knowledge_reuse_for_sessions(&store, sessions, "test-cycle", &attributed).await

Assert:
  result.is_ok()
  result.unwrap().explicit_read_count == 1
```
This confirms `normalize_tool_name` is applied in the full pipeline (R-02) and the
`Value::String` input form (hook listener path) is parsed correctly.

---

## EXPLICIT_READ_META_CAP Boundary (R-04)

The `EXPLICIT_READ_META_CAP = 500` constant must exist in `tools.rs` near
`compute_knowledge_reuse_for_sessions`. These tests are structural — they verify the
constant exists and is used correctly.

**Test: `test_explicit_read_meta_cap_constant_exists`**
```
// Compile-time structural test — not a runtime test.
// Verified by: grep EXPLICIT_READ_META_CAP crates/unimatrix-server/src/mcp/tools.rs
// Asserts: constant is 500 via #[allow(dead_code)] annotation or usage in function body.
```

For the boundary behavior (501 IDs emitting a tracing::warn), a unit-level behavior test
is not practical without a mock store. This is documented as a code review check:
- When `explicit_ids.len() > EXPLICIT_READ_META_CAP`: `tracing::warn!` is emitted and the
  lookup uses only the first 500 IDs.
- `explicit_read_count` still equals the full set size (not capped).

---

## I-02: Two batch_entry_meta_lookup Calls — Connection Safety

Both calls to `batch_entry_meta_lookup` must be sequential awaited calls on the pool, not
concurrent. Connection is released between awaits (Rust async drop at `.await` boundary).

Structural verification (code review): the implementation pattern must be:
```rust
let meta_map_owned = batch_entry_meta_lookup(store, &ids_vec).await;
// ... (meta_map_owned is fully owned here, connection released)
let explicit_meta_map = batch_entry_meta_lookup(store, &explicit_ids_vec).await;
```
NOT concurrent (`join!` or simultaneous borrows). This is verified by code inspection, not
by a test assertion.

---

## R-11: Batch Query Structure (Structural Check)

`compute_knowledge_reuse_for_sessions` must call `batch_entry_meta_lookup` exactly once
for explicit read IDs — not in a loop. Verified by code review:

- One call: `batch_entry_meta_lookup(store, &explicit_ids_vec)` where `explicit_ids_vec`
  is the full (possibly capped) Vec of IDs at once.
- NOT: `for id in explicit_ids { batch_entry_meta_lookup(store, &[id]).await; }`

The existing `batch_entry_meta_lookup` function already handles 100-ID chunking internally.

---

## Expected Test Count Delta

- 1 updated existing test (signature change + field rename)
- 2 new integration tests (AC-05 × 2 variants)
- Total: 1 test updated + 2 new tests in `crates/unimatrix-server/src/mcp/tools.rs` test module
