# Agent Report: crt-031-agent-6-status

## Task
Wire `Arc<CategoryAllowlist>` as a new field on `StatusService`, update all 4 `StatusService::new()` construction sites (R-02), add `category_lifecycle: Vec<(String, String)>` to `StatusReport`, populate in `compute_report()`, and update both formatters.

## R-02 Site Enumeration (Pre-Implementation)

All 4 sites confirmed before any code change:
1. `services/mod.rs:461` — `ServiceLayer::new()` → `with_rate_config()`
2. `background.rs:446` — `run_single_tick`
3. `services/status.rs:1886` — test helper `make_status_service()`
4. `services/status.rs:2038` — test helper `make_status_service_with_index()`

Site 2 (`run_single_tick`) already had `category_allowlist: &Arc<CategoryAllowlist>` parameter added by the background agent. Only needed to pass `Arc::clone(category_allowlist)` to `StatusService::new()` — operator-loaded Arc, not `CategoryAllowlist::new()` inline.

## Files Modified

- `crates/unimatrix-server/src/mcp/response/status.rs` — `category_lifecycle` field on `StatusReport`, Default, summary formatter (adaptive-only), JSON formatter (all categories via BTreeMap), 6 new crt-031 tests
- `crates/unimatrix-server/src/services/status.rs` — `category_allowlist` field on `StatusService`, updated `new()` (both test helpers + struct init), `compute_report()` populates with alphabetic sort, 2 new crt-031 async tests
- `crates/unimatrix-server/src/services/mod.rs` — `category_allowlist: Arc<CategoryAllowlist>` param on `new()` and `with_rate_config()`, forwarded to `StatusService::new()`
- `crates/unimatrix-server/src/background.rs` — Site 2 `StatusService::new()` updated to pass `Arc::clone(category_allowlist)`
- `crates/unimatrix-server/src/infra/shutdown.rs` — 2 `ServiceLayer::new()` test sites updated
- `crates/unimatrix-server/src/server.rs` — `ServiceLayer::new()` test site updated
- `crates/unimatrix-server/src/test_support.rs` — `ServiceLayer::with_rate_config()` test site updated
- `crates/unimatrix-server/src/services/index_briefing.rs` — `ServiceLayer::new()` test site updated
- `crates/unimatrix-server/src/uds/listener.rs` — `ServiceLayer::new()` test site updated
- `crates/unimatrix-server/src/eval/profile/layer.rs` — `ServiceLayer::with_rate_config()` updated with eval-profile lifecycle policy
- `crates/unimatrix-server/src/mcp/response/mod.rs` — 8 `StatusReport` struct initializers in tests updated with `category_lifecycle: Vec::new()`

## Tests: 92 passed / 0 failed

New tests added:
- `mcp::response::status::tests::test_status_report_default_category_lifecycle_is_empty` (I-02)
- `mcp::response::status::tests::test_status_report_summary_lists_only_adaptive` (AC-09 summary)
- `mcp::response::status::tests::test_status_report_summary_no_adaptive_section_when_empty` (E-01)
- `mcp::response::status::tests::test_category_lifecycle_json_sorted_and_deterministic` (R-08, I-03)
- `mcp::response::status::tests::test_status_report_json_includes_all_categories` (AC-09 JSON)
- `mcp::response::status::tests::test_category_lifecycle_alphabetic_sort_golden` (R-08)
- `services::status::tests_crt031::test_status_service_compute_report_has_lifecycle` (R-02/3, AC-09)
- `services::status::tests_crt031::test_status_service_compute_report_sorted_lifecycle` (R-08)

Full workspace: 0 failures.

## Issues / Blockers

None. Background agent had already added `category_allowlist: &Arc<CategoryAllowlist>` to `run_single_tick` — required no coordination, just adding the `Arc::clone(category_allowlist)` argument to `StatusService::new()` at that site.

Additional `ServiceLayer::new()` and `with_rate_config()` call sites beyond the 4 documented R-02 sites were found in test helpers across `shutdown.rs`, `server.rs`, `test_support.rs`, `uds/listener.rs`, `index_briefing.rs`, and `eval/profile/layer.rs` — all updated with `Arc::new(CategoryAllowlist::new())` defaults (tests) or profile-derived allowlist (eval harness).

`mcp/response/mod.rs` had 8 `StatusReport` struct initializers in integration tests that needed `category_lifecycle: Vec::new()` added.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — surfaced #3213 (run_single_tick also constructs services directly) and #2553 (constructor signature changes cascade to background.rs test helpers). Both directly applicable — confirmed 4-site enumeration was correct and background agent coordination needed.
- Stored: superseded entry #3603 → #3780 "StatusReport struct literal locations — four files require updates when adding fields (8+ sites in mod.rs alone)" via context_correct. Updated count from three to four files; added grep pre-check advice for `StatusReport {` across crates/.
