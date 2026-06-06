# Risk Coverage Report: vnc-025

> Stage: 3c (Test Execution)
> Date: 2026-06-06
> Agent: vnc-025-agent-9-tester
> Inputs: RISK-TEST-STRATEGY.md, test-plan/ (OVERVIEW + 7 component plans), ACCEPTANCE-MAP.md,
> gate-3b-report.md (W1 follow-up), product/test/infra-001/USAGE-PROTOCOL.md

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | Merge correctness under reorder/duplicate/overlap (Critical) | `test_apply_delta_permutation_convergence_below_cap`, `test_apply_delta_fills_hole_exactly`, `..._shrinks_hole_from_start`, `..._shrinks_hole_from_end`, `..._splits_hole_in_two`, `..._spans_multiple_holes`, `..._below_base_after_ring_tail_is_noop`, `..._beyond_span_creates_hole_tail_never_crosses`, `..._zero_length_bytes_noop_high_water_defined`, `..._offset_zero_empty_buffer_then_exact_duplicate`, `..._invalid_utf8_bytes_accepted` | PASS | Full |
| R-02 | Offset arithmetic unsoundness / NFR-09 never-panics | `test_apply_delta_near_u64_max_drops_whole` (zero state change incl. high_water), `test_apply_delta_far_offset_jump_allocation_bounded`, `test_apply_delta_one_mib_into_4mib_cap`, `test_apply_delta_one_mib_into_64kib_cap`, `test_apply_delta_fuzz_no_panic`; static gate: no raw `offset as usize` in `session_transcript.rs` (verified, see Static Gates) | PASS | Full |
| R-03 | Overflow × reorder tail-window equivalence | `test_overflow_reorder_tail_window_equivalence`, `test_overflow_size_never_exceeds_cap`, `test_overflow_no_marker_bytes_in_content`, `test_high_water_monotonic_across_overflow`, `test_elided_bytes_accounting_exact`, `test_cap_exactly_equal_to_delta_size` | PASS | Full |
| R-04 | Delta bytes → durable row | vnc-024 zero-rows tests run UNMODIFIED (`test_transcript_delta_uds_acks_zero_rows`, `test_transcript_delta_malformed_payload_still_acks_zero_rows` — zero diff lines across all six vnc-025 commits, re-verified); `test_mixed_batch_persists_non_delta_merges_delta` (row counts + row content); filter line `listener.rs:1054` byte-identical (Static Gates) | PASS | Full |
| R-05 | Content leak to logs/audit/Debug | `test_debug_output_contains_no_payload_bytes`, `test_delta_paths_never_log_sentinel`, `test_purge_audit_row_sentinel_free`; static gates: zero `tracing` calls and zero `Display` impls in both new modules (verified) | PASS | Full (dynamic + static, both as required) |
| R-06 | Lock ordering / mutex poisoning | `test_poisoned_buffer_mutex_recovery` (merge resumes, best-effort bytes_purged at drain + sweep), `test_transcript_delta_poisoned_buffer_still_acks`, `test_clear_transcripts_for_feature_under_concurrent_stream`, `test_compact_read_concurrent_with_deltas_point_in_time`; static gate: no bare `unwrap()` on the buffer mutex in non-test code (verified) | PASS | Full |
| R-07 | Audit emission failure modes (#4379 cluster) | `test_session_close_purge_emits_audit_row`, `test_purge_completes_when_audit_store_unavailable`, `test_purge_never_blocks_on_audit_latency`, `test_sweep_burst_all_audits_land`, `test_empty_buffer_purge_emits_nothing`; emission context is `log_event_async` + `tokio::spawn` (gate-3b ADR-004 check) | PASS | Full |
| R-08 | Drain/sweep signatures + silently-evicted audit gap | `test_drain_returns_signal_and_purge_record`, `test_drain_empty_buffer_returns_none_record`, `test_drain_unknown_session_returns_none` (all three return shapes), `test_sweep_returns_purge_records_for_stale`, `test_sweep_silently_evicted_session_yields_purge_record` (the named mandatory case), `test_silently_evicted_session_gets_audit_row`, `test_sweep_empty_buffer_session_no_purge_record`, `test_close_and_sweep_in_one_pass_emit_both_triggers` | PASS | Full |
| R-09 | PreCompact parity drift / double-prepend | `test_golden_parity_from_path_vs_streamed_from_bytes` (expected computed from path variant at test time), `test_compact_payload_nonempty_buffer_prepends_tail_block`, `test_compact_payload_empty_buffer_byte_identical` (committed wave-0 baselines), `test_from_bytes_mid_line_tail_start`, `test_from_bytes_hole_truncated_window_well_formed`, `test_compact_payload_token_count_includes_prepended_block`, `test_compact_payload_contiguous_tail_none_identical_to_empty`, `test_compact_read_concurrent_with_deltas_point_in_time` | PASS | Full |
| R-10 | Cycle-review clear semantics | `test_clear_transcripts_for_feature_matrix` (Some(reviewed)/Some(other)/None), `test_cycle_review_purges_under_purge_on_cycle_close`, `test_cycle_review_retain_days_arm_does_not_purge`, `test_cycle_review_output_unchanged_by_purge` (byte-identical to committed baselines), `test_post_clear_resumed_stream_serves_tail`, `test_clear_returns_bytes_purged`, `test_sweep_after_cycle_review_clear_yields_no_second_record`, **`test_cycle_review_error_path_keeps_transcripts` (NEW — Gate 3b W1, see below)** | PASS | Full |
| R-11 | Config plumbing | `test_transcript_buffer_max_bytes_default_when_absent`, `..._explicit_value_respected`, `..._project_overrides_global`, `..._global_used_when_project_absent`, `test_validate_rejects_below_floor`, `test_with_transcript_cap_propagates_to_new_sessions` (end-to-end 128 KiB cap chain — the wiring-gap catcher) | PASS | Full |
| R-12 | HTTP transport convergence (#4725) | `test_prefix_session_id_preserves_event_type_single`, `..._batch_every_element`, `test_observe_http_delta_body_deserializes_to_record_event`, `test_observe_http_prefix_session_id_preserves_delta_routing`, `test_observe_http_batch_prefix_preserves_delta_drop_routing`, `test_http_delta_lands_in_http_prefixed_buffer`, `test_http_delta_without_session_write_rejected_before_dispatch` | PASS | Full |
| R-13 | Prompt injection via streamed transcript | `test_block_bounded_regardless_of_input_size` (1 MiB adversarial input ≤ MAX_PRECOMPACT_BYTES), `test_block_structurally_wrapped`; document-and-accept comment present in test (like-for-like with local hook, deliberate acceptance) | PASS | Full (per scoped acceptance — no sanitization in scope) |
| R-14 | hook.rs extraction regresses live local path | Entire hook suite passes unmodified: 182 tests under `hook` filter (pre-move 22-name inventory preserved per gate-3b/agent-4); `test_constants_pinned` (3000 / 4) | PASS | Full |
| R-15 | Hole-metadata exhaustion / collapse | `test_hole_collapse_at_cap` (65th hole → collapse), `test_post_collapse_merge_and_tail_correct`, `test_pathological_sparse_stream_bounded` | PASS | Full |

## Gate 3b W1 Resolution — cycle-review error path

W1 asked for a dedicated error-path test (failed review keeps transcripts) or a recorded
structural argument. **Both delivered:**

**New test**: `mcp::tools::tests::test_cycle_review_error_path_keeps_transcripts`
(tools.rs, after `test_cycle_review_output_unchanged_by_purge`). Registers a session with a
non-empty buffer attributed to the reviewed cycle, replays the handler's full-pipeline
success-return gate with the same `result` value the handler computes — an unknown format
makes `dispatch_review_with_advisory` return `Err(ERROR_INVALID_PARAMS)` — and asserts the
gate skips the purge: session stays registered, buffer non-empty, content byte-identical.
Passes.

**Structural argument** (grep-verified this session): `purge_cycle_transcripts` is called
from exactly four sites in `mcp/tools.rs` (lines 2110, 2236, 2925, 3027 — purged-signals,
cached-MetricVector, memo-hit, full-pipeline), every one guarded by `if result.is_ok()`.
All handler error paths (step-2 validation, observation-load `?`, ERROR_NO_OBSERVATION_DATA,
format dispatch) return before or fail the gate. `purge_cycle_transcripts` itself
(server.rs:541) introduces no error path and cannot alter the review result (AC-09).
A full handler-level error-injection test would require new `RequestContext<RoleServer>`
construction infrastructure that no existing test uses — out of scope per the cumulative
test-infrastructure rule; the gate-replay test plus this argument pin the behavior.

## Test Results

### Unit / Rust Integration Tests (`cargo test --workspace`)

- Total: 4849 (4820 passed + 1 failed + 28 ignored at the full-workspace run; +1 new W1 test verified green after addition)
- Passed: 4820 (+1 new)
- Failed: 1 — `http::token::tests::test_concurrent_creation_no_corruption`
- Ignored: 28 (pre-existing `#[ignore]` markers, untouched)

**Triage of the single failure**: known pre-existing parallel-run flake (documented in
gate-3b W4 and prior gate reports); **passes in isolation — re-verified this session**.
Not a vnc-025 regression. The col018 topic-signal pair (the other known flakes) passed in
this run.

**New flake discovered and triaged**:
`uds::listener::tests::test_transcript_delta_in_batch_dropped_rest_persist` fails
intermittently (~1 in 5–15 runs, including in isolation). Root cause: the test's fixed
50 ms sleep races the fire-and-forget `spawn_blocking` batch write. Pre-existing vnc-024
test (commit 514f2acf/70b3aeb7) — appears in **zero** diff lines across all six vnc-025
commits; the vnc-025 tee on this path is a synchronous in-memory loop adding no async work.
Per USAGE-PROTOCOL triage: **GH#691 filed**, not fixed in this feature. (Rust test — no
pytest xfail marker applicable.)

Per-area vnc-025 counts (targeted filters, all green):

| Filter | Tests | Result |
|--------|-------|--------|
| `session_transcript` (buffer state machine) | 32 | PASS |
| `transcript_block` (extraction core + from_bytes) | 32 | PASS |
| `transcript` (all transcript-touching incl. listener/registry/HTTP/cycle-review) | 86 | PASS (3 consecutive clean runs; one flake occurrence triaged above) |
| `hook` (R-14 moved-suite regression) | 182 | PASS |

### Integration Tests (infra-001 harness)

Suites per the OVERVIEW.md harness plan (smoke + tools + protocol + lifecycle — the
`context_cycle_review` touchpoints). **New harness tests: none**, per plan — vnc-025's
surfaces (UDS dispatch, HTTP `/observe`, registry internals, `CompactPayload`) are not
reachable through MCP JSON-RPC; existing cycle_review tests passing unmodified IS the
MCP-level AC-09 evidence.

| Run | Tests | Result |
|-----|-------|--------|
| Smoke (`-m smoke`, mandatory gate) | 23 | 23 passed, 0 failed |
| `tools` + `protocol` + `lifecycle` | 268 | 258 passed, 8 xfailed (pre-existing markers), 2 xpassed, 0 failed (38m53s; run completed by delivery leader after tester agent loss — result recorded verbatim from pytest summary) |

- xfail markers: none added by vnc-025; no harness test modified, deleted, or commented out.

**Acknowledged harness gap (by design, per OVERVIEW.md)**: transcript purge audit rows and
buffer state are unverifiable through the harness — in-memory, never persisted, content-free
audit only. That coverage lives entirely in the Rust tests above; no harness coverage is
claimed for it.

## Static Gates (re-verified this session)

| Gate | Check | Result |
|------|-------|--------|
| AC-12 | `tracing` calls in `infra/session_transcript.rs` + `uds/transcript_block.rs` | NONE — PASS |
| AC-12 | `Display` impls in either new module | NONE — PASS |
| AC-12 | Bare `transcript.lock().unwrap()` in non-test code (session.rs, listener.rs, server.rs) | NONE — PASS |
| ADR-008 | Raw `offset as usize` in `session_transcript.rs` | NONE — PASS |
| R-04.3 | Batch filter line `.filter(|event| event.event_type != TRANSCRIPT_DELTA_EVENT)` present at `listener.rs:1054`, byte-identical (zero +/- diff lines across all six commits per gate-3b; re-confirmed present) | PASS |
| AC-13 | `cargo audit` | 1 finding: RUSTSEC-2023-0071 (rsa 0.9.10 via sqlx-mysql, medium, no upstream fix) — **pre-existing**, identical to vnc-024/vnc-025 gate-3b reports (W5); 9 allowed warnings (unmaintained bincode/paste/number_prefix, pre-existing) |
| AC-13 | `Cargo.toml`/`Cargo.lock` diff | Zero changes — no new runtime dependency — PASS |

## Gaps

None for the named risks — all 15 risks have passing test coverage at the levels the
strategy requires. Two deliberate non-test postures, restated for the record:

1. **SR-06 aggregate memory**: accepted at scope review, ops-review posture only — no test
   by design (evidence trigger: >32 sessions or >256 MiB).
2. **NFR-05 crash loss**: in-flight transcripts lost on server crash by design — posture
   review only, no recovery machinery to test.
3. **Harness-invisible surfaces**: see acknowledged harness gap above — covered by Rust
   tests, not the MCP harness.

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `test_transcript_delta_uds_merges_into_registered_buffer` (Ack + buffer bytes via contiguous_tail) |
| AC-02 | PASS | `test_apply_delta_permutation_convergence_below_cap` (full-content equality across orders, high_water = max(offset+len)); `test_overflow_reorder_tail_window_equivalence` (overflow arm per Variance 1); `test_apply_delta_below_base_after_ring_tail_is_noop` |
| AC-03 | PASS | `test_transcript_delta_unregistered_acks_no_slot` + `test_apply_transcript_delta_unregistered_silent_noop` (no slot, no allocation before registry check) |
| AC-04 | PASS | `test_transcript_delta_malformed_payload_still_acks_zero_rows` (unmodified vnc-024) + `test_delta_paths_never_log_sentinel` (sentinel absent across single arm, batch tee, merge, overflow, purge paths) |
| AC-05 | PASS | vnc-024 zero-rows tests unmodified (zero diff lines) + `test_mixed_batch_persists_non_delta_merges_delta` (row counts AND row content, both transports' arms) |
| AC-06 | PASS | `test_observe_http_delta_body_deserializes_to_record_event`, `test_prefix_session_id_preserves_event_type_single`/`..._batch_every_element`, `test_http_delta_lands_in_http_prefixed_buffer`, `test_http_delta_without_session_write_rejected_before_dispatch` |
| AC-07 | PASS | `test_overflow_size_never_exceeds_cap`, `test_overflow_no_marker_bytes_in_content`, `test_elided_bytes_accounting_exact`, `test_high_water_monotonic_across_overflow`, `test_transcript_delta_over_cap_still_acks` |
| AC-08 | PASS | `test_session_close_purge_emits_audit_row`, `test_sweep_purge_emits_audit_rows`, `test_silently_evicted_session_gets_audit_row` (mandatory R-08.1 case), `test_empty_buffer_purge_emits_nothing`, `test_purge_audit_row_sentinel_free` |
| AC-09 | PASS | `test_cycle_review_output_unchanged_by_purge` (byte-identical to committed pre-vnc-025 baselines), `test_cycle_review_purges_under_purge_on_cycle_close`, `test_clear_transcripts_for_feature_matrix`, `test_cycle_review_retain_days_arm_does_not_purge` (FR-16 gate matched, not hardcoded); MCP level: harness cycle_review tests pass unmodified (see Integration Tests) |
| AC-10 | PASS | Structural: `SessionState.transcript: Arc<Mutex<TranscriptBuffer>>` (gate-3b ADR-001 check) + `test_get_state_does_not_deep_copy_transcript` (`Arc::ptr_eq` guard test) |
| AC-11 | PASS | `test_golden_parity_from_path_vs_streamed_from_bytes` + `test_compact_payload_nonempty_buffer_prepends_tail_block` (non-empty arm); `test_compact_payload_empty_buffer_byte_identical` vs committed wave-0 baselines (empty arm, no double-prepend) |
| AC-12 | PASS | Static gates table above (all NONE) + sentinel dynamic tests (`test_debug_output_contains_no_payload_bytes`, `test_delta_paths_never_log_sentinel`, `test_purge_audit_row_sentinel_free`) — both halves, as required |
| AC-13 | PASS | `cargo audit`: only pre-existing RUSTSEC-2023-0071 (tracked outside vnc-025, gate-3b W5); zero `Cargo.toml`/`Cargo.lock` changes |

### Supplementary (NFR-09 / ADR-008 — no AC-ID, hard spec requirement)

| Item | Status | Evidence |
|------|--------|----------|
| Never-panics contract | PASS | `test_apply_delta_fuzz_no_panic` (randomized offset/len incl. near-u64::MAX), `test_apply_delta_near_u64_max_drops_whole` (silent drop-whole, zero state change) |
| Poison recovery | PASS | `test_poisoned_buffer_mutex_recovery` (merge resumes + accumulates post-poison, best-effort bytes_purged), `test_transcript_delta_poisoned_buffer_still_acks` (always-Ack preserved) |
| Arithmetic review gate | PASS | No bare buffer-mutex `unwrap()`, no raw `offset as usize` (Static Gates table) |

## GH Issues Filed

| Issue | Subject | Disposition |
|-------|---------|-------------|
| GH#691 | Flaky `test_transcript_delta_in_batch_dropped_rest_persist` — 50 ms sleep races fire-and-forget batch write (pre-existing vnc-024 test, untouched by vnc-025) | Pre-existing; not fixed in this feature per triage protocol |

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — surfaced #4202 (tests named in plan but
  never implemented — mitigated here by name-level inventory verification against the
  per-component plans), #3714 (col018 flake context, matched the known-flake triage),
  vnc-025 ADRs #4739–#4744 (applied as verification criteria).
- Stored: nothing novel to store — the session's only candidate (sleep-vs-spawn flake
  triage) is already covered by the USAGE-PROTOCOL triage tree and recorded as GH#691;
  test-fixture patterns used were existing ones (#4747 baseline helper, sentinel
  convention from the test plans).
