# crt-028 Test Plan: `services/index_briefing.rs`

Component covers two changes (GH #355):
1. Doc comment addition on `pub(crate) async fn index()` — verified by grep
2. Regression test `index_briefing_excludes_quarantined_entry` — verified by unit test

No logic changes to `IndexBriefingService::index()` itself. The `se.entry.status == Status::Active`
post-filter at step 5 already exists. The test verifies it cannot be silently removed.

Existing test infrastructure in `index_briefing.rs`: uses `SessionState` builder `make_session_state`
from the existing `#[cfg(test)] mod tests` block. New test follows the same `#[tokio::test]` pattern.

---

## AC-13: Doc Comment Verification (grep)

### `index_briefing_index_has_delegation_doc_comment`

**Verification method**: grep (not a runtime test)

```bash
grep -A 5 "pub.*async fn index" crates/unimatrix-server/src/services/index_briefing.rs
```

Assertions:
- Output contains `"delegated to"` (or `"delegates to"`)
- Output contains `"validate_search_query"`
- Output contains `"SearchService"`

This verifies AC-13: the doc comment documents that query validation is delegated to
`SearchService.search()` → `gateway.validate_search_query()`, serving as a guard comment
to prevent future removal of the `SearchService` delegation without realizing validation disappears.

**Rationale**: AC-13 — GH #355 explicitly requires this documentation guard on `index()`.

---

## AC-12: Quarantine Exclusion Regression Test (R-08 — non-negotiable gate)

### `index_briefing_excludes_quarantined_entry` (AC-12, R-08)

**Type**: `#[tokio::test]`
**Location**: `#[cfg(test)] mod tests` block in `services/index_briefing.rs`

Setup:
1. Create a real in-memory `Store` (same pattern as existing `index_briefing.rs` tests)
2. Create an `IndexBriefingService` with the real store + real `SearchService` against test DB
3. Store an entry with content matching a predictable query (e.g., "unimatrix quarantine test entry")
   and `status: Quarantined` — use the store's direct write path, not a mock

Test steps:
1. Call `index_briefing_service.index(params, &audit_ctx, None).await`
   where `params.query` matches the quarantined entry's content
2. Assert: the quarantined entry's ID is absent from the returned `Vec<IndexEntry>`
3. Store a second entry with matching content but `status: Active`
4. Call `index()` again
5. Assert: the Active entry IS present in results; the Quarantined entry is still absent

Assertions (explicit):
```rust
// The quarantined entry must not appear
assert!(
    result.iter().all(|e| e.id != quarantined_id),
    "index() must not return quarantined entries (GH #355, FR-08, AC-12)"
);

// The active entry must appear
assert!(
    result.iter().any(|e| e.id == active_id),
    "index() must return active entries"
);
```

**Why real store, not mock**: The post-filter is in the `index()` method body. If we mock
`SearchService` to return only an Active entry, we don't test that the filter actually removes
a Quarantined entry returned by the search layer. The test must exercise the filter removal risk:
if the `filter(|se| se.entry.status == Status::Active)` line is deleted, the quarantined entry
appears in results and this test fails.

**Rationale**: R-08 — T-BS-08 was deleted with `BriefingService`. This test mirrors it. GH #355
gap. The post-filter must be exercised directly.

---

## R-08: Filter Removal Detection (design verification)

The test `index_briefing_excludes_quarantined_entry` is designed so that removing the post-filter
line in `index()` causes the test to fail. Verify this property during implementation:

1. Temporarily remove the `filter(|se| se.entry.status == Status::Active)` line from `index()`
2. Run `cargo test -p unimatrix-server index_briefing_excludes_quarantined_entry`
3. Assert: test FAILS (the quarantined entry now appears in results)
4. Restore the filter line

This "mutation test" confirms the test actually guards the filter — not just a vacuous assertion.

**Rationale**: R-08 explicitly calls this out: "Remove the `status == Active` post-filter from
`index()` and verify the test fails — the test must be designed to catch filter removal."

---

## Existing Tests (AC-14 non-regression)

All existing `index_briefing.rs` tests must continue to pass:
- `derive_briefing_query_task_param_takes_priority`
- `derive_briefing_query_session_signals_step_2`
- `derive_briefing_query_no_session_fallback_to_topic`
- `extract_top_topic_signals_*` suite

Verify with: `cargo test -p unimatrix-server -- index_briefing::tests 2>&1 | tail -30`

The doc comment addition and new regression test are purely additive — no existing logic is modified.
