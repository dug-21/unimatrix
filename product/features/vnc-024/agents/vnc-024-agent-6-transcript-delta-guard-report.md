# Agent Report — vnc-024-agent-6-transcript-delta-guard (Stage 3b, Wave 2, Component 4 — GATE-CRITICAL)

## Scope
Component 4 — `transcript_delta` accept-and-drop guard (Deliverable 3, ADR-004). AC-12 / R-03 /
principle 8: raw conversation bytes must never reach durable storage on any path.

## Files modified
- `crates/unimatrix-server/src/uds/listener.rs` — import of `TRANSCRIPT_DELTA_EVENT` + `TranscriptDeltaPayload` from `unimatrix_engine::wire`; guard in the `RecordEvent` arm; drop in the `RecordEvents` batch arm; 5 tests.

## Tests — 5 passed / 0 failed (listener::tests module: 198 passed, no regressions)
- `test_transcript_delta_uds_acks_zero_rows` (UDS direct dispatch → Ack + zero durable rows)
- `test_transcript_delta_in_batch_dropped_rest_persist` (batch: delta dropped, N=2 non-delta persist)
- `test_transcript_delta_malformed_payload_still_acks_zero_rows` (offset:0/empty/missing/extra keys → Ack, no Error)
- `test_transcript_delta_requires_session_write` (NFR-04 auth inheritance, code -32003)
- `test_transcript_delta_parses_into_typed_payload` (shared shape: parses into `TranscriptDeltaPayload`, not raw `Value`)

## Confirmed
- RecordEvent early return: immediately after the `sanitize_session_id` block + its log, BEFORE col-022 lifecycle routing (:767), feature-extraction (:793), and `insert_observation` (:849) — `:793`/`:849` provably unreachable for a delta. Parses `event.payload.clone()` into `TranscriptDeltaPayload`; parse drives only a `debug!` and never alters control flow (Err still returns Ack).
- Batch arm: a single `.filter(event_type != TRANSCRIPT_DELTA_EVENT)` on the `obs_batch` build (:975); deltas never enter the `Vec<ObservationRow>` passed to `insert_observations_batch`. The pre-persistence #198/col-017 loops record only registry signals from `feature_cycle`/`topic_signal` fields, not payload bytes — not a durable delta-bytes path.
- Does NOT reuse the col-022 #1266 specialize-then-fall-through. No new wire variant (Constraint 3). No in-memory accumulation (SR-05 — #670 not pulled forward). Guard sits after the existing SessionWrite check (Constraint 7).
- HTTP `/observe` transport half of AC-12 + full integration runs are Stage 3c.

## Knowledge Stewardship
- Queried: `context_briefing` + 3 `context_search` — #1266 (col-022 specialize-then-fall-through anti-pattern), #4711 (new event_type must not inherit generic-observation disk fall-through), #4720 (ADR-004), #763 (observation intercept). Applied the #1266 contrast directly.
- Stored: entry **#4723** (pattern, topic `unimatrix-server`) — "Accept-and-drop guard in listener.rs: early-return in single arm, filter() in batch arm — not symmetric"; captures the two-arm asymmetry and the non-obvious batch pre-persistence-loop safety fact, neither visible in source.
