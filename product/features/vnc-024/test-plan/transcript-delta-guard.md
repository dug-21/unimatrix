# Test Plan — transcript-delta-guard (Deliverable 3) — GATE PREREQUISITE

> Covers **AC-12 (GATE)** + auth inheritance (NFR-04). Risks R-03 (secrets-to-disk hole, Critical),
> R-04 (one-transport / batch missed, Critical). Pseudocode: `pseudocode/transcript-delta-guard.md`.
> File: `uds/listener.rs` (accept-and-drop branch in `RecordEvent` arm + same drop in `RecordEvents`
> batch arm), `wire.rs` (`TRANSCRIPT_DELTA_EVENT` const, `TranscriptDeltaPayload`).
>
> **This is the gate prerequisite (SR-07 / #4711 / #4311): the zero-durable-rows test must be GREEN on
> BOTH transports + the batch arm before any downstream AC is trusted.** Principle-8 enforcement — raw
> conversation `bytes` may contain secrets and must NEVER reach SQLite. There is no content
> secret-scanner to lean on (Constraint 9); the accept-and-drop guard IS the secrets guarantee.

## What the guard must do (under test)

In `dispatch_request`, the `RecordEvent` arm gets an explicit branch, placed **after** the
`SessionWrite` capability check + `sanitize_session_id` (`~:757`) and **before** feature-extraction
(`:793`) and `insert_observation` (`:849`):

```
if event.event_type == TRANSCRIPT_DELTA_EVENT {
    // parse payload into TranscriptDeltaPayload (same shape as AC-11)
    return HookResponse::Ack;   // persist nothing, buffer nothing
}
```

The branch must **early-`return Ack`** — it must NOT reuse the col-022
"specialize-then-fall-through-to-generic-persistence" pattern (#1266), which reintroduces the hole.

## R-03 — zero-durable-rows (the GATE assertion)

### Direct-dispatch / UDS arm
- **test_transcript_delta_uds_acks_zero_rows**: dispatch a `RecordEvent` with
  `event_type == "transcript_delta"`, `payload = {offset, bytes}` over **direct UDS dispatch** →
  assert response is **`Ack`** AND the observation-row count is **unchanged (zero rows for the delta)**.
  Count rows by querying the `insert_observation` target table directly (not via search).

### Structure / unreachability assertion
- **test_guard_returns_before_persistence** (structure/code-level): the guard's `return Ack` sits
  after the `SessionWrite` check + `sanitize_session_id` but **before** feature-extraction (`:793`) and
  `insert_observation` (`:849`) — the disk insert is provably **unreachable** for a delta. Verify by
  ordering in the function and/or by asserting no DB write occurs even with a delta crafted to look like
  a persistable observation.

### Anti-pattern guard (#1266)
- Confirm the delta branch does **NOT** follow the col-022 specialize-then-fall-through shape — it
  early-returns, persisting nothing. (Reviewer + the zero-rows test as canary.) If a future refactor
  moves persistence ahead of the guard or adds a durable-write arm a delta can reach, the zero-rows
  test is what fails.

## R-04 — both transports + batch arm

A single-transport pass does NOT satisfy this risk; the hole exists exactly where the test does not look.

### HTTP transport arm
- **test_transcript_delta_http_acks_zero_rows**: the same delta via **`POST /observe`** (reaches
  dispatch via `router.rs:234`) → `Ack` + zero rows. Note the integration trap: `prefix_session_id`
  runs on the HTTP path before dispatch, so a UDS-only unit test could be bypassed — this end-to-end
  HTTP test is mandatory, not redundant.

### RecordEvents batch arm
- **test_transcript_delta_in_batch_dropped_rest_persist**: a `RecordEvents` batch containing **one**
  `transcript_delta` element among **N normal** events → assert the delta element persists **nothing**
  while the N normal events persist **normally** (delta dropped, rest survive). Assert exact row counts:
  N observation rows from the normal events, zero from the delta.

**Coverage requirement (hard):** all three arms — HTTP `/observe`, direct UDS, and the batch — asserted.

## Auth inheritance (NFR-04, Security Risks)

- **test_transcript_delta_requires_session_write**: a `RecordEvent` delta from an agent **lacking
  `SessionWrite`** is rejected on the delta path **exactly as any `RecordEvent`** — the guard sits
  AFTER the capability check, so no new auth surface is introduced. (No bearer/cap bypass via the new
  event_type.)

## Shared-shape coupling (with contract-fixtures)

- The guard parses the payload into the **typed `TranscriptDeltaPayload`** — the **same** struct the
  AC-11 dual-sided fixture round-trips. Assert the guard deserializes into `TranscriptDeltaPayload`, not
  raw `serde_json::Value`. The drop path and the typed contract share one shape; a divergence is a defect.

## Edge cases (from RISK strategy)

- `transcript_delta` with `offset: 0` and **empty `bytes`** → still `Ack`, still zero rows.
- Payload **missing `offset` or `bytes`**, or with **extra keys** → the guard keys on `event_type`
  **only**, so the event is still dropped (`Ack`, zero rows) and **must not error** trying to parse the
  payload. (Malformed-payload-still-dropped is an explicit Failure Mode.)
- A `transcript_delta` near `MAX_PAYLOAD_SIZE` (1 MiB frame ceiling) → still framed and dropped
  normally; no per-delta cap server-side (the soft 64 KiB cap is the F2 client's concern).

## SR-05 — non-persistence only, NOT buffering

- AC-12 asserts **zero rows**, never buffering. F1 proves only **non-persistence**. The reviewer
  rejects any in-memory transcript accumulation (offset-merge / high-water / distill) — that is the
  re-scoped #670's job, explicitly out of scope. No test asserts or requires buffering.

## Out of scope for this plan

- The dual-sided fixture round-trip of `TranscriptDeltaPayload` (AC-11) → `contract-fixtures.md`
  (shares the struct shape).
- Content negotiation / text formatting (AC-07/08/09) → `observe-content-negotiation.md`.

## Self-check
- [ ] **GATE:** zero-durable-rows asserted on HTTP `/observe` + direct UDS + RecordEvents batch — all three arms.
- [ ] Response is `Ack`; observation-row count unchanged (queried directly), for every arm.
- [ ] Structure: `return Ack` after SessionWrite+sanitize, before `:793`/`:849`; insert provably unreachable.
- [ ] Anti-pattern: early-return, NOT col-022 specialize-then-fall-through (#1266).
- [ ] Guard parses into typed `TranscriptDeltaPayload` (same shape as AC-11), not raw Value.
- [ ] Auth: delta path rejected without `SessionWrite` exactly as any RecordEvent (NFR-04).
- [ ] Edge: offset:0/empty bytes, malformed/extra-key payload still Ack+zero-rows-no-error, 1 MiB framing.
- [ ] SR-05: zero rows only; no buffering asserted/required.
