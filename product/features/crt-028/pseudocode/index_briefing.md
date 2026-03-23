# crt-028: index_briefing.rs Pseudocode — GH #355 Quarantine Test + Doc Comment

## Purpose

Fix GH #355: `IndexBriefingService::index()` lacks (a) a doc comment documenting where
input validation lives, and (b) a regression test verifying the `status == Active`
post-filter excludes quarantined entries. The deleted `BriefingService` had T-BS-08 for
this; the post-filter exists in `index()` but has no test coverage.

---

## File: `crates/unimatrix-server/src/services/index_briefing.rs`

---

## Change 1: Doc Comment on `index()`

Add the following doc comment immediately above the `pub(crate) async fn index` declaration
(currently at line 129 in the existing file). The existing `///` comment block above `index`
ends with the line `/// Returns Ok(vec![]) on no results (R-10, AC-18).`

Append to the existing doc comment:

```
/// Input validation is delegated to `SearchService.search()` which calls
/// `self.gateway.validate_search_query()`. Guards enforced:
///   - Query content (S3)
///   - Length ≤ 10,000 chars
///   - Control characters rejected
///   - k bounds enforced
///
/// WARNING: Do not remove the `SearchService` delegation or replace it with
/// a direct store call without adding an equivalent `validate_search_query()`
/// call here. Removing the delegation silently removes all input validation
/// (GH #355, ADR documented in crt-028).
```

The merged comment block on `index()` will be:

```
/// Query the knowledge index and return a ranked, active-only result set.
///
/// Steps:
/// 1. Determine effective k (params.k; clamp 0 → default_k per EC-03 guard)
/// 2. Delegate to `SearchService.search()` with `RetrievalMode::Strict`
/// 3. Post-filter to `Status::Active` only (defensive — Strict mode guarantees
///    this but we filter explicitly per the spec)
/// 4. Map each `ScoredEntry` to `IndexEntry` (snippet = first SNIPPET_CHARS chars)
/// 5. Sort by fused score descending
/// 6. Truncate to effective k
///
/// Returns `Ok(vec![])` on no results (R-10, AC-18).
///
/// Input validation is delegated to `SearchService.search()` which calls
/// `self.gateway.validate_search_query()`. Guards enforced:
///   - Query content (S3)
///   - Length ≤ 10,000 chars
///   - Control characters rejected
///   - k bounds enforced
///
/// WARNING: Do not remove the `SearchService` delegation or replace it with
/// a direct store call without adding an equivalent `validate_search_query()`
/// call here. Removing the delegation silently removes all input validation
/// (GH #355, ADR documented in crt-028).
pub(crate) async fn index(...)
```

---

## Change 2: Regression Test `index_briefing_excludes_quarantined_entry`

Add inside the existing `#[cfg(test)] mod tests` block at the bottom of
`index_briefing.rs`. The test follows the same pattern as the existing
`derive_briefing_query_*` tests.

```
/// GH #355: Regression — quarantined entries must not appear in index() results.
///
/// Mirrors the deleted T-BS-08 test from BriefingService. Verifies that the
/// `se.entry.status == Status::Active` post-filter in step 3 of index() is
/// present and effective.
///
/// If this test is deleted or the post-filter is removed, quarantined entries
/// will appear in compaction briefings (R-08, AC-12, FR-08.1).
#[tokio::test]
async fn index_briefing_excludes_quarantined_entry() {
    // 1. Build test infrastructure (same pattern as existing integration tests
    //    in listener.rs — real in-memory store, real SearchService)
    let store    = build_test_store().await;     // helper in test module
    let embed    = build_test_embed_service();   // helper in test module
    let vs       = build_test_vector_store();    // helper in test module
    let gateway  = build_test_gateway();         // helper in test module
    let eff      = build_test_effectiveness();   // helper in test module

    // 2. Store a quarantined entry with content that would match a query
    let quarantined_id = store.insert(EntryPayload {
        content:       "quarantined knowledge that must not appear".to_string(),
        topic:         "test-quarantine".to_string(),
        category:      "decision".to_string(),
        tags:          vec!["crt-028".to_string()],
        source:        "test".to_string(),
        status:        Status::Quarantined,    // <- this is what we are testing
        created_by:    "test".to_string(),
        feature_cycle: "crt-028".to_string(),
        trust_source:  "system".to_string(),
    }).await.expect("insert quarantined entry");

    // 3. Store a second active entry to confirm the active path is not broken
    let active_id = store.insert(EntryPayload {
        content:       "active knowledge that should appear".to_string(),
        topic:         "test-active".to_string(),
        category:      "decision".to_string(),
        tags:          vec!["crt-028".to_string()],
        source:        "test".to_string(),
        status:        Status::Active,
        created_by:    "test".to_string(),
        feature_cycle: "crt-028".to_string(),
        trust_source:  "system".to_string(),
    }).await.expect("insert active entry");

    // 4. Build IndexBriefingService with real service dependencies
    let search  = SearchService::new(Arc::clone(&store), embed, vs, Arc::clone(&gateway));
    let service = IndexBriefingService::new(
        Arc::clone(&store),
        search,
        gateway,
        eff,
    );

    // 5. Call index() with a query that would match both entries
    let params = IndexBriefingParams {
        query:              "knowledge appear".to_string(),
        k:                  20,
        session_id:         None,
        max_tokens:         None,
        category_histogram: None,
    };
    let audit_ctx = AuditContext {
        source: AuditSource::Uds {
            uid: 0,
            pid: None,
            session_id: String::new(),
        },
    };

    let results = service
        .index(params, &audit_ctx, None)
        .await
        .expect("index() must not error");

    // 6. Assert quarantined entry is absent from results (AC-12, FR-08.1/08.2)
    let result_ids: Vec<u64> = results.iter().map(|e| e.id).collect();
    assert!(
        !result_ids.contains(&quarantined_id),
        "quarantined entry must not appear in index() results (GH #355)"
    );

    // 7. Assert active entry IS present (non-quarantine path not broken)
    assert!(
        result_ids.contains(&active_id),
        "active entry must appear in index() results"
    );
}
```

### Notes on test helper infrastructure

The test uses helpers `build_test_store`, `build_test_embed_service`,
`build_test_vector_store`, `build_test_gateway`, and `build_test_effectiveness`.
These are the same helpers used in `listener.rs` integration tests
(approximately `tests::build_test_store` and similar). The tester must either:

- Reuse existing helpers from `listener.rs` tests if they are visible from
  `index_briefing.rs` tests (same crate, different module), or
- Add equivalent helpers local to the `index_briefing.rs` test module.

The test MUST use the real `Store` path to insert the quarantined entry — not a
mock. The post-filter operates on real `ScoredEntry.entry.status` values returned
by `SearchService`. Mocking the store would bypass the filter under test.

---

## Error Handling

No new error-handling logic. This change is:
1. A doc comment (no runtime effect)
2. A test that asserts existing behavior

The `index()` function's own error handling is unchanged.

---

## Key Test Scenarios

### R-08 (High): Quarantine post-filter

1. Quarantined entry: assert absent from `index()` results (primary AC-12 check)
2. Active entry alongside quarantined: assert active appears (AC-12 non-regression)
3. Structural check: verify the `se.entry.status == Status::Active` line exists in
   `index()` by confirming the test fails when that line is removed — the test
   design must be sensitive to filter removal (R-08 design requirement)
