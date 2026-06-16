# Risk Coverage Report: crt-054

**Date**: 2026-06-16 — Stage 3c (test execution).
**Branch**: `feature/crt-054`. **Schema version**: 29 (crt-054 claimed the 28→29 bump; crt-055 reconciles to 30 at merge per R-04 SM gate, #4095).
**Inputs**: `RISK-TEST-STRATEGY.md` (R-01..R-15), `ACCEPTANCE-MAP.md` (AC-01..AC-16), `test-plan/*.md`, `testing/CALIBRATION.md` (AC-10a), `product/test/infra-001/USAGE-PROTOCOL.md`.

> crt-054 is the producer half (Surface A durable `compaction_events` table; Surface B in-memory `activity_snapshot()` fold). The believable-zero family (R-01/AC-06 routing, R-02/AC-07 sequencing) was exercised as held-route drain→hold→re-adopt **integration** tests on the cumulative crt-052 Wave B fixtures, each with a **negative-mutation check that was confirmed to bite RED** (§Believable-Zero Guard Evidence). A registered-only / unit-only test does NOT satisfy AC-06/AC-07 (pattern #3624); these do not.

---

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | Held-route believable-zero (Critical) | `activity::test_held_route_fold_nonempty_at_review`, `activity::test_held_route_fold_continuity_across_drain` (+ neg-mutation confirmed), `activity::test_collector_includes_declared_held_excludes_undeclared` (integration) | PASS | Full |
| R-02 | Read-after-purge / fold dropped (Critical) | `activity::test_read_before_purge_ordering`, `activity::test_snapshot_survives_drain_hold_review` (integration); `test_clear_preserves_activity_accumulator`, `test_activity_fold_continues_after_clear` (unit) | PASS | Full |
| R-03 | Lock-graph deadlock/stall at INSERT seam (Critical) | `compaction_events::test_compaction_writes_one_row`, `..._high_water_equals_buffer_high_water`, `..._insert_failure_non_blocking_no_row` (integration through `handle_compact_payload`); ADR-007 lock-ordering review (documented) | PASS | Full (within OSS in-crate seam) |
| R-04 | Schema-version sequencing collision (Critical) | `migration_v28_to_v29::test_migration_v28_to_v29_adds_compaction_events`, `..._idempotent`, `..._columns_match_contract`, `test_current_schema_version_is_at_least_29`; infra-001 `test_compaction_events_table_survives_restart` | PASS | Full |
| R-05 | Producer-contract drift | `session_transcript_tests_snapshot::test_activity_snapshot_shape_matches_contract`, `..._is_copy`; `transcript_signals_config_tests::test_class_index_mapping_stable`, `test_max_signal_classes_is_16` | PASS | Full |
| R-06 | Single-event/per-turn-drain dependence | Transitive via R-01 held-route coverage; structural (no `PreToolUse`/single-hook read on either surface — confirmed by diff grep) | PASS | Full |
| R-07 | Producer→consumer width truncation | `transcript_activity_tests::test_bytes_total_holds_large_value_un_narrowed`, `test_fold_saturates_rather_than_panics`; producer cast-free diff grep (no `as i64`/`as i32` on snapshot path or collector) | PASS | Full (producer half; saturating →i64 is crt-055's) |
| R-08 | Late-bind attribution fabricates a zero | `activity::test_collector_includes_declared_held_excludes_undeclared`; `compaction_events::test_compaction_row_written_for_undeclared_session` | PASS | Full |
| R-09 | Surface A row under buffer lock | `compaction_events::test_high_water_equals_buffer_high_water` (+ ADR-007 guard-dropped-before-INSERT review) | PASS | Full |
| R-10 | `[transcript_signals]` silent fallback | `transcript_signals_config_tests::test_config_over_cap_rejected`, `test_config_invalid_regex_rejected`, `test_config_duplicate_class_name_rejected`, `test_default_config_error_refusal_only` | PASS | Full |
| R-11 | `compacted_at` granularity / cross-gate | `compaction_events::test_compacted_at_is_seconds_within_tolerance`, `test_second_compaction_adds_monotonic_row` (AC-16 producer half, integration); AC-01a "Unix SECONDS" DDL comment present both paths | PASS | Full (producer half; gate-side ts/1000 = crt-055) |
| R-12 | Stale-scope residue re-imported | `transcript_signals_config_tests::test_default_catalog_no_reread_or_compaction_class`, `test_default_catalog_no_sdlc_literals`; diff grep: no `cycle_review_index`/`SUMMARY_SCHEMA_VERSION` ALTER/bump, no `token_*` symbol | PASS | Full |
| R-13 | `high_water` over-trusted | `compaction_events::test_high_water_equals_buffer_high_water`, `test_compaction_row_for_absent_session_high_water_zero`; reserved semantics documented (ADR-007) | PASS | Full |
| R-14 | Multi-compaction row semantics | `compaction_events::test_second_compaction_adds_monotonic_row` (two rows, insert-only) | PASS | Full |
| R-15 | INSERT-failure silent undercount (High) | `compaction_events::test_insert_failure_increments_named_counter` (named counter +1, fault-injected), `test_insert_failure_counter_is_content_free`, `test_insert_failure_non_blocking_no_row` | PASS | Full |

Every R-01..R-15 has ≥1 passing test. The four Critical risks (R-01/R-02 believable-zero family, R-03 lock seam, R-04 schema version) all carry the held-route / contention / migration integration coverage the strategy mandates.

---

## Believable-Zero Guard Evidence (R-01/R-02, AC-06/AC-07) — Negative-Mutation Confirmed

The AC-06/AC-07 guards are NON-NEGOTIABLY integration tests driving the real `SessionRegistry` + crt-052 Wave B `TranscriptHold` through production entry points (register=readopt, `apply_transcript_delta`=route-to-held, drain=`hold_on_drain`), reading at review via the held-aware collector `activity_snapshots_for_feature` BEFORE purge. New file: `crates/unimatrix-server/src/infra/transcript_hold_activity_tests.rs` (child module of `transcript_hold.rs::tests`, reuses the `ac11` fixture pattern — no isolated scaffolding).

**Negative-mutation check (mandatory, ADR-009) — BITES RED, verified.** The fold call at `session_transcript.rs:264` (`self.activity.fold(...)`, the single shared call site for both routes) was temporarily no-op'd. `test_held_route_fold_continuity_across_drain` failed RED with the intended assertion:

```
assertion `left == right` failed: AC-06 continuity: snapshot must equal K+M across the
drain boundary (held-route fold MISS would read K=18, not K+M=36)
```

The fold call was reverted immediately; the production code is unchanged from Stage 3b. A test that stayed green under that mutation would be invalid (#3624); these fail red, so they hold.

- **AC-06 nonempty-at-review**: register → stream → DRAIN → stream MORE on the held Arc → review yields `bytes_total>0, delta_count>0`. The post-drain bytes route through the held branch — a registered-only path cannot satisfy this.
- **AC-06 continuity**: K bytes registered + M bytes post-drain held → snapshot reads `bytes_total==K+M`, `delta_count==2` (same embedded accumulator across the drain boundary).
- **AC-07 read-before-purge**: non-zero collect FIRST, then `purge_held_for_feature` drops the held Arc, then a second collect returns no entry — the read provably occurred before the purge.
- **AC-12 honesty**: undeclared session contributes NO entry (no fabricated zero) while a declared held session does.

---

## Test Results

### Unit Tests (cargo, per-crate)

| Crate / target | Result |
|----------------|--------|
| `unimatrix-store` (`cargo test -p unimatrix-store`) | PASS — 0 failed |
| `unimatrix-store` migration (`--features test-support --test migration_v28_to_v29`) | PASS — 4 passed, 0 failed |
| `unimatrix-server --lib` | PASS — **4108 passed, 0 failed, 1 ignored** |
| `unimatrix-server --bin unimatrix` | PASS — **62 passed, 0 failed** (incl. 3 wave-b precondition tests: handle-wired-passes / unwired-fails-loud / max-sessions-zero-fails-loud, ADR-010/AC-07 NFR-7) |

crt-054-specific in-crate test groups (all PASS, subset of the above):
- Surface A writer + failure counter — `listener::tests::compaction_events` — **10 tests**.
- Surface B held-route believable-zero guards (NEW, Stage 3c) — `infra::transcript_hold::tests::activity` — **5 tests**.
- Surface B fold arithmetic + scanner — `infra::transcript_activity::tests` — 18 tests.
- `[transcript_signals]` config + validate — `infra::config::transcript_signals_config_tests` — 21 tests.
- Registered-route fold + accumulator preservation — `infra::session_transcript::tests` (apply_delta group) — present.
- `ActivitySnapshot` shape / Copy / content-opacity / Debug — `session_transcript_tests_snapshot` — 4 crt-054 tests.

Full-workspace `cargo test --workspace` was NOT used as the final gate: a concurrent run hit a **linker OOM** (`ld terminated with signal 9 [Killed]` while linking the `export_integration` test binary) caused by memory pressure from simultaneous release-build + pytest server processes — a build-environment failure, NOT a test failure and unrelated to crt-054. Per-crate runs (the routine convention per `.claude/rules/rust-workspace.md`) all pass clean.

### Integration Tests (infra-001, MCP JSON-RPC seam)

All suites run **sequentially, one suite per process** (not the full `pytest suites/` in one shot) to avoid the linker/embedding-model memory pressure the prior 3c run hit — see the Unit-Tests note on the `cargo test --workspace` OOM. Each suite's authoritative `-q` summary line is recorded below; `-p no:cacheprovider` was used so the stale `lastfailed` cache (see §xfail / Pre-Existing) could not skew selection. Binary under test: `target/release/unimatrix` built from `feature/crt-054`.

| Suite | Result |
|-------|--------|
| `smoke` (mandatory gate) | PASS — **23 passed, 0 failed** (199s) |
| `protocol` | PASS — **13 passed, 0 failed** (107s) |
| `tools` | PASS — **191 passed, 0 failed, 1 xfailed** (1592s; xfail pre-existing, unrelated to crt-054 — see §xfail) |
| `lifecycle` (incl. NEW `test_compaction_events_table_survives_restart`) | PASS — **66 passed, 0 failed, 5 xfailed, 2 xpassed** (733s). All 5 xfails + 2 xpasses are pre-existing tick-interval / ONNX-embedding-model / GH#406-traversal cases, none on crt-054's surface (no tick, ONNX, or traversal code in this diff). The NEW test passed (also confirmed in isolation: `1 passed in 16.5s`). |
| `edge_cases` | PASS — **23 passed, 0 failed, 1 xfailed** (207s; xfail pre-existing) |
| `volume` | PASS — **11 passed, 0 failed** (35s) |

**Totals across the six relevant suites: 327 passed, 0 hard failures, 7 xfailed (all pre-existing/unrelated), 2 xpassed (pre-existing tick/ONNX tests that incidentally passed — not crt-054-introduced).** Every count is authoritative; no `<FILL>` placeholders remain and no suite is claimed PASS without an executed summary line behind it.

**New infra-001 test (Stage 3c, per OVERVIEW §4.3):**
- `suites/test_lifecycle.py::test_compaction_events_table_survives_restart` — PASS. Boots the server (migration creates `compaction_events`), asserts columns `[id, session_id, compacted_at, high_water]`, restarts in place, asserts the table + identical schema survive (R-04/AC-01 schema durability across restart).

**Deferred to a GH Issue (not landed in this PR):** the infra-001 MCP-level seconds-boundary test (`test_compaction_events_seconds_boundary`) that drives a real PostToolUse-read just-before/just-after a compaction boundary. The infra-001 hook client exposes no compact/PreCompact op, so this needs harness infrastructure neither feature should own in a feature PR (USAGE-PROTOCOL: "harness needs significant infrastructure changes → file a GH Issue"). This is OVERVIEW §7 Open Question 1. **The AC-16 seconds-PRODUCER half is fully landed in-crate** at the real `handle_compact_payload` seam (`compaction_events::test_compacted_at_is_seconds_within_tolerance` + `test_second_compaction_adds_monotonic_row`), which is integration-level (driven through the compaction seam), NOT unit-only.

---

## xfail / Pre-Existing Failures — and the Stale `lastfailed` Cache (TRIAGED)

### The 13 `.pytest_cache/lastfailed` entries are ORPHANED node IDs — NOT live failures

The pytest `lastfailed` cache (`product/test/infra-001/.pytest_cache/v/cache/lastfailed`) listed 13 entries:
`test_protocol.py::test_list_tools_returns_nine`, `::test_list_tools_returns_eleven`;
`test_tools.py::test_search_excludes_deprecated`, `::test_deprecated_excluded_from_search`, `::test_status_observation_fields_default_values`, `::test_quarantine_requires_admin`;
`test_lifecycle.py::test_store_deprecate_search_excluded`;
`test_volume.py::TestVolume1K::test_status_report_at_1k`, `::test_100_distinct_topics`, `::test_100_rapid_store_search_pairs`, `TestLargeContent::test_large_content_100kb`, `::test_large_content_500kb`;
`test_security.py::test_restricted_agent_quarantine_allowed_write`.

**Triage method (USAGE-PROTOCOL decision tree):** `python -m pytest suites/ --co -q -p no:cacheprovider` collects **378 tests**, and a node-ID grep confirms **none of the 13 IDs exist in the current harness**. They are leftovers from a harness version that predates the rmcp 0.16→1.7 migration (#674) and the vnc-035 carry-forward churn (#730) — the suites were refactored/renamed since:
- `test_list_tools_returns_nine`/`_eleven` → the tool count grew; current assertion is `test_list_tools_returns_fourteen` (14 tools — crt-054 adds **no** tools, so this drift is not crt-054's).
- `test_search_excludes_deprecated` / `test_deprecated_excluded_from_search` / `test_store_deprecate_search_excluded` → deprecation behavior was changed so deprecated entries are now *visible with a confidence/topology penalty* (current `test_deprecated_visible_in_search_with_lower_confidence`, `test_store_deprecate_status_changed`); the old "excluded" assertions were removed pre-crt-054.
- `test_quarantine_requires_admin` / `test_restricted_agent_quarantine_allowed_write` / `test_status_observation_fields_default_values` / the four `volume` IDs → renamed/removed in the same refactors.

`lastfailed` carries forward unknown node IDs verbatim until a full run reconciles them. **Disposition: none of the 13 is a live failure, a pre-existing live failure, OR a crt-054 regression — there is nothing to fix, nothing to mark `xfail`, and no GH Issue to file.** crt-054's diff does not touch the harness suites at all except *adding* the new `test_compaction_events_table_survives_restart` (no deletions/edits to existing suite tests; `git status` shows only an addition to `test_lifecycle.py`). All runs used `-p no:cacheprovider` so the stale cache could not influence selection or results.

### Live xfails observed in this run (all pre-existing, all unrelated to crt-054)

| Item | Disposition |
|------|-------------|
| `tools` — 1 xfailed | Pre-existing marker in `test_tools.py`; unrelated to the compaction/transcript surface crt-054 changed. Not crt-054-introduced. |
| `lifecycle` — 5 xfailed + 2 xpassed | All are tick-interval / ONNX-embedding-model / multi-hop-traversal cases (`test_lifecycle.py` markers: GH#406 traversal; "no ONNX model in CI"; "remove xfail when CI configures short tick interval"). crt-054 contains **no** tick, embedding, or graph-traversal code, so these neither fail because of it nor are masked by it. The 2 `xpassed` (tick/ONNX tests that incidentally passed in this env) are likewise outside crt-054's surface — a marker-hygiene note for the harness owner, **not** a crt-054 finding. |
| `edge_cases` — 1 xfailed | Pre-existing marker; unrelated. |
| `eval::runner::sweep_tests::test_ac14_correlated_sweep_non_vacuous` (cargo, Gate 3b-flagged flake) | **Pre-existing, unrelated, already tracked by GH#746** (~2% HNSW approximate-retrieval membership flip). NOT touched by crt-054 (eval/runner is out of crt-054's diff). Passed 3/3 in isolation and within the `--lib` run. No new issue filed (one already exists); no xfail added (cargo unit flake, not a pytest test — xfail is pytest-only). |

No infra-001 integration test was marked `xfail`, deleted, or commented out by crt-054. **No new GH Issues filed** — the only pre-existing cargo flake encountered is already tracked by GH#746, the live pytest xfails are pre-existing and already carry their own GH references in-suite, and the 13 cache entries are non-existent node IDs (no defect to track). The AC-16 harness-injection item remains OVERVIEW §7 OQ1, to be filed by the SM if the consumer-side crt-055 test needs it.

---

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `migration_v28_to_v29` (fresh + v28→v29 upgrade + idempotent + columns-contract); infra-001 `test_compaction_events_table_survives_restart`; index `idx_compaction_events_session` present |
| AC-01a | PASS | DDL "Unix SECONDS" comment present in both fresh-create (`db.rs`) and migration paths; `compacted_at_secs = unix_now_secs()` |
| AC-02 | PASS | `compaction_events::test_compaction_writes_one_row`, `..._compacted_at_is_seconds_within_tolerance`, `..._high_water_equals_buffer_high_water`, `..._second_compaction_adds_monotonic_row` |
| AC-03 | PASS | `compaction_events::test_compaction_row_written_for_undeclared_session`, `..._no_feature_cycle_or_content_column` |
| AC-04 | PASS | `compaction_events` writer tests drive `handle_compact_payload` (INSERT after `increment_compaction`, guard dropped before INSERT, ACK completes); ADR-007 lock-ordering review documented |
| AC-04a | PASS | `compaction_events::test_insert_failure_increments_named_counter` (named counter +1, fault-injected), `..._counter_is_content_free`, `..._non_blocking_no_row` |
| AC-05 | PASS | `session_transcript::tests::test_apply_delta_registered_route_folds_counters`, `..._fold_counts_class_matches`, `..._fold_runs_after_merge` |
| AC-06 | PASS | `activity::test_held_route_fold_nonempty_at_review`, `..._continuity_across_drain` (negative-mutation CONFIRMED red), `..._collector_includes_declared_held_excludes_undeclared` — integration, held route, on crt-052 Wave B fixtures |
| AC-07 | PASS | `activity::test_read_before_purge_ordering`, `..._snapshot_survives_drain_hold_review` — read provably before purge |
| AC-08 | PASS | `session_transcript_tests_snapshot::test_activity_snapshot_is_copy`, `..._shape_matches_contract`, `..._debug_is_metadata_only`; structural no-content-field; no `Display` |
| AC-09 | PASS | `transcript_activity_tests::test_scanner_single_scan_increments_matched_classes`, `..._match_count_is_per_delta_not_per_occurrence`, `..._scan_over_delta_matching_both_yields_zero_and_one` |
| AC-10 | PASS | `transcript_signals_config_tests::test_default_config_error_refusal_only`, `..._no_sdlc_literals`, `..._no_reread_or_compaction_class`, `..._class_index_mapping_stable` |
| AC-10a | PASS | `testing/CALIBRATION.md` reviewed — patterns anchored-by-construction (no real transcript corpus in-repo); counts documented as **DIRECTIONAL, NOT PRECISE** (mandatory statement present, §"Precision / false-positive notes"); validated against positive/negative fixtures (`test_default_error_pattern_*`, `test_default_refusal_pattern_*`) |
| AC-11 | PASS | `transcript_signals_config_tests::test_max_signal_classes_is_16`, `..._config_over_cap_rejected`, `..._config_invalid_regex_rejected` |
| AC-12 | PASS | `activity::test_collector_includes_declared_held_excludes_undeclared` (undeclared contributes no entry, no fabricated zero) |
| AC-13 | PASS | Transitive via AC-06 held-route coverage; diff grep confirms neither surface reads `PreToolUse`/single-hook presence |
| AC-14 | PASS | `transcript_activity_tests::test_bytes_total_holds_large_value_un_narrowed`; producer cast-free diff grep (no `as i64`/`as i32` on `activity_snapshot` path or `activity_snapshots_for_feature` collector) |
| AC-15 | PASS | Diff grep: no `cycle_review_index` ALTER, no `SUMMARY_SCHEMA_VERSION` bump (only ABSENCE-asserting comments), no `token_*` symbol, no `reread`/`compaction` regex class |
| AC-16 | PASS (producer half) | `compaction_events::test_compacted_at_is_seconds_within_tolerance` + `..._second_compaction_adds_monotonic_row` land the seconds-PRODUCER guarantee at the real seam (integration). **Consumer half (ts/1000 normalization + `read_ts_secs > compacted_at` classification) is crt-055's** per OVERVIEW §5; the full MCP-level pre/post-boundary classification test is deferred to a harness GH Issue (OVERVIEW §7 OQ1) since infra-001 has no compact op. |

---

## Gaps

None at the risk level — every R-01..R-15 has passing coverage and every AC-01..AC-16 is verified.

Two scope notes (by design, not gaps):
1. **AC-16 consumer half** (ts/1000 normalization + pre/post classification) is crt-055's by the §5 ownership split — crt-054 lands only the seconds-producer half (done, in-crate integration). The full end-to-end MCP boundary-classification test needs a harness compact op (no infra-001 support today) → OVERVIEW §7 OQ1, to be filed as a GH Issue by the SM at the producer/consumer handoff rather than overloading this PR.
2. **R-04 schema version** is pinned at 29 here; if crt-055 merges first the SM reconciles crt-054 to 30 (migration file rename + `>= NN` assertions) per the #4095 pre-delivery grep — a merge-order coordination point, not a coverage gap.

---

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` — surfaced ADR-009 (#5033 held-route believable-zero guard, non-empty source + negative-mutation), ADR-006 (#5031 survival-to-review), ADR-010 (#5034 Wave B startup precondition), ADR-004 (#5029 late-bind no-fabricated-zero), and the Gate-3b deferral lessons (#3806/#4202/#4515 — production code without handler-specific integration tests). All load-bearing for the AC-06/AC-07 integration discipline executed here.
- Stored: a reusable held-route believable-zero integration-test fixture pattern (drain→hold→re-adopt on the registry+hold pair via `activity_snapshots_for_feature`, with a K+M-continuity negative-mutation guard that bites when the shared fold call is removed). See report-block in the agent report. Stored under topic `testing`, category `pattern`.
