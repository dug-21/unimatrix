# Risk-Based Test Strategy: vnc-024

> F1 wire/server foundation (issue #672). The dominant risk class is **silent infidelity**:
> codegen/dispatch/config that *compiles and looks correct but models the wrong thing* — a
> ts-rs type that mis-serializes, a delta that silently lands on disk, a retention enum that
> accepts a value OSS cannot safely honor. F1 freezes a contract F2–F5 / #670 inherit without re-negotiation,
> so an undetected defect here is not a bug — it is a permanent wrong foundation. Historical
> evidence (Unimatrix #885, #3557, #4711, #4070, #3905) confirms each of these as recurring
> gate-failure categories, not hypotheticals.

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | ts-rs emits `.ts` that **compiles but mis-models serde behavior** — tagged discriminant dropped/renamed, `flatten` extra-keys not modeled, `None` rendered as `null` not omitted. Type-compile-only check passes; wrong contract ships to F2/#670. *(Risk-reduced for the delta payload: the previously-untyped `payload` delta field — formerly `serde_json::Value` → emitted as `any` in TS, the single highest-drift-risk field — is now the typed `TranscriptDeltaPayload {offset:u64, bytes:String}` (6th export) closed by a dual-sided fixture; remaining R-01 surface is the other tagged/flatten variants.)* | High | Med | **Critical** |
| R-02 | `None`-vs-omission / `#[serde(default)]` optionality tested in only **one direction** (emit-absent OR parse-default, not both), or with a trivial all-`None` fixture that never exercises the field. The single most-omitted test category (#885, #3557). | High | High | **Critical** |
| R-03 | **Secrets-to-disk hole.** `transcript_delta` raw `bytes` reach the generic-observation insert (`listener.rs:849`) — principle-8 violation. Reintroduced if the guard is placed after persistence, or if a delivery agent reuses the col-022 "specialize-then-fall-through" pattern (#1266) by reflex instead of the early-`return Ack`. | High | High | **Critical** |
| R-04 | The accept-and-drop negative test covers **one transport only** (HTTP `/observe` OR UDS), and the untested path silently regresses. Also the `RecordEvents` **batch** arm is missed — a batch containing a delta persists that element. | High | Med | **Critical** |
| R-05 | `format_injection` byte-identity drift: the `/observe` text path uses a different `max_bytes` budget than the production UDS caller (open question, ARCHITECTURE.md §Open Questions), so AC-07 passes against a self-consistent-but-wrong value, or a re-implementation creeps in. | High | Med | **High** |
| R-06 | A non-injection response (`Pong`/`Ack`/`Error`) is accidentally text-formatted under `Accept: text/plain` — `Pong.server_version` emitted as text breaks the F2 handshake. The allowlist is `Entries`/`BriefingContent` only. | High | Low | **High** |
| R-07 | `Accept` read **after** `request.into_parts()` (`router.rs:203`) consumes the request → header silently lost → JSON returned when text requested. Ordering bug yields no error, just wrong content-type. | Med | Med | **High** |
| R-08 | Contract incompleteness (SR-04): the frozen wire surface omits a field F2/#670 need — a `skip_serializing_if` field or a retention variant — forcing F2 to re-open the "frozen" contract. *(Risk-reduced: the delta `offset`/`bytes` typing — previously the prime incompleteness vector, riding untyped `serde_json::Value` → `any` — is now the explicit typed `TranscriptDeltaPayload` 6th export, verified by the dual-sided fixture. The delta is no longer an under-specified field F2 must re-type.)* | High | Med | **High** |
| R-09 | Retention enum threaded through **fewer than all four** touchpoints (defaulter, `Default` impl, `validate()`, project-wins merge). The "hidden site" failure mode (#4070, #2730): a literal construction or a merge arm is missed, so absent-config or merge silently yields the wrong policy. **OSS `validate()` must REJECT `RetainDays(N)` with the enterprise-only error and accept ONLY `PurgeOnCycleClose` (ADR-005)** — a missed/weak rejection arm lets an operator believe durable retention is on when OSS cannot honor it safely (no encrypt-at-rest; principle 8 / ass-069 in-memory-only). | Med | Med | **High** |
| R-10 | *(Mostly DISSOLVED by ADR-005.)* The only live OSS value is the unit `transcript_retention = "PurgeOnCycleClose"`, which serde's externally-tagged default renders as a bare string — no ambiguity remains. `RetainDays` is rejected at `validate()`, so its tagged TOML form is an enterprise-build concern; no coverage required for prettifying a rejected form. Residual: confirm `"PurgeOnCycleClose"` deserializes and a bare `u32` is rejected. | Low | Low | **Low** |
| R-11 | `PartialEq` derive missing/incorrect on `TranscriptRetention` → the merge arm's `!=` comparison fails to compile, OR (if hand-implemented) compares wrong → project value silently not picked up. | Med | Low | **Medium** |
| R-12 | ts-rs leaks into the runtime dependency graph (feature-unification / transitive pull-in) despite dev-only intent — supply-chain footprint on shipped crate. | Med | Low | **Medium** |
| R-13 | Two transcript carriers (`transcript_excerpt` legacy vs `transcript_delta` forward) ship without a stated precedence note, inviting silent downstream drift in #670. | Low | Med | **Low** |
| R-14 | CI diff-gate is non-functional — it runs `cargo test` but `git diff --exit-code` targets the wrong path / runs before generation / passes on a dirty tree, so drift can merge undetected. The gate that protects every other codegen AC is itself unverified. | Med | Med | **High** |

## Risk-to-Scenario Mapping

### R-01: ts-rs compiles but mis-models serde behavior
**Severity**: High **Likelihood**: Med
**Impact**: F2 builds a TS client against a contract that disagrees with the Rust server on the wire — the exact drift the migration exists to eliminate, now baked into "generated" (trusted) artifacts.

**Test Scenarios**:
1. Per-tagged-variant fixture: one committed JSON fixture per `HookRequest` and `HookResponse` variant; the `node --test` harness asserts the fixture deserializes to the **correct union member** keyed on the literal `type` discriminant (not merely "parses").
2. Flatten fixture: a `HookInput` JSON carrying extra unknown top-level keys; assert the named fields parse AND the extras land under `extra` — in both the Rust round-trip and the Node harness.
3. Inspect generated `HookRequest.ts`/`HookResponse.ts`: assert each variant carries its literal `type` field (AC-04) — but treat the round-trip fixture, not the type, as the authority (ADR-002).
4. **Typed delta payload — DUAL-SIDED fixture (drift-closing, ADR-002/ADR-004)**: a committed fixture for `TranscriptDeltaPayload {offset:u64, bytes:String}` (the 6th exported binding) round-trips **Rust→TS AND TS→Rust**, parsing into the typed struct on **both** sides. This closes the formerly-highest-drift field (the delta payload previously rode `serde_json::Value` → `any` in TS, hand-typed by F2). A Rust-emit-only check does NOT satisfy this — the TS→Rust direction is what catches a client-side shape drift on `offset`/`bytes`.

**Coverage Requirement**: Every internally-tagged variant has a fixture asserted by the Node harness; the flatten case has a non-empty-`extra` fixture; **the `TranscriptDeltaPayload` fixture is asserted dual-sided (both parse directions into the typed struct)**. The fixture is the contract authority — type-compilation alone does NOT satisfy this risk.

### R-02: None-vs-omission tested one-directional or trivial
**Severity**: High **Likelihood**: High
**Impact**: A `skip_serializing_if` field that emits `null` instead of being absent (or a `default` field that fails to round-trip) ships silently; F2's deserializer diverges on optional fields. #3557 shows a single-direction test leaves the other copy unguarded.

**Test Scenarios**:
1. For each of `ImplantEvent.topic_signal`, `ImplantEvent.provider`, `ContextSearch.source`, `CompactPayload.transcript_excerpt`: **dual-direction** assertion — (a) when `None`, the key is **absent** from emitted JSON (not `null`); (b) a fixture **omitting** the key deserializes to the default. Both directions, in both Rust and Node (per #3557).
2. Negative guard: a fixture with the field present and a **non-trivial value** round-trips it intact — so a partial wiring (added to emitter but not consumer) cannot pass on the all-`None` path (#3557).

**Coverage Requirement**: All four named fields covered dual-direction in both runtimes. No field is "covered" by a single-direction or all-`None` assertion.

### R-03: Secrets-to-disk hole (transcript_delta → durable storage)
**Severity**: High **Likelihood**: High **GATE PREREQUISITE**
**Impact**: Raw conversation bytes (possibly secrets/keys) written to SQLite — principle-8 violation. Per #4711 this is a *false-safety* trap: "doesn't error today" and "no client streams yet" are NOT safety properties.

**Test Scenarios**:
1. Send a `RecordEvent` with `event_type == "transcript_delta"`, `payload = {offset, bytes}` → assert response is `Ack` AND **observation-row count is unchanged (zero rows for the delta)**.
2. Code/structure assertion: the guard's `return Ack` sits **after** the `SessionWrite` check + `sanitize_session_id` but **before** feature-extraction (`:793`) and `insert_observation` (`:849`) — i.e., the disk insert is provably unreachable for a delta.
3. Anti-pattern guard: confirm the delta branch does NOT follow the col-022 "specialize-then-fall-through-to-generic-persistence" shape (#1266) — it must early-return, persisting nothing.

**Coverage Requirement**: Zero-durable-rows test is **green before any downstream AC is trusted** (SR-07 / #4711 / pattern #4311). Covers the single dispatch arm both transports converge on.

### R-04: Guard tested on one transport / batch arm missed
**Severity**: High **Likelihood**: Med
**Impact**: The untested transport (or the `RecordEvents` batch path) silently persists deltas — the hole exists exactly where the test does not look.

**Test Scenarios**:
1. Run the zero-rows negative test **twice**: once via HTTP `POST /observe` (reaches dispatch via `router.rs:234`), once via direct UDS dispatch. Both assert `Ack` + zero rows.
2. `RecordEvents` batch containing one `transcript_delta` element among normal events → assert the delta element persists **nothing** while the other elements persist normally (delta dropped, rest survive).

**Coverage Requirement**: Both transports AND the batch arm asserted. A single-transport pass does not satisfy this risk.

### R-05: format_injection byte-identity / budget drift
**Severity**: High **Likelihood**: Med
**Impact**: The `/observe` text body diverges from the UDS hook's injection text; F2 receives subtly-different formatting than the local path, defeating the "server is the single formatting source" goal.

**Test Scenarios**:
1. Integration test: `POST /observe` with `Accept: text/plain` for an `Entries` response → response body **byte-identical** to a direct `format_injection(&items, max_bytes)` call for the same entries (AC-07).
2. Budget-parity assertion: the `max_bytes` passed by the text path equals the constant the **production UDS caller** uses (resolve the open question) — test with an entry set large enough to exercise the truncation boundary, so a wrong budget produces a detectable length difference.
3. Reviewer/structure check: the text path **calls** `hook.rs:1047`, not a re-implementation (Constraint 4).

**Coverage Requirement**: Byte-identity asserted against the real `format_injection` with the production budget, including an over-budget/truncation case — not just a small happy-path entry set.

### R-06: Non-injection response accidentally text-formatted
**Severity**: High **Likelihood**: Low
**Impact**: `Pong` emitted as text breaks the F2 handshake (`server_version` is parsed structured); `Ack`/`Error` as text breaks status/JSON-envelope expectations.

**Test Scenarios**:
1. `POST /observe` with `Accept: text/plain` resolving to `Pong` → assert 200 **JSON** envelope (not text), `server_version` parseable.
2. Same with `Ack` (→204) and `Error` (→400 JSON) under `Accept: text/plain` → assert JSON/no-text.
3. Positive control: `BriefingContent` under `text/plain` DOES return formatted text (AC-09) — confirms the allowlist is exactly `{Entries, BriefingContent}`.

**Coverage Requirement**: All three non-injection responses asserted to stay JSON under `text/plain`; both injection responses asserted to honor it.

### R-07: Accept read after request consumed
**Severity**: Med **Likelihood**: Med
**Impact**: Header silently lost → JSON returned when text requested; no error, just wrong negotiation. F2's text path appears broken with no server-side signal.

**Test Scenarios**:
1. End-to-end: `Accept: text/plain` for an `Entries` response → assert `Content-Type: text/plain` actually returned (proves the header survived `into_parts`).
2. Negative: no `Accept` header → assert JSON. Both branches assert the **negotiated content-type**, not just status (SR-08).

**Coverage Requirement**: Negotiated content-type asserted on both the text and JSON branches at the HTTP boundary (not a unit test on the mapper in isolation, which would bypass the ordering bug).

### R-08: Frozen contract omits an F2/#670 field
**Severity**: High **Likelihood**: Med
**Impact**: F2 or #670 must re-open the "frozen" contract — defeating F1's entire purpose and forcing a re-negotiation across chunks.

**Test Scenarios**:
1. Cross-check the emitted `bindings/*.ts` against the ass-069 Q2/Q7 field list: the typed `TranscriptDeltaPayload {offset:u64, bytes:String}` (6th export), both `skip_serializing_if` `ImplantEvent` fields, both retention variants — present and correctly typed. The delta is now an explicit named binding, not an `any` payload F2 must re-type.
2. `transcript_delta` round-trips through `RecordEvent` with the exact `{offset, bytes}` payload, parsing into `TranscriptDeltaPayload`, and the typed payload binding appears in `bindings/` (AC-11) — covered by R-01 scenario 4's dual-sided fixture.
3. Retention enum exposes **both** `PurgeOnCycleClose` and `RetainDays(u32)` (AC-13) — neither variant dropped from the binding (the enum shape is the frozen enterprise seam even though OSS rejects `RetainDays` at validate; the *binding* must still carry both for F2/#670).

**Coverage Requirement**: Every field on the ass-069 provenance list verified present in the emitted artifact before merge; bindings + retention enum treated as the frozen F2/#670 interface.

### R-09: Retention enum missing a touchpoint
**Severity**: Med **Likelihood**: Med
**Impact**: Per #4070/#2730, config extensions reliably miss a site — a hidden literal construction or a merge arm. Result: absent-config loads a wrong default, or a project override is silently ignored.

**Test Scenarios**:
1. Absent `[retention]` section loads → `transcript_retention == PurgeOnCycleClose` (defaulter + `Default` impl both exercised) (AC-13).
2. **OSS `validate()` rejects `RetainDays(N)` outright with the enterprise-only error** (e.g. `EnterpriseOnly { field: "transcript_retention" }` / a message naming `RetainDays` as enterprise-only — NOT a generic range error), AND **accepts `PurgeOnCycleClose`** (AC-13, ADR-005). This is a new/updated obligation: there is no range-check arm in OSS — the only path through `RetainDays` is rejection. Footgun closed: an operator who sets `RetainDays` is told *why* it failed, rather than believing durable retention is silently in effect when OSS has no encrypt-at-rest to honor it safely (principle 8 / ass-069 in-memory-only).
3. Project-wins merge: distinct project (e.g. project sets a non-default it is *allowed* to set; merge picks project over global) → project wins (AC-14). Also assert the **merged result is re-validated** (#3905 — per-file validation misses merged invariants), so a merged `RetainDays` is still rejected.

**Coverage Requirement**: All four touchpoints exercised (default, **validate-reject-`RetainDays` + accept-`PurgeOnCycleClose`**, merge-project-wins, absent-config-load). Grep for any `RetentionConfig { .. }` literal construction missing the new field (#2730).

### R-10: TranscriptRetention TOML representation (MOSTLY DISSOLVED — ADR-005)
**Severity**: Low **Likelihood**: Low
**Impact**: Largely moot. The only **live OSS value** is the unit `transcript_retention = "PurgeOnCycleClose"`, which serde's externally-tagged default renders as a bare string — no representation ambiguity remains. `RetainDays` is rejected at `validate()` (R-09 scenario 2), so its tagged TOML form (`{ RetainDays = N }`) is an **enterprise-build concern, not F1's** — do **not** spend coverage prettifying the tagged form of a value OSS rejects.

**Test Scenarios**:
1. Confirm `transcript_retention = "PurgeOnCycleClose"` deserializes to `TranscriptRetention::PurgeOnCycleClose`.
2. Negative: a bare-`u32` (`transcript_retention = 30`) is **rejected**, not silently coerced (AC-13). *(Note: a TOML-supplied `RetainDays` that does parse is then rejected by `validate()` — R-09 scenario 2 — so no separate TOML-form coverage for it is required.)*

**Coverage Requirement**: `"PurgeOnCycleClose"` parses to the intended variant; bare-`u32` rejected. No coverage obligation for the rejected `RetainDays` tagged form.

### R-11: PartialEq on TranscriptRetention
**Severity**: Med **Likelihood**: Low
**Impact**: Merge `!=` fails to compile, or a hand-rolled `PartialEq` compares wrong → project override silently dropped.

**Test Scenarios**:
1. Compile-time: the merge arm using `!=` builds (derive present).
2. Equality test: `RetainDays(30) != RetainDays(31)`, `PurgeOnCycleClose != RetainDays(0)`, `RetainDays(30) == RetainDays(30)` — covered transitively by the merge test (R-09 scenario 3).

**Coverage Requirement**: Derive `PartialEq` (not hand-impl, per #3437 prefer derive); merge test exercises a real inequality.

### R-12: ts-rs runtime leak
**Severity**: Med **Likelihood**: Low
**Impact**: Dev-only tool enters the shipped dependency closure — supply-chain footprint the constraint forbids.

**Test Scenarios**:
1. `cargo tree --edges normal` for shipped crates → ts-rs **absent**.
2. `cargo metadata` → ts-rs only under `[dev-dependencies]`.
3. `cargo audit` passes (AC-15).

**Coverage Requirement**: All three checks; ts-rs proven absent from the runtime edge set.

### R-13: Two transcript carriers drift
**Severity**: Low **Likelihood**: Med
**Impact**: Downstream #670 may merge excerpt and deltas inconsistently absent a stated precedence.

**Test Scenarios**:
1. Doc/reviewer check: the wire contract documents that `transcript_delta` (forward path) supersedes `transcript_excerpt` (legacy) when both present. No merge logic added in F1 (SR-06).

**Coverage Requirement**: Precedence note present; reviewer confirms no merge code pulled forward.

### R-14: CI diff-gate non-functional
**Severity**: Med **Likelihood**: Med
**Impact**: The gate guarding every codegen AC silently passes on drift — the meta-risk that voids R-01/R-02/R-08's protection.

**Test Scenarios**:
1. Mutate a wire field **without** regenerating → assert the CI step exits **non-zero** on the `git diff --exit-code crates/unimatrix-engine/bindings/` (AC-03).
2. Restore/regenerate → assert it passes. Confirm `cargo test` runs (generates) **before** the diff, and the diff path is the bindings dir.

**Coverage Requirement**: The gate is proven to fail on real drift and pass on clean state — not merely present in the workflow.

## Integration Risks

- **Dispatch convergence (R-03/R-04)**: HTTP `/observe` and UDS both reach the single `RecordEvent` arm via `dispatch_request`. This is a strength (one guard covers both) but a test trap: a guard that works in a UDS unit test can be bypassed if the HTTP path captures/transforms the event differently (e.g. `prefix_session_id` runs before dispatch). Both transports must be exercised end-to-end, plus the `RecordEvents` batch arm.
- **col-022 pattern collision (#1266)**: The established listener pattern is "specialized handler THEN fall-through to generic persistence." The delta guard inverts this — it must early-`return Ack` and never reach generic persistence. A delivery agent reusing #1266 by muscle memory reintroduces R-03. The zero-rows gate test is the canary.
- **Mapper signature change (R-06/R-07)**: `observe_response_to_http(resp, wants_text)` touches the single existing caller (`router.rs:250`). The bool must be computed before `into_parts` and threaded correctly; testing the mapper in isolation hides the ordering bug — assert at the HTTP boundary.
- **Config merge re-validation (#3905)**: per-file `validate()` does not cover invariants that only emerge after the project-wins merge; the merged `transcript_retention` must be re-validated.
- **Budget coupling (R-05)**: the text path's `max_bytes` is implicitly coupled to the UDS hook's injection constant; if that constant changes later, byte-identity silently breaks unless both reference one source.

## Edge Cases

- Empty `Entries` (no items) under `Accept: text/plain` → `format_injection` returns `None` → 204 (no-content), matching ADR-003.
- Over-budget `Entries` exceeding `max_bytes` → truncation boundary must match `format_injection` exactly (R-05 scenario 2).
- `transcript_delta` with `offset: 0` and empty `bytes` → still `Ack`, still zero rows.
- `transcript_delta` payload missing `offset` or `bytes`, or with extra keys → guard keys on `event_type` only, so still dropped (must not error trying to parse payload).
- `RetainDays(N)` for **any** `N` (including `0`) → OSS `validate()` rejects with the enterprise-only error (no range check; rejection is the only OSS path through the variant — ADR-005).
- Absent `[retention]` block entirely vs present-but-`transcript_retention`-absent → both load `PurgeOnCycleClose`.
- `Accept: text/plain, application/json` (multi-value) and `Accept: */*` → define and test the `wants_text` predicate (contains `text/plain`).
- 1 MiB frame ceiling unchanged — a `transcript_delta` near `MAX_PAYLOAD_SIZE` is still framed/dropped normally (no per-delta cap server-side).
- `HookInput` flatten with keys that collide with named fields → assert named fields win, extras isolated in `extra`.

## Security Risks

- **Untrusted input — `transcript_delta.bytes`**: raw conversation bytes from a client, possibly containing secrets/API keys. **Blast radius if mishandled**: permanent secret exposure in durable SQLite, surviving session end, readable by any DB-access path — a principle-8 violation. Mitigation: accept-and-drop guard (R-03), zero-rows gate test on both transports (R-04). This is the highest-consequence risk in the feature.
- **Untrusted input — `Accept` header**: attacker-controlled; worst case is forcing the text path. Bounded — text path only runs for `Entries`/`BriefingContent` and only calls `format_injection` (no injection/eval). No path-traversal or deserialization surface. Low blast radius.
- **Untrusted input — `HookRequest` body / fixtures**: 1 MiB ceiling (`MAX_PAYLOAD_SIZE`) and `/observe` body limit remain authoritative — DoS bounded. `flatten extra: serde_json::Value` accepts arbitrary keys by design but they are inert data, not executed.
- **Config (`transcript_retention`)**: operator-supplied; governs the **raw, possibly-secret-bearing session transcript** (ephemeral working state — the `transcript_delta` bytes), not durable/sanitized artifacts. OSS `validate()` **rejects `RetainDays(N)` outright** (enterprise-only error) and accepts only `PurgeOnCycleClose` — because honoring durable retention of raw transcript requires encrypt-at-rest the OSS build lacks (principle 8 / ass-069 in-memory-only). Accept-and-ignore would be a footgun (operator believes retention is on). No injection surface (TOML → enum).
- **Guardrail — no content secret-scanner exists**: Unimatrix has **no** reusable content secret-redactor/scanner; write-path defenses are structural validation + metadata sanitization, and content is stored verbatim (ADR-005, `hook.rs:2348`). The architectural control — **accept-and-drop (ADR-004) + in-memory-ephemeral (#670) + purge-on-cycle-close** — IS the secrets guarantee. **No test or design may assume a redactor licenses persisting raw transcript.** A reviewer must reject any path (now or in #670) that justifies durable raw-transcript writes by appeal to a scanner. This is why `RetainDays` is rejected rather than accepted-and-ignored in OSS.
- **Auth inheritance (NFR-04)**: `transcript_delta` rides the existing `SessionWrite` capability + bearer auth — assert a request lacking `SessionWrite` is rejected on the delta path exactly as on any `RecordEvent`. No new auth surface must be introduced.
- **Supply chain (R-12)**: ts-rs must not enter the runtime closure (`cargo tree --edges normal`, `cargo audit`).

## Failure Modes

| Condition | Expected behavior | Verified by |
|-----------|-------------------|-------------|
| Maintainer edits `wire.rs`, forgets to regenerate bindings | CI diff-gate fails the merge (non-zero exit) | R-14 / AC-03 |
| Client streams `transcript_delta` before #670 buffer exists | `Ack` returned, nothing persisted (graceful no-op) | R-03 / AC-12 |
| `Accept: text/plain` on `Pong`/`Ack`/`Error` | JSON envelope returned unchanged (text ignored, not error) | R-06 / AC-09 |
| `format_injection` returns `None` (empty/over-budget) | 204 no-content, not a 500 | R-05 / ADR-003 |
| `RetainDays(N)` set in OSS config | Startup `validate()` **rejects** it loudly with the enterprise-only error naming `RetainDays` (not a silent accept-and-ignore, not a generic range error) | R-09 / AC-13 / ADR-005 |
| `PurgeOnCycleClose` set (or absent config) in OSS | Accepted; loads `PurgeOnCycleClose` | R-09 / AC-13 |
| Malformed `transcript_delta` payload (missing offset/bytes) | Still `Ack` + drop (guard keys on event_type, not payload shape) | Edge case |
| ts-rs accidentally in runtime graph | `cargo audit`/`cargo tree` check fails CI | R-12 / AC-15 |
| Bare `u32` written for `transcript_retention` | Config load rejects it (enum is the only accepted shape) | R-10 / AC-13 |

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution / Test Coverage |
|-----------|------------------|----------------------------|
| SR-01 (ts-rs tagged/flatten fidelity) | R-01 | Round-trip fixture is the contract authority (ADR-002); per-tagged-variant + flatten fixtures asserted by Node harness, not type-compile-only (AC-04/AC-05). |
| SR-02 (None-vs-omission serde behavior) | R-02 | Dual-direction assertion per named field, both runtimes, non-trivial value (#3557); AC-06/FR-07. |
| SR-03 (ts-rs runtime leak) | R-12 | `cargo tree --edges normal` + `cargo metadata` + `cargo audit`; AC-15/NFR-01. |
| SR-04 (contract completeness for F2/#670) | R-08, R-01 | Emitted bindings + retention enum cross-checked against ass-069 Q2/Q7 field list before merge; AC-11/AC-13. **Delta payload now a typed `TranscriptDeltaPayload` (6th export) verified dual-sided** (R-01 scenario 4), closing the formerly-untyped (`any`) highest-drift field — F2 no longer re-types it. |
| SR-05 (scope creep into #670) | R-03 (guard asserts non-persistence, not buffering) | AC-12 asserts **zero rows**, never buffering; reviewer rejects any in-memory accumulation. |
| SR-06 (two transcript carriers drift) | R-13 | Precedence note in wire contract (deltas supersede excerpt); no merge logic in F1; FR-15. |
| SR-07 (secrets-to-disk hole) | R-03, R-04 | Accept-and-drop guard; zero-durable-rows **gate** test on HTTP + UDS + batch arm; green before downstream ACs (#4711/#4311); AC-12. |
| SR-08 (Accept read ordering) | R-07 | Header read before `into_parts`; negotiated content-type asserted on both branches at HTTP boundary; AC-07/AC-08. |
| SR-09 (format_injection re-impl drift) | R-05 | Byte-identical gate against the real `hook.rs:1047` fn with production budget; Constraint 4; AC-07. |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 4 (R-01, R-02, R-03, R-04) | 12 scenarios — R-03 zero-rows is a **gate prerequisite** (green before any downstream AC); R-01 now includes the dual-sided `TranscriptDeltaPayload` fixture |
| High | 6 (R-05, R-06, R-07, R-08, R-09, R-14) | 14 scenarios — byte-identity, allowlist, ordering, contract-completeness, config-touchpoints (incl. `RetainDays`-reject), CI-gate-self-test |
| Medium | 2 (R-11, R-12) | 5 scenarios — PartialEq, runtime-leak |
| Low | 2 (R-13, R-10) | 2 scenarios — precedence note (reviewer); R-10 mostly dissolved (only `"PurgeOnCycleClose"` + bare-`u32`-reject) |

> Note: R-14 (CI gate self-test) is High-priority despite Med severity because it is the **meta-gate** —
> if it is non-functional, the protection R-01/R-02/R-08 depend on silently evaporates.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_search` for serde round-trip / silent-fallthrough / config-merge risk patterns — found #4711 (delta secrets-to-disk, the central F1 risk), #3557 (dual-direction serde test pattern), #1266 (col-022 specialize-then-fall-through — the *anti*-pattern the delta guard must NOT reuse), #4070/#2730 (config-extension hidden-site misses), #3905 (merge re-validation), #885 (serde-heavy types under-tested).
- Stored: nothing novel to store — the governing patterns (#4711, #3557, #1266, #4070) already exist and are feature-specific in their current form; this strategy applies them rather than generalizing a new cross-feature pattern. The one candidate generalization — "an early-`return`-drop guard inverts the col-022 specialize-then-fall-through pattern; reusing #1266 by reflex reintroduces a secrets-to-disk hole" — is a refinement of #4711/#1266 and better captured as a delivery-time lesson if the trap actually materializes than pre-emptively stored.
