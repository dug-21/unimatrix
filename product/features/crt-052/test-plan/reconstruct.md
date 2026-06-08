# Test Plan — C5 Reconstruction Fallback

**Component**: `unimatrix-observe/src/distill/reconstruct.rs` —
`reconstruct_from_observations(session_id, obs: &[ObservationRecord], session_cap) -> Vec<TranscriptCandidate>`.
**ADRs**: ADR-006 (trigger + topic_source), ADR-007 (provenance), ADR-002 (tail-window-equivalence).
**Wave**: A. Pure: no I/O, no buffer write, no observation-row insert.
**Tests live in**: `#[cfg(test)] mod tests` in `reconstruct.rs`. The TRIGGER (when to fall back)
lives in the handler glue (distill-handler.md); this module is the reconstruction itself + topic_source
ordering.

## Unit Test Expectations — reconstruction (AC-07)
- `test_reconstruct_builds_from_observations` — input from `ObservationRecord` (tool, input,
  response_snippet ≤500 chars); emits `TranscriptCandidate`s labeled provenance `Reconstructed`
  (the label is carried per-session via `SessionLossInfo`, set by the handler — assert the candidates
  are tagged so the handler can mark provenance).
- `test_reconstruct_distillation_input_only` (AC-07(iii)) — assert the function NEVER writes the byte
  buffer and NEVER produces observation rows (pure, distillation-input only). Structural: no write/
  insert symbol reachable.
- `test_reconstruct_respects_session_cap` — output bounded by `session_cap`.
- `test_reconstruct_empty_observations` — no observations → empty Vec (caller emits a loss row with
  zero candidates so the loss stays visible).

## topic_source SOFT preference (AC-07(iv), R-14)
- `test_topic_source_reorders_declared_first` — observations with mixed `topic_source`
  (declared/extracted/registry-fill/vote/NULL) → reconstruction ORDERS declared/registry-fill rows
  ahead of vote/extracted; assert a stable sort key, not a filter.
- `test_topic_source_drops_no_observation` (R-14) — assert NO observation is dropped for its
  `topic_source`; every feature-matched observation contributes.
- `test_all_vote_observations_still_reconstruct` — a session whose observations are all `topic_source
  = vote` still reconstructs (not filtered out). Guards the SR-06 banned-hard-filter regression.
- `test_topic_source_read_only` — crt-052 reads the already-loaded `topic_source` column only; never
  persists or re-derives it.

## Fidelity floor (ADR-006)
- `test_reconstruct_is_degraded_label_present` — candidates from reconstruction are discriminable from
  primary by the `Reconstructed` provenance label (the 0.81-ceiling fidelity floor is made
  discriminable by labeling, not parity). No recall threshold asserted here (that is AC-03 for the
  primary path); the label is the load-bearing artifact.

## Cross-component (trigger + provenance — exercised here, owned by distill-handler)
- The whole-session either/or trigger and the cap-edge / ring-tail boundary tests (AC-07 (i)(ii),
  R-09) live in distill-handler.md because they read the `TranscriptSnapshot` metadata produced by the
  seam. This module is invoked by the trigger; its tests assume the trigger already fired.

## Wave-boundary (R-11)
- `test_reconstruct_no_transcript_hold_reference` — zero compile-time reference to `transcript_hold.rs`
  (Wave A module). Reconstruction is precisely the Wave-A degrade mode when every buffer is empty.

## Assertions Summary (concrete)
- `reconstruct_from_observations` is pure: no buffer write, no observation insert, no I/O.
- topic_source is a STABLE SORT KEY only — reorders, drops nothing, excludes no feature-matched session.
- All output is labeled `Reconstructed` (per-session provenance), the discriminable fidelity floor.
