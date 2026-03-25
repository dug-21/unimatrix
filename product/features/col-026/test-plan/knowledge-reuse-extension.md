# Test Plan: knowledge-reuse-extension

**Crate**: `unimatrix-server/src/mcp/knowledge_reuse.rs`
**Risks covered**: R-04, R-13
**ACs covered**: AC-12, AC-17, AC-18

---

## Component Scope

This component:
1. Adds second closure `entry_meta_lookup: impl Fn(&[u64]) -> HashMap<u64, EntryMeta>` to
   `compute_knowledge_reuse`.
2. Adds new `EntryMeta` struct (private to `knowledge_reuse.rs`).
3. Populates `cross_feature_reuse`, `intra_cycle_reuse`, `total_served`,
   `top_cross_feature_entries` on `FeatureKnowledgeReuse`.
4. Updates all three `FeatureKnowledgeReuse {}` construction sites.

All unit tests extend the existing `#[cfg(test)] mod tests` block in `knowledge_reuse.rs`.
The `entry_meta_lookup` closure is always synthetic in unit tests (returns a `HashMap`
directly). The production closure (IN-clause SQL query) is validated through infra-001.

---

## Unit Test Expectations

### R-04: Partial Batch Lookup Return

#### Test: `test_knowledge_reuse_partial_meta_lookup` (R-04, AC-12)

**Scenario**: 5 served entries; `entry_meta_lookup` returns metadata for only 3.
**Setup**:
- Query log: entries `[10, 20, 30, 40, 50]` served across sessions.
- `entry_meta_lookup` returns: `{10: EntryMeta{feature_cycle: "prior"}, 20: EntryMeta{feature_cycle: "prior"}, 30: EntryMeta{feature_cycle: "col-026"}}`.
- IDs 40 and 50 absent (quarantined/deleted).
- `current_feature_cycle = "col-026"`.
**Assert**:
- No panic.
- `cross_feature_reuse <= delivery_count` (entries 10, 20 are cross-feature).
- `cross_feature_reuse + intra_cycle_reuse <= delivery_count` (arithmetic consistent).
- IDs 40, 50 not counted in either bucket (silently excluded, same as existing `entry_category_lookup` behavior).

#### Test: `test_knowledge_reuse_all_meta_missing` (R-04)

**Scenario**: All served entries return no metadata (empty HashMap from lookup).
**Assert**:
- `cross_feature_reuse == 0`.
- `intra_cycle_reuse == 0`.
- `top_cross_feature_entries.is_empty()`.
- No panic.

### R-04: Empty Entry ID Set — Skip Batch Call

#### Test: `test_knowledge_reuse_empty_entry_set_skips_lookup` (R-04)

**Scenario**: No query_log or injection_log records (zero served entries).
**Setup**: A closure that panics if called (to verify it is NOT called).

```rust
let panic_lookup = |_: &[u64]| -> HashMap<u64, EntryMeta> { panic!("must not be called") };
let result = compute_knowledge_reuse(&[], &[], &HashMap::new(), "col-026", |_| None, panic_lookup);
assert_eq!(result.delivery_count, 0);
```

**Assert**: Closure not invoked. `delivery_count == 0`. `cross_feature_reuse == 0`.

### Cross-Feature vs Intra-Cycle Split

#### Test: `test_knowledge_reuse_cross_feature_split` (AC-12)

**Scenario**: 4 entries served. 2 have `feature_cycle != current_cycle` (cross-feature).
2 have `feature_cycle == current_cycle` (intra-cycle).
**Assert**:
- `cross_feature_reuse == 2`.
- `intra_cycle_reuse == 2`.
- `cross_feature_reuse + intra_cycle_reuse == delivery_count`.

#### Test: `test_knowledge_reuse_all_cross_feature` (AC-12)

**Scenario**: All served entries are from prior feature cycles.
**Assert**: `intra_cycle_reuse == 0`. `cross_feature_reuse == delivery_count`.

#### Test: `test_knowledge_reuse_all_intra_cycle` (AC-12)

**Scenario**: All served entries have `feature_cycle == current_cycle`.
**Assert**: `cross_feature_reuse == 0`. `intra_cycle_reuse == delivery_count`.
`top_cross_feature_entries.is_empty()`.

### Top Cross-Feature Entries

#### Test: `test_top_cross_feature_entries_sorted_by_serve_count` (AC-12)

**Scenario**: 6 cross-feature entries with different serve counts.
**Assert**: `top_cross_feature_entries` contains at most 5 entries, sorted by `serve_count`
descending. The entry with the highest serve_count is first.

#### Test: `test_top_cross_feature_entries_fewer_than_three` (AC-12, edge case)

**Scenario**: Only 2 cross-feature entries exist.
**Assert**: `top_cross_feature_entries.len() == 2`. No error. No padding with dummy entries.

#### Test: `test_top_cross_feature_entries_empty_when_none` (AC-12)

**Scenario**: No cross-feature entries (all intra-cycle).
**Assert**: `top_cross_feature_entries.is_empty()`. Formatter omits the table (verified in
formatter tests).

### Batch Lookup Called Exactly Once

#### Test: `test_entry_meta_lookup_called_once` (ADR-003, R-04)

**Scenario**: Multiple query_log and injection_log records totaling 5 distinct entry IDs.
**Setup**: Closure with a call counter (using `std::cell::Cell` or `Arc<AtomicUsize>`).
**Assert**: Counter == 1 after `compute_knowledge_reuse` returns.

```rust
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
let call_count = Arc::new(AtomicUsize::new(0));
let cc = call_count.clone();
let meta_lookup = move |ids: &[u64]| -> HashMap<u64, EntryMeta> {
    cc.fetch_add(1, Ordering::SeqCst);
    // return synthetic metadata for all IDs
    ids.iter().map(|&id| (id, EntryMeta { title: "t".into(), feature_cycle: Some("prior".into()), category: "decision".into() })).collect()
};
compute_knowledge_reuse(&query_logs, &injection_logs, &HashMap::new(), "col-026", |_| None, meta_lookup);
assert_eq!(call_count.load(Ordering::SeqCst), 1);
```

### Existing Tests — Signature Migration (AC-17)

All existing `compute_knowledge_reuse` tests must be updated to supply the new
`entry_meta_lookup` closure. The minimal migration is:

```rust
|_: &[u64]| HashMap::new()  // returns empty metadata for all IDs
```

This preserves existing test semantics (category-based lookups still use `entry_category_lookup`;
the meta lookup just returns empty, leaving `cross_feature_reuse = 0`).

**Tests to update** (all existing in `knowledge_reuse.rs` `mod tests`):
- `test_knowledge_reuse_cross_session_query_log`
- `test_knowledge_reuse_cross_session_injection_log`
- `test_knowledge_reuse_single_session_not_cross_session`
- `test_knowledge_reuse_deduplication_across_sources`
- Any other tests in the module calling `compute_knowledge_reuse`

### Construction Site Validation (R-13)

#### Test: `test_feature_knowledge_reuse_construction_compiles`

Not a runtime test. Verified by `cargo build`. The implementation agent must update all three
`FeatureKnowledgeReuse {}` literals to include new fields. If any site is missed, the build
fails. This is the R-13 coverage requirement.

### `total_served` vs `delivery_count`

#### Test: `test_total_served_equals_delivery_count` (AC-12)

**Scenario**: `total_served` counts all distinct entries delivered across sessions.
**Assert**: `total_served == delivery_count` (these are the same metric; `total_served` is the
new field name, `delivery_count` is the existing one — confirm they are set identically).

### `total_stored` Field

Note: `total_stored` is populated by the caller in `tools.rs` (not by `compute_knowledge_reuse`
itself). It represents the count of `feature_entries` rows for this cycle. It is passed as a
parameter or set after the call. The unit test for this lives in `phase-stats.md` (tools.rs).

---

## Integration Test Expectations

infra-001 `test_lifecycle.py`:

- **Test**: `test_cycle_review_knowledge_reuse_cross_feature_split`
  - Store 3 entries with `feature_cycle = "prior-001"`.
  - Switch to a new feature cycle `"col-026-test"`.
  - Serve (search) the prior-001 entries in the new cycle.
  - Call `context_cycle_review(feature_cycle="col-026-test", format="markdown")`.
  - Assert response contains "Cross-feature" and count > 0.
  - Assert response contains "Intra-cycle" label.

See OVERVIEW.md Integration Test 3.

---

## Edge Cases

- Entry ID set of exactly 100: no chunking needed. Verify single batch call.
- Entry ID set of 101: chunking into 2 calls (100 + 1). Verify both chunks unioned correctly.
  This is tested through the production closure in `tools.rs`, not in `knowledge_reuse.rs` unit
  tests (where the closure is synthetic).
- `entry_meta_lookup` returns a `feature_cycle = None` for an entry: treat as intra-cycle or
  cross-feature per spec. Spec must define this; assumed to be cross-feature (unknown origin).
- Duplicate entry IDs in query_log (same entry, same session): deduplicated before calling
  batch lookup. Verify the deduplication step occurs before the closure is called.
