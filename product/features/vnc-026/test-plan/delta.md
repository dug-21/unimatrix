# Test Plan: delta.js (offset tracking + delta POST)

New module (mechanism per ass-069 Q2). ADR-004 (never queued, uniform advance), ADR-007 (separate
concurrent POST), ADR-008 (end-anchored elision). Risks: R-04 (High), R-05, R-06, R-07, R-17;
AC-06, AC-07, AC-08, AC-09 (delta arm), AC-15 (amended delta arm), AC-10.
Suites: `test/hook-client/delta.test.js` (unit) + `test/hook-client/parity-layer2.test.js`
(integration vs merged F2 server, PR #692).

## UTF-8 Boundary Trim (R-04 — High)

- `test_trim_mid_2byte / mid_3byte / mid_4byte` — file ends mid-sequence: span end backs off to last complete char; shipped `bytes.length` == bytes actually shipped; persisted offset == `declared offset + bytes.length`; next spawn's span starts boundary-clean (no replacement chars in concatenation).
- `test_span_inside_one_multibyte_char` — file grew by 1–3 continuation bytes only → ship NOTHING, offset unchanged.
- `test_growth_replay_sequence` — grow file in adversarial increments (1 B, mid-char, exactly-64 KiB, huge) across 10 spawns → Layer 2: final server buffer byte-equals the transcript file.
- `test_property_contiguous_prefix` — property-style: random multi-byte content, random growth steps → invariant `concat(shipped spans) == contiguous prefix of file` and `last_offset == prefix length` after every spawn (AC-06 coverage requirement: assert persisted offset VALUES, not just POST presence).

## Frame Shape + Offset Advance (AC-06 / AC-07 / ADR-008)

- `test_normal_frame_declares_last_offset` — non-elided: `payload.offset == last_offset`; `bytes` = trimmed span; advance to `offset + bytes.length`.
- `test_no_growth_no_post` — `file_len == last_offset` → no delta POST (fstat-gated; fs spy shows stat then no read).
- `test_elided_frame_end_anchored` — span > 64 KiB: SINGLE frame; `bytes = head(48 KiB) ++ "…[N bytes elided]…" ++ tail(12 KiB)`; **declared `offset == file_len − bytes.length` — explicitly assert it is NOT the span start `last_offset`**; `offset + bytes.length == file_len`; persisted offset advances to `file_len` (uniform rule, no special case); elided bytes never re-sent on subsequent spawns.
- `test_elision_truncation_at_multibyte_boundary` — 48 KiB head cut and 12 KiB tail cut each landing mid-char → byte-safe cuts.
- `test_post_serialization_1mib_assert` — escape-heavy content (~6x inflation, control-char dense): serialized frame < 1 MiB; the post-serialization re-truncate path exercised (SR-04).
- `test_frame_matches_binding` — frame validates against `TranscriptDeltaPayload`/`RecordEvent` contract fixtures (AC-14 tie-in, build-request.md).

## Failure Semantics (ADR-004 — AMENDED AC-15; offset re-drive)

- `test_delta_failure_no_advance_no_queue` — stub fails the delta POST: `last_offset` file unchanged AND **`queue/` contains NO file for the delta** (the amended AC-15 letter; never assert queue presence). Next FNF spawn re-derives `[last_offset, file_len)` — possibly larger span — and ships it.
- `test_delta_independence` — `Promise.allSettled` outcomes independent (FR-10/AC-09): delta fails + carrying event succeeds (carrying frame NOT queued); carrying fails + delta succeeds (carrying frame queued per FR-12, delta offset advances); both POSTs concurrent (stub records overlapping in-flight requests or both received without serialization delay).
- `test_livelock_bounded` (R-07) — stub permanently 413 (then 401) on delta path only, across 5 spawns: offset never advances; no queue file ever; carrying events unaffected; breadcrumb records class; per-spawn delta cost is exactly one fstat + one failed POST (fs/network spies — no growth in work).

## Rewrite Guard + TOCTOU (FR-11 / A-4, edge cases)

- `test_rewrite_guard` — `file_len < last_offset` → reset `last_offset = file_len`, ship nothing, never a negative span.
- `test_toctou_delete_between_stat_and_read` — file deleted after fstat → read throws → ship nothing, offset unchanged, exit 0.
- `test_corrupt_offset_file` — non-JSON / negative / non-numeric offset → treated as 0 (re-ship from 0, safe via idempotent merge), never throws.
- `test_binary_transcript` — huge non-JSONL binary at `transcript_path` → content-opaque: caps and ships (or skips) without throwing.

## Concurrency (R-05)

- `test_concurrent_spawn_offset_race` — two spawns of one session interleaved: offset file never observably partial (atomic rename — read loop sees only valid JSON); worst case a re-shipped span; Layer 2: final server buffer correct (F2 dedupe).

## Layer 2 Integration (merged F2 server — AC-05/AC-07/AC-10, R-06, R-17)

Pre-population/inspection isolated behind ONE helper (SR-11), pinned to wire behavior + committed fixtures, not vnc-025 internals.

- `test_l2_elision_mid_session` — outage → >64 KiB growth → catch-up delta with elision marker → subsequent normal delta → PreCompact restoration succeeds (W5-with-a-hole). **Four pinned ADR-008 assertions (gate-binding)**:
  1. hole forms BEHIND content: `holes == [(last_offset, file_len − bytes.length)]`; server `elided_bytes` counter unchanged;
  2. `high_water == file_len` (server coverage == client `last_offset`);
  3. `contiguous_tail(12000)` returns pure client-tail bytes immediately after the elided frame and crosses the elision seam once later deltas extend at `file_len`;
  4. no NUL bytes ever served (zero-fill never escapes `contiguous_tail`).
  Plus: post-elision delta extends contiguously at `file_len` (no further holes).
- `test_l2_drops_content_equivalence` — streamed deltas with injected drops → content-equivalence modulo elision markers (FR-24).
- `test_l2_concurrency_attribution` (AC-10/FR-26) — ≥8 interleaved sessions with per-session byte tagging + injected drops → each buffer holds only its own bytes; raw `session_id` on the wire (server-side buffer key is `http-{session_id}` — no double prefix).
