# vnc-024: Wire-Contract Codegen + Server Content Negotiation + transcript_delta (F1)

> **Reframed** from the obsolete "Unimatrix Remote Client" scope. Per ass-068 (Q7) and
> ass-069 (Roadmap Fit), vnc-024 is now **Chunk 1 / F1** of the five-chunk all-Rust→TS
> client migration: pure plumbing. The standalone `@dug-21/unimatrix-client` npm package,
> `init --remote`, transport/transform/normalize JS, and packaging are **removed from this
> feature** and moved to later chunks (F2–F5). What remains here is the wire/codegen/server
> foundation everything downstream depends on.

## Problem Statement

The all-Rust telemetry client (`unimatrix hook`) is being replaced by a single edge-language
(JS/TS) client over the migration roadmap (ass-068). Before any TS client can be built, three
foundations must exist in the Rust workspace, and they must exist *first* so the generated wire
contract carries every field the later chunks consume:

1. **No machine-checked wire contract for non-Rust clients.** `wire.rs` is the authoritative
   serde schema, but a TS client would hand-mirror it — the primary maintenance/drift risk that
   ass-068 Q2/Q3 identified. A TS client cannot be trusted until its types are *generated* from
   `wire.rs` and CI-gated against drift.
2. **All response formatting lives client-side.** `format_injection` (the byte-budgeted
   injection text) runs in the Rust hook binary (`hook.rs:1047`). A TS client would have to
   re-implement it, duplicating ~40+ lines of parity-critical formatting. ass-068 Q4 moves this
   server-side so the TS client's transform surface shrinks to host-envelope serialization.
3. **The wire contract cannot yet carry streamed transcript.** ass-069 (Q2) defines client-streamed
   transcript deltas as the mechanism that gives the server the authoritative conversation over
   either transport. The `transcript_delta` event-type and the `transcript_retention` policy knob
   must land in the wire contract / config **now**, in F1, so ts-rs codegen ships them from day one
   (ass-069 Roadmap Fit) — even though the server-side buffer/distill machinery that *consumes*
   them is a separate chunk (the re-scoped #670).

Who is affected: the migration itself — F2/F3 (TS client) and the re-scoped #670 (server transcript
buffer) are blocked on this foundation. No end-user surface ships in F1 alone.

Why now: `/observe` shipped (vnc-022, #669); rmcp 1.7 stabilized (vnc-023); ass-068 selected the
pure-TS architecture; ass-069's attribution gate **passed** (string-keyed `session_id`, PoC-clean to
128 concurrent mixed-transport sessions), removing the dependency uncertainty that blocked this.

## Goals

1. Generate committed TypeScript bindings from the `wire.rs` serde types via `ts-rs`, CI-gated so
   the generated `.ts` must match `cargo test` output (drift cannot merge).
2. Add JSON-fixture round-trip contract tests (Rust ↔ TS) for serde behaviors codegen cannot
   capture (e.g. `None`-vs-omission under `skip_serializing_if`, internally-tagged enums).
3. Add server-side content negotiation to `/observe`: `Accept: text/plain` → server runs
   `format_injection` and returns formatted text; `application/json` (or no header) → current
   JSON envelope, unchanged. UDS path unaffected.
4. Add the `transcript_delta` event-type to the fire-and-forget `RecordEvent` family
   (payload `{ "offset": u64, "bytes": "<text>" }`) — wire contract only, so ts-rs codegen ships it.
5. Add a `transcript_retention` field to the existing `RetentionConfig`, typed as a `RetentionConfig`
   purge-policy **enum** — `PurgeOnCycleClose` (default) | `RetainDays(u32)` — config field only. The
   enum (not a bare `u32`) is the enterprise extension seam; a bare day-count cannot encode the
   event-driven purge-on-cycle-close default.

## Non-Goals

- **The TS hook client** (transport, transform, normalize, event queue, per-session offset
  tracking) — chunks F2/F3.
- **`init --remote` and npm packaging** — chunk F4/F5. No new package; the future client bundles
  into the existing `@dug-21/unimatrix` root package.
- **The server-side transcript buffer, offset-merge, cycle-review distillation, purge lifecycle** —
  the re-scoped #670 server chunk. F1 ships the *wire field and the retention knob*; it does **not**
  build the buffer that consumes them. ass-069 Q4/Q5/Q6.
- **Distillation logic** (decisions/rework/phase-narrative extraction) — re-scoped #670, ass-069 Q5.
- **Enterprise acknowledged-delivery / audit-confidence path** — named gap only (ass-069 Q7).
- **`hook.rs` retirement** — chunk F5.
- **Content negotiation on the UDS path** — UDS clients continue to receive JSON and format locally
  (no regression). Negotiation is HTTP-only.
- **macOS/Windows platform packages, Codex/Gemini client formats, OAuth/multi-tenant keying** —
  later chunks / enterprise.

## Background Research

All claims below are grounded in the current Rust workspace (read 2026-06-05) and the two spikes.

### Wire types — `crates/unimatrix-engine/src/wire.rs`
The authoritative serde contract. The codegen target types are: `HookInput` (`wire.rs:44`,
`#[serde(default)]` on all fields + `#[serde(flatten)] extra`), `HookRequest` (`wire.rs:93`,
`#[serde(tag = "type")]` internally-tagged enum), `HookResponse` (`wire.rs:175`, `#[serde(tag = "type")]`),
`ImplantEvent` (`wire.rs:200`, with `topic_signal`/`provider` under `skip_serializing_if = "Option::is_none"`),
and `EntryPayload` (`wire.rs:233`). These are exactly the annotations `ts-rs` serde-compat handles
(ass-068 Q3). `MAX_PAYLOAD_SIZE = 1 MiB` (`wire.rs:16`) is the hard frame ceiling — already enforced
by `write_frame`/`read_frame`. The file already has an extensive `#[cfg(test)]` round-trip suite
(`wire.rs:379+`), the natural home to wire the ts-rs export and fixture-emit step.

### `transcript_delta` carrier — `RecordEvent` / `ImplantEvent`
`RecordEvent` wraps a flattened `ImplantEvent` (`wire.rs:115-118`); `event_type` is a free-form
`String` (`wire.rs:204`), and the payload is `serde_json::Value` (`wire.rs:211`). **No new variant
is needed** — `transcript_delta` is a new *value* of the existing `event_type` string, with
`{ offset, bytes }` carried in the existing `payload`. Confirmed in the dispatch path
(`listener.rs:736`): the general `RecordEvent` arm validates `session_id`, routes only the three
cycle events (`CYCLE_START/PHASE_END/STOP`, `listener.rs:767-791`) specially, and otherwise persists
any unknown `event_type` as a generic observation and returns `Ack` (`listener.rs:849-865`). An
unrecognized `transcript_delta` **does not error today** — but the generic-observation fall-through is
**not safe** to inherit: a `transcript_delta` payload is **raw conversation bytes**, which ass-069 Q4
states may contain secrets/keys and must **never** be written to durable storage (principle 8 — "no
secrets in any database"). The fall-through at `listener.rs:849` persists that payload **to disk** as
an observation — exactly what the secrets posture forbids. Sequencing (no client streams deltas until
the re-scoped #670 builds the in-memory buffer) is **not** a safety property and cannot be relied on.
F1 must therefore add an explicit **accept-and-drop** branch: recognize `event_type == "transcript_delta"`,
return `Ack`, and persist **nothing**, until #670 wires the in-memory buffer that legitimately consumes
it. The HTTP `/observe` path reaches the same dispatch via `router.rs:234` and inherits the
`SessionWrite` capability check (`listener.rs:737`).

**Two transcript-carrying mechanisms now coexist on the wire** — the existing
`CompactPayload.transcript_excerpt` (client-populated at PreCompact, from vnc-022) and the new
`transcript_delta` stream. ass-069 establishes that streaming **supersedes** the excerpt:
`transcript_delta` is the **forward path** (authoritative, continuous, both transports);
`transcript_excerpt` is **legacy / local-fallback** (12 KB reactive tail). They must not silently
drift — when both are present, the streamed deltas win.

### Content negotiation — `/observe` handler + `format_injection`
The `/observe` handler lives in `crates/unimatrix-server/src/http/router.rs:172-252`. It collects the
body, deserializes `HookRequest` (`router.rs:220`), calls `prefix_session_id` (`router.rs:231`,
defined in `http/router/observe.rs:69`), dispatches (`router.rs:234`), then maps the `HookResponse`
to HTTP via `observe_response_to_http` (`http/router/observe.rs:18`). The `Accept` header is readable
from `request.headers()` at `router.rs` (same access pattern as the existing `CONTENT_LENGTH` check,
`router.rs:191-196`) and must be captured **before** `request.into_parts()` (`router.rs:203`). The
insertion point is `observe_response_to_http`: when the caller sent `Accept: text/plain` **and** the
response is `Entries`/`BriefingContent`, run `format_injection` and return `text/plain`; otherwise the
current JSON envelope is unchanged (`Ack`→204, `Error`→400). `format_injection(entries, max_bytes)`
is defined at `crates/unimatrix-server/src/uds/hook.rs:1047` (a free `fn`, server-reachable — same
crate); it is the function whose output the gate must match byte-for-byte.

### Retention config — `RetentionConfig`
`crates/unimatrix-server/src/infra/config.rs:1499-1559`. The established pattern: each field has a
`#[serde(default = "default_*")]` defaulter, a `Default` impl (`config.rs:1551`), a range check in
`validate()` (`config.rs:1571`) raising `ConfigError::RetentionFieldOutOfRange`, and a per-field
project-wins merge arm (`config.rs:3307-3325`). `transcript_retention` follows this exact pattern;
"purge-on-cycle-close" is its default value (ass-069 Q7). The existing crt-036 background GC
(`retention.rs`) is where a future enterprise retain-N-days policy would consume it — F1 adds the
field only.

### `ts-rs` dependency status
Confirmed **not** present in the workspace (`Cargo.lock` / crate `Cargo.toml` grep: no hits). It must
be added as a **dev-dependency** of `unimatrix-engine` (ass-068 Q3) so it has zero runtime/supply-chain
footprint. The codegen-on-`cargo test` mechanism (ts-rs `#[ts(export)]` writes `.ts` files when the
test binary runs) is **not yet wired anywhere** — this feature introduces it. Generated bindings live
at the **ts-rs default `crates/unimatrix-engine/bindings/`** as the CI-gated source of truth; they are
**not** promoted to a workspace-root `bindings/`. The downstream consumption path is fixed here so F2
inherits it without guessing: F2/F5 **copy/vendor** the bindings from
`crates/unimatrix-engine/bindings/` into the bundled client at build time.

### Spike provenance
- ass-068 Q3 (ts-rs codegen + contract tests), Q4 (server-side content negotiation), Q7 Chunk 1
  ("This IS vnc-024, reframed").
- ass-069 Q2 (`transcript_delta` `{offset,bytes}` on the fire-and-forget family), Q7
  (`transcript_retention` on `RetentionConfig`, default purge-on-cycle-close), Roadmap Fit
  ("add the `transcript_delta` event-type to the wire contract here, so ts-rs codegen carries it
  from day one").

## Proposed Approach

1. **ts-rs codegen.** Add `ts-rs` dev-dependency to `unimatrix-engine`. Derive `TS` + `#[ts(export)]`
   on `HookInput`, `HookRequest`, `HookResponse`, `ImplantEvent`, `EntryPayload`. Commit the generated
   `bindings/*.ts`. CI runs `cargo test` and `git diff --exit-code` on the bindings dir — generated
   must equal committed, or the build fails.
2. **Round-trip fixtures.** A Rust test serializes every `HookRequest`/`HookResponse` variant (and the
   serde edge cases: `None`-omission, internally-tagged discriminants, flatten) to committed JSON
   fixtures; a TS test deserializes the same fixtures and asserts structure. Catches behavioral
   mismatches type-level codegen cannot (ass-068 Q3). Extend the existing `wire.rs` test module rather
   than scaffolding new infra.
3. **Content negotiation.** Capture `Accept` in the `/observe` handler before body consumption; thread
   it (or a `wants_text` bool) into the response mapping. When `Accept: text/plain` and the response is
   `Entries`/`BriefingContent`, call `format_injection` server-side and emit `text/plain`; else unchanged
   JSON. UDS untouched. Additive new code path, fully reversible.
4. **`transcript_delta` wire contract + accept-and-drop guard.** Document `event_type: "transcript_delta"`
   with payload `{ "offset": u64, "bytes": "<text>" }` as a recognized member of the fire-and-forget
   `RecordEvent` family; ensure it round-trips and ships in the generated bindings. Add an **explicit
   accept-and-drop branch** in the `RecordEvent` dispatch: when `event_type == "transcript_delta"`,
   return `Ack` and persist **nothing** — the raw bytes must not reach the generic-observation disk
   insert (`listener.rs:849`), per the principle-8 secrets posture (ass-069 Q4). The guard stays until
   the re-scoped #670 wires the in-memory buffer that legitimately consumes deltas.
5. **`transcript_retention` config field.** Add the field to `RetentionConfig`, typed as the purge-policy
   enum `PurgeOnCycleClose` (default) | `RetainDays(u32)`, with defaulter, `Default` impl, `validate()`
   check on the `RetainDays` payload, and project-wins merge arm — mirroring the existing fields. The
   enum shape (not a bare `u32`) is the enterprise retain/encrypt seam; a bare day-count cannot encode
   the event-driven purge-on-cycle-close default (`0` would be ambiguous).

## Acceptance Criteria

- AC-01: `ts-rs` is a dev-dependency of `unimatrix-engine`; `HookInput`, `HookRequest`, `HookResponse`,
  `ImplantEvent`, `EntryPayload` derive `TS` with `#[ts(export)]`.
- AC-02: Running `cargo test` generates committed `.ts` bindings for all five types; the generated
  output is checked into the repo.
- AC-03: A CI gate fails the build when the committed `.ts` bindings differ from fresh `cargo test`
  output (drift cannot merge).
- AC-04: The generated discriminated unions correctly express the `#[serde(tag = "type")]` enums
  (`HookRequest`, `HookResponse`) with a literal `type` field per variant.
- AC-05: JSON round-trip contract fixtures exist for every `HookRequest` and `HookResponse` variant;
  a Rust test and a standalone **`node --test` harness** (run over the committed ts-rs bindings +
  Rust-emitted fixtures, no TS client package required) both deserialize the same fixtures and agree on
  structure. The TS-side round-trip ships **in F1** — it is not deferred to F2, since F1's purpose is to
  lock the contract and a deferred TS check would ship the contract unverified.
- AC-06: Contract fixtures cover `None`-vs-omission for `skip_serializing_if = "Option::is_none"`
  fields (`ImplantEvent.topic_signal`, `ImplantEvent.provider`, `ContextSearch.source`,
  `CompactPayload.transcript_excerpt`).
- AC-07: `POST /observe` with `Accept: text/plain` for an `Entries` response returns `Content-Type:
  text/plain` whose body is **byte-identical** to `format_injection` output for the same entries.
- AC-08: `POST /observe` with `Accept: application/json` or no `Accept` header returns the current
  JSON `HookResponse` envelope, unchanged (`Ack`→204, `Entries`/`BriefingContent`/`Pong`→200 JSON,
  `Error`→400 JSON).
- AC-09: The `BriefingContent` response honors `Accept: text/plain` (returns the formatted text body)
  consistently with `Entries`. Text negotiation applies **only** to these two injection-bearing
  responses; `Pong`/`Ack`/`Error` remain JSON regardless of `Accept` (`Pong` carries a structured
  `server_version` the client parses during handshake — emitting it as text would break the handshake).
- AC-10: The UDS hook path produces identical output before and after this change (no behavior change
  on the local transport).
- AC-11: `event_type: "transcript_delta"` with payload `{ "offset": u64, "bytes": "<text>" }`
  round-trips through `HookRequest::RecordEvent` (serialize → deserialize) and appears in the generated
  TS bindings' wire surface.
- AC-12: A `transcript_delta` event sent to `/observe` (and over UDS) is handled by an **explicit
  accept-and-drop branch**: it returns `Ack`, and its raw `bytes` payload is persisted **nowhere
  durable** (no generic-observation insert at `listener.rs:849`, no other disk write). A test asserts
  no observation row is created for a `transcript_delta` event. This guard enforces the principle-8
  secrets posture (raw conversation bytes must not reach durable storage) until #670 lands the
  in-memory buffer.
- AC-13: `RetentionConfig` gains a `transcript_retention` field typed as a purge-policy **enum**
  (`PurgeOnCycleClose` (default) | `RetainDays(u32)`); the compiled default is `PurgeOnCycleClose`, an
  absent-`[retention]` config still loads to that default, and `validate()` enforces any documented
  range on the `RetainDays` payload. A bare `u32` is explicitly rejected — the enum is the enterprise
  retain/encrypt seam.
- AC-14: `transcript_retention` participates in the per-field project-wins config merge
  (`config.rs:3307` pattern).
- AC-15: No new runtime dependency is added to any shipped crate (ts-rs is dev-only); `cargo audit`
  passes.

## Constraints

1. **ts-rs is dev-only** — it must not enter the runtime dependency graph or the shipped binary
   (ass-068 Q3; rust-workspace minimal-footprint rule).
2. **Content negotiation is additive and HTTP-only** — UDS path and the current JSON envelope must be
   untouched (no regression). The `Accept` header must be read before `request.into_parts()`
   (`router.rs:203`) consumes the request.
3. **No new wire variant for `transcript_delta`** — it is a new `event_type` string value on the
   existing `RecordEvent`/`ImplantEvent` (`wire.rs:115`, `:204`), keeping the change backward-compatible
   and codegen-stable. **But the dispatch must not inherit the generic-observation fall-through**: raw
   `transcript_delta` bytes may contain secrets (ass-069 Q4, principle 8) and must never be persisted to
   durable storage. F1's accept-and-drop branch is a **required guard**, not an optimization.
4. **`format_injection` is the single source of formatting truth** — the server text path must call the
   existing `hook.rs:1047` function, not a re-implementation, so the byte-identical gate (AC-07) holds.
5. **1 MiB payload ceiling** — `MAX_PAYLOAD_SIZE` (`wire.rs:16`) and the `/observe` body limit
   (`router.rs:42`) remain authoritative; the `transcript_delta` contract documents `{offset,bytes}`
   but the soft 64 KiB per-delta cap is the client's concern (F2), not enforced here.
6. **`transcript_retention` is config-only and enum-typed** — F1 adds the `PurgeOnCycleClose` |
   `RetainDays(u32)` field and its validation/merge; no GC code consumes it yet (the crt-036 background
   GC integration is the re-scoped #670's concern). The enum shape is load-bearing: shipping a bare
   `u32` here forces an enterprise re-architecture later (goal "extend, never re-architect").
7. **`SessionWrite` capability + bearer auth are inherited, not extended** — `transcript_delta` rides
   the existing `RecordEvent` gating (`listener.rs:737`); no new auth surface.
8. **500-line-per-file rule** — `config.rs` and `listener.rs` are already large; additions should slot
   into existing sections, and any new codegen/fixture scaffolding should be a focused module.

## Open Questions

All six questions raised at scoping are now **RESOLVED** (recorded inline below); none remain open.

- OQ-01 — CI diff-gate location. **RESOLVED:** fold the `cargo test` + `git diff --exit-code` bindings
  check into the **existing test job/workflow** — no new workflow. Low-stakes delivery detail; the exact
  step placement is left to the delivery session.
- OQ-02 — generated bindings location. **RESOLVED:** bindings live at the ts-rs default
  `crates/unimatrix-engine/bindings/` as the CI-gated source of truth — **not** promoted to a
  workspace-root `bindings/`. F2/F5 copy/vendor them into the bundled client at build time (recorded in
  Background Research and inherited by F2).
- OQ-03 — TS-side contract test without a TS package. **RESOLVED:** do **not** defer. F1 ships a
  standalone `node --test` harness (~dozen lines) over the committed bindings + Rust-emitted fixtures.
  See AC-05.
- OQ-04 — `transcript_delta` dispatch. **RESOLVED (overruling the "harmless" framing):** the
  generic-observation fall-through writes raw conversation bytes — which may contain secrets — to disk,
  violating principle 8. F1 adds an **explicit accept-and-drop branch** (return `Ack`, persist nothing).
  See AC-12 and Constraint 3.
- OQ-05 — `transcript_retention` field type. **RESOLVED:** enum, not a number —
  `PurgeOnCycleClose` (default) | `RetainDays(u32)`. A bare `u32` cannot encode the event-driven default
  and would force an enterprise re-architecture. See AC-13, Goal 5, Constraint 6.
- OQ-06 — `Pong` text negotiation. **RESOLVED:** `Accept: text/plain` applies **only** to the
  injection-bearing responses (`Entries`, `BriefingContent`); `Pong`/`Ack`/`Error` stay JSON always
  (`Pong` carries structured `server_version` the client parses — text would break the handshake).
  See AC-09.

## Prior Art

- **ass-068** — Telemetry client architecture (TS rewrite, ts-rs codegen, server content negotiation,
  five-chunk roadmap). This feature is its Chunk 1.
- **ass-069** — Client-streamed transcript (attribution gate GO; `transcript_delta`; `transcript_retention`;
  re-scope of #670 as the server buffer chunk).
- **vnc-022 (#669)** — `/observe` endpoint shipped; the content-negotiation insertion point and
  `prefix_session_id` / `transcript_excerpt` forward-compat field originate here.
- **vnc-023 (#674)** — rmcp 1.7 migration; no impact on `/observe` (custom tower handler).
- **#670** — Re-scoped (per ass-069 Q6) from "observation reconstruction" to "server-side transcript
  buffer (primary: streamed deltas; fallback: reconstruction) + distill + purge". Consumes
  `transcript_delta` and `transcript_retention` but is **not** built in F1.
- **crt-036** — `RetentionConfig` + background GC policy seam that `transcript_retention` extends.

## Tracking

GitHub Issue: #672
