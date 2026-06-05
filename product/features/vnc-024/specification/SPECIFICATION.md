# SPECIFICATION: vnc-024 — Wire-Contract Codegen + Content Negotiation + transcript_delta (F1)

**Feature ID**: vnc-024 (GitHub #672)
**Source scope**: `product/features/vnc-024/SCOPE.md`
**Risk input**: `product/features/vnc-024/SCOPE-RISK-ASSESSMENT.md`
**Research grounding**: ass-068 (Q3/Q4/Q7), ass-069 (Q2/Q4/Q7, Roadmap Fit)

---

## Objective

vnc-024 is Chunk 1 / F1 of the five-chunk all-Rust→TS client migration (ass-068): the wire/codegen/server foundation everything downstream inherits. It delivers four artifacts in the Rust workspace — (1) `ts-rs` machine-generated TypeScript wire bindings, CI-gated against drift, with a Node round-trip harness asserting serde behavior; (2) HTTP content negotiation on `/observe` so `Accept: text/plain` returns server-formatted injection text; (3) the `transcript_delta` fire-and-forget event-type with a required accept-and-drop guard so raw conversation bytes never reach durable storage; (4) a `transcript_retention` enum on `RetentionConfig`. No end-user surface ships in F1 alone; its purpose is to freeze a contract that F2–F5 and the re-scoped #670 build on without re-negotiation.

---

## Domain Models & Ubiquitous Language

### Wire types (codegen targets)
The six authoritative serde types in `crates/unimatrix-engine/src/wire.rs` that ship as generated TypeScript bindings:

| Type | wire.rs | Key serde shape |
|------|---------|-----------------|
| `HookInput` | :44 | `#[serde(default)]` on all fields + `#[serde(flatten)] extra` |
| `HookRequest` | :93 | `#[serde(tag = "type")]` internally-tagged enum (the request family, incl. `RecordEvent`) |
| `HookResponse` | :175 | `#[serde(tag = "type")]` internally-tagged enum (incl. `Entries`, `BriefingContent`, `Pong`, `Ack`, `Error`) |
| `ImplantEvent` | :200 | `topic_signal`/`provider` under `skip_serializing_if = "Option::is_none"`; `event_type: String`; `payload: serde_json::Value` |
| `EntryPayload` | :233 | injection-entry payload |
| `TranscriptDeltaPayload` | (new) | `{ offset: u64, bytes: String }` — typed shape of the `transcript_delta` payload (ADR-001/ADR-004). The **carrier is unchanged**: the value still rides `ImplantEvent.payload: serde_json::Value`; no new wire variant. The struct exists to give the one genuinely-new field a typed cross-language binding (stops ts-rs emitting `any`/`JsonValue` and stops F2 hand-typing it) and to be the deserialization shape the accept-and-drop guard parses into. |

- **Bindings location**: `crates/unimatrix-engine/bindings/` (ts-rs default). This is the CI-gated source of truth, **not** promoted to a workspace-root `bindings/`. F2/F5 copy/vendor it at build time.
- **`MAX_PAYLOAD_SIZE = 1 MiB`** (`wire.rs:16`) — hard frame ceiling, enforced by `write_frame`/`read_frame`. Unchanged.

### transcript_delta
A new **value** of the existing free-form `event_type: String` on `RecordEvent`/`ImplantEvent` — **not** a new wire variant. Carried in the existing `payload: serde_json::Value`:

```json
{ "offset": <u64>, "bytes": "<text>" }
```

`offset` is the per-session byte offset against `transcript_path`; `bytes` is the raw transcript span `[last_offset, file_len)`. Rides the **fire-and-forget** `RecordEvent` family (returns `Ack`).

This payload has a **typed shape**, `TranscriptDeltaPayload { offset: u64, bytes: String }`, which ships as the sixth exported codegen binding (ADR-001). The carrier is **unchanged** — the value still travels through `ImplantEvent.payload: serde_json::Value`; no new wire variant is introduced. The typed struct is both the cross-language contract (round-tripped dual-sided, AC-11) and the deserialization shape the accept-and-drop guard parses into (AC-12), so the contract and the guard share one shape.

### transcript_delta vs transcript_excerpt
Two transcript-carrying mechanisms coexist on the wire after F1:
- **`transcript_delta`** — the **forward path**: authoritative, continuous, both transports (ass-069 Q2).
- **`CompactPayload.transcript_excerpt`** — **legacy / local-fallback**: a 12 KB reactive tail populated client-side at PreCompact (vnc-022).
When both are present, streamed deltas supersede the excerpt (ass-069). F1 documents this precedence; it does **not** build the merge logic (that is the re-scoped #670).

### transcript_retention (RetentionConfig enum)
A purge-policy **enum** field on `RetentionConfig` (`crates/unimatrix-server/src/infra/config.rs`). It governs the **raw session transcript — ephemeral working state**: the verbatim, possibly-secret-bearing conversation bytes the client streams as `transcript_delta`. It explicitly does **NOT** govern distilled knowledge, observations, or the audit log — those are durable, sanitized, non-secret-bearing artifacts with their own existing retention knobs. Conflating the two is the central footgun this field's scope definition closes (ADR-005).

```
PurgeOnCycleClose            // default — event-driven purge on cycle/session close (the only value OSS honors)
RetainDays(u32)             // enterprise retain-N-days seam — REJECTED by validate() in the OSS build
```

The enum (not a bare `u32`) is the enterprise extension seam: a bare day-count cannot encode the event-driven default (`0` would be ambiguous), and a future encrypt-at-rest / data-residency policy has no numeric home. The enum shape is **retained** as that seam. But in the OSS build, `RetainDays(N)` is **not** accepted-and-ignored — `validate()` **rejects** it with a clear "enterprise-only" error, because honoring it would imply durable persistence of raw secret-bearing transcript that OSS cannot provide safely (PRODUCT-VISION principle 8). `PurgeOnCycleClose` is the only value OSS accepts. F1 adds the field, defaulter, `Default` impl, the OSS `validate()` rejection of `RetainDays`, and project-wins merge arm only — no GC code consumes it yet (re-scoped #670).

### accept-and-drop
The required `RecordEvent` dispatch branch for `event_type == "transcript_delta"`: return `Ack`, persist **nothing**. The branch exists because the generic-observation fall-through (`listener.rs:849`) would write the raw `bytes` payload — which may contain secrets — to durable storage, violating PRODUCT-VISION principle 8. Guard holds on both `/observe` (HTTP) and UDS dispatch until #670 wires the legitimate in-memory consumer.

### format_injection
The single source of formatting truth: `crates/unimatrix-server/src/uds/hook.rs:1047`, a free `fn` reachable from the server crate. The content-negotiation text path **calls** this function — never a re-implementation — so the byte-identical gate holds.

---

## Functional Requirements

### FR-01 — ts-rs codegen derives on the six exported types
`ts-rs` is added as a **dev-dependency** of `unimatrix-engine`. The five wire types `HookInput`, `HookRequest`, `HookResponse`, `ImplantEvent`, `EntryPayload`, plus the typed payload struct `TranscriptDeltaPayload { offset: u64, bytes: String }`, derive `TS` with `#[ts(export)]`. `TranscriptDeltaPayload` is not a new wire carrier — `transcript_delta` still rides `ImplantEvent.payload: serde_json::Value` unchanged — it is the typed binding for the one genuinely-new field, so ts-rs does not emit it as `any`/`JsonValue`.
*Verification*: source inspection + `cargo tree --edges normal` shows ts-rs absent from the normal (runtime) edge set; `cargo metadata` shows it under `[dev-dependencies]` only.

### FR-02 — `cargo test` generates committed bindings
Running `cargo test` exports `.ts` bindings for all six exported types (including `TranscriptDeltaPayload`) to `crates/unimatrix-engine/bindings/`. The generated output is checked into the repo.
*Verification*: from a clean checkout, run `cargo test`; assert each expected `bindings/*.ts` exists and is non-empty.

### FR-03 — CI diff-gate rejects binding drift
A CI step runs the binding generation (via `cargo test`) then `git diff --exit-code` over `crates/unimatrix-engine/bindings/`. Divergence between committed and freshly-generated bindings fails the build. Folded into the existing test job/workflow (no new workflow).
*Verification*: mutate a wire field locally without regenerating; assert the gate step exits non-zero. Restore; assert it passes.

### FR-04 — tagged-union codegen fidelity
Generated bindings for `HookRequest` and `HookResponse` express the `#[serde(tag = "type")]` enums as TypeScript discriminated unions, each variant carrying a literal `type` field.
*Verification*: inspect generated `.ts`; assert each variant has the correct `type` literal. Asserted further by FR-06 round-trip over per-variant fixtures.

### FR-05 — JSON round-trip fixtures for every request/response variant
A Rust test serializes every `HookRequest` and `HookResponse` variant to committed JSON fixtures (extending the existing `wire.rs` `#[cfg(test)]` suite, not new scaffolding). Coverage includes the serde edge cases: at least one fixture per internally-tagged variant, and a `HookInput`-with-`extra`-keys (flatten) fixture.
*Verification*: Rust test deserializes each fixture back and asserts structural equality round-trip.

### FR-06 — Node round-trip harness over Rust-emitted fixtures
A standalone `node --test` harness (no TS client package required) imports the committed ts-rs bindings and deserializes the same Rust-emitted JSON fixtures, asserting structure. The TS-side round-trip ships **in F1** — not deferred to F2 — because F1's purpose is to lock the contract; a deferred check would ship it unverified.
*Verification*: `node --test` over `bindings/` + fixtures exits 0; a deliberately malformed fixture makes it exit non-zero.

### FR-07 — serde-behavior assertions (None-vs-omission, tagged, flatten)
The fixtures + harness assert serde **behavior** codegen alone cannot capture, in both directions:
- **None-vs-omission** under `skip_serializing_if = "Option::is_none"` for, at minimum: `ImplantEvent.topic_signal`, `ImplantEvent.provider`, `ContextSearch.source`, `CompactPayload.transcript_excerpt` — each asserted **emitted-as-absent when `None`** and **deserialized-to-default when key absent** (dual-direction, per SR-02).
- **Internally-tagged discriminant** present and correct on every `HookRequest`/`HookResponse` variant fixture (per SR-01).
- **Flatten**: a `HookInput` fixture with extra top-level keys round-trips with the extra keys preserved under `extra` (per SR-01).
*Verification*: dedicated assertions in both the Rust test and the `node --test` harness; the round-trip fixture — not the generated type — is the authority.

### FR-08 — `/observe` text negotiation for injection-bearing responses
The `/observe` handler captures the `Accept` header **before** `request.into_parts()` consumes the request. When `Accept: text/plain` **and** the dispatched `HookResponse` is `Entries` or `BriefingContent`, the server calls `format_injection` (`hook.rs:1047`) and returns the formatted text with `Content-Type: text/plain`.
*Verification*: integration test posts an `Entries`-producing request with `Accept: text/plain`; assert `Content-Type: text/plain` and body equals `format_injection` output for the same entries.

### FR-09 — JSON path unchanged for non-text Accept
With `Accept: application/json` or no `Accept` header, `/observe` returns the current JSON `HookResponse` envelope unchanged: `Ack`→204, `Entries`/`BriefingContent`/`Pong`→200 JSON, `Error`→400 JSON.
*Verification*: integration tests over each response type with and without the header assert status + JSON body identical to pre-change behavior.

### FR-10 — text negotiation restricted to injection responses
`Pong`, `Ack`, and `Error` remain JSON regardless of `Accept` — `Accept: text/plain` is honored **only** for `Entries` and `BriefingContent`. (`Pong` carries a structured `server_version` the client parses during handshake; emitting it as text would break the handshake.)
*Verification*: post `Accept: text/plain` requests that resolve to `Pong`/`Ack`/`Error`; assert JSON envelope (not text) returned.

### FR-11 — transcript_delta typed contract + dual-sided round-trip + bindings presence
`event_type: "transcript_delta"` carries payload `{ "offset": u64, "bytes": "<text>" }`, typed as `TranscriptDeltaPayload`. The carrier (`RecordEvent`/`ImplantEvent` with free-form `event_type` + `serde_json::Value` payload) is unchanged and appears in the generated TS bindings' wire surface; **no new wire variant is introduced**. The typed `TranscriptDeltaPayload` ships as the sixth exported binding and is verified by a **dual-sided** (Rust↔TS) cross-language fixture — like AC-06's None-vs-omission case — not a Rust-emit-only check, closing the F2 hand-mirror drift gap.
*Verification*: see AC-11 — a cross-language fixture for `{offset, bytes}` round-trips through the `node --test` harness, parsing into `TranscriptDeltaPayload` on both the Rust and TS sides; binding inspection confirms the `RecordEvent`/`ImplantEvent` surface carries arbitrary `event_type`/`payload` and that `TranscriptDeltaPayload.ts` is emitted.

### FR-12 — accept-and-drop guard (HTTP + UDS)
The `RecordEvent` dispatch gains an explicit branch: when `event_type == "transcript_delta"`, return `Ack` and persist **nothing**. The raw `bytes` payload must not reach the generic-observation insert (`listener.rs:849`) or any other durable write. The guard applies on both the `/observe` (HTTP, via `router.rs:234`) and UDS dispatch paths.
*Verification*: see AC-12 — negative test asserts zero observation rows for a delta on both transports.

### FR-13 — transcript_retention enum field with default; OSS honors only PurgeOnCycleClose
`RetentionConfig` gains a `transcript_retention` field typed as the enum `PurgeOnCycleClose | RetainDays(u32)`, with a `#[serde(default = "default_*")]` defaulter and a `Default` impl yielding `PurgeOnCycleClose`. In the OSS build, `validate()` **rejects** `RetainDays(_)` outright with a clear "enterprise-only" error (naming `RetainDays` as an enterprise-only policy); OSS honors **only** `PurgeOnCycleClose`, which is always valid. The enum shape is retained as the enterprise seam, but `RetainDays` is a hard validation failure in OSS — not accepted-and-ignored — so an operator cannot believe durable retention is in effect when OSS cannot safely provide it. An absent `[retention]` section still loads to the `PurgeOnCycleClose` default.
*Verification*: see AC-13.

### FR-14 — transcript_retention project-wins merge
`transcript_retention` participates in the per-field project-wins config merge (`config.rs:3307` pattern).
*Verification*: see AC-14 — merge test with project and global values asserts project wins.

### FR-15 — transcript precedence documented
The wire contract documents that `transcript_delta` (forward path) supersedes `transcript_excerpt` (legacy/local-fallback) when both are present. F1 ships the documentary note only — no merge logic.
*Verification*: doc-comment / contract note present at the relevant wire types; no merge code added (reviewer check, per SR-06).

---

## Non-Functional Requirements

### NFR-01 — ts-rs dev-only, zero runtime/release impact
ts-rs must not enter the runtime dependency graph or the shipped binary. No new runtime dependency is added to any shipped crate.
*Measurable*: ts-rs absent from `cargo tree --edges normal`; present only under `[dev-dependencies]`; `cargo audit` passes; release binary size and dependency closure unchanged.

### NFR-02 — content negotiation additive and HTTP-only
The new text path is additive new code, not a modification of the JSON path. UDS transport is untouched. The current JSON envelope is byte-for-byte unchanged for all non-text-negotiated cases.
*Measurable*: UDS hook output identical before/after (AC-10); JSON responses identical for `application/json`/no-header (AC-08/FR-09).

### NFR-03 — backward compatibility (unknown clients ignore transcript_delta)
No new wire variant; `transcript_delta` is a new `event_type` string on the existing `RecordEvent`. Existing clients that never emit it are unaffected; existing clients that emit unrecognized event types continue to receive `Ack`. Generated bindings remain codegen-stable (no breaking shape change to existing types).
*Measurable*: existing wire round-trip suite passes unchanged; binding diff for pre-existing types is empty.

### NFR-04 — SessionWrite + bearer auth inherited, not extended
`transcript_delta` rides the existing `RecordEvent` gating (`SessionWrite` capability check at `listener.rs:737`) and the bearer auth in front of the HTTP listener. No new auth surface is added.
*Measurable*: a request lacking `SessionWrite` is rejected on the delta path exactly as on any other `RecordEvent`.

### NFR-05 — reversibility
Every change is fully reversible: ts-rs is dev-only (removable with no runtime effect); content negotiation is a new code path (removable, leaving JSON path intact); the accept-and-drop branch and `transcript_retention` field are additive.
*Measurable*: reviewer confirms no existing path is modified destructively; removal of each artifact leaves prior behavior intact.

### NFR-06 — payload ceiling unchanged
`MAX_PAYLOAD_SIZE` (`wire.rs:16`) and the `/observe` body limit (`router.rs:42`) remain authoritative. The `transcript_delta` contract documents `{offset,bytes}`; the soft 64 KiB per-delta cap is the client's concern (F2), **not** enforced here.
*Measurable*: no change to frame/body limit constants; no per-delta cap logic added server-side.

### NFR-07 — file-size discipline
`config.rs` and `listener.rs` are already large; additions slot into existing sections per the established pattern. New codegen/fixture scaffolding is a focused module. Respect the 500-line-per-file rule for any new module.
*Measurable*: no new file exceeds 500 lines; additions follow existing field/arm patterns.

---

## Acceptance Criteria

Each AC carries an explicit verification method. AC-12 is a **gate prerequisite** (per SR-07/#4311): it must be green before any downstream AC is trusted.

| AC | Criterion | Verification |
|----|-----------|--------------|
| **AC-01** | ts-rs is a dev-dependency of `unimatrix-engine`; the six exported types (five wire types + `TranscriptDeltaPayload`) derive `TS` + `#[ts(export)]`. | Source inspection; `cargo metadata` shows ts-rs under `[dev-dependencies]` only. |
| **AC-02** | `cargo test` generates committed `.ts` bindings for all six exported types (incl. `TranscriptDeltaPayload`); output is checked in. | Clean-checkout `cargo test`; assert each `bindings/*.ts` exists, non-empty. |
| **AC-03** | CI gate fails when committed bindings differ from fresh `cargo test` output. | Mutate a wire field without regenerating → gate exits non-zero; restore → passes. |
| **AC-04** | Generated `HookRequest`/`HookResponse` discriminated unions carry a literal `type` field per variant. | Binding inspection + per-variant round-trip fixtures (AC-05). |
| **AC-05** | JSON round-trip fixtures exist for every `HookRequest`/`HookResponse` variant; a Rust test **and** a standalone `node --test` harness (over committed bindings + Rust-emitted fixtures, no TS package) both deserialize the same fixtures and agree on structure. Ships in F1. | Run Rust test + `node --test`; both exit 0; malformed fixture makes `node --test` fail. |
| **AC-06** | Fixtures cover `None`-vs-omission for `skip_serializing_if = "Option::is_none"` fields: `ImplantEvent.topic_signal`, `ImplantEvent.provider`, `ContextSearch.source`, `CompactPayload.transcript_excerpt`. | Dual-direction assertion (emitted-absent when `None`; deserialized-to-default when key absent) in both Rust test and `node --test`. |
| **AC-07** | `POST /observe` with `Accept: text/plain` for an `Entries` response returns `Content-Type: text/plain` with body **byte-identical** to `format_injection` output for the same entries. | Integration test compares response body bytes to direct `format_injection` call output. |
| **AC-08** | `POST /observe` with `Accept: application/json` or no header returns the current JSON envelope unchanged (`Ack`→204, `Entries`/`BriefingContent`/`Pong`→200 JSON, `Error`→400 JSON). | Integration tests over each response type assert status + JSON body unchanged. |
| **AC-09** | `BriefingContent` honors `Accept: text/plain` (formatted text body) consistently with `Entries`. Text applies **only** to these two; `Pong`/`Ack`/`Error` stay JSON regardless of `Accept`. | Integration tests: `BriefingContent` returns text under `text/plain`; `Pong`/`Ack`/`Error` return JSON even under `text/plain`. |
| **AC-10** | UDS hook path output is identical before and after this change. | UDS round-trip parity test / golden comparison; no behavior change. |
| **AC-11** | The typed `TranscriptDeltaPayload { offset: u64, bytes: String }` ships as the sixth exported binding; a cross-language fixture for `{ "offset": u64, "bytes": "<text>" }` round-trips **dual-sided** (Rust↔TS) through the `node --test` harness — parsing into `TranscriptDeltaPayload` on **both** sides, like AC-06 — not Rust-emit-only. The carrier (`RecordEvent`/`ImplantEvent`, free-form `event_type` + `Value` payload) is unchanged and appears in the generated TS bindings' wire surface; no new wire variant. | Cross-language `{offset, bytes}` fixture deserializes into `TranscriptDeltaPayload` in both the Rust test and the `node --test` harness; binding inspection confirms `TranscriptDeltaPayload.ts` is emitted and the `RecordEvent`/`ImplantEvent` surface carries arbitrary `event_type`/`payload`. |
| **AC-12 (GATE)** | A `transcript_delta` event sent to `/observe` **and** over UDS is handled by an explicit accept-and-drop branch: returns `Ack`, and its raw `bytes` payload is persisted **nowhere durable** (no generic-observation insert at `listener.rs:849`, no other disk write). The branch parses the payload into the typed `TranscriptDeltaPayload` — the same shape as the AC-11 contract, so the drop path and the typed contract share one shape (rather than inspecting raw `serde_json::Value`). A test asserts **zero observation rows** are created for a `transcript_delta` event on **both** transports. | Negative integration test on HTTP and UDS: send a delta, assert `Ack` returned and observation-row count is unchanged (zero rows for the delta). Must be green before downstream ACs are trusted (SR-07/#4311). |
| **AC-13** | `RetentionConfig` gains `transcript_retention` typed as the enum (`PurgeOnCycleClose` default \| `RetainDays(u32)`); compiled default is `PurgeOnCycleClose`; an absent `[retention]` config loads to that default. In the OSS build, `validate()` **REJECTS** `RetainDays(_)` with a clear enterprise-only error and **ACCEPTS** `PurgeOnCycleClose` (always valid). The enum shape (not a bare `u32`) is retained as the enterprise seam. | Unit tests: default value; absent-config load; `validate()` rejects `RetainDays` with an enterprise-only error; `validate()` accepts `PurgeOnCycleClose`; reviewer confirms enum (not `u32`) shape. |
| **AC-14** | `transcript_retention` participates in the per-field project-wins config merge (`config.rs:3307` pattern). | Merge test with distinct project + global values asserts project value wins. |
| **AC-15** | No new runtime dependency on any shipped crate (ts-rs dev-only); `cargo audit` passes; ts-rs absent from `cargo tree --edges normal`. | `cargo tree --edges normal` grep; `cargo audit`. |

---

## User / Agent Workflows

F1 ships no end-user surface; its consumers are downstream chunks and the CI pipeline.

1. **Maintainer regenerates bindings** — edits `wire.rs`, runs `cargo test`, commits regenerated `bindings/*.ts`. If they forget, the CI diff-gate (AC-03) blocks the merge.
2. **CI drift check** — the existing test job runs `cargo test` then `git diff --exit-code` on `bindings/`; drift fails the build.
3. **Future TS client (F2/F5)** — copies/vendors `crates/unimatrix-engine/bindings/` into the bundled `@dug-21/unimatrix` client at build time; sends HTTP `/observe` with `Accept: text/plain` to receive pre-formatted injection text; streams `transcript_delta` events on fire-and-forget hooks.
4. **Re-scoped #670 (server buffer)** — consumes `transcript_delta` via an in-memory buffer and reads `transcript_retention` for purge policy; at that point the accept-and-drop guard is replaced by the legitimate consumer.

---

## Constraints

1. **ts-rs is dev-only** — must not enter the runtime dependency graph or shipped binary (ass-068 Q3; minimal-footprint rule).
2. **Content negotiation is additive and HTTP-only** — UDS path and current JSON envelope untouched; `Accept` must be read before `request.into_parts()` (`router.rs:203`).
3. **No new wire variant for transcript_delta** — a new `event_type` string on the existing `RecordEvent`/`ImplantEvent` (`wire.rs:115`, `:204`). The dispatch must **not** inherit the generic-observation fall-through; the accept-and-drop branch is a **required guard**, not an optimization (ass-069 Q4, principle 8).
4. **`format_injection` is the single source of formatting truth** — the server text path calls `hook.rs:1047`, not a re-implementation, so the byte-identical gate (AC-07) holds.
5. **1 MiB payload ceiling** — `MAX_PAYLOAD_SIZE` and the `/observe` body limit remain authoritative; the soft 64 KiB per-delta cap is the client's concern (F2), not enforced here.
6. **`transcript_retention` is config-only and enum-typed** — `PurgeOnCycleClose | RetainDays(u32)`; no GC consumes it yet. The enum shape is load-bearing — a bare `u32` forces an enterprise re-architecture (goal "extend, never re-architect"). The enum is the enterprise seam, but OSS honors **only** `PurgeOnCycleClose`: `validate()` **rejects** `RetainDays` in the OSS build with an enterprise-only error (not accepted-and-ignored).
7. **`SessionWrite` capability + bearer auth inherited, not extended** — `transcript_delta` rides the existing `RecordEvent` gating; no new auth surface.
8. **500-line-per-file rule** — additions slot into existing sections of `config.rs`/`listener.rs`; new codegen/fixture scaffolding is a focused module.
9. **No content secret-scanner exists to reuse** — Unimatrix has **no** reusable content secret-redactor/scanner; write-path defenses are structural validation + metadata sanitization, and content is stored verbatim and trusted. The architectural control — accept-and-drop (FR-12/AC-12) + in-memory-ephemeral buffering (#670) + purge-on-cycle-close — **is** the secrets guarantee. No requirement may assume a secret-redactor licenses persisting raw transcript; this is exactly why `RetainDays` cannot be honored in OSS (ADR-005).

---

## Dependencies

- **vnc-022 (#669)** — shipped `/observe` endpoint, `prefix_session_id`, and `CompactPayload.transcript_excerpt`. F1 inserts content negotiation at `observe_response_to_http` and reaches dispatch via `router.rs:234`.
- **vnc-023 (#674)** — rmcp 1.7 migration; no impact (`/observe` is a custom tower handler).
- **`ts-rs`** — new **dev-dependency** of `unimatrix-engine` (serde-compat feature default). Not present in the workspace today.
- **`format_injection`** (`uds/hook.rs:1047`) — existing free fn, reused by the text path.
- **`RetentionConfig`** (`config.rs:1499-1559`) + crt-036 background GC seam — extended by `transcript_retention`.
- **`RecordEvent`/`ImplantEvent` dispatch** (`listener.rs:736-865`) — gains the accept-and-drop branch.
- **Node.js** — available in CI for the `node --test` round-trip harness.

---

## NOT in Scope (explicit exclusions)

- **The TS hook client** (transport, transform, normalize, event queue, per-session offset tracking) — F2/F3.
- **`init --remote` and npm packaging** — F4/F5. No new package; the future client bundles into `@dug-21/unimatrix`.
- **The server-side transcript buffer, offset-merge, cycle-review distillation, purge lifecycle** — re-scoped #670. F1 ships the wire field and retention knob only; **no in-memory transcript accumulation in F1** (reviewer rejects any #670 pull-forward, per SR-05).
- **Distillation logic** (decisions/rework/phase-narrative extraction) — re-scoped #670.
- **Enterprise acknowledged-delivery / audit-confidence path** — named gap only (ass-069 Q7).
- **`hook.rs` retirement** — F5.
- **Content negotiation on the UDS path** — UDS clients continue to receive JSON and format locally.
- **The 64 KiB soft per-delta cap and head+tail truncation** — client concern (F2).
- **GC consumption of `transcript_retention`** — re-scoped #670 / crt-036 integration.
- **Composite `(tenant, project, session)` registry key** — enterprise / later chunk.
- **macOS/Windows platform packages, Codex/Gemini client formats, OAuth/multi-tenant keying** — later chunks / enterprise.

---

## Traceability

| SCOPE AC | This spec |
|----------|-----------|
| AC-01..AC-04 | AC-01..AC-04, FR-01..FR-04 |
| AC-05 (node harness) | AC-05, FR-06 |
| AC-06 | AC-06, FR-07 |
| AC-07..AC-10 | AC-07..AC-10, FR-08..FR-10 |
| AC-11 | AC-11, FR-11 |
| AC-12 (accept-and-drop, zero rows) | AC-12 (GATE), FR-12 |
| AC-13 (retention enum) | AC-13, FR-13 |
| AC-14 | AC-14, FR-14 |
| AC-15 | AC-15, NFR-01 |

| Risk | Covered by |
|------|-----------|
| SR-01 (tagged/flatten fidelity) | FR-07, AC-04, AC-05 (per-variant + flatten fixtures, harness is authority) |
| SR-02 (serde behavior None-vs-omission) | FR-07, AC-06 (dual-direction) |
| SR-03 (ts-rs runtime leak) | NFR-01, AC-15 (`cargo tree --edges normal`) |
| SR-04 (contract completeness for F2/#670) | FR-11, FR-13, AC-11/AC-13 (frozen bindings incl. typed `TranscriptDeltaPayload` + retention enum) |
| SR-05 (scope creep into #670) | NOT-in-scope, AC-12 asserts non-persistence not buffering |
| SR-06 (two carriers / hand-mirror drift) | FR-15 (precedence documented); FR-11/AC-11 (typed `TranscriptDeltaPayload` round-tripped dual-sided closes the F2 hand-mirror gap) |
| SR-07 (secrets-to-disk) | AC-12 GATE, FR-12 (negative test, both transports) |
| SR-08 (Accept read ordering) | FR-08, AC-07/AC-08 (negotiated content-type asserted both branches) |
| SR-09 (format_injection re-impl) | Constraint 4, AC-07 byte-identical |
