# Agent Report: vnc-025-agent-8-cycle-review-purge

Status: COMPLETE (not committed — per spawn instructions)

## Files Modified

- `crates/unimatrix-server/src/server.rs` — `retention_config: Arc<RetentionConfig>` field + test-ctor default (#561 store_config precedent); new `purge_cycle_transcripts(&self, feature_cycle)` helper holding the exhaustive `TranscriptRetention` match (FR-16, Constraint 7) + `emit_purge_audits(..., "cycle_review")` emission; `mod tests` → `pub(crate) mod tests` so tools.rs tests can reuse `make_server`; 4 new tests + 2 test helpers.
- `crates/unimatrix-server/src/main.rs` — `server.retention_config = Arc::clone(&retention_config);` at both daemon (~:765) and stdio (~:1194) paths, reusing the existing crt-036 snapshot Arc.
- `crates/unimatrix-server/src/mcp/tools.rs` — `context_cycle_review`: `self.purge_cycle_transcripts(&feature_cycle)` gated on `result.is_ok()` at all four success-return sites (purged-signals, cached-MetricVector, memo-hit, final full-pipeline dispatch). Error paths unreachable from the purge (Gate 3a disposition 2). 1 new test.
- `crates/unimatrix-server/src/uds/listener.rs` — `emit_purge_audits` trigger doc comment extended with `"cycle_review"` (1 line; shape unchanged).
- `crates/unimatrix-server/src/infra/session_transcript_tests.rs` — `test_post_clear_resumed_stream_serves_tail` (§3, R-10.2/.3 gap-resume pin).

## Design Notes (deliberate, non-silent adaptations of pseudocode)

1. **Four success returns, not one.** Pseudocode pins a single insertion "before returning Ok(result)"; the handler factually has four success-return points (purged-signals ~:2100, cached-MetricVector ~:2215, memo-hit ~:2915, final dispatch ~:2960). Test-plan scenario 4 (cached re-review purges idempotently) requires the memo-hit site, so the match was factored into `purge_cycle_transcripts` (server.rs, beside `audit_fire_and_forget`) and called at every site gated on `result.is_ok()` — error paths (incl. unknown-format Err) keep transcripts. Stored as pattern #4750.
2. **Audit emission reuses `emit_purge_audits`** (the Wave 3a shared helper, `tokio::spawn` + `log_event_async`) instead of open-coding `audit_fire_and_forget` per the pseudocode sketch — identical mechanics, and the ADR-004 pinned shape stays defined in exactly one place. Zero-byte purges spawn nothing.
3. **Gate 3a disposition 6 (RetainDays arm)**: direct enum injection succeeded — `RetentionConfig` fields are pub; `test_cycle_review_retain_days_arm_does_not_purge` assigns `RetainDays(30)` to `server.retention_config` (bypassing, never weakening, `validate()`) and asserts no clear + no audit. No fallback to compile-gate-only needed.
4. **§3 `test_clear_return_and_metadata_semantics_pinned` not duplicated**: its assertions are already pinned verbatim by the committed `test_clear_returns_bytes_purged` (session_transcript_tests.rs) — clear() return = span length, `base_offset = high_water`, `high_water`/`elided_bytes` unchanged, below-floor no-ops. Only the far-gap resume case was missing; added as `test_post_clear_resumed_stream_serves_tail`.
5. **`server::tests` made `pub(crate)`** so `test_cycle_review_output_unchanged_by_purge` (tools.rs) reuses `make_server` — cumulative test infra, no parallel fixture.

## Test Results

New component tests (6) — all pass:
- `server::tests::test_cycle_review_clears_only_matching_feature_sessions` (§1 R-10.1 + idempotent re-purge)
- `server::tests::test_cycle_review_zero_attributed_sessions_noop` (§1 R-10.5)
- `server::tests::test_cycle_review_purges_under_purge_on_cycle_close` (§2 FR-16 default gate)
- `server::tests::test_cycle_review_retain_days_arm_does_not_purge` (§2 enum injection)
- `mcp::tools::tests::test_cycle_review_output_unchanged_by_purge` (§4 AC-09, consumes committed Wave 0 fixtures)
- `infra::session_transcript::tests::test_post_clear_resumed_stream_serves_tail` (§3 R-10.2/.3)

Hard gates verified:
- `test_cycle_review_render_baseline_byte_identical` (Wave 0, AC-09) — passes unmodified.
- Full lib run: **3600 passed, 0 failed** (`cargo test -p unimatrix-server --lib`).
- `cargo build --workspace` clean; clippy warning count identical pre/post (431, all pre-existing); `cargo fmt` applied.
- Integration tests untouched (per spawn instructions).

## Issues / Blockers

None.

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing — surfaced ADR-004 (#4742), ADR-006 (#4744), vnc-024 ADR-005 (#4721), ADR-001 (#4739); all already read from architecture files. Pattern #4748 (lock_buffer poison recovery) already applied in committed registry code — no rediscovery needed.
- Stored: entry #4750 "context_cycle_review has four success-return points — success-only side effects must gate every one" via /uni-store-pattern (topic unimatrix-server; crt-052 inherits this seam shape).
