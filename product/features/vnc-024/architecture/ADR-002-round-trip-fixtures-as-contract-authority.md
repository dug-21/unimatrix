## ADR-002: JSON Round-Trip Fixtures Asserting Serde BEHAVIOR Are the Contract Authority

### Context
Type-level codegen (ADR-001) captures *structure* but not *serde behavior*. The behaviors that ship
wrong silently — and that Unimatrix history (#885, #3557, #4311) flags as the single most-omitted
test category — are: `None`-vs-omission under `skip_serializing_if`, `#[serde(default)]` optionality,
internally-tagged enum discriminants, and `#[serde(flatten)]` of unknown keys. If ts-rs emits a
shape that *compiles* but mis-models a tag or a flatten, a type-compile-only check passes and the
wrong contract ships — and F2/#670 then build on it (SR-01/SR-02/SR-04). F1's entire purpose is to
lock the contract; a deferred or compile-only TS check would ship it unverified.

### Decision
Make the round-trip JSON fixture — not the generated `.ts` — the authority on the contract. The
fixture is produced by Rust (the source of truth) and verified independently by both Rust and a
standalone Node harness:

1. **Rust emit + assert** (extend the existing `wire.rs` `#[cfg(test)]` suite at `:379+`, do not
   scaffold new infra — Constraint 8): serialize every `HookRequest` and `HookResponse` variant, plus
   the serde edge cases, to committed fixtures under `crates/unimatrix-engine/bindings/fixtures/*.json`.
   Assert serialize→deserialize identity in Rust (AC-11).
2. **Node round-trip** (`crates/unimatrix-engine/bindings/contract.test.mjs`, ~dozen lines, run via
   `node --test`, no TS client package — OQ-03/AC-05): import the committed ts-rs bindings, deserialize
   the same committed fixtures, and assert **behavior**, not just that it parses:
   - **Tagged variant**: each fixture carries the correct literal `type` discriminant and narrows to
     the right union member (AC-04). At least one fixture per tagged variant of `HookRequest` and
     `HookResponse`.
   - **None-vs-omission**: for every `skip_serializing_if = "Option::is_none"` field —
     `ImplantEvent.topic_signal`, `ImplantEvent.provider`, `ContextSearch.source`,
     `CompactPayload.transcript_excerpt` (AC-06) — a dual-direction assertion: the field is
     *absent from the JSON* when `None` (not `null`), and a fixture omitting it deserializes to the
     default. (Pattern #3557.)
   - **Flatten**: a `HookInput` fixture with extra unknown keys asserts they land in `extra` and the
     named fields parse alongside them.
   - **transcript_delta payload**: a fixture for the typed `TranscriptDeltaPayload { offset, bytes }`
     struct (the 6th exported binding, ADR-001) round-trips **dual-sided** — Rust→TS *and* TS→Rust,
     like AC-06's None-vs-omission case, not Rust-emit-only. The delta payload is the single
     genuinely-new field this feature introduces; a one-directional check would let the TS client's
     `offset`/`bytes` shape drift from the Rust struct. The fixture parses into `TranscriptDeltaPayload`
     on both sides (this is the same struct the accept-and-drop guard deserializes into — ADR-004).
     (AC-11; see ADR-004.)

The fixture set is frozen against the ass-069 Q2/Q7 field list before merge (SR-04) — the emitted
bindings carry every field F2/#670 consume.

### Consequences
**Easier**: Behavioral mismatches that codegen cannot catch are caught at CI by an independent
runtime in the consuming language. The contract is locked by evidence, not by trust in a new tool.
F2/#670 inherit a verified surface.

**Harder**: Every new wire variant or `skip_serializing_if` field requires a new fixture + a Node
assertion, or coverage silently regresses. The Node harness is a second test runner in CI (Node is
already required for the npm package, so no new toolchain). Fixtures are committed artifacts that
must be regenerated and reviewed when the wire changes.

Cross-references: ADR-001 (codegen produces the types these fixtures verify); ADR-004 (the
transcript_delta fixture).
