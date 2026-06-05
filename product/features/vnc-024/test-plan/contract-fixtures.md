# Test Plan — contract-fixtures (Deliverable 1, round-trip fixtures + node harness)

> Covers AC-04, AC-05, AC-06, AC-11 (dual-sided), and the binding-completeness side of AC-13/R-08.
> Risks R-01 (mis-modeled serde — tag/flatten), R-02 (None-vs-omission, the single most-omitted test
> category #885/#3557), R-08 (frozen-contract completeness), R-13 (precedence note). Pseudocode:
> `pseudocode/contract-fixtures.md`. **ADR-002: the round-trip JSON fixture — not the generated `.ts`
> — is the contract authority.** Type-compilation alone does NOT satisfy any risk here.

## Scope of this component

Rust-emitted JSON fixtures under `crates/unimatrix-engine/bindings/fixtures/*.json`, the extended
`wire.rs:379+` `#[cfg(test)]` Rust round-trip suite, and the standalone Node harness
`crates/unimatrix-engine/bindings/contract.test.mjs` (`node --test`, ~dozen lines, no TS client package).
Both runtimes deserialize the **same committed fixtures** and assert **behavior**, not "it parses".

## Two-runtime contract: every behavioral assertion runs in BOTH Rust and Node

A fixture is the contract only if both the source-of-truth runtime (Rust) and the consuming-language
runtime (Node, against the ts-rs bindings) agree. A single-runtime assertion leaves the other side
unguarded (#3557).

## R-01 — tagged variants & flatten (AC-04, AC-05)

### Per-tagged-variant fixtures
- At least one committed fixture **per variant** of `HookRequest` and `HookResponse` (both
  `#[serde(tag = "type")]`).
- **Rust**: `serialize → deserialize` identity for every variant; serialized JSON carries the correct
  literal `type` discriminant.
- **Node** (`node --test`): each fixture deserializes and **narrows to the correct union member keyed
  on the literal `type`** — assert a member-specific field is present (AC-04), not merely that JSON
  parses. One assertion per tagged variant.
- **Negative**: a deliberately malformed fixture (wrong/absent `type`) makes `node --test` exit
  **non-zero** (AC-05) — proves the harness actually discriminates.

### Flatten (`HookInput.extra`)
- Fixture: a `HookInput` JSON with extra **unknown top-level keys** alongside named fields.
- Assert (Rust **and** Node): named fields parse into their fields AND the extras land under `extra`
  (non-empty `extra`).
- **Collision edge case**: a flatten key colliding with a named field → assert named field wins, extras
  isolated in `extra` (Edge Cases list).

## R-02 — None-vs-omission, dual-direction, non-trivial (AC-06)

For **each** of `ImplantEvent.topic_signal`, `ImplantEvent.provider`, `ContextSearch.source`,
`CompactPayload.transcript_excerpt` (`skip_serializing_if = "Option::is_none"`), assert in **BOTH**
Rust and Node:

1. **Emit-absent**: when the field is `None`, the key is **absent** from the emitted JSON — **not
   `null`** (the #3557 trap). Assert the key string does not appear in the serialized output.
2. **Parse-default**: a fixture **omitting** the key deserializes to the default (`None`).
3. **Non-trivial round-trip guard**: a fixture with the field **present and a non-trivial value**
   round-trips it intact — so a partial wiring (added to emitter but not consumer) cannot pass on the
   all-`None` path (#3557). Use a real value (e.g. a concrete `topic_signal`/`provider` string), not
   an empty/None placeholder.

**Coverage requirement (hard):** all four fields, both directions, both runtimes, with a non-trivial
value. No field is "covered" by a single-direction or all-`None` assertion. A reviewer rejects a
4-field × 1-direction matrix.

## R-01 scenario 4 / AC-11 — TranscriptDeltaPayload DUAL-SIDED (drift-closing)

The single genuinely-new field. The guard (transcript-delta-guard.md) parses into the **same**
`TranscriptDeltaPayload { offset: u64, bytes: String }` struct — the fixture and the guard share one shape.

- Committed fixture: `{ "offset": <u64>, "bytes": "<non-empty text>" }` with non-trivial values.
- **Rust→TS direction**: the Rust round-trip test serializes/deserializes the fixture into the Rust
  `TranscriptDeltaPayload` (offset/bytes intact).
- **TS→Rust direction** (`node --test`): the fixture deserializes into the `TranscriptDeltaPayload`
  binding on the TS side — this is what catches a client-side `offset`/`bytes` shape drift.
- **A Rust-emit-only check does NOT satisfy AC-11.** Both parse directions, into the typed struct on
  both sides, are mandatory.
- Cross-check (AC-11 grep): `bindings/TranscriptDeltaPayload.ts` is emitted; `RecordEvent`/`ImplantEvent`
  bindings still carry arbitrary `event_type: string` + `payload` (the carrier is unchanged — no new wire variant).

## R-08 — frozen-contract completeness (binding side; AC-11, AC-13 binding)

- Cross-check the emitted `bindings/*.ts` against the **ass-069 Q2/Q7 field list**: the typed
  `TranscriptDeltaPayload`, both `ImplantEvent` `skip_serializing_if` fields, and — for the retention
  enum binding — **both** `PurgeOnCycleClose` and `RetainDays(u32)` variants present (the enum *shape*
  is the frozen F2/#670 seam even though OSS `validate()` rejects `RetainDays`; the **binding** must
  still carry both variants).
- Freeze the fixture set against the provenance list **before merge** — every field F2/#670 consume is
  present and correctly typed. The delta is now a named binding, not an `any` payload F2 re-types.

## R-13 — precedence note (Low, reviewer/doc)

- Reviewer/doc check: the wire contract documents that `transcript_delta` (forward path) **supersedes**
  `CompactPayload.transcript_excerpt` (legacy) when both present. **No merge logic added in F1** (SR-06)
  — reviewer confirms no merge code pulled forward.

## Edge cases

- Malformed fixture → `node --test` non-zero (proves harness rigor, AC-05).
- Flatten key colliding with a named field → named field wins, extras in `extra`.
- A `skip_serializing_if` field emitting `null` instead of absent → caught by the emit-absent assertion
  (the exact #3557 regression).
- `offset: 0` / empty `bytes` is the guard's concern (transcript-delta-guard.md); the contract fixture
  uses non-trivial values to keep the round-trip exercising the field.

## Out of scope for this plan

- The codegen mechanism / CI drift-gate / runtime-leak → `ts-rs-codegen.md`.
- Runtime drop behavior of `transcript_delta` (zero rows) → `transcript-delta-guard.md` (shares the struct shape).

## Self-check
- [ ] Per-tagged-variant fixtures for HookRequest+HookResponse; Node narrows to correct member (AC-04/AC-05); malformed → non-zero.
- [ ] Flatten fixture: named parse + non-empty extra, both runtimes; collision edge case.
- [ ] All four None-vs-omission fields: emit-absent + parse-default + non-trivial round-trip, both runtimes (AC-06).
- [ ] TranscriptDeltaPayload fixture asserted DUAL-SIDED into the typed struct on both sides (AC-11) — not Rust-emit-only.
- [ ] Bindings cross-checked vs ass-069 Q2/Q7 list; retention binding carries both variants (R-08).
- [ ] Precedence note present; no F1 merge code (R-13).
