## ADR-004: transcript_delta as a New event_type Value with a REQUIRED Accept-and-Drop Guard

### Context
ass-069 Q2 defines client-streamed transcript deltas as the forward path for authoritative
conversation capture. The carrier already exists: `RecordEvent` wraps a flattened `ImplantEvent`
(`wire.rs:115`), `event_type` is a free-form `String` (`wire.rs:204`), and `payload` is
`serde_json::Value` (`wire.rs:211`). No new wire variant is needed — `transcript_delta` is a new
*value* of `event_type`, carrying `{ offset, bytes }` in the existing payload. This keeps the change
backward-compatible and codegen-stable (Constraint 3).

The `{ offset, bytes }` payload is the **one genuinely-new field** this feature introduces. Because
it rides `ImplantEvent.payload: serde_json::Value`, ts-rs would emit it as untyped `any`/`JsonValue`
and F2's TS client would hand-type it — reintroducing the hand-mirror drift this feature exists to
kill. So a typed payload struct is defined for it (ADR-001's 6th exported type), used both as the
cross-language contract and as the deserialization shape this guard parses into:

```rust
#[derive(serde::Deserialize, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../bindings/")]
pub struct TranscriptDeltaPayload { pub offset: u64, pub bytes: String }
```

The wire carrier is unchanged — no new `HookRequest`/`RecordEvent` variant, the value still travels
through `ImplantEvent.payload: serde_json::Value`. The struct exists only to (a) give the field a
typed cross-language binding and (b) be the `(de)serialization shape` for the guard below.

But the dispatch fall-through is **not** safe to inherit. In `listener.rs`, the `RecordEvent` arm
persists *any* unrecognized `event_type` as a generic observation to durable SQLite storage
(`listener.rs:849-863`, `insert_observation`). A `transcript_delta` payload is **raw conversation
bytes**, which ass-069 Q4 states may contain secrets/keys and must **never** reach durable storage
(PRODUCT-VISION principle 8 — "no secrets in any database"). This is a silent-fallthrough exactly
matching pattern #4311: valid-looking output, no error, wrong durability (SR-07). Sequencing ("no
client streams deltas until #670 builds the buffer") is a release-ordering accident, not a safety
property, and cannot be relied on. Both transports reach this arm — HTTP `/observe` via
`router.rs:234 → dispatch_request`, and UDS via the listener loop — so the hole exists on both.

### Decision
Add an **explicit accept-and-drop branch** to the `RecordEvent` arm in `dispatch_request`, placed
**after** the `SessionWrite` capability check and `sanitize_session_id` (`listener.rs:757`) and
**before** any persistence (before the feature-extraction at `:793` and the observation insert at
`:849`):

```rust
if event.event_type == TRANSCRIPT_DELTA_EVENT {
    // ass-069 Q4 / principle 8: raw conversation bytes may contain secrets.
    // Accept-and-drop until #670 wires the in-memory buffer that legitimately consumes them.
    return HookResponse::Ack;
}
```

- Define `pub const TRANSCRIPT_DELTA_EVENT: &str = "transcript_delta";` as a shared constant
  (mirroring the `CYCLE_START_EVENT` pattern from ADR-001 col-022) so the hook/listener coupling is
  not stringly-typed.
- Because both transports converge on `dispatch_request`, **one branch covers HTTP and UDS** — no
  HTTP-specific code is required (SR-07).
- The `RecordEvents` batch arm must apply the same drop per-element: a batch containing a delta drops
  that element and persists the rest.
- The branch persists **nothing** and buffers **nothing** in memory. Accumulation (offset-merge,
  high-water, distill) is the re-scoped #670's job; pulling it forward is explicitly out of scope
  (SR-05). F1 proves only non-persistence.
- The wire contract documents `event_type: "transcript_delta"`, payload typed as
  `TranscriptDeltaPayload { offset: u64, bytes: String }`, and the precedence rule that streamed
  deltas supersede the legacy `CompactPayload.transcript_excerpt` (SR-06, ass-069). The payload
  struct ships as the 6th generated binding (ADR-001) and round-trips dual-sided in the ADR-002
  fixture set (AC-11). The guard parses the payload into `TranscriptDeltaPayload` rather than
  inspecting raw `serde_json::Value`, so the drop path and the typed contract share one shape.

A negative test asserts that a `transcript_delta` sent to `/observe` and over UDS returns `Ack` and
creates **zero** observation rows (AC-12). Per SR-07 / pattern #4311 this is a **gate prerequisite**
— it must be green before any downstream AC is trusted.

### Consequences
**Easier**: The secrets-to-disk hole is closed by construction before any client can stream. The
wire contract carries the delta from day one (ts-rs ships it) without a new variant or serde-compat
risk. The guard is a single, easily-reviewed branch covering both transports.

**Harder**: An implicit string-constant coupling between client and listener (mitigated by the shared
`TRANSCRIPT_DELTA_EVENT` constant). The guard is load-bearing — if a future refactor moves
persistence ahead of it, or adds another durable-write arm a delta can reach (assumption A3), the
hole reopens; the negative test is the canary. The 1 MiB frame ceiling (`MAX_PAYLOAD_SIZE`) and the
soft 64 KiB per-delta cap remain the client's concern (F2), not enforced here.

Cross-references: ADR-001 col-022 (event_type-as-routing precedent); ADR-002 (the round-trip
fixture); ADR-005 (`transcript_retention` is the purge policy the #670 buffer will obey).
