# Test Plan — C6 Distill Helper / Handler Glue

**Component**: `unimatrix-server/src/mcp/distill_handler.rs` —
`distill_before_purge(registry, feature_cycle, &observations, cfg) -> Option<TranscriptCandidatesSection>`;
plus the four `result.is_ok()` call sites in `tools.rs:2110/2236/2925/3027` and assembly-level attach.
**ADRs**: ADR-005 (four returns + exhaustive gate), ADR-004 (assembly attach), ADR-006 (fallback
trigger), ADR-007 (loss assembly). **Wave**: A. **Tests live in**: `mcp/distill_handler.rs`
`#[cfg(test)]` + a server-level cycle-review test module. **Merge gates**: four-return exhaustiveness
(AC-05), content-leak (AC-06), AC-V-FUZZ (handler level), Wave-boundary (R-11).

## Four-return wiring (AC-05, **merge gate**, R-07)
- `test_distill_before_purge_at_return_2110` (purged-signals), `_2236` (cached-MetricVector), `_2925`
  (memoization-hit), `_3027` (full-pipeline) — one per-path test exercising each `result.is_ok()`
  return; assert at EACH: distill runs, THEN `purge_cycle_transcripts` runs (distill strictly precedes
  purge), via the ONE shared helper (no per-site copy).
- `test_exhaustiveness_fifth_return_fails` (**merge gate**) — a regression test that FAILS if a fifth
  success return is added without wiring the shared helper (modeled on vnc-025's purge exhaustiveness
  shape, #4750). The guard: enumerate the `result.is_ok()` purge sites and assert each is preceded by
  the distill helper.
- `test_memoization_hit_distills_fresh` (AC-05, OQ-4, #3800) — on a cache hit (return #2925) the
  deserialized cached report is returned UNCHANGED while candidates are distilled FRESH from call-time
  buffer content; assert candidates MAY differ from the cached report (documented divergence).
- `test_error_path_keeps_transcripts_no_candidates` — error paths (`result.is_err()`) keep transcripts
  and produce no candidates (helper not called / returns None).

## Retention gate (AC-10) — delegated to retention-gate.md
- The exhaustive `TranscriptRetention` match (`PurgeOnCycleClose` proceed, `RetainDays(_)` → None) is
  asserted in retention-gate.md; here `test_helper_returns_none_on_retaindays` confirms the helper's
  gate step returns `None` (no distill, no purge) when `cfg.transcript_retention` is `RetainDays`.

## Fallback trigger (AC-07 (i)(ii), R-09) — owned here (reads snapshot metadata)
- `test_trigger_empty_snapshot_falls_back` — snapshot yields no user/assistant blocks after JSONL
  filtering → reconstruct whole-session (calls reconstruct.rs).
- `test_trigger_elided_above_threshold_falls_back` — `elided_bytes > 0` indicating ring-tail clipping
  at/above threshold → fallback; asserted against ADR-002 tail-window-equivalence, NOT assumed
  losslessness (SR-08).
- `test_trigger_holes_fraction_boundary` — holes covering MORE than the configured fraction → fallback;
  below → primary path. Boundary at the fraction edge.
- `test_trigger_at_cap_edge_and_overflow` (R-09 boundary, **cite #4764**) — buffer at the 4 MiB cap
  edge and under ring-tail overflow → trigger evaluates against tail-window-equivalence; assert the
  exact calibration points SR-08 flags. No over-fire (good buffer discarded) / under-fire (clipped
  tail shipped as full).
- `test_trigger_whole_session_either_or` (AC-07, OQ-2) — assert no byte-level primary/reconstructed
  mix within one session; the session is entirely Primary or entirely Reconstructed.

## Per-cycle aggregate cap (AC-02, AC-08, R-15) — owned here (across the union)
- `test_cycle_cap_truncation_chronological_keep_earliest_repeatable` — candidates exceeding
  `transcript_candidate_cycle_cap_bytes` (≈256 KB) across sessions → DETERMINISTIC truncation,
  chronological keep-earliest (per brief); assert repeatable across runs (no flakiness).
- `test_session_and_cycle_caps_independent` — the per-session cap (24 KB, in select.rs) and the
  per-cycle aggregate cap are two independent knobs (FR-4); exercise each in isolation.
- `test_aggregate_cap_single_oversized_vs_many_small` — cap hit by one oversized session vs many small
  → deterministic, documented order.

## Loss assembly (AC-08, ADR-007) — owned here (assembly step)
- `test_loss_row_for_elided_session` — a session with `elided_bytes > 0` produces a `SessionLossInfo`
  row in `section.loss`.
- `test_loss_row_for_reconstructed_session` — reconstructed session → `provenance: Reconstructed` loss
  row, even with zero candidates (loss never invisible).
- `test_loss_omitted_for_clean_primary` — a clean primary session with no loss may be omitted from
  `loss` (silence == no loss to report).
- `test_loss_predicate_same_as_fallback` — the Primary/Reconstructed label is derived from the SAME
  predicate ADR-006 uses for the trigger, not a re-computation (ADR-007 warning).
- `test_aggregate_cap_drop_surfaces_count` (AC-08) — per-cycle cap truncation surfaces a dropped-count
  / affected-session (no silent aggregate-cap drop).
- `test_poison_recovery_surfaces_loss` (R-16) — a poison-recovered (treat-as-empty) session surfaces
  as lossy in `SessionLossInfo`, not silently absent.

## Content-leak (AC-06, **merge gate**, R-04, R-19)
- `test_candidates_attached_after_memoization` — assembly attaches `transcript_candidates` AFTER
  `store_cycle_review()` (the synchronous SQL persist #3793) — candidates never reach the persisted
  path.
- `test_rereview_stored_record_no_candidates` (AC-06(b)) — load the stored `cycle_review_index` record
  (cache-hit, #3800) → returned report carries NO stale candidates.
- `test_content_leak_grep_log_sql_gate` (AC-06(c)) — run a full cycle review with candidates present;
  assert no candidate or buffer bytes appear in any SQL write, file write, or log line across ALL new
  paths (Wave A + Wave B); extends vnc-025 AC-12. Includes a grep gate: no `#[derive(Debug)]` on
  content-bearing snapshot types (R-19).
- `test_audit_detail_content_free` (AC-06(d)) — `transcript_session_purged.detail` is
  `bytes=<n> trigger=<…>`, no transcript bytes (overlaps held-buffer-store.md R-03).

## AC-V-FUZZ — handler level (**merge gate**, R-10)
- `test_handler_fully_corrupt_snapshot_normal_response` — feed a fully-corrupt snapshot through
  `distill_before_purge` → the handler returns a normal review response (candidates absent), NEVER
  panics. (Module-level fuzz lives in selection-module.md; this is the handler blast-radius assertion.)

## Additive / absent-when-empty (AC-04)
- `test_zero_attributed_sessions_section_absent` — no attributed session yields candidates →
  `distill_before_purge` returns `None` → `transcript_candidates` absent from response.
- `test_golden_no_transcript_review_byte_identical` — golden-output diff of a no-transcript cycle
  review vs pre-crt-052 output: existing fields byte-identical (col-024/crt-033 behaviors unchanged).

## Two-pipe boundary (AC-09)
- `test_detection_isolation_with_distill_active` — extend the `detection_isolation` tests to run with
  distillation active; assert the 23 detection rules' inputs are BIT-IDENTICAL to pre-crt-052.
- `test_no_buffer_bytes_into_insert_observations_batch` — assert no new path feeds buffer bytes into
  `insert_observations_batch`; batch filter `listener.rs:1238` unchanged.

## Wave-boundary (R-11, **merge gate**)
- `test_wave_a_handler_no_transcript_hold_dependency` — source/dep assertion: the handler + `distill/`
  + seam + response types have ZERO compile-time reference to `transcript_hold.rs`. Reverting Wave B
  leaves Wave A compiling.
- `test_wave_a_only_empty_buffers_degrade_to_fallback` — with no held-buffer machinery and every
  buffer empty at call time, the handler degrades cleanly to the reconstruction fallback (AC-07) — a
  real tested mode, not degenerate.

## Assertions Summary (concrete)
- Distill→purge fires at all four `result.is_ok()` returns via one helper; a fifth unwired return
  fails the exhaustiveness test.
- Candidates are attached at assembly AFTER memoization; never persisted; re-review returns none.
- Fallback trigger reads snapshot metadata against tail-window-equivalence; whole-session either/or.
- Per-cycle aggregate-cap truncation is deterministic (chronological keep-earliest) and surfaces loss.
- Handler never panics on corrupt input; section absent when empty.
