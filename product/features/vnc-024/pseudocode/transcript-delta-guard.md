# Component 4 — transcript_delta Accept-and-Drop Guard (Deliverable 3) — GATE-CRITICAL

> ADR-004. **AC-12 / R-03 is a gate prerequisite (principle 8).** A `transcript_delta` event on
> `/observe` (HTTP) AND over UDS — and inside a `RecordEvents` batch — must yield `Ack` and create
> **ZERO durable observation rows**. Raw conversation bytes may contain secrets and must never reach
> SQLite. The guard **early-returns `Ack`** — it MUST NOT reuse the col-022 specialize-then-
> fall-through pattern (#1266), which still persists.

## Purpose

Intercept `event_type == "transcript_delta"` in the `RecordEvent` dispatch (and the `RecordEvents`
batch arm), parse the payload into the typed `TranscriptDeltaPayload`, and return `Ack` — persisting
nothing, buffering nothing. Both transports converge on `dispatch_request`, so one guard per arm
covers HTTP and UDS. Until #670 wires the legitimate in-memory consumer, accept-and-drop IS the
secrets guarantee (Constraint 9 — no secret-scanner exists to license persistence).

## Files

| File | Action | Anchor |
|------|--------|--------|
| `crates/unimatrix-engine/src/wire.rs` | Modify — `TRANSCRIPT_DELTA_EVENT` const + `TranscriptDeltaPayload` struct | (shared; defined in Component 1) |
| `crates/unimatrix-server/src/uds/listener.rs` | Modify — guard in `RecordEvent` arm AND `RecordEvents` batch arm | RecordEvent :736 (after :757, before :793/:849); RecordEvents :868 (before :972 batch persist) |

## CONTRAST: this guard vs the col-022 pattern (#1266) — the central trap (R-03)

| | col-022 `CYCLE_START_EVENT` (existing, listener.rs:767) | `transcript_delta` (THIS guard) |
|---|---|---|
| Shape | specialize **then fall through** to generic persistence | **early `return Ack`**, never reach persistence |
| After the branch | `:793` feature-extract + `:849` `insert_observation` STILL RUN | NOTHING runs — `:793`/`:849` unreachable for a delta |
| Why | cycle events SHOULD be recorded as observations | delta bytes may be secrets — MUST NOT be recorded |

> A delivery agent reusing #1266 by muscle memory (specialize-then-fall-through) reintroduces the
> secrets-to-disk hole (R-03). The zero-rows gate test is the canary. The guard is a REQUIRED
> structural gate, not an optimization (Constraint 3).

## RecordEvent arm — guard placement (listener.rs:736)

Insert the guard AFTER the `SessionWrite` capability check (:737-742) and `sanitize_session_id`
(:747-757), and BEFORE the col-022 lifecycle routing (:767), feature-extraction (:793), and the
observation insert (:849). It rides the existing `RecordEvent` gating — no new auth surface (NFR-04).

```
HookRequest::RecordEvent { event } => {
    // (:737) capability check — UNCHANGED
    IF NOT capabilities.contains(SessionWrite): RETURN Error{ -32003, "... SessionWrite required" }

    // (:747) sanitize_session_id — UNCHANGED (SEC-01, load-bearing before any registry/DB write)
    IF sanitize_session_id(event.session_id) is Err(e): RETURN Error{ ERR_INVALID_PAYLOAD, e }

    // ===== vnc-024 ADR-004 accept-and-drop guard — INSERT HERE (after :757, before :767) =====
    IF event.event_type == TRANSCRIPT_DELTA_EVENT:
        // principle 8 / ass-069 Q4: raw conversation bytes may contain secrets and must never
        // reach durable storage. Accept-and-drop until #670 wires the in-memory consumer.
        // Parse the payload into the typed shape (same as AC-11 contract) — defensive/contract
        // alignment only; the DROP does not depend on the parse succeeding.
        LET parsed = serde_json::from_value::<TranscriptDeltaPayload>(event.payload.clone())
        MATCH parsed:
            Ok(delta)  -> tracing::debug!(offset = delta.offset, "transcript_delta accepted-and-dropped")
            Err(e)     -> tracing::debug!(error = %e, "transcript_delta dropped (unparsed payload)")
        // EARLY RETURN — persists nothing, buffers nothing. :793 / :849 are unreachable for a delta.
        RETURN HookResponse::Ack
    // ===========================================================================================

    // (:767+) col-022 lifecycle routing, #198 feature_cycle, col-017 topic signal — UNCHANGED
    // (:849) insert_observation — UNCHANGED, but NEVER reached by a transcript_delta
    ...
    HookResponse::Ack
}
```

### Parse semantics (R-03 edge — guard keys on event_type, NOT payload shape)
- The guard's DROP decision is `event_type == TRANSCRIPT_DELTA_EVENT` ONLY. A delta with a missing
  `offset`/`bytes`, empty `bytes`, `offset: 0`, or extra keys is STILL dropped and STILL `Ack`.
- The `from_value::<TranscriptDeltaPayload>` parse is the contract-alignment step (drop path and
  AC-11 share one shape, ADR-004) and a debug observability hook. A parse error MUST NOT change
  control flow — it logs at debug and still returns `Ack`. **Do not propagate a parse error as
  `HookResponse::Error`** (the contract is fire-and-forget `Ack`; erroring would leak shape info and
  break backward-compat for malformed clients — RISK-TEST-STRATEGY edge case).
- `event.payload.clone()` is used because the generic path borrows `event` later; for the early-
  return delta path the clone is cheap and the event is dropped immediately after. (Delivery may
  use `&event.payload` with `from_value` on a reference if the borrow checker allows — equivalent.)

## RecordEvents batch arm — per-element drop (listener.rs:868) — R-04

A batch containing a delta must drop THAT element while the rest persist normally. The batch arm
builds `obs_batch` at :972-987 and inserts at :988. Filter deltas out before the persistence build,
so they never enter `obs_batch`. Capability check (:869) and per-event `sanitize_session_id`
(:878-890) are UNCHANGED and run first for the whole batch.

```
HookRequest::RecordEvents { events } => {
    // (:869) capability check — UNCHANGED
    // (:878) per-event sanitize_session_id over the whole batch — UNCHANGED
    // (:893-970) feature_cycle + topic-signal accumulation — these MAY iterate `events`;
    //   they are side-effect-only (registry signals), they do NOT persist the delta bytes to disk,
    //   so a delta passing through them is acceptable. The DURABLE write is the obs_batch insert.

    // ===== vnc-024 ADR-004: exclude deltas from the durable observation batch — INSERT HERE
    //       (at the obs_batch construction, :975) =====
    LET obs_batch: Vec<ObservationRow> = events.iter()
        .filter(|event| event.event_type != TRANSCRIPT_DELTA_EVENT)   // DROP deltas from persistence
        .map(|event| { ... existing extract_observation_fields + enrich + phase ... })
        .collect();
    // (:988) insert_observations_batch(obs_batch) — UNCHANGED; deltas are absent from obs_batch
    // ====================================================================================

    HookResponse::Ack   // whole-batch Ack unchanged; the delta element simply persisted nothing
}
```
> **Reviewer note (R-04 / SR-05):** the delta element must not be accumulated into any in-memory
> transcript buffer here either — F1 proves only NON-persistence; buffering is #670's job. The only
> change is the `.filter(...)` excluding deltas from `obs_batch`. Confirm the topic-signal /
> feature_cycle loops (:897-970) do not durably write the delta payload — they record registry
> signals derived from `topic_signal`/`feature_cycle` fields, not the `bytes` payload, so they are
> safe; but a reviewer must verify no future durable-write arm a delta can reach was added (ADR-004
> assumption A3).

## State machine

None. The guard is stateless per-event: `event_type` match → parse (observability) → `Ack`. No
accumulation, no buffer, no session state mutation on the delta path.

## Data flow

```
ImplantEvent { event_type: "transcript_delta", payload: {offset,bytes}, session_id, ... }
  ► capability(SessionWrite) ► sanitize_session_id
  ► GUARD: event_type == TRANSCRIPT_DELTA_EVENT
       ► from_value::<TranscriptDeltaPayload>(payload)  (debug log only; control flow independent)
       ► return Ack
  ► [feature-extract :793 / topic-signal :817 / insert_observation :849]  ◄── UNREACHABLE for delta
```
Both transports reach this arm: HTTP via `router.rs:234 → dispatch_request`; UDS via the listener
loop. One branch per arm covers both (SR-07). HTTP's `prefix_session_id` runs before dispatch and
does not bypass the guard (the integration test asserts this — R-04 integration risk).

## Error handling

| Condition | Behavior |
|-----------|----------|
| Missing `SessionWrite` | `Error{-32003}` — guard sits AFTER the check, inherits gating (NFR-04) |
| Invalid `session_id` | `Error{ERR_INVALID_PAYLOAD}` — guard sits AFTER `sanitize_session_id` |
| `event_type == transcript_delta`, well-formed payload | `Ack`, zero rows |
| `event_type == transcript_delta`, malformed/empty/extra-keys payload | `Ack`, zero rows (debug log; NO `Error`) |
| `event_type` anything else | unchanged generic path (col-022 / #198 / insert_observation) |

## Constraints honored

- **No new wire variant** (Constraint 3): new `event_type` VALUE on existing `ImplantEvent`; carrier
  unchanged. Generated bindings for existing types are diff-empty (NFR-03).
- **Early return, not fall-through** (R-03 / #1266): persistence is unreachable for a delta.
- **Both transports + batch arm** (R-04): single-transport or no-batch coverage does NOT satisfy AC-12.
- **No buffering** (SR-05): F1 proves non-persistence only; #670 owns accumulation.
- **Shared constant + shared shape** (ADR-004): `TRANSCRIPT_DELTA_EVENT` and `TranscriptDeltaPayload`
  come from `wire.rs` (Component 1).
- **No secret-scanner reliance** (Constraint 9): the drop IS the guarantee.

## Key test scenarios (hints — full plan in test-plan/transcript-delta-guard.md) — AC-12 GATE

1. **GATE**: `RecordEvent` with `event_type=="transcript_delta"`, `payload={offset,bytes}` →
   response `Ack` AND observation-row count **unchanged (zero rows for the delta)** — run **twice**:
   once via HTTP `POST /observe`, once via direct UDS dispatch. (R-03 + R-04.)
2. **Batch**: `RecordEvents` with one delta among normal events → delta persists nothing, the rest
   persist normally (row count == number of non-delta events). (R-04.)
3. **Structure/anti-pattern**: assert the `return Ack` sits after `sanitize_session_id` but before
   `:793`/`:849`; confirm the branch does NOT follow #1266's specialize-then-fall-through shape. (R-03.)
4. **Edge**: `offset:0` + empty `bytes` → `Ack`, zero rows. Malformed payload (missing field / extra
   keys) → `Ack`, zero rows, no error. (RISK-TEST-STRATEGY edge cases.)
5. **Auth**: a `transcript_delta` request lacking `SessionWrite` is rejected exactly as any
   `RecordEvent` (guard is after the capability check). (NFR-04.)

## Open questions / gaps

- **Batch-arm pre-persistence loops (:897-970)**: confirmed safe (registry-signal side effects, not
  durable `bytes` writes) but flagged for reviewer verification per ADR-004 assumption A3 — no
  future durable-write arm a delta can reach must be introduced. Non-blocking; the zero-rows batch
  test is the canary.
- **`event.payload.clone()` vs borrow**: cosmetic; delivery picks whichever the borrow checker
  accepts. The DROP semantics are identical either way.
