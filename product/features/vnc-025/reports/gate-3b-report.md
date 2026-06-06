# Gate 3b Report: vnc-025

> Gate: 3b (Code Review)
> Date: 2026-06-06
> Result: PASS

Commits reviewed: f1c14876 (config-knob), 10d5b75c (wave 0 baselines), 35dbeef4 (wave 1),
e77821ed (wave 2), 65d0a5a1 (wave 3a), 72464806 (wave 3b). 4ddee702 excluded (unrelated
human research-scoping work).

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Pseudocode fidelity | PASS | All 7 components match validated pseudocode; 3 flagged deviations + 1 extension assessed and approved (below) |
| Architecture compliance | PASS | ADR-001..008 all verifiably implemented; batch filter line byte-identical; lock discipline registry→buffer everywhere |
| Interface implementation | PASS | Every signature in the ARCHITECTURE Integration Surface table implemented exactly (drain/sweep tuples, `clear_transcripts_for_feature`, `contiguous_tail`, audit event shape) |
| Test case alignment | PASS (1 WARN) | Test names match per-component plans one-to-one (spot-verified transcript-buffer 32/32, registry 14, dispatch/compact/purge 22, HTTP 3, cycle-review 6); cycle-review plan scenario 7 (error path keeps transcripts) covered structurally, no dedicated test |
| Code quality | PASS (3 WARN) | Builds clean; zero stubs/TODO/FIXME; no `.unwrap()` in new non-test code; all new files ≤500 lines; WARNs all pre-existing (clippy lints in untouched crates, 3 flaky tests, host-file monoliths) |
| Security | PASS (1 WARN) | AC-12 static gates all pass; no secrets; input validation sound; `cargo audit`: 1 pre-existing advisory (RUSTSEC-2023-0071, rsa via sqlx-mysql), zero dependency changes in vnc-025 (AC-13 "no new runtime dependency" PASS) |
| Knowledge stewardship | PASS | All 7 implementation-agent reports carry `## Knowledge Stewardship` with Queried + Stored (#4747–#4750) or reasoned decline |

## Detailed Findings

### 1. Pseudocode fidelity
**Status**: PASS

`infra/session_transcript.rs` (359 lines) implements `apply_delta` step-for-step against
transcript-buffer.md (checked_add drop-whole → high_water → len-0 return → ring-tail →
clip → hole push → resize → in-place write → hole surgery → collapse-at-65). Invariants
I1–I5 documented at the struct and at every u64→usize conversion site as the pseudocode
requires. `clear()` pins `base_offset = high_water`, returns span length, leaves
`high_water`/`elided_bytes` unchanged — the crt-052 semantics pinned in OVERVIEW.md gap 5.
`uds/transcript_block.rs` implements the pseudocode's refactor rule literally: one private
`block_from_lines` core called by both `extract_transcript_block(path)` and
`extract_transcript_block_from_bytes` (lossy decode). Constants pinned 3000/4. Registry,
dispatch, purge-audit, config-knob, and cycle-review wiring all match their files (evidence
in checks 2–4).

**Three flagged deviations — all APPROVED:**

1. **`Mutex::clear_poison()` in poison recovery** (`session.rs::lock_buffer` and
   `purge_record_for`; pattern #4748). Pseudocode's `into_inner()` + `clear()` alone leaves
   std's poison flag set permanently, so every later lock would re-enter the recovery arm
   and re-clear — ADR-008/R-06.2's required "merge resumes; subsequent deltas accumulate"
   could never hold. The deviation preserves ADR-008's intent (recover once,
   treat-as-empty, session never bricked) and is pinned by
   `test_poisoned_buffer_mutex_recovery`, which asserts post-poison deltas accumulate
   (`"after"` + `"-poison"` → `"after-poison"`) and best-effort `bytes_purged` at drain and
   sweep. Correct fix, not a policy change.
2. **`category = ?e.classify()` instead of `error = %e`** on delta parse failure
   (`listener.rs:799/:1034`; pattern #4749). serde_json's error `Display` embeds offending
   string values (`invalid type: string "<payload>"...`) — the pseudocode's own `error = %e`
   would have violated AC-04/AC-12/NFR-01. `serde_json::error::Category` is a content-free
   four-variant enum. This deviation *strengthens* the spec's hard gate. Pinned by
   `test_delta_paths_never_log_sentinel`.
3. **`purge_cycle_transcripts()` helper at four success-return sites** (`server.rs:522`,
   called from `tools.rs` at purged-signals, cached-MetricVector, memo-hit, and
   full-pipeline returns; pattern #4750). Pseudocode assumed one success return; the handler
   factually has four, and the test plan's cached-re-review idempotency scenario requires
   the memo-hit site. The exhaustive `TranscriptRetention` match (FR-16 — no `_` arm,
   `RetainDays` present and non-purging) lives in exactly one place; every call is gated on
   `result.is_ok()` so error paths keep transcripts (Gate 3a disposition 2 honored).

**One extension beyond pseudocode — approved**: `services/status.rs` maintenance tick (the
primary `sweep_stale_sessions` driver) also consumes purge records and emits
`stale_sweep` audits. The pseudocode only listed the `listener.rs` call site; dropping
records at the tick would have left most sweep purges unaudited — an FR-12/AC-08 hole.
Documented in agent-7's report with rationale; `AuditLog` construction there is a stateless
handle over the same store, not new audit plumbing.

Minor noted-and-accepted: `apply_transcript_delta` bumps activity via
`last_activity_at.max(now_secs())` (monotonic guard) vs pseudocode's plain assignment —
behaviorally equivalent or safer.

### 2. Architecture compliance
**Status**: PASS

- **ADR-001**: `SessionState.transcript: Arc<Mutex<TranscriptBuffer>>`; registry lock does
  lookup + `Arc::clone` + scalar bump only; memcpy under the buffer lock
  (`session.rs::apply_transcript_delta` two-phase shape verbatim). AC-10 pinned by
  `test_get_state_does_not_deep_copy_transcript` (`Arc::ptr_eq`).
- **ADR-002**: span+holes representation, ring-tail, metadata-only elision, manual
  metadata-only `Debug` (no `Display`, no derive). `test_overflow_no_marker_bytes_in_content`
  asserts no spliced markers.
- **ADR-003**: single arm replaced in place (early-return shape kept); batch tee inserted
  *before* the vnc-024 filter; the filter line
  `.filter(|event| event.event_type != TRANSCRIPT_DELTA_EVENT)` (`listener.rs:1054`) appears
  in zero +/- diff lines across all six commits — byte-identical (R-04.3 review-diff gate).
  Always-Ack preserved on all outcomes.
- **ADR-004**: collect-under-lock/emit-after-release at all three purge points; pinned audit
  shape (`operation`, `agent_id "server"`, `detail "bytes=<n> trigger=<t>"`, Success, empty
  target_ids) defined once in `emit_purge_audits`; `log_event_async` + `tokio::spawn`
  fire-and-forget (#4379-safe); zero-byte purges filtered at record construction.
- **ADR-005**: shared core, two front-ends; hook.rs shrank 656 lines, call sites re-import,
  bodies unchanged; hook suite 174/174 unmodified (agent-4, R-14.1); constants test moved.
- **ADR-006**: cap injected at construction; `with_transcript_cap` at the two production
  sites (`main.rs:647/:1078`); `server.rs:340` test ctor keeps `new()` — exactly the Gate 3a
  OQ-3 disposition.
- **ADR-007**: `session_key()` degenerate seam with the required LOAD-BEARING doc comment;
  routed at all new key paths.
- **ADR-008**: both layers implemented (see check 6 static gates and deviation 1).

### 3. Interface implementation
**Status**: PASS

Signatures match the Integration Surface table exactly:
`apply_transcript_delta(&self, &str, u64, &[u8])`; `contiguous_tail(usize) -> Option<Vec<u8>>`;
`clear() -> u64`; `len/is_empty/high_water/elided_bytes`;
`drain_and_signal_session -> Option<(SignalOutput, Option<TranscriptPurgeRecord>)>`
(`SignalOutput` shape untouched — wave 0 fixture pins it);
`sweep_stale_sessions -> (Vec<SweepResult>, Vec<TranscriptPurgeRecord>)`;
`clear_transcripts_for_feature -> Vec<TranscriptPurgeRecord>`;
`extract_transcript_block_from_bytes(&[u8]) -> Option<String>`;
`RetentionConfig.transcript_buffer_max_bytes` (serde default 4_194_304, validate floor 65_536,
project-wins merge arm); `UnimatrixServer.retention_config` per #561 precedent, threaded at
both main.rs paths. Wire surface untouched (NFR-08): zero diffs in `wire.rs`/bindings.

### 4. Test case alignment
**Status**: PASS (1 WARN)

Implemented test names match the per-component plans one-to-one. Hard gates all present:
- R-09.1 golden parity: `test_golden_parity_from_path_vs_streamed_from_bytes` (expected
  computed from the path variant at test time, >12 KB fixture, shuffled+duplicated deltas) +
  end-to-end `test_compact_payload_nonempty_buffer_prepends_tail_block`.
- R-09.4 empty-buffer byte-identity: `test_compact_payload_empty_buffer_byte_identical`
  against committed wave-0 baselines (unknown-session / registered-no-state / histogram
  variants) via `assert_matches_committed_baseline` — no hand-written expectation (#2984
  honored; Gate 3a W3 sequencing instruction followed: baselines landed in 10d5b75c before
  any implementation edit).
- R-06.2 poisoned mutex: `test_poisoned_buffer_mutex_recovery`,
  `test_transcript_delta_poisoned_buffer_still_acks`.
- R-08.1 silently-evicted: `test_sweep_silently_evicted_session_yields_purge_record` +
  `test_silently_evicted_session_gets_audit_row`.
- NFR-09 fuzz: `test_apply_delta_fuzz_no_panic` (+ `test_apply_delta_near_u64_max_drops_whole`
  asserting zero state change incl. `high_water`).
- AC-05: vnc-024 zero-rows tests (`test_transcript_delta_uds_acks_zero_rows`,
  `..._malformed_payload_still_acks_zero_rows`) appear in zero diff lines — unmodified;
  `test_mixed_batch_persists_non_delta_merges_delta` adds row-content assertions.
- R-05 sentinel: `test_delta_paths_never_log_sentinel`, `test_purge_audit_row_sentinel_free`,
  `test_debug_output_contains_no_payload_bytes`.
- #3902 signature: `test_delta_dispatch_emits_no_new_audit_events`.
- R-11.5 cap chain: `test_with_transcript_cap_propagates_to_new_sessions` (128 KiB overflow
  at 128 KiB, not 4 MiB).
- R-12/#4725: three `prefix_session_id` transform tests +
  `test_http_delta_lands_in_http_prefixed_buffer` +
  `test_http_delta_without_session_write_rejected_before_dispatch`.
- AC-09: `test_cycle_review_output_unchanged_by_purge` (purge between report build and
  render; byte-identical to committed baselines) + R-10 matrix / RetainDays-injection /
  zero-attributed / idempotent-re-purge tests in `server.rs`.
- R-13.1: `test_block_bounded_regardless_of_input_size` (1 MiB adversarial input,
  document-and-accept comment present).
- R-14: 22-name pre-move inventory preserved (agent-4 comm-verified) + `test_constants_pinned`.

**WARN (W1)**: cycle-review-purge test-plan scenario 7 ("review error path: transcripts NOT
cleared") has no dedicated test. Coverage is structural — every purge call is gated on
`result.is_ok()` and error paths return before/around it. Stage 3c tester should add one
explicit error-path test (e.g. unknown format → `Err` → buffers still non-empty) or record
the structural argument in RISK-COVERAGE-REPORT.

### 5. Code quality
**Status**: PASS (3 WARN — all pre-existing)

- `cargo build --workspace`: success (25 warnings in unimatrix-server lib — pre-existing,
  count matches the vnc-024-era baseline class).
- No `todo!()`/`unimplemented!()`/`TODO`/`FIXME` in any touched file.
- No new `.unwrap()` in non-test code across all six diffs (grep over `+` lines); registry
  mutex keeps its existing `unwrap_or_else(into_inner)` idiom; buffer mutex never bare-unwrapped.
- New files all ≤500 lines: `session_transcript.rs` 359, `transcript_block.rs` 420, three
  listener test files 325/331/397, buffer test files 414/286, block test files 297/262
  (test splits via `#[path]` modules — cumulative infra, helpers shared).
- `cargo test --workspace`: 3597 passed, 3 failed — all three
  (`http::token::test_concurrent_creation_no_corruption`,
  `col018_topic_signal_from_file_path`, `col018_topic_signal_from_feature_id`) pass in
  isolation and are documented pre-existing parallel-run flakes (col018: "embedding model is
  initializing", same failure recorded in crt-030/crt-038 gate-3b reports; token: test-env
  file race). Not vnc-025 regressions.

**WARN (W2, pre-existing)**: `cargo clippy --workspace -- -D warnings` fails — every error is
in `unimatrix-observe`/`unimatrix-engine`/`patches/anndists` (rust-1.95 lint promotions),
untouched by vnc-025; same disposition as the vnc-024 gate-3b report. vnc-025-introduced
lines add zero new clippy warnings (agent-7/agent-8 verified identical pre/post counts; the
`hook.rs:207/:239` redundant-closure lints sit on call-site lines character-identical to
baseline).
**WARN (W3, pre-existing)**: host files `listener.rs` (8851), `tools.rs` (10157),
`config.rs` (10664), `session.rs` (2768), `server.rs` (4092), `hook.rs` (4183) exceed the
500-line cap — all pre-date vnc-025; the feature added thin wiring plus tests per NFR-07
(hook.rs actually shrank 656 lines). New-component logic landed in new focused modules as
designed.
**WARN (W4, pre-existing)**: 3 flaky workspace tests as above — recommend tracking the
`http::token` concurrency flake; the col018 pair is already documented.

### 6. Security
**Status**: PASS (1 WARN — pre-existing)

AC-12 static gates (all pass):
- No `tracing` call in `infra/session_transcript.rs` or `uds/transcript_block.rs` (doc-comment
  mentions only).
- No `Display` impl in either new module; `TranscriptBuffer` `Debug` is manual metadata-only;
  `ExchangeTurn` deliberately has no derived `Debug`.
- No raw `offset as usize` anywhere in the new modules — every u64→usize conversion is
  span-relative with an I5 invariant comment.
- No bare `.unwrap()` on the buffer mutex in non-test code (the only production lock sites are
  `lock_buffer`, `purge_record_for`, and the compact-path `lock_buffer` call — all
  poison-recovering).
- Delta parse-failure logging uses content-free `e.classify()` at both arms (deviation 2).
- Audit `detail` interpolates only the u64 count and a static trigger token; the emission
  helper takes counts-only `TranscriptPurgeRecord` — structurally incapable of carrying content.

General security:
- No hardcoded secrets/keys; nothing new reads env or config credentials.
- Untrusted-input surface: `offset: u64` fully covered by checked arithmetic (drop-whole on
  overflow) + fuzz test; far-offset jumps bounded by ring-tail
  (`test_apply_delta_far_offset_jump_allocation_bounded`); sparse-delta metadata exhaustion
  bounded by collapse-at-65 (`test_hole_collapse_at_cap`,
  `test_pathological_sparse_stream_bounded`); unregistered-session deltas allocate nothing
  before the registry check. Malformed payloads cannot panic (matched serde parse → Ack).
- No path traversal (`from_bytes` takes memory; the only file I/O is the pre-existing hook
  path variant); no shell/process invocation.
- Prompt-injection channel (R-13) bounded to `MAX_PRECOMPACT_BYTES` and documented-and-accepted
  in test comments per strategy.
- **AC-13**: zero changes to `Cargo.toml`/`Cargo.lock` across all six commits — no new
  dependency. **WARN (W5, pre-existing)**: `cargo audit` reports RUSTSEC-2023-0071 (rsa 0.9.10,
  Marvin timing sidechannel, medium, no fix upstream) reaching the graph via `sqlx-mysql`
  (workspace uses SQLite); identical finding recorded as pre-existing in the vnc-024 gate-3b
  report. Not introduced by vnc-025; track separately. Plus pre-existing unmaintained-crate
  warnings (bincode, paste, number_prefix).

### 7. Knowledge stewardship
**Status**: PASS

All seven implementation reports under `product/features/vnc-025/agents/` carry
`## Knowledge Stewardship` blocks with `Queried:` evidence (context_briefing/context_search,
entries cited and applied) and `Stored:` lines:
- baselines — stored #4747 (baseline-fixture helper pattern).
- agent-3 (transcript-buffer) — declined with reason (followed pseudocode; trick subsumed).
- agent-4 (transcript-block) — declined with reason (equivalence already in ADR-005, pinned by test).
- agent-5 (config-knob) — declined with reason (direct application of #4070).
- agent-6 (registry-wiring) — stored #4748 (clear_poison pairing).
- agent-7 (dispatch/purge) — stored #4749 (serde Display leak).
- agent-8 (cycle-review) — stored #4750 (four success-return gating).

## Rework Required

None blocking. WARN follow-ups for Stage 3c:

| Item | Owner | Action |
|------|-------|--------|
| W1 | tester (Stage 3c) | Add an explicit cycle-review error-path test (transcripts survive a failed review) or document the structural `is_ok()` argument in RISK-COVERAGE-REPORT |
| W2/W3/W4/W5 | SM / backlog | Pre-existing: clippy lint debt (observe/engine), host-file monoliths, `http::token` flake, RUSTSEC-2023-0071 — track outside vnc-025 |

## Scope Concerns

None. No finding indicates wrong scope, unworkable technology, or an architecture unable to
support a requirement.
