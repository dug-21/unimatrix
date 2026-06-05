# vnc-024 Implementation Brief — Wire-Contract Codegen + Content Negotiation + transcript_delta + transcript_retention (F1)

> Chunk 1 / F1 of the all-Rust→TS client migration (ass-068). Pure plumbing that freezes a
> wire/codegen/server contract every later chunk (F2–F5, re-scoped #670) inherits without
> re-negotiation. No end-user surface ships in F1. GitHub: #672.
>
> **Design-review rework applied (2026-06-05):** (1) the `transcript_delta` payload is now a typed,
> exported binding — `TranscriptDeltaPayload { offset: u64, bytes: String }` (a **6th** ts-rs export),
> with AC-11 verified **dual-sided** (Rust↔TS); (2) OSS `validate()` **rejects** `RetainDays(N)` with
> an enterprise-only error rather than range-checking it — `PurgeOnCycleClose` is the only OSS-accepted
> value. ADR-001/002/004/005 were revised accordingly.

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/vnc-024/SCOPE.md |
| Scope Risk Assessment | product/features/vnc-024/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/vnc-024/architecture/ARCHITECTURE.md |
| Specification | product/features/vnc-024/specification/SPECIFICATION.md |
| Risk-Based Test Strategy | product/features/vnc-024/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/vnc-024/ALIGNMENT-REPORT.md |
| ADR-001 (ts-rs dev-dependency codegen — REVISED: 6 exported types) | product/features/vnc-024/architecture/ADR-001-ts-rs-codegen-dev-dependency.md |
| ADR-002 (round-trip fixtures as contract authority — REVISED: dual-sided delta fixture) | product/features/vnc-024/architecture/ADR-002-round-trip-fixtures-as-contract-authority.md |
| ADR-003 (/observe content negotiation) | product/features/vnc-024/architecture/ADR-003-observe-content-negotiation.md |
| ADR-004 (transcript_delta accept-and-drop guard — REVISED: typed payload parse) | product/features/vnc-024/architecture/ADR-004-transcript-delta-accept-and-drop-guard.md |
| ADR-005 (transcript_retention enum — REVISED: OSS rejects RetainDays) | product/features/vnc-024/architecture/ADR-005-transcript-retention-enum.md |

## Goal

Land four additive Rust-workspace foundations that freeze the downstream wire/server contract: (1) `ts-rs` machine-generated TypeScript wire bindings for **six** exported types (the five wire types plus the typed `TranscriptDeltaPayload`), CI-gated against drift, with a Node round-trip harness asserting serde behavior; (2) HTTP-only content negotiation on `/observe` so `Accept: text/plain` returns server-formatted injection text for `Entries`/`BriefingContent`; (3) the `transcript_delta` fire-and-forget event-type carrying a REQUIRED accept-and-drop guard so raw conversation bytes never reach durable storage; (4) a `transcript_retention` purge-policy enum on `RetentionConfig` whose OSS build accepts only `PurgeOnCycleClose`. Its purpose is to lock a contract F2–F5 and the re-scoped #670 build on without re-negotiation — no end-user surface ships in F1 alone.

## GATE PREREQUISITE — read first

**AC-12 / R-03 (secrets-to-disk) is a gate prerequisite.** A `transcript_delta` event sent to `/observe` (HTTP) AND over UDS dispatch must yield `Ack` and create **ZERO durable observation rows** — and a delta inside a `RecordEvents` batch must be dropped while the rest persist. This is principle-8 enforcement (raw conversation bytes may contain secrets and must never reach SQLite). The guard parses the payload into the typed `TranscriptDeltaPayload` — the same shape as the AC-11 contract — rather than inspecting raw `serde_json::Value`, so the drop path and the typed contract share one shape. The zero-durable-rows negative test on **both transports + the batch arm** must be green **before any downstream AC is trusted** (SR-07 / #4711 / #4311). The guard must `return Ack` early — it must NOT reuse the col-022 "specialize-then-fall-through-to-generic-persistence" pattern (#1266), which would reintroduce the hole.

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| ts-rs codegen + CI diff-gate (Deliverable 1) | pseudocode/ts-rs-codegen.md | test-plan/ts-rs-codegen.md |
| Round-trip fixtures + node harness (Deliverable 1) | pseudocode/contract-fixtures.md | test-plan/contract-fixtures.md |
| /observe content negotiation (Deliverable 2) | pseudocode/observe-content-negotiation.md | test-plan/observe-content-negotiation.md |
| transcript_delta accept-and-drop guard (Deliverable 3) | pseudocode/transcript-delta-guard.md | test-plan/transcript-delta-guard.md |
| transcript_retention enum (Deliverable 4) | pseudocode/transcript-retention.md | test-plan/transcript-retention.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

> Pseudocode and test-plan files are produced in Session 2 Stage 3a. The Component Map lists the
> expected components from the architecture's four deliverables; actual file paths are filled during
> delivery.

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| ts-rs as dev-dependency; derive `TS`+`#[ts(export)]` on **6** exported types (5 wire types + the typed `TranscriptDeltaPayload`); codegen on `cargo test`; bindings at `crates/unimatrix-engine/bindings/` (CI-gated truth, F2/F5 vendor) | Dev-only, zero runtime/supply-chain footprint; typing the one genuinely-new field stops F2 hand-mirror drift; CI `git diff --exit-code` in existing test job | SCOPE OQ-01/OQ-02, ass-068 Q3 | architecture/ADR-001-ts-rs-codegen-dev-dependency.md (**revised**) |
| Round-trip JSON fixtures (Node harness) assert serde BEHAVIOR; the fixture — not the generated `.ts` — is the contract authority; the `TranscriptDeltaPayload` fixture round-trips **dual-sided** (Rust↔TS) | Per-tagged-variant + flatten + dual-direction None-omission + dual-sided delta fixtures asserted by `node --test`; not type-compile-only | SCOPE OQ-03, ass-068 Q3, SR-01/SR-02/SR-06 | architecture/ADR-002-round-trip-fixtures-as-contract-authority.md (**revised**) |
| `/observe` content negotiation: `Accept: text/plain` → `format_injection` for `Entries`/`BriefingContent` ONLY; `Pong`/`Ack`/`Error` stay JSON; UDS untouched; reuse single `hook.rs:1047` fn; read `Accept` before `into_parts()` | HTTP-only, additive, reversible | SCOPE OQ-06, ass-068 Q4, SR-08/SR-09 | architecture/ADR-003-observe-content-negotiation.md |
| `transcript_delta` is a new `event_type` VALUE (no new wire variant) + REQUIRED accept-and-drop guard intercepting before durable storage on both paths + batch arm; the guard parses the payload into the typed `TranscriptDeltaPayload` (not raw `serde_json::Value`) | Early `return Ack`, persist nothing, buffer nothing; shared `TRANSCRIPT_DELTA_EVENT` constant; the guard and the AC-11 contract share one shape | SCOPE OQ-04, ass-069 Q2/Q4, SR-07, principle 8 | architecture/ADR-004-transcript-delta-accept-and-drop-guard.md (**revised**) |
| `transcript_retention` typed as `TranscriptRetention` enum (`PurgeOnCycleClose` \| `RetainDays(u32)`), threaded through all four `RetentionConfig` touchpoints; **OSS `validate()` REJECTS `RetainDays(N)`** with an enterprise-only error and accepts only `PurgeOnCycleClose`; bare `u32` rejected | Enum is the enterprise extend-not-rearchitect seam (shape kept); OSS cannot durably persist raw secret-bearing transcript (no encrypt-at-rest), so `RetainDays` is a hard validation failure, not accepted-and-ignored; default `PurgeOnCycleClose` | SCOPE OQ-05, ass-069 Q7, goal #4710, principle 8 | architecture/ADR-005-transcript-retention-enum.md (**revised**) |

## Files to Create / Modify

| File | Action | Summary |
|------|--------|---------|
| `crates/unimatrix-engine/Cargo.toml` | Modify | Add `ts-rs` under `[dev-dependencies]` only. |
| `crates/unimatrix-engine/src/wire.rs` | Modify | Derive `TS`+`#[ts(export, export_to = "../bindings/")]` on the 5 wire types **and** the new `TranscriptDeltaPayload` struct (6 exports total); add `TRANSCRIPT_DELTA_EVENT` const; doc the delta payload + precedence note; extend `#[cfg(test)]` suite (`:379+`) with codegen test + fixture emitter + dual-sided delta round-trip. |
| `crates/unimatrix-engine/bindings/*.ts` | Create | Committed generated bindings for the 6 exports: `HookInput`, `HookRequest`, `HookResponse`, `ImplantEvent`, `EntryPayload`, `TranscriptDeltaPayload`. CI-gated source of truth. |
| `crates/unimatrix-engine/bindings/fixtures/*.json` | Create | Rust-emitted JSON fixtures, one per request/response variant + serde edge cases + the dual-sided `TranscriptDeltaPayload` fixture. |
| `crates/unimatrix-engine/bindings/contract.test.mjs` | Create | Standalone `node --test` harness (~dozen lines); imports bindings, deserializes fixtures, asserts behavior incl. the dual-sided delta-payload parse. |
| existing CI workflow | Modify | Fold `cargo test` + `git diff --exit-code crates/unimatrix-engine/bindings/` + `node --test` into the existing test job (no new workflow). |
| `crates/unimatrix-server/src/http/router.rs` | Modify | Capture `Accept` → `wants_text: bool` before `request.into_parts()` (`~:202`); pass to mapper. |
| `crates/unimatrix-server/src/http/router/observe.rs` | Modify | Change `observe_response_to_http(resp)` → `(resp, wants_text)`; text branch for `Entries`/`BriefingContent`. |
| `crates/unimatrix-server/src/uds/hook.rs` | Modify | Promote `format_injection` (`:1047`) to `pub(crate)`. No re-implementation; UDS path untouched. |
| `crates/unimatrix-server/src/uds/listener.rs` | Modify | Add accept-and-drop branch in `RecordEvent` arm (after `sanitize_session_id` `~:757`, before feature-extraction `:793` / `insert_observation` `:849`); parse payload into `TranscriptDeltaPayload`; same drop in `RecordEvents` batch arm. |
| `crates/unimatrix-server/src/infra/config.rs` | Modify | Add `TranscriptRetention` enum + `transcript_retention` field on `RetentionConfig` (`:1501`); defaulter; `Default` impl (`:1551`); `validate()` arm rejecting `RetainDays` as enterprise-only (`:1571`); project-wins merge arm (`:3307`). |

## Data Structures

- **`TranscriptDeltaPayload`** (new, `crates/unimatrix-engine/src/wire.rs`):
  `struct { pub offset: u64, pub bytes: String }`; derives `serde::Deserialize, serde::Serialize, ts_rs::TS` + `#[ts(export, export_to = "../bindings/")]` → the **6th** exported binding `bindings/TranscriptDeltaPayload.ts`. This is **not** a new wire carrier — `transcript_delta` still rides `ImplantEvent.payload: serde_json::Value` unchanged. The struct exists to (a) give the one genuinely-new field a typed cross-language binding (stops ts-rs emitting `any`/`JsonValue`, stops F2 hand-typing it) and (b) be the deserialization shape the accept-and-drop guard parses into. Verified dual-sided (AC-11).
- **`TranscriptRetention`** (new, `config.rs` near `:1499`):
  `enum { PurgeOnCycleClose, RetainDays(u32) }`; derives `serde::Deserialize, Debug, Clone, PartialEq`. `PartialEq` is mandatory — the merge arm compares with `!=`. The enum shape is **kept** as the enterprise seam, but OSS honors only `PurgeOnCycleClose` (the unit variant; serde externally-tagged default renders TOML `transcript_retention = "PurgeOnCycleClose"`). `RetainDays(N)` is rejected by OSS `validate()` — its tagged TOML form is an enterprise-build concern, not F1's (R-10 mostly dissolved). Config-only; not in the wire bindings set.
- **`RetentionConfig.transcript_retention`**: `transcript_retention: TranscriptRetention` with `#[serde(default = "default_transcript_retention")]`.
- **transcript_delta payload** (carried in existing `ImplantEvent.payload: serde_json::Value`, carrier unchanged): `{ "offset": u64, "bytes": "<text>" }`, typed as `TranscriptDeltaPayload`. NO new wire variant — a new VALUE of the free-form `event_type: String`.
- **Codegen targets (5 wire types, unchanged shapes)**: `HookInput` (`:44`, `#[serde(default)]` all fields + `#[serde(flatten)] extra`), `HookRequest` (`:93`, `#[serde(tag="type")]`), `HookResponse` (`:175`, `#[serde(tag="type")]`), `ImplantEvent` (`:200`, `topic_signal`/`provider` `skip_serializing_if`), `EntryPayload` (`:233`).

## Function Signatures

| Symbol | Signature | Location |
|--------|-----------|----------|
| `TranscriptDeltaPayload` | `pub struct TranscriptDeltaPayload { pub offset: u64, pub bytes: String }` (derives `Deserialize, Serialize, TS` + `#[ts(export)]`) | new — `crates/unimatrix-engine/src/wire.rs` |
| `TRANSCRIPT_DELTA_EVENT` | `pub const TRANSCRIPT_DELTA_EVENT: &str = "transcript_delta";` | new — `wire.rs` constants |
| `format_injection` | `pub(crate) fn format_injection(entries: &[EntryPayload], max_bytes: usize) -> Option<String>` (visibility bump only) | `crates/unimatrix-server/src/uds/hook.rs:1047` |
| `observe_response_to_http` | `fn observe_response_to_http(resp: HookResponse, wants_text: bool) -> Response<BoxBody<Bytes, Infallible>>` | `crates/unimatrix-server/src/http/router/observe.rs:18` |
| Accept read | `request.headers().get(http::header::ACCEPT)` → `wants_text` (contains `text/plain`), before `request.into_parts()` | `crates/unimatrix-server/src/http/router.rs:~202` |
| Accept-and-drop branch | `if event.event_type == TRANSCRIPT_DELTA_EVENT { /* parse payload into TranscriptDeltaPayload */ return HookResponse::Ack; }` | `crates/unimatrix-server/src/uds/listener.rs:~757` |
| `default_transcript_retention` | `fn default_transcript_retention() -> TranscriptRetention { TranscriptRetention::PurgeOnCycleClose }` | `config.rs:~1541` |
| `validate()` retention arm | OSS: reject `TranscriptRetention::RetainDays(_)` with an enterprise-only error (`field: "transcript_retention"`, message naming `RetainDays` as enterprise-only); accept `PurgeOnCycleClose` unconditionally | `config.rs:~1571` |

## Constraints

1. **ts-rs is dev-only** — must not enter the runtime dependency graph or shipped binary (ass-068 Q3; minimal-footprint rule). Proven via `cargo tree --edges normal` (absent) + `cargo metadata` (under `[dev-dependencies]`) + `cargo audit`.
2. **Content negotiation is additive and HTTP-only** — UDS path and current JSON envelope untouched; `Accept` MUST be read before `request.into_parts()` (`router.rs:203`).
3. **No new wire variant for transcript_delta** — a new `event_type` string value on existing `RecordEvent`/`ImplantEvent`; the payload is typed as `TranscriptDeltaPayload` but the carrier (`ImplantEvent.payload: serde_json::Value`) is unchanged. The dispatch must NOT inherit the generic-observation fall-through; the accept-and-drop branch is a REQUIRED guard, not an optimization.
4. **`format_injection` is the single source of formatting truth** — the server text path calls `hook.rs:1047`, not a re-implementation, so the byte-identical gate (AC-07) holds.
5. **1 MiB payload ceiling** — `MAX_PAYLOAD_SIZE` (`wire.rs:16`) and the `/observe` body limit (`router.rs:42`) remain authoritative; the soft 64 KiB per-delta cap is the client's concern (F2), not enforced here.
6. **`transcript_retention` is config-only and enum-typed; OSS honors only `PurgeOnCycleClose`** — `PurgeOnCycleClose | RetainDays(u32)`; no GC consumes it yet. The enum shape is the load-bearing enterprise seam (a bare `u32` is rejected). OSS `validate()` **rejects** `RetainDays` with an enterprise-only error (not accepted-and-ignored) — durable persistence of raw secret-bearing transcript collides with principle 8 / ass-069 in-memory-only and needs encrypt-at-rest the OSS build lacks.
7. **`SessionWrite` capability + bearer auth inherited, not extended** — `transcript_delta` rides existing `RecordEvent` gating (`listener.rs:737`); no new auth surface. The guard sits AFTER the capability check.
8. **500-line-per-file rule** — additions slot into existing sections of `config.rs`/`listener.rs`; new codegen/fixture scaffolding is a focused module; no new file exceeds 500 lines.
9. **No content secret-scanner exists to reuse** — Unimatrix has **no** reusable content secret-redactor/scanner; write-path defenses are structural validation + metadata sanitization, and content is stored verbatim and trusted (`hook.rs:2348`). The architectural control — accept-and-drop (Deliverable 3) + in-memory-ephemeral buffering (#670) + purge-on-cycle-close — **is** the secrets guarantee. No requirement, test, or design may assume a redactor licenses persisting raw transcript; this is exactly why `RetainDays` is rejected in OSS (ADR-005). A reviewer must reject any path (now or in #670) that justifies durable raw-transcript writes by appeal to a scanner.
10. **Config merge must be re-validated** (#3905) — per-file `validate()` does not cover invariants that only emerge after the project-wins merge; the merged `transcript_retention` must be re-validated (so a merged `RetainDays` is still rejected).

## Dependencies

- **`ts-rs`** — new **dev-dependency** of `unimatrix-engine` (serde-compat feature, default-on). Not present in the workspace today.
- **Node.js** — required in CI for the `node --test` round-trip harness (already required for the npm package; no new toolchain).
- **vnc-022 (#669)** — shipped `/observe` handler, `prefix_session_id`, `observe_response_to_http`, `CompactPayload.transcript_excerpt`. Content negotiation extends the mapper.
- **vnc-023 (#674)** — rmcp 1.7 migration; no impact (`/observe` is a custom tower handler).
- **`format_injection`** (`uds/hook.rs:1047`) — existing free fn, reused by the text path.
- **`RetentionConfig`** (`config.rs:1499-1559`) + crt-036 background GC seam — extended by `transcript_retention`.
- **`RecordEvent`/`ImplantEvent` dispatch** (`listener.rs:736-865`) — gains the accept-and-drop branch.
- **col-022 / ADR-001 (`event_type`-as-routing precedent)** — the delta guard reuses the shared-constant pattern but inverts the fall-through (see GATE / #1266).

## NOT in Scope

- **The TS hook client** (transport, transform, normalize, event queue, per-session offset tracking) — F2/F3.
- **`init --remote` and npm packaging** — F4/F5. No new package; future client bundles into `@dug-21/unimatrix`.
- **The server-side transcript buffer, offset-merge, cycle-review distillation, purge lifecycle** — re-scoped #670. F1 ships the wire field and retention knob only; **no in-memory transcript accumulation in F1** (reviewer rejects any #670 pull-forward, SR-05).
- **Distillation logic** — re-scoped #670.
- **Enterprise acknowledged-delivery / audit-confidence path** — named gap only (ass-069 Q7).
- **Enterprise honoring of `RetainDays`** (retain-N-days, encrypt-at-rest, data-residency) — enterprise build; OSS rejects it at `validate()`. The tagged `{ RetainDays = N }` TOML form is an enterprise-build concern, not F1's.
- **`hook.rs` retirement** — F5.
- **Content negotiation on the UDS path** — UDS clients continue to receive JSON and format locally.
- **The 64 KiB soft per-delta cap and head+tail truncation** — client concern (F2).
- **GC consumption of `transcript_retention`** — re-scoped #670 / crt-036 integration.
- **Composite `(tenant, project, session)` registry key, macOS/Windows packages, Codex/Gemini formats, OAuth/multi-tenant keying** — later chunks / enterprise.

## Alignment Status

**PASS x6, WARN x1, VARIANCE x0, FAIL x0.** No blocking variances. (ALIGNMENT-REPORT.md)

- **Vision Alignment — PASS**: advances personal-cloud (#4710) — single-edge-language, remote fidelity via streamed deltas, and the enterprise retention seam all directly served. Principle 8 (no secrets in any database) honored via the accept-and-drop guard and the OSS rejection of `RetainDays` (which would imply durable secret-bearing persistence OSS cannot provide safely).
- **Milestone Fit — PASS**: correctly scoped as Chunk 1/F1; #670 and F2–F5 explicitly OUT; no future-chunk pull-forward.
- **Scope Gaps — PASS**: all five SCOPE goals + AC-01..AC-15 mapped into spec FR/AC. No dropped items.
- **Scope Additions — PASS**: one additive item (`RecordEvents` batch-arm guard coverage) is implied by SCOPE constraint 3, not new scope.
- **Architecture / Risk Completeness — PASS**: all three docs agree on the four deliverables and the principle-8 guard placement; secrets-to-disk elevated to gate prerequisite on both transports + batch arm. The rework reduces R-01/R-08 (the formerly-untyped delta payload is now the explicit `TranscriptDeltaPayload` 6th export, closed by a dual-sided fixture) and updates R-09 (OSS rejects `RetainDays`); R-10 mostly dissolved.

### External follow-up (NOT a vnc-024 work item)

- **WARN — PRODUCT-VISION.md principle 6 wording drift (ass-068 Q6)**: principle 6 still carries pre-ass-068 "single binary" wording; Q6 recommended a "single binary server / client-is-an-adapter" rewrite. Editorial, separately owned by the vision steward, and **NOT part of vnc-024 delivery**. Noted here only as an external follow-up; the vnc-024 documents already use the correct "adapter" framing.

## Open Questions (delivery-time, non-blocking)

1. **`format_injection` byte budget for the text path** — the `Entries` text path needs a `max_bytes` argument; it must reuse the same injection budget the production UDS caller uses so AC-07 byte-identity holds (including an over-budget/truncation case). Delivery confirms the exact constant. (ARCHITECTURE.md §Open Questions; R-05.)
2. **`TranscriptRetention` serde representation (mostly dissolved — ADR-005)** — confirm only that the unit variant `transcript_retention = "PurgeOnCycleClose"` deserializes and that `RetainDays` (and a bare `u32`) is rejected. The tagged `{ RetainDays = N }` TOML form is an enterprise-build concern — do **not** spend effort prettifying a value OSS rejects. `TranscriptRetention` is config-only in F1 (not in the wire bindings set). (ARCHITECTURE.md §Open Questions; R-10.)
