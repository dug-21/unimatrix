# Risk Coverage Report: vnc-047

> `context_cycle` whole-set-once opaque run-identity `tags` → `cycle_tags` junction → surfaced in
> `context_cycle_review`. Tracks GH #940. Stage 3c execution on branch `feature/vnc-047`
> (waves 1+2 committed, Gate 3b PASS). Executed 2026-07-09.

## SR-02 / AC-EXTRA-4 Version Re-Verification (recorded at Stage 3c start)

Design synthesis confirmed both numbers free at HEAD (v31 / SUMMARY v6) on 2026-07-09. At Stage 3c
the vnc-047 bumps are committed, so HEAD now reads:

- `CURRENT_SCHEMA_VERSION = 31` (`crates/unimatrix-store/src/migration.rs:26`) — the vnc-047 bump.
- `SUMMARY_SCHEMA_VERSION = 6` (`crates/unimatrix-store/src/cycle_review_index.rs:58`) — the vnc-047 bump.

No collision: exactly one v31 migration exists (`tests/migration_v30_to_v31.rs` + the single
`if current_version < 31` step in `migration.rs`); the SUMMARY v6 bump has a single owning commit
(4a57aba5). No parallel feature claimed either number. **No renumber required.**

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | `CURRENT_SCHEMA_VERSION` 30→31 cascade incomplete | `test_current_schema_version_is_at_least_31`, `test_schema_version_is_31` (sqlite_parity), `test_fresh_db_creates_cycle_tags_table`, `test_migration_v30_to_v31_creates_cycle_tags`, `test_fresh_create_and_migration_schemas_identical` | PASS | Full |
| R-02 | `SUMMARY_SCHEMA_VERSION` 5→6 cascade incomplete | `test_summary_schema_version_is_6`, `test_retrospective_report_tags_roundtrip`, `test_v5_blob_deserializes_tags_default_empty`, `test_tags_field_is_not_transient` | PASS | Full |
| R-03 | Assembled path proven only by store-only tests | **`test_cycle_start_tags_flow_from_hook_to_cycle_tags`** (hook→listener→store), **`test_review_surfaces_tags_json_and_markdown_assembled`** (populate_review_tags→render) | PASS | Full (assembled) |
| R-04 | Tags silently dropped on absent/evicted session | **`test_evicted_session_tags_persist`**, `test_empty_feature_cycle_no_orphan_rows`, `test_review_degrades_to_empty_on_getter_error` | PASS | Full (assembled) |
| R-05 | Transaction not atomic (start row + guard + tag rows) | `test_start_row_and_tag_rows_share_commit`, `test_dup_tag_in_set_no_txn_abort`, `test_first_call_inserts_full_set` | PASS | Full |
| R-06 | Second persistence route regression | `test_bare_mcp_cycle_tags_not_persisted` (Python), single-writer grep (`cycle_tags` INSERT only in `insert_cycle_start_with_tags`) | PASS | Full |
| R-07 | GC purges `cycle_tags` | `test_gc_protected_tables_regression` (extended: 4 seeded rows survive BOTH `gc_cycle_activity` + `gc_unattributed_activity`; positive controls = 3 sessions purged), `test_gc_protected_tables_row_level` | PASS | Full |
| R-08 | Whole-set-once reads as data loss | `test_whole_set_once_changed_set_is_noop`, `test_whole_set_once_subset_and_superset_noop`, `test_whole_set_once_single_then_single`, `test_tagless_call_does_not_lock` (store, EXACT equality); assembled `test_whole_set_once_changed_set_noop`, `test_whole_set_once_superset_noop`, `test_tagless_start_does_not_lock` | PASS | Full |
| R-09 | 15 untouched `insert_cycle_event` call sites regress | `test_cycle_start_goal_flows_from_hook_payload_to_db` (goal path unchanged), `test_subagent_start_goal_absent_uses_existing_path`, `test_no_embed_task_on_absent_goal`; `insert_cycle_event` 8-arg signature unchanged (workspace compiles + 6174 tests green) | PASS | Full |
| R-10 | Version-number collision at merge | SR-02 re-verification above (no collision) | PASS | Full |
| R-11 | Empty-tag / opacity leak | `test_empty_string_tag_rejected_others_stored`, `test_colon_and_bare_stored_identically_no_branching`, `test_large_and_unicode_and_whitespace_tag_stored_verbatim`, `test_get_cycle_tags_verbatim` | PASS | Full |
| R-12 | Markdown render divergence | `test_render_tags_section_present`, `test_render_no_spurious_section_when_empty`, `test_render_tags_verbatim_no_derivation`, `test_render_tags_order_deterministic` | PASS | Full |
| R-13 | Migration not idempotent / not old-DB-safe | `test_migration_v30_to_v31_idempotent`, `test_migration_from_populated_v30_data_intact`, `test_migration_with_stray_cycle_tags_no_error` | PASS | Full |
| R-14 | No back-fill misread as regression | `test_v5_blob_deserializes_tags_default_empty` (v5 read → empty), documented in ADR-004 | PASS | Full |
| R-15 | Whole-set EXISTS-guard TOCTOU (concurrency) | `test_concurrent_same_cycle_starts_one_whole_set` (exactly one intact whole set, no merge); `BEGIN IMMEDIATE` verified in `db.rs:409` (`sqlx::query("BEGIN IMMEDIATE")`) | PASS | Full |
| R-16 | Best-effort ack echo drift (NON-GATING) | `test_ack_start_with_tags_accept_for_recording`, `test_ack_non_start_with_tags_ignored_note`, `test_ack_no_tags_unchanged`, `test_context_cycle_ack_echoes_tags` (Python); listener wrote-set/frozen-skip trace present | PASS (non-gating) | Full |

## Assembled-Path Proof Citations (R-03 / SR-08 — gate-critical)

Both `[assembled-path]` ACs are proven by tests that drive the **real production wiring**, not
store-only proxies or hand-built literals:

- **AC-02 (persist via hook path):** `uds::listener::tests::test_cycle_start_tags_flow_from_hook_to_cycle_tags`
  — fires a `RecordEvent` cycle_start through `handle_cycle_event` (the real hook→listener→store
  seam via the established col-025/GH#389 `make_*` helpers), settles the fire-and-forget spawn, then
  reads back with the real `store.get_cycle_tags(fc)`. Not a direct `insert_cycle_start_with_tags`
  call.
- **AC-05 (surface in review):** `uds::listener::tests::test_review_surfaces_tags_json_and_markdown_assembled`
  — drives the extracted `pub(crate) populate_review_tags(&store, fc, &mut report)` seam (which calls
  the real `get_cycle_tags`, reading from `cycle_tags` as source of truth), then
  `format_retrospective_markdown(&report)`, asserting both the `## Tags` markdown section AND the
  serialized JSON `"tags"` carry the stored values. It reads the DB, not a literal it wrote into the
  report by hand.
- **AC-EXTRA-2 (absent/evicted session):** `test_evicted_session_tags_persist` +
  `test_empty_feature_cycle_no_orphan_rows` + `test_review_degrades_to_empty_on_getter_error` — all
  driven through the assembled listener seam (#519 pre-register exercised, not asserted by inspection).

## Test Results

### Unit / Lib / Rust Integration Tests
Command: `cargo test -p unimatrix-observe -p unimatrix-store -p unimatrix-server` (rc=0).

- Total: 6177 (6174 passed, 0 failed, 3 ignored) across lib + all integration-test binaries in the
  three feature crates.
- Passed: 6174
- Failed: 0
- vnc-047-specific tests (representative): 16 in `tests/cycle_tags.rs`, 8 in
  `tests/migration_v30_to_v31.rs`, `test_schema_version_is_31` (sqlite_parity), 5 in
  `mcp::response::retrospective` render tests, 3 in `types.rs` (roundtrip / v5-blob / not-transient),
  cycle-params + ack tests in `mcp::tools`, hook-extraction tests in `uds::hook`, and the assembled
  tests in `uds::listener` cited above — all green.

Two schema cascades verified as **discrete** per-path assertions (not one lumped bump):
- v31 (DB migration): constant + fresh-create + migration-from-v30 + idempotent-rerun +
  populated-v30-data-intact + fresh-vs-migration-DDL-parity + no-FK, all asserted separately.
- SUMMARY v6 (fidelity stamp): constant `test_summary_schema_version_is_6` (pinned) + populated-tags
  round-trip + `#[serde(default)]` v5-blob backward-read (mandatory) all asserted separately.

GC regression is **non-vacuous**: seeds 4 `cycle_tags` rows, runs both `gc_cycle_activity` and
`gc_unattributed_activity`, asserts `cycle_tags` count unchanged, WITH positive controls proving 3
sessions ARE purged (2 via cycle-activity, 1 via unattributed).

### Full-Workspace Link Smoke (#878 guard — MANDATORY)
Command: `bash product/test/infra-002/check-workspace-link-smoke.sh` (rc=0). Profile presence OK;
full-workspace `--no-run` link completed at configured parallelism. #878 invariant holds.

### Integration Tests (infra-001 harness, `target/release/unimatrix`)

| Run | Passed | xfail | xpass | Failed | Notes |
|-----|--------|-------|-------|--------|-------|
| smoke (MANDATORY gate) | 35 | 0 | 0 | 0 | `pytest -m smoke` — PASS |
| protocol | 14 | 0 | 0 | 0 | tool-discovery / handshake stable |
| security | 26 | 0 | 0 | 0 | capability enforcement intact |
| tools (228 total) | 226 | 2 | 0 | 0 | 1 pre-existing harness xfail + GH#942 xfail |
| lifecycle (109 total) | 102 | 6 | 1 | 0 | pre-existing xfails/xpass, unrelated to vnc-047 |

- **Total integration suite tests run:** 368 passed, 8 xfailed, 1 xpassed, **0 hard failures**
  (smoke's 35 overlap the suites as a marker subset).
- **New Python integration tests added (Stage 3c, per OVERVIEW §5):**
  - `suites/test_tools.py::test_context_cycle_accepts_tags_param` — additive `tags` param accepted by
    the bare handler (interface stable, AC-06). **PASS.**
  - `suites/test_lifecycle.py::test_bare_mcp_cycle_tags_not_persisted` — bare MCP start-with-tags then
    review surfaces NO tags; the no-second-route proof from the MCP boundary (AC-EXTRA-1). **PASS.**
    (Assertion corrected during 3c: the bare handler persists nothing at all, so review returns
    `-32010 "No observation data found"` for the cycle — that error itself proves no persistence
    occurred; the test accepts either that error or a tag-less success, and fails only if tag strings
    appear. Documented under Triage below.)
  - `suites/test_tools.py::test_context_cycle_ack_echoes_tags` — best-effort ack echo (AC-09,
    NON-GATING). **PASS.**

## Integration Failure Triage

1. **`test_context_edge_tool_registered` (test_tools.py) — PRE-EXISTING, unrelated to vnc-047.**
   Asserts `len(tools) == 14`; server now advertises **15** (the 15th is `context_tag`, added by
   vnc-045 #929, a base commit). vnc-047 adds NO new tool (only the additive `tags` param), so this
   is stale-assertion drift from vnc-045. **Filed GH#942**, marked
   `@pytest.mark.xfail(reason="Pre-existing: GH#942 …")`. Confirmed reports `XFAIL` after marking. Not
   fixed in this PR (would be scope creep per USAGE-PROTOCOL).

2. **`test_bare_mcp_cycle_tags_not_persisted` (new) — bad initial assertion, fixed in this PR.** My
   first draft expected a successful review with empty tags; the bare handler persists nothing at all,
   so review errors `-32010`. Corrected the test to treat that error (or a tag-less success) as the
   pass condition (harness code is a legitimate fix target per triage tree). Now PASS.

3. All other suites: no failures.

The KNOWN pre-existing flake `eval::runner::sweep_tests::test_ac14_correlated_sweep_non_vacuous`
(green in isolation, fails only under full-parallel `--workspace` load; lives in eval/sweep, touches
no vnc-047 code) did NOT surface — the Stage 3c unit run was scoped to the three feature crates
per-crate, which avoids that cross-crate parallel condition. No action needed; unrelated to vnc-047.

## Gaps

None. Every risk R-01…R-16 has test coverage at the required tier (assembled where mandated). The
five gate-critical coverage obligations are all satisfied:
1. AC-02 + AC-05 assembled-path tests cite the real hook→listener→store and populate→render chains. ✓
2. Two independent version cascades, each discrete per-path + pinned test; SUMMARY v6 has the
   `#[serde(default)]` v5-blob backward-read. ✓
3. `test_gc_protected_tables_regression` extended across BOTH GC surfaces with a positive control. ✓
4. Absent/evicted-session persistence exercised on the assembled path. ✓
5. Whole-set-once by EXACT stored-set equality (changed/subset/superset/different + tagless) plus
   `BEGIN IMMEDIATE` verified and a concurrent same-cycle-start test (one intact whole set, no merge). ✓

The non-gating C12 (ack echo) and C13 (listener freeze-outcome trace) are both implemented and
verified (see R-16); a miss there would not have blocked delivery.

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `test_empty_string_tag_rejected_others_stored`, `test_large_and_unicode_and_whitespace_tag_stored_verbatim`, `test_colon_and_bare_stored_identically_no_branching` |
| AC-02 | PASS | **assembled** `test_cycle_start_tags_flow_from_hook_to_cycle_tags`; non-start ignored + duplicate-start no-dup covered in listener + `test_dup_tag_in_set_no_txn_abort` |
| AC-02a | PASS | `test_whole_set_once_changed_set_noop`, `test_whole_set_once_superset_noop`, `test_tagless_start_does_not_lock` (assembled) + store EXACT-equality set |
| AC-02b | PASS | `BEGIN IMMEDIATE` at `db.rs:409`; `test_concurrent_same_cycle_starts_one_whole_set` |
| AC-03 / a–d | PASS | `test_current_schema_version_is_at_least_31`, `test_fresh_db_creates_cycle_tags_table`, `test_migration_v30_to_v31_creates_cycle_tags`, `test_migration_v30_to_v31_idempotent`, `test_migration_from_populated_v30_data_intact`, `test_fresh_create_and_migration_schemas_identical`, `test_schema_version_is_31` |
| AC-04 | PASS | `test_gc_protected_tables_regression` (both surfaces + `sessions` positive control) |
| AC-05 / a–d | PASS | `test_summary_schema_version_is_6`, `test_retrospective_report_tags_roundtrip`, `test_v5_blob_deserializes_tags_default_empty`, **assembled** `test_review_surfaces_tags_json_and_markdown_assembled`, `test_render_tags_section_present`, `test_render_no_spurious_section_when_empty` |
| AC-06 | PASS | `test_cycle_params_tags_optional_deserializes`, `test_cycle_params_tags_null`; no-new-tool confirmed (GH#942 enumerates all 15 tools, `context_cycle` single/additive); Write-cap via security suite; `context_tag`/`context_correct` diff-clean |
| AC-07 | PASS | `test_colon_and_bare_stored_identically_no_branching`, `test_render_tags_verbatim_no_derivation` |
| AC-08 | PASS (doc) | `test_v5_blob_deserializes_tags_default_empty` confirms v5 read → empty; documented no-back-fill (ADR-004) |
| AC-09 | PASS (NON-GATING) | `test_ack_start_with_tags_accept_for_recording`, `test_ack_non_start_with_tags_ignored_note`, `test_context_cycle_ack_echoes_tags` |
| AC-EXTRA-1 | PASS | `test_bare_mcp_cycle_tags_not_persisted` (Python) + single-writer grep |
| AC-EXTRA-2 | PASS | `test_evicted_session_tags_persist`, `test_empty_feature_cycle_no_orphan_rows`, `test_review_degrades_to_empty_on_getter_error` |
| AC-EXTRA-3 | PASS | `test_start_row_and_tag_rows_share_commit`, `test_dup_tag_in_set_no_txn_abort`, `test_tag_write_uses_parameterized_binds` |
| AC-EXTRA-4 | PASS | SR-02 re-verification recorded above |

## GH Issues Filed

- **GH#942** — `[infra-001] test_context_edge_tool_registered: hardcoded tool count 14 stale after
  vnc-045 added context_tag (15th tool)`. Pre-existing, unrelated to vnc-047. Test marked `xfail`
  referencing this issue.
