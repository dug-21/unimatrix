# vnc-024 Pseudocode — OVERVIEW (F1)

> Wire-Contract Codegen + Content Negotiation + transcript_delta + transcript_retention.
> Pure additive plumbing that freezes a contract F2–F5 / #670 inherit. No end-user surface.
> Source of truth: ARCHITECTURE.md, SPECIFICATION.md, RISK-TEST-STRATEGY.md, ADR-001..005.

## Components (Component Map)

| # | Component | File | Crate touched | Deliverable |
|---|-----------|------|---------------|-------------|
| 1 | ts-rs codegen + CI diff-gate | `ts-rs-codegen.md` | unimatrix-engine | D1 |
| 2 | Round-trip fixtures + node harness | `contract-fixtures.md` | unimatrix-engine | D1 |
| 3 | /observe content negotiation | `observe-content-negotiation.md` | unimatrix-server (http) | D2 |
| 4 | transcript_delta accept-and-drop guard | `transcript-delta-guard.md` | unimatrix-server (uds) + engine const | D3 — **GATE** |
| 5 | transcript_retention enum | `transcript-retention.md` | unimatrix-server (infra/config) | D4 |

The four deliverables are independent in code but share one theme: freeze the F2/#670 interface
now. Build order is not strictly sequential, but two shared symbols (below) must land first
because three components reference them.

## Shared types / constants (define ONCE, reference everywhere)

### `TranscriptDeltaPayload` — the one load-bearing shared shape
```
// crates/unimatrix-engine/src/wire.rs (new, near ImplantEvent :200)
#[derive(serde::Deserialize, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../bindings/")]
pub struct TranscriptDeltaPayload {
    pub offset: u64,
    pub bytes:  String,
}
```
- **Component 1** (codegen) emits it as the **6th** binding `bindings/TranscriptDeltaPayload.ts`.
- **Component 2** (fixtures) round-trips it **dual-sided** (Rust↔TS) — AC-11.
- **Component 4** (guard) parses the delta `payload` into it (NOT raw `serde_json::Value`), so the
  drop path and the AC-11 contract share exactly one shape (ADR-004).
- It is **not** a new wire carrier. The value still rides `ImplantEvent.payload: serde_json::Value`
  unchanged. No new `HookRequest`/`HookResponse` variant. (Constraint 3.)

### `TRANSCRIPT_DELTA_EVENT` — the routing constant
```
// crates/unimatrix-engine/src/wire.rs constants block (near :13-36)
pub const TRANSCRIPT_DELTA_EVENT: &str = "transcript_delta";
```
- Mirrors the existing `CYCLE_START_EVENT` pattern (ADR-001 col-022) so the hook/listener
  coupling is not stringly-typed.
- **Component 4** matches `event.event_type == TRANSCRIPT_DELTA_EVENT` on both the `RecordEvent`
  arm (listener.rs:736) and the `RecordEvents` batch arm (listener.rs:868).
- Lives in `wire.rs` (shared engine crate) so both the listener and any future TS-contract doc
  reference one literal. (The existing `CYCLE_*` constants live in the listener module; placing
  `TRANSCRIPT_DELTA_EVENT` in `wire.rs` is the deliberate choice per ARCHITECTURE — it is part of
  the frozen wire surface F2 vendors.)

## Data flow across boundaries

```
   cargo test ─► ts-rs #[ts(export)] ─► bindings/*.ts        (Component 1)
              └► fixture-emit test    ─► bindings/fixtures/*.json
   node --test ◄── imports *.ts, deserializes fixtures        (Component 2)
              └── TranscriptDeltaPayload fixture: Rust→TS AND TS→Rust (dual-sided, AC-11)

   HTTP POST /observe ─► router.rs (read Accept BEFORE into_parts ─► wants_text)  (Component 3)
        │                      │
        │                      ▼ Step6 dispatch_request ──────────┐
        │                                                          │ both transports
   UDS listener loop ─► dispatch_request ──────────────────────────┤  converge here
                                                                    ▼
                              listener.rs RecordEvent / RecordEvents arm  (Component 4)
                              capability check ─► sanitize_session_id ─► GUARD:
                                 event_type == TRANSCRIPT_DELTA_EVENT
                                   ► parse payload into TranscriptDeltaPayload (defensive)
                                   ► return Ack (RecordEvent) / `continue` (RecordEvents)
                                   ► NEVER reaches feature-extraction (:793) or
                                     insert_observation (:849) — ZERO durable rows
        │
        ▼ (non-delta responses only)
   observe_response_to_http(resp, wants_text)                  (Component 3)
        ├ Entries        + wants_text ─► format_injection(items, MAX_INJECTION_BYTES) ─► text/plain
        ├ BriefingContent+ wants_text ─► content body          ─► text/plain
        └ else (Pong/Ack/Error, or JSON Accept) ─► unchanged JSON envelope

   config.rs RetentionConfig.transcript_retention: TranscriptRetention  (Component 5)
        defaulter ─► Default impl ─► validate() (OSS rejects RetainDays) ─► project-wins merge
```

## Sequencing constraints (what must be built first)

1. **`TranscriptDeltaPayload` + `TRANSCRIPT_DELTA_EVENT` in `wire.rs` land first.** Components 1, 2,
   and 4 all reference them. Without the struct, Component 1 has nothing to emit as the 6th
   binding and Component 4 has no typed parse target.
2. **Component 1 before Component 2.** The node harness imports the generated `.ts`; the fixtures
   are emitted by the same Rust test that drives codegen.
3. **Component 4's zero-durable-rows negative test (AC-12) is the GATE prerequisite** — it must be
   green on HTTP + UDS + the batch arm before any other AC is trusted (SR-07 / #4711 / #4311).
4. Components 3 and 5 are independent of the above and of each other.

## Cross-cutting invariants (apply to all components)

- **No new wire variant** (Constraint 3): `transcript_delta` is a new VALUE of the free-form
  `event_type: String`, not a new enum arm. Existing pre-vnc-024 binding diffs MUST be empty (NFR-03).
- **`format_injection` is the single formatting truth** (Constraint 4): Component 3 CALLS
  `hook.rs:1047`; no re-implementation.
- **No content secret-scanner exists** (Constraint 9): accept-and-drop (Component 4) +
  OSS-rejects-`RetainDays` (Component 5) ARE the secrets guarantee. No file may assume a redactor
  licenses persisting raw transcript.
- **Merged config is re-validated** (Constraint 10 / #3905): a `transcript_retention` that survives
  the project-wins merge must be re-validated, so a merged `RetainDays` is still rejected.
- **ts-rs is dev-only** (Constraint 1): never in the runtime edge set; `cargo tree --edges normal`
  proves absence.
- **500-line rule** (Constraint 8): additions slot into existing sections of `config.rs` /
  `listener.rs`; new codegen scaffolding stays inside the `wire.rs` `#[cfg(test)]` module + the
  `bindings/` dir.

## AC → component map

| AC | Component(s) |
|----|--------------|
| AC-01..AC-06 | 1, 2 |
| AC-07..AC-10 | 3 |
| AC-11 | 1 (binding emit) + 2 (dual-sided fixture) + shared `TranscriptDeltaPayload` |
| **AC-12 (GATE)** | 4 |
| AC-13, AC-14 | 5 |
| AC-15 | 1 |
