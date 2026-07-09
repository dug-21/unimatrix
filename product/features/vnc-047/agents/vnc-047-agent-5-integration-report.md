# vnc-047 Agent 5 — Integration/Surface vertical (C5, C7, C8, C9)

Agent: `vnc-047-agent-5-integration` | Tracks GH #940 | Wave 2

## Scope delivered
Wired the tags feature end to end: listener persistence routing (C5), RetrospectiveReport.tags
field + SUMMARY v6 cascade (C7), review-handler populate seam (C8), markdown render (C9), plus the
cross-crate verify_integration schema pin fix and two necessary golden/pinned-test cascades.

## SR-02 version re-verification (at implementation start, HEAD)
- `CURRENT_SCHEMA_VERSION` = **31** — already bumped by Wave 1 (migration.rs:26). Not touched here.
- `SUMMARY_SCHEMA_VERSION` = **5** at entry → bumped to **6** here (cascade #2). Neither number was
  claimed by a parallel merge. No renumber needed.

## Files modified
- `crates/unimatrix-observe/src/types.rs` — C7: added `#[serde(default)] pub tags: Vec<String>` after
  `goal`; updated construction sites; added 3 tests.
- `crates/unimatrix-store/src/cycle_review_index.rs` — C7 cascade #2: bumped `SUMMARY_SCHEMA_VERSION`
  5→6; renamed/updated pinned test `test_summary_schema_version_is_6` (vnc-047 message); fixed two
  version-5 round-trip assertions (:1432, :2531) that rode the const.
- `crates/unimatrix-server/src/uds/listener.rs` — C5: step-5 routing (`Start && !tags.is_empty()` →
  `insert_cycle_start_with_tags`, else `insert_cycle_event`), gated on `!feature_cycle.is_empty()`;
  added 13 assembled-path tests.
- `crates/unimatrix-server/src/mcp/tools.rs` — C8: `pub(crate) async fn populate_review_tags`; called
  in cycle_review handler beside the goal populate.
- `crates/unimatrix-server/src/mcp/response/retrospective.rs` — C9: `render_tags_section` (empty →
  "" no section; present → `## Tags` + escaped bullets); call after `render_goal_section`; 5 tests.
- `crates/unimatrix-server/tests/verify_integration.rs` — fixed pinned `test_schema_version_still_30`
  → `_still_31` (cross-crate breakage flagged by the store agent).
- `crates/unimatrix-server/tests/fixtures/vnc-025/cycle_review_render.format_json.json` — re-blessed
  golden baseline (adds `"tags": []`; see cascade note below).

## Construction sites updated for the new required field
23 `RetrospectiveReport { … }` literals: observe/report.rs (1, production), observe/types.rs (8),
observe/phase_narrative.rs (2), server/mcp/tools.rs (10), server/mcp/distill_handler.rs (1),
server/mcp/response/retrospective.rs (1). Most = `tags: Vec::new()`; the sole real populate is C8
(`populate_review_tags` via `get_cycle_tags`). Workspace compiles clean → all sites covered.

## Tests — pass/fail
Full lib suites (hardened run): observe **581 passed / 0 failed**, server **4545 passed / 0 failed /
1 ignored**, store **422 passed / 0 failed**. `verify_integration test_schema_version_still_31` green.
clippy on the three crates EXIT=0 (my code warning-free; 2 pre-existing warnings in verbosity.rs test
code, out of scope). No new failures.

### Gate-critical assembled-path tests written (by name)
AC-02 / AC-02a (C5, in `uds::listener::tests`, driven through `dispatch_request`):
- `test_cycle_start_tags_flow_from_hook_to_cycle_tags` (AC-02 anchor)
- `test_non_start_tags_not_persisted`, `test_duplicate_start_no_dup_no_error`
- `test_whole_set_once_changed_set_noop`, `test_whole_set_once_superset_noop`,
  `test_tagless_start_does_not_lock`
- `test_start_without_tags_routes_to_insert_cycle_event`, `test_malformed_tags_payload_degrades`

AC-05 (assembled read+surface):
- `test_review_surfaces_tags_json_and_markdown_assembled` — drives hook→listener→cycle_tags, then
  REAL `populate_review_tags` (get_cycle_tags) + public `format_retrospective_markdown` + serde_json;
  asserts tags in BOTH markdown (`## Tags`) and JSON. **AC-05 anchor.** Markdown asserted via the
  real formatter call site (SPEC/parity semantics), not the ARCHITECTURE illustrative header.

AC-EXTRA-2 (absent-session + degrade):
- `test_evicted_session_tags_persist` (session NOT registered → #519 pre-register path),
  `test_empty_feature_cycle_no_orphan_rows` (documented drop),
  `test_review_degrades_to_empty_on_getter_error` (closed write_pool → report.tags=[] + warn, no panic)

C7: `test_retrospective_report_tags_roundtrip`, `test_v5_blob_deserializes_tags_default_empty`
(#[serde(default)] backward-read, R-02.4), `test_tags_field_is_not_transient`, updated
`test_summary_schema_version_is_6`.
C9: `test_render_tags_section_present`, `test_render_no_spurious_section_when_empty` (PINNED empty
divergence), `test_render_tags_verbatim_no_derivation`, `test_render_tags_order_deterministic`,
`test_render_tags_escapes_metacharacters`.

## Out-of-scope changes made (necessary cascades, flagged)
1. **vnc-025 golden fixture re-bless** (`cycle_review_render.format_json.json`) — the always-serialized
   `tags` field appends `"tags": []` to the pinned review JSON. This golden gate is designed to be
   re-blessed on intentional output changes (test_support.rs writes the baseline when absent). Diff is
   EXACTLY one line (`,"tags": []`). The markdown fixture is unchanged (tag-less → no section).
2. **Two version-5 round-trip assertions** in cycle_review_index.rs (:1432, :2531) hard-coded `5`;
   updated to `6`. These are in my owned file but are separate from the dedicated pinned test.

Both are direct, spec-mandated consequences of C7 (AC-05d always-serialize + SUMMARY v6), analogous to
the compiler-enforced construction-site fan-out. Reported for validator awareness.

## Deviations from spawn prompt (with rationale)
- The `#[serde(default)]` backward-read test was moved from `cycle_review_index.rs` (store crate) to
  `unimatrix-observe/src/types.rs`. **unimatrix-store cannot reference `unimatrix_observe::Retrospective
  Report`** — unimatrix-observe depends on unimatrix-store, so the reverse is a dependency cycle. The
  test belongs with the type it exercises (report-field.md test-plan lists it under types.rs anyway).
- AC-05 markdown asserted via the public `format_retrospective_markdown` rather than the private
  `render_tags_section` directly (the `retrospective` module is private in `mcp::response`). This is a
  stronger assembled proof — it exercises the real formatter call site. `render_tags_section` was made
  `pub(crate)` (harmless; used within the crate).

## Issues / blockers
None. All gate-critical assembled-path obligations (AC-02, AC-05, AC-EXTRA-2) proven by assembled
tests, not store-only structural tests.

## Untouched (per constraints)
`insert_cycle_event` (15 call sites), `insert_cycle_start_with_tags`/`get_cycle_tags` (Wave 1),
CycleParams struct, context_cycle ack, context_tag seam, Python integration harness (Stage 3c).
No git operations (leader commits).

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` — surfaced ADR-001..007 (#5651-5659) and pattern #5661
  (context_cycle opaque-array field: hook is the persist source, omit payload key when empty). Applied
  #5661's payload-key-omission and value-opacity guidance to the C5 listener routing (degrade to []
  on any non-array shape; route on key presence).
- Stored: entry #5663 "Adding an always-serialized RetrospectiveReport field triggers two invisible
  runtime cascades beyond compiler-flagged construction sites" via context_store (topic
  unimatrix-server, pattern) — the golden-fixture re-bless, the hard-coded version-5 assertions, and
  the store↛observe dependency-direction trap. Novel: invisible at `cargo build`, only bites at
  `cargo test`; not covered by #5661.
