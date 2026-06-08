# Risk Coverage Report: vnc-030

GH Issue: #699 · Stage 3c (Test Execution) · Date: 2026-06-08 · Branch: `feature/vnc-030`
Inputs: RISK-TEST-STRATEGY.md (R-01..R-23), test-plan/OVERVIEW.md + seam-and-roundtrip.md, ACCEPTANCE-MAP.md (AC-01..AC-10 + seam items), USAGE-PROTOCOL.md.
Pinned CLI: claude 2.1.167 (R-08/NFR-08 — `--resume` id-reuse, depth-1 root-id inheritance).

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | Three server stamp-read sites drift (#3486) | cargo `stamp_read::{stamp_read_site_a_records_declared, site_b, batch_n_declared_rows, unstamped_frame_legacy_chain_not_declared}` (real-DB, per-site independent); helper layer `apply_stamp_to_row_*` (9) | PASS | Full |
| R-02 | Delete-on-close kills stamp after turn 1 | JS `index-decoration` `test_lifecycle_events_never_touch_tracker`, `test_multiturn_stop_does_not_kill_stamp`, `test_cycle_stop_frame_deletes_tracker`; `cycles.test.js` lifecycle | PASS | Full |
| R-03 | Fail-open violation in new fs touchpoint | JS `cycles.test.js` fail-open injection (per-fs-call EACCES/ENOENT/disk-full/corrupt-JSON), `index-decoration` fail-open block | PASS | Full |
| R-04 | FeatureSource guard mis-implemented | cargo `enrich_*` decision-tree (7), `stamp_read::close_declared_beats_contradicting_vote_gate`, session.rs `test_sweep_declared_beats_contradicting_vote`; helper `apply_stamp_to_row_unstamped_declared_registry_wins_588` | PASS | Full |
| R-05 | Extraction suppression strip wrong | JS `index-decoration` suppression block (strip on non-CYCLE_* both directions, CYCLE_* keeps topic_signal, unstamped byte-unchanged); cargo tally-skip assertions in `stamp_read` | PASS | Full |
| R-06 | Batch/replay frame class missed | JS `test_recordevents_batch_every_event_stamped`, `test_send_failure_enqueue_replay_carries_stamp`; cargo `stamp_read_batch_n_declared_rows` | PASS | Full |
| R-07 | vnc-027 interception-seam drift (post-merge) | JS spawn `test_seam_cycle_start_yields_stamped_frame_and_writes_tracker`, `test_seam_noncycle_pretooluse_yields_sentinel_no_side_effects`, `test_seam_cli_validation_gate_rejects_invalid_params_no_tracker` | PASS | Full |
| R-08 | Uncontracted CLI behavior drift | JS canary fixtures (state-canary, index-decoration depth-gating); pinned 2.1.167 in test docs; crash+resume in `cycles.test.js` | PASS | Full (within pin) |
| R-09 | Never-declare floor regression | cargo `enrich_extraction_wins_against_inferred_registry`, `enrich_registry_fill_when_no_extraction`, `enrich_null_when_nothing_attributes`; helper extracted/registry-fill/vote/NULL | PASS | Full |
| R-10 | Non-canonical stamped topic mis-attributes | JS `test_seam_cli_validation_gate_rejects_invalid_params_no_tracker` (validation gates tracker write); `cycles.test.js` verbatim topic store | PASS | Full |
| R-11 | Migration schema-version collision | cargo store `test_current_schema_version_is_28`, `test_v28_migration_is_idempotent`; delivery check: `CURRENT_SCHEMA_VERSION=28` unique vs main | PASS | Full |
| R-12 | topic_source INSERT site missed (#4372) | cargo `stamp_read` per-value (declared/extracted/registry-fill/vote/NULL); helper `apply_stamp_to_row_unstamped_{extracted,registry_fill,vote,null}_source`; infra-001 `test_topic_source_column_per_value` | PASS | Full |
| R-13 | register_session overwrite vote-inverts | cargo session.rs `test_reregister_then_sweep_before_stamp_degrades_then_restores`, `test_apply_stamp_restores_declared_after_reregister` | PASS | Full |
| R-14 | depth>1 grandchild silent stamp miss | JS `test_depthgt1_grandchild_no_tracker_lands_in_stamp_miss` (state-canary + index-decoration) | PASS | Full (forward-compat) |
| R-15 | Raw-cwd hashing splits worktree tracker | JS `cycles.test.js` `test_worktree_tracker_under_main_root_hash`, `test_no_stamp_path_hashes_raw_cwd` | PASS | Full |
| R-16 | Wire non-additivity / binding drift | cargo engine serde trio (9: none-absent/some-present/null-tolerant/serialize-omits/serialize-includes/old-server-tolerance), `test_export_bindings_all_seven_written`, `test_round_trip_request_fixtures` | PASS | Full |
| R-17 | apply_stamp not idempotent | cargo session.rs `test_apply_stamp_sets_declared_idempotent`, `test_apply_stamp_contradicting_topic_last_writer_wins`, `test_apply_stamp_absent_session_is_noop` | PASS | Full |
| R-18 | crt-052 adjacency / non-minimal diff | cargo session.rs `test_sweep_returns_resolved_feature_for_crt052_interface`; minimal-diff verified at gate (no change to drain_and_signal_session/clear_transcripts_for_feature) | PASS | Full |
| R-19 | Subagent-gated canary residual | JS canary quartet: depth-0 no-increment, depth-1 inherited no-increment, depth-1 drift one-increment, depth>1 grandchild lands in stamp_miss; healthy `stamp_miss==0`; `subagentContext` independence (OQ-E Branch A) | PASS | Full |
| R-20 | topic_source windows over pre-migration NULLs | cargo store `test_migration_leaves_existing_rows_null` (no backfill); AC-07 methodology documented post-migration window | PASS | Full |
| R-21 | #574 lands before vnc-030 | Delivery expiry check: #574 OPEN (not merged) → no-race assumption holds, no re-verification triggered | PASS (n/a) | Full |
| R-22 | Stale phase from updatePhase no-op | JS `cycles.test.js` updatePhase-on-missing-file no-op; `test_apply_stamp_to_row_stamp_phase_none_uses_registry_phase` | PASS | Full |
| R-23 | Transport-specific (UDS) stamp loss | JS `test_stamped_recordevent_over_uds_and_http_identical_cycle_stamp` (offline byte-equivalence, UNGUARDED); replay carries stamp; parity-layer2-uds live daemon (win32-guarded) | PASS | Full |

No risk is uncovered. See Gaps section.

## Test Results

### Unit Tests (cargo)
| Crate | Passed | Failed |
|-------|--------|--------|
| unimatrix-engine (wire serde trio + 7th binding) | 457 | 0 |
| unimatrix-store --lib | 335 | 0 |
| unimatrix-store --test migration_v27_to_v28 (feature test-support) | 5 | 0 |
| unimatrix-observe (FR-25 docstring) | 440 | 0 |
| unimatrix-server --lib (incl. +9 new `apply_stamp_to_row` helper tests, 19 `stamp_read` integration tests, session.rs precedence/sweep/apply_stamp) | 3662 | 0 (1 ignored) |

- cargo total: 4899 passed, 0 failed (1 ignored).
- New tests added this stage: cargo +9 (`apply_stamp_to_row` per-site/per-value helper-layer round-trip in `listener.rs`).

### Unit Tests (JS — `node --test`, name filters unsupported per pattern #4841)
| Suite | Passed | Failed |
|-------|--------|--------|
| cycles.test.js | 27 | 0 |
| index-decoration.test.js (incl. seam quartet, canary quartet, UDS byte-equiv) | 31 | 0 |
| state-canary.test.js (`stamp_miss==0` invariant) | 15 | 0 |
| state.test.js | 31 | 0 |
| index.test.js | 54 | 0 |
| parity-layer2-uds.test.js (win32-guarded live daemon) | 16 | 0 |
| transport-uds.test.js | 33 | 0 |
| build-request.test.js | 90 | 0 |
| contract-roundtrip.test.js | 10 | 0 |

- JS total (vnc-030-relevant): 307 passed, 0 failed, 0 skipped (Linux dev OS; UDS-live `{skip:IS_WINDOWS}` guards preserve Windows coverage on offline byte-compare suites per lesson #4832).

### Integration Tests (infra-001 harness, MCP JSON-RPC over the release binary)
| Suite | Result |
|-------|--------|
| smoke (`-m smoke`, MANDATORY GATE) | 23 passed |
| protocol | 13 passed |
| lifecycle (incl. +3 new vnc-030 tests) | passed (5 xfail pre-existing, 2 xpass pre-existing) |
| volume | 11 passed |
| tools | 185 passed, 3 xfailed, 0 failed (full run, 25m57s) |

- protocol + lifecycle + volume combined run: 87 passed, 5 xfailed, 2 xpassed.
- tools full run: 185 passed, 3 xfailed (pre-existing markers), 0 failed. The wire field is additive (frozen-F1) and the `topic_source` ALTER is migration-on-restart — no tools-suite regression.
- New integration tests added (test_lifecycle.py): `test_topic_source_column_per_value` (R-12/AC-05), `test_stamped_event_attributes_declared` (AC-04/AC-05), `test_declared_survives_vote_at_close` (R-04/AC-04) — all PASS.
- The 5 xfail / 2 xpass in lifecycle are PRE-EXISTING markers (e.g. GH#406 find_terminal_active multi-hop) from prior features; none reference vnc-030, none introduced or modified this stage. Per USAGE-PROTOCOL triage, untouched.

### Gate-Blocking Seam / Round-Trip / UDS / Canary (per seam-and-roundtrip.md)
| Gate | Test(s) | Result |
|------|---------|--------|
| GATE 1 — interception-seam survival (R-07/FR-28) | `test_seam_cycle_start_yields_stamped_frame_and_writes_tracker`, `test_seam_noncycle_pretooluse_yields_sentinel_no_side_effects`, `test_seam_cli_validation_gate_rejects_invalid_params_no_tracker` | PASS |
| GATE 3 — 3-site round-trip (#3486/R-01/FR-13) | `stamp_read::stamp_read_site_a_records_declared`, `site_b`, `batch_n_declared_rows`, `unstamped_frame_legacy_chain_not_declared` (real-DB, per-site independent) + 9 helper-layer | PASS |
| GATE 4 — UDS stamp byte-equivalence (AC-10/FR-29/R-23) | `test_stamped_recordevent_over_uds_and_http_identical_cycle_stamp` (offline, unguarded) | PASS |
| GATE 5 — subagent-gated canary quartet (AC-06/R-19) | depth-0/-1/-1-drift/grandchild + healthy `stamp_miss==0` | PASS |

## Gaps

None. Every risk R-01..R-23 maps to at least one executed, passing test.

Notes on coverage boundaries (not gaps — by design):
- The raw stamped `cycle_stamp` wire frame and the 3-site read are emitted by the TS hook client over UDS, which the infra-001 MCP harness does not drive. Per the OVERVIEW gap analysis, the per-site stamp→declared contract is validated at the cargo `stamp_read` real-DB integration layer (Site A/B/C) and the JS spawn-level seam tests; the infra-001 additions validate the MCP-visible RESULTS (migrated `topic_source` column accepts all five values; declaration lifecycle accepted; close path reachable). This is the correct seam split, not a hole.
- R-08/R-14 depth>1 inheritance is forward-compat only — unverifiable until Claude Code lifts the subagent-nesting constraint (pinned claude 2.1.167); the canary is the tripwire, asserted via fixtures.
- AC-07 (manual) and FR-26 (#588 disposition, manual/PR-description) are PR-time/manual verification items, out of automated-test scope; methodology is documented.

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `cycles.test.js` (27) lifecycle/crash-resume/prune/fail-open + `index-decoration` lifecycle dispatch incl. multi-turn Stop survival |
| AC-02 | PASS | engine serde trio (9) + `test_export_bindings_all_seven_written` + frozen fixtures byte-unchanged (`test_round_trip_request_fixtures`) |
| AC-03 | PASS | `index-decoration` suppression block (strip present/absent, CYCLE_* keeps topic_signal) |
| AC-04 | PASS | `stamp_read::close_declared_beats_contradicting_vote_gate`, session.rs sweep inversion + enrich decision tree (7) + helper round-trip; infra-001 `test_declared_survives_vote_at_close` |
| AC-05 | PASS | store `test_fresh_db_has_topic_source_column`, `test_migration_v27_to_v28_adds_topic_source`, idempotence; `stamp_read` per-value; infra-001 `test_topic_source_column_per_value` |
| AC-06 | PASS | canary quartet + healthy `stamp_miss==0`; `subagentContext` independence (OQ-E Branch A); pinned 2.1.167 in docs |
| AC-07 | DEFERRED (manual) | Accuracy/fallback sample is a PR-time manual check (declared-session denominator); methodology documented; canary decoupled per ADR-006 rev2 |
| AC-08 | PASS | `cycles.test.js` `test_worktree_tracker_under_main_root_hash`, `test_no_stamp_path_hashes_raw_cwd` |
| AC-09 | PASS | grep: re-declaration line present in all three protocols (design/delivery/bugfix) |
| AC-10 | PASS | `test_stamped_recordevent_over_uds_and_http_identical_cycle_stamp` (byte-equivalent UDS↔HTTP, offline unguarded) + replay |

Seam verification (gate-blocking per ADR-007): interception-seam survival PASS; 3-site round-trip PASS; UDS stamp PASS; OQ-E probe → Branch A (subagent marker on `input.extra.agent_type`, independent of session_id — production canary ships ACTIVE); #574 expiry → OPEN, no-race holds; FR-25 docstring corrected (verified in attribution.rs + topic-signal.js); #588 disposition → PR-description manual item.

## Pre-existing Failures (confirmed independent of vnc-030)

- `http::token::tests::test_concurrent_creation_no_corruption` — parallel-only flake. CONFIRMED independent: passes in isolation (`-- --exact`, 1 passed). No code overlap with vnc-030's surface (http/token vs cycles/wire/session/listener/migration). No GH Issue filed (known transient, not a vnc-030 regression; already tracked as a parallel-execution flake).
- infra-001 lifecycle 5 xfail / 2 xpass — pre-existing markers (GH#406 et al.) from prior features; not introduced, modified, or attributable to vnc-030. Untouched per triage protocol.

## GH Issues Filed

None. No new pre-existing failure was discovered that required an Issue + xfail; the one known flake is independent and pre-tracked, and all integration failures triaged to pre-existing markers already carrying GH references.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — surfaced #4832 (cross-platform UDS win32 skip-guard, applied to UDS-live test reasoning), #4781 (xfail-only-for-feature-owned-suites triage), #4774 (spawn-entry stub-server idiom, mirrored by the seam tests), #4834 (ADR-007 seam contracts). All applied.
- Stored: nothing novel — the governing patterns (#3486 per-site round-trip evidence, #4372 multi-surface INSERT, #4092 idempotent ALTER, #4832 win32 guard, #4774 spawn-stub idiom) already exist in Unimatrix. The vnc-030 helper-layer `apply_stamp_to_row` test split (fast unit helper layer beneath the real-DB `stamp_read` integration layer) is one feature's application of #3486, not yet a cross-feature (2+) pattern. Re-evaluate at retro if crt-052 hits the contractual-write-field-across-N-sites shape.
