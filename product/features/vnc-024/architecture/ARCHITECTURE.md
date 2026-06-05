# vnc-024 Architecture — Wire-Contract Codegen + Content Negotiation + transcript_delta + transcript_retention (F1)

> Chunk 1 / F1 of the all-Rust→TS client migration (ass-068). Pure plumbing that locks a
> wire/codegen/server contract every later chunk (F2–F5, re-scoped #670) inherits without
> re-negotiation. No end-user surface ships in F1.

## System Overview

Four additive foundations land in the Rust workspace. None changes existing runtime behavior on
the local (UDS) path; all are reversible.

```
                          ┌──────────────────────────────────────────────────────┐
                          │  unimatrix-engine (wire.rs)                            │
   ts-rs (dev-dep) ───────│  HookInput, HookRequest, HookResponse, ImplantEvent,  │
        │   #[derive(TS)]  │  EntryPayload, TranscriptDeltaPayload ── 6 exported    │
        │                  │  types (authoritative serde)                          │
        ▼                  └──────────────────────────────────────────────────────┘
  bindings/*.ts  ◄── cargo test emits ── CI-gated truth (F2/F5 vendor from here)
        ▲                                          │
        │  node --test round-trip                  │  Rust-emitted JSON fixtures
        └──────────────────────────────────────────┘  (serde BEHAVIOR, not just types)

   HTTP POST /observe                                UDS dispatch
        │                                                  │
        ▼                                                  ▼
   router.rs: read Accept BEFORE into_parts()        listener.rs RecordEvent arm
        │                                                  │
   dispatch_request ──────────────────────────────────────┤
        │                                                  │
   observe_response_to_http(resp, wants_text)         accept-and-drop guard:
        │  Entries/BriefingContent + text/plain →       event_type=="transcript_delta"
        │  format_injection() → text/plain              → Ack, persist NOTHING
        │  else → JSON (unchanged)                      (both paths reach this arm)
        ▼
   text/plain | application/json

   config.rs RetentionConfig
        └── transcript_retention: TranscriptRetention  (PurgeOnCycleClose | RetainDays(u32))
            threaded: defaulter → Default impl → validate() → project-wins merge
```

The four deliverables are independent in code but share one theme: **freeze the F2/#670 interface
now**. The generated bindings, the `transcript_delta` payload shape, and the retention enum are the
frozen surface; F2 and the re-scoped #670 consume them with no second negotiation (SR-04).

## Component Breakdown

### Deliverable 1 — ts-rs wire-contract codegen + CI diff-gate + node round-trip fixtures

| Component | Responsibility | Location |
|-----------|---------------|----------|
| `ts-rs` dev-dependency | Derive-macro codegen of `.ts` from serde types. **Dev-only** — zero runtime/supply-chain footprint. | `crates/unimatrix-engine/Cargo.toml` `[dev-dependencies]` |
| `#[derive(TS)]` + `#[ts(export, export_to = "../bindings/")]` | Placed on the 5 wire types **plus** the typed `TranscriptDeltaPayload { offset:u64, bytes:String }` struct (6 exported types). Macro is inert outside `cfg(test)` because export only fires when the test binary runs. | `crates/unimatrix-engine/src/wire.rs` (the 6 type definitions) |
| Codegen test | `#[test] fn export_bindings()` (or ts-rs's auto-export on any `cargo test`) writes `bindings/*.ts`. | `wire.rs` `#[cfg(test)]` module (`wire.rs:379+`) |
| Committed bindings | `bindings/HookInput.ts`, `HookRequest.ts`, `HookResponse.ts`, `ImplantEvent.ts`, `EntryPayload.ts`, `TranscriptDeltaPayload.ts` (6th — the one genuinely-new field, typed so F2 does not hand-mirror it). CI-gated source of truth. | `crates/unimatrix-engine/bindings/` |
| Fixture emitter | Rust test serializes every `HookRequest`/`HookResponse` variant + serde edge cases to committed JSON fixtures. | `wire.rs` `#[cfg(test)]` module → `bindings/fixtures/*.json` |
| `node --test` harness | Standalone (~dozen lines), imports committed `.ts`, deserializes the Rust-emitted fixtures, asserts serde **behavior** (None-omission, tagged discriminant, flatten). No TS client package. | `crates/unimatrix-engine/bindings/contract.test.mjs` |
| CI diff-gate | `cargo test` then `git diff --exit-code crates/unimatrix-engine/bindings/`. Folded into the **existing** test job (OQ-01), no new workflow. | existing CI workflow |

**Boundary**: codegen scaffolding is confined to the `wire.rs` `#[cfg(test)]` module and the
`bindings/` directory. No shipped crate gains a dependency (AC-15). Extend the existing round-trip
suite — do not scaffold new infra (Constraint 8).

### Deliverable 2 — /observe server-side content negotiation

| Component | Responsibility | Location |
|-----------|---------------|----------|
| Accept capture | Read `request.headers().get(http::header::ACCEPT)` and compute `wants_text: bool` **before** `request.into_parts()` consumes the request (Constraint 2, SR-08). Mirrors the existing CONTENT_LENGTH read at `router.rs:191-196`. | `crates/unimatrix-server/src/http/router.rs` (~before line 203) |
| Mapper signature change | `observe_response_to_http(resp, wants_text)` — thread the bool into the mapper. | `http/router/observe.rs:18` |
| Text branch | When `wants_text` AND response is `Entries`/`BriefingContent`: call `format_injection` (Entries) / emit `content` directly (BriefingContent), return `Content-Type: text/plain`. Else current JSON envelope unchanged. | `http/router/observe.rs` |
| Formatting source of truth | `format_injection(entries, max_bytes)` — the existing `hook.rs:1047` free `fn`. Made crate-visible (`pub(crate)`) so `observe.rs` calls the same function (Constraint 4, SR-09). **No re-implementation.** | `crates/unimatrix-server/src/uds/hook.rs:1047` |

**Boundary**: HTTP-only. The UDS hook path (`hook.rs`) is untouched (AC-10, Constraint 2). The change
is a new code path in the response mapper, not a modification of existing JSON behavior.

### Deliverable 3 — transcript_delta accept-and-drop guard

| Component | Responsibility | Location |
|-----------|---------------|----------|
| Event-type constant | `pub const TRANSCRIPT_DELTA_EVENT: &str = "transcript_delta";` — shared constant (mirrors `CYCLE_START_EVENT` pattern, ADR-001 col-022) to avoid string-literal coupling. | `crates/unimatrix-engine/src/wire.rs` or listener constants module |
| Accept-and-drop branch | In the `RecordEvent` arm, **after** capability check + `sanitize_session_id`, **before** any persistence (before `wire.rs:793` feature extraction / `:849` observation insert): `if event.event_type == TRANSCRIPT_DELTA_EVENT { return HookResponse::Ack; }`. Persists **nothing**. | `crates/unimatrix-server/src/uds/listener.rs` (~after line 757) |
| Path coverage | The HTTP `/observe` path reaches this same arm via `router.rs:234 → dispatch_request`, so **one branch covers both transports** (SR-07). No HTTP-specific code needed. The same applies to the `RecordEvents` batch arm — a batch containing a delta must drop that element. | `listener.rs` RecordEvent + RecordEvents arms |
| Typed payload struct | `TranscriptDeltaPayload { offset: u64, bytes: String }` derives `TS` + `#[ts(export)]` → 6th binding. The guard deserializes the payload into this struct (not raw `serde_json::Value`); it is the typed cross-language contract for the one new field. Wire carrier unchanged — value still rides `ImplantEvent.payload`. | `crates/unimatrix-engine/src/wire.rs` |
| Wire contract doc | `event_type: "transcript_delta"`, payload typed as `TranscriptDeltaPayload { offset:u64, bytes:String }`. No new variant — a new value of the existing free-form `event_type` String (Constraint 3). Payload round-trips **dual-sided** (Rust↔TS) in the ADR-002 fixture set and ships in bindings (AC-11). | `wire.rs` doc comment on `ImplantEvent` |

**Boundary**: The guard is a **required secrets-posture gate** (principle 8, SR-07), not an
optimization. It must intercept before the generic-observation disk insert. It accumulates nothing
in memory either — buffering is the re-scoped #670's job (SR-05). F1 only proves non-persistence.

### Deliverable 4 — transcript_retention enum on RetentionConfig

**Scope**: `transcript_retention` governs the **raw session transcript (ephemeral working state)** —
the verbatim, possibly-secret-bearing conversation bytes streamed as `transcript_delta`. It does
**not** govern distilled knowledge, observations, or the audit log (durable, sanitized, with their
own knobs). This distinction is load-bearing, not cosmetic.

| Component | Responsibility | Location |
|-----------|---------------|----------|
| `TranscriptRetention` enum | `PurgeOnCycleClose` \| `RetainDays(u32)`. Derives `Deserialize, Debug, Clone, PartialEq`. `PartialEq` is mandatory — the merge arm uses `!=`. Enum shape **kept** as the enterprise seam. | `config.rs` (near RetentionConfig, `:1499`) |
| Struct field | `transcript_retention: TranscriptRetention` with `#[serde(default = "default_transcript_retention")]`. | `RetentionConfig` (`config.rs:1501`) |
| Defaulter + Default impl | `fn default_transcript_retention() -> TranscriptRetention { TranscriptRetention::PurgeOnCycleClose }`; add to the `Default for RetentionConfig` impl (`:1551`). | `config.rs:1541-1559` |
| validate() arm | **OSS build REJECTS `RetainDays(_)`** with a clear "enterprise-only" error (`field: "transcript_retention"`). `PurgeOnCycleClose` is the **only** OSS-accepted value and is always valid. Not accepted-and-ignored — durable retention of secret-bearing transcript collides with principle 8 + ass-069 in-memory-only and needs enterprise encrypt-at-rest/residency apparatus OSS lacks. | `config.rs:1571` |
| project-wins merge arm | `transcript_retention: if project != default { project } else { global }` — added to the `retention:` merge block. | `config.rs:3307-3329` |

**Boundary**: config-only (Constraint 6). No GC code consumes it in F1 — the crt-036 background GC
integration is the re-scoped #670's concern. The enum shape is the load-bearing enterprise seam
(goal #4710); a bare `u32` is explicitly rejected (AC-13), and the enterprise `RetainDays` variant is
rejected by OSS `validate()` (not silently accepted).

**Secrets-posture guardrail (no scanner)**: there is **no** reusable content secret-scanner/redactor
to lean on — write-path defenses are structural validation + metadata sanitization, and content is
stored verbatim/trusted (`hook.rs:2348`). No path may assume a redactor licenses persisting raw
transcript. The architectural control (accept-and-drop + in-memory-ephemeral + purge) **is** the
guarantee; a scanner could only supplement distilled output, never replace this control.

## Component Interactions / Data Flow

### Codegen + contract verification flow
1. `cargo test` runs → ts-rs `#[ts(export)]` writes `bindings/*.ts`; the fixture-emit test writes `bindings/fixtures/*.json`.
2. CI runs `git diff --exit-code crates/unimatrix-engine/bindings/` → drift fails the build (AC-03).
3. CI runs `node --test crates/unimatrix-engine/bindings/contract.test.mjs` → imports the committed `.ts`, deserializes the committed fixtures, asserts behavior (AC-05/AC-06). The `TranscriptDeltaPayload {offset,bytes}` fixture round-trips **dual-sided** (Rust↔TS), like AC-06.
4. The Rust round-trip suite asserts serialize→deserialize identity on the same variants, including `TranscriptDeltaPayload` (AC-11).

### /observe content negotiation flow
1. `router.rs` reads `Accept` → `wants_text` (before `into_parts`).
2. Body collected → `HookRequest` deserialized → `prefix_session_id` → `dispatch_request` → `HookResponse`.
3. `observe_response_to_http(resp, wants_text)`:
   - `Entries` + `wants_text` → `format_injection(&items, DEFAULT_MAX_BODY_BYTES-budget)` → 200 `text/plain` (AC-07).
   - `BriefingContent` + `wants_text` → 200 `text/plain` body = `content` (AC-09).
   - All other (`Ack`/`Error`/`Pong`, or no/`application/json` Accept) → unchanged JSON (AC-08/AC-09).

### transcript_delta dispatch flow (both transports)
1. Request arrives (HTTP `/observe` → `dispatch_request`, or UDS listener loop → `dispatch_request`).
2. `RecordEvent` arm: capability check → `sanitize_session_id`.
3. **Guard**: `event_type == "transcript_delta"` → parse payload into `TranscriptDeltaPayload {offset,bytes}` → return `Ack`. No feature extraction, no topic signal, **no `insert_observation`** (`listener.rs:849` never reached). (AC-12)

### retention config flow
- Load: absent `[retention]` → `#[serde(default)]` → `default_transcript_retention()` = `PurgeOnCycleClose` (AC-13).
- Validate: startup `validate()` **rejects `RetainDays(_)`** in the OSS build with an enterprise-only error; `PurgeOnCycleClose` is the only accepted value (AC-13).
- Merge: two-source load → project-wins arm picks `transcript_retention` (AC-14).

## Technology Decisions (see ADRs)

| Decision | ADR | Rationale summary |
|----------|-----|-------------------|
| ts-rs as **dev-dependency**, derive on **6** exported types (5 wire types + `TranscriptDeltaPayload`), codegen on `cargo test`, bindings at `crates/unimatrix-engine/bindings/` (CI-gated truth; F2/F5 vendor) | ADR-001 | Zero runtime/supply-chain footprint (AC-15, SR-03); ts-rs serde-compat handles the exact annotation set (ass-068 Q3); typing the one new field stops F2 hand-mirror drift. |
| Round-trip JSON fixtures via `node --test` assert serde **BEHAVIOR** (None-vs-omission, tagged variant, flatten) — the fixture is the contract authority, not the generated `.ts` | ADR-002 | Codegen captures structure, not behavior (SR-01/SR-02). Type-compile-only would ship the contract unverified. |
| /observe content negotiation: `Accept: text/plain` → `format_injection` for `Entries`/`BriefingContent` ONLY; `Pong`/`Ack`/`Error` stay JSON; UDS untouched; reuse the single `hook.rs:1047` fn | ADR-003 | `Pong.server_version` is parsed during handshake — text would break it (OQ-06/AC-09). Single formatting source (Constraint 4, SR-09). Read Accept before `into_parts` (SR-08). |
| transcript_delta is a new `event_type` **value** (no new variant) + a **required accept-and-drop guard**; payload typed as the exported `TranscriptDeltaPayload {offset,bytes}` struct (the guard parses into it) | ADR-004 | Raw conversation bytes may contain secrets; the generic-observation fall-through (`listener.rs:849`) writes to disk — principle-8 violation (SR-07). Typed payload kills the one new field's hand-mirror drift; backward-compatible, codegen-stable (Constraint 3). |
| transcript_retention typed as `TranscriptRetention` enum (`PurgeOnCycleClose` \| `RetainDays(u32)`); OSS `validate()` **rejects `RetainDays`** as enterprise-only | ADR-005 | Enum is the enterprise extend-not-rearchitect seam (goal #4710); `RetainDays` implies durable secret-bearing persistence (principle 8 / ass-069 in-memory-only) and must be rejected, not accepted-and-ignored (AC-13, SR-04). |

## Integration Points

- **vnc-022 (#669)** — `/observe` handler, `prefix_session_id`, `observe_response_to_http`, `transcript_excerpt` field. Content negotiation extends the mapper; no regression.
- **#670 (re-scoped)** — consumes `transcript_delta` (in-memory buffer/distill) and `transcript_retention` (purge lifecycle). F1 freezes both; builds neither.
- **F2/F3 (TS client)** — vendors `bindings/` and uses the `Accept: text/plain` path. Inherits the frozen contract.
- **crt-036** — `RetentionConfig` + background GC; `transcript_retention` extends the existing policy object.
- **col-022 / ADR-001** — `event_type`-as-routing precedent; the delta guard reuses the shared-constant pattern.

### Two transcript carriers — documented precedence (SR-06)
`CompactPayload.transcript_excerpt` (legacy, 12 KB reactive tail, vnc-022) and the new
`transcript_delta` stream coexist on the wire. The wire contract documents that **streamed deltas
supersede the excerpt** (ass-069): `transcript_delta` is the authoritative forward path;
`transcript_excerpt` is legacy/local-fallback. F1 ships only the precedence note — the merge logic
is #670's.

## Integration Surface

| Integration Point | Type / Signature | Source |
|-------------------|------------------|--------|
| `HookRequest` | `#[serde(tag = "type")]` enum; variants `Ping`, `SessionRegister`, `SessionClose`, `RecordEvent{ #[serde(flatten)] event: ImplantEvent }`, `RecordEvents`, `ContextSearch`, `Briefing`, `CompactPayload` | `crates/unimatrix-engine/src/wire.rs:93` |
| `HookResponse` | `#[serde(tag = "type")]` enum; `Pong{server_version:String}`, `Ack`, `Error{code:i32,message:String}`, `Entries{items:Vec<EntryPayload>,total_tokens:u32}`, `BriefingContent{content:String,token_count:u32}` | `wire.rs:175` |
| `ImplantEvent` | `{event_type:String, session_id:String, timestamp:u64, payload:serde_json::Value, topic_signal:Option<String> (skip_if none), provider:Option<String> (skip_if none)}` | `wire.rs:200` |
| `HookInput` | all fields `#[serde(default)]`; `#[serde(flatten)] extra: serde_json::Value` | `wire.rs:44` |
| `EntryPayload` | `{id:u64, title:String, content:String, confidence:f64, similarity:f64, category:String}` | `wire.rs:233` |
| `MAX_PAYLOAD_SIZE` | `pub const = 1_048_576` (1 MiB frame ceiling, unchanged) | `wire.rs:16` |
| `TranscriptDeltaPayload` | `struct { offset: u64, bytes: String }`; derives `Deserialize, Serialize, TS` + `#[ts(export)]` → 6th binding `bindings/TranscriptDeltaPayload.ts`. Guard deserializes payload into this. | new — `crates/unimatrix-engine/src/wire.rs` |
| transcript_delta payload | `event_type == "transcript_delta"`, `payload` deserialized as `TranscriptDeltaPayload {offset:u64, bytes:String}`; carried in existing `ImplantEvent.payload: serde_json::Value` (carrier unchanged) | new value; documented `wire.rs` |
| `TRANSCRIPT_DELTA_EVENT` | `pub const &str = "transcript_delta"` (shared constant) | new — `wire.rs` constants |
| `format_injection` | `fn format_injection(entries: &[EntryPayload], max_bytes: usize) -> Option<String>` — make `pub(crate)` | `crates/unimatrix-server/src/uds/hook.rs:1047` |
| `observe_response_to_http` | change to `(resp: HookResponse, wants_text: bool) -> Response<BoxBody<Bytes, Infallible>>` | `crates/unimatrix-server/src/http/router/observe.rs:18` |
| Accept read point | `request.headers().get(http::header::ACCEPT)` before `request.into_parts()` (`router.rs:203`) | `crates/unimatrix-server/src/http/router.rs:~202` |
| `dispatch_request` RecordEvent arm | guard insert after `sanitize_session_id` (`listener.rs:757`), before feature extraction (`:793`) | `crates/unimatrix-server/src/uds/listener.rs:736` |
| `insert_observation` (the disk write to AVOID) | `listener.rs:859-862` spawn_blocking → `insert_observation(&store, &obs)` | `listener.rs:849-863` |
| `TranscriptRetention` | `enum { PurgeOnCycleClose, RetainDays(u32) }`; derives `Deserialize, Debug, Clone, PartialEq`. OSS `validate()` rejects `RetainDays`; only `PurgeOnCycleClose` accepted. Governs **raw session transcript (ephemeral working state)** only. | new — `config.rs` near `:1499` |
| `RetentionConfig.transcript_retention` | `transcript_retention: TranscriptRetention` `#[serde(default = "default_transcript_retention")]` | `config.rs:1501` (struct), `:1551` (Default), `:1571` (validate), `:3307` (merge) |
| `ConfigError::RetentionFieldOutOfRange` | existing variant `{path, field:&'static str, value:String, reason:&'static str}` reused for `RetainDays` range | `config.rs` |

## Open Questions

None blocking. All six scoping OQs are RESOLVED in SCOPE.md. Items for the delivery session
(non-blocking, low-stakes):
- **R-10 (RetainDays TOML representation) — mostly dissolved.** Because OSS `validate()` rejects
  `RetainDays`, the only live OSS value is the unit variant `transcript_retention = "PurgeOnCycleClose"`
  (serde's externally-tagged default renders it as a bare string). Do **not** spend design effort
  prettifying the tagged `{ RetainDays = N }` form of a value OSS rejects — that is an enterprise-build
  concern. Delivery confirms only that `"PurgeOnCycleClose"` deserializes and that `RetainDays` is
  rejected with a clear enterprise-only error. `TranscriptRetention` is config-only (not in the wire
  bindings set).
- **format_injection byte budget for the text path** — the Entries text path needs a `max_bytes` argument; reuse the same injection budget the UDS path uses (the hook's `MAX_*_BYTES`) so AC-07 byte-identity holds against `format_injection`'s actual production caller. Delivery confirms the exact constant.
