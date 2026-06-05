## ADR-001: ts-rs Wire-Contract Codegen as a Dev-Dependency with CI-Gated Bindings

### Context
The all-Rust→TS client migration (ass-068 Q3) requires the TS client's wire types to be *generated*
from the authoritative Rust serde schema (`crates/unimatrix-engine/src/wire.rs`), not hand-mirrored.
Hand-mirroring is the primary drift risk the migration exists to eliminate. ts-rs is not currently
in the workspace. Three constraints shape the choice: (1) it must add zero runtime/supply-chain
footprint to any shipped crate (AC-15, SR-03; rust-workspace minimal-footprint rule); (2) the
generated bindings must be a single CI-gated source of truth that downstream chunks consume without
guessing; (3) the codegen must run as part of the normal test cycle, not a bespoke build step.

ts-rs's `serde-compat` feature (default-on) handles the workspace's exact annotation set —
`#[serde(tag = "type")]` internally-tagged enums (`HookRequest`/`HookResponse`),
`#[serde(flatten)]` (`HookInput.extra`), `#[serde(default)]`, and
`#[serde(skip_serializing_if = "Option::is_none")]` (ass-068 Q3).

### Decision
Add `ts-rs` to `crates/unimatrix-engine/Cargo.toml` under `[dev-dependencies]` only. Derive `TS`
and annotate `#[ts(export, export_to = "../bindings/")]` on **six** exported types: the five wire
types `HookInput`, `HookRequest`, `HookResponse`, `ImplantEvent`, `EntryPayload`, plus the typed
payload struct `TranscriptDeltaPayload { offset: u64, bytes: String }` (ADR-004). The sixth type is
not a new wire carrier — `transcript_delta` still rides `ImplantEvent.payload: serde_json::Value`
unchanged — but giving the one genuinely-new field a typed `TranscriptDeltaPayload.ts` binding stops
ts-rs from emitting it as `any`/`JsonValue` and stops F2's TS client from hand-typing it, which is
exactly the hand-mirror drift this feature exists to kill. The `#[derive(TS)]` macro is inert at
runtime — `.ts` export fires only when the test binary runs (`#[ts(export)]` is driven by a
`cargo test` invocation). Running `cargo test` writes the committed bindings to
`crates/unimatrix-engine/bindings/` (the ts-rs default location).

These bindings are the **CI-gated source of truth**, *not* promoted to a workspace-root `bindings/`.
F2/F5 copy/vendor them into the bundled `@dug-21/unimatrix` client at build time. The CI gate runs
`cargo test` then `git diff --exit-code crates/unimatrix-engine/bindings/` inside the **existing**
test job (OQ-01 — no new workflow); committed bindings must equal fresh codegen output or the build
fails (AC-02/AC-03). An explicit assertion confirms ts-rs is absent from `cargo tree --edges normal`
and present only under `[dev-dependencies]` (SR-03).

The generated discriminated unions must express the `#[serde(tag = "type")]` enums with a literal
`type` field per variant (AC-04); this is verified behaviorally by ADR-002's fixtures, not assumed
from compilation.

### Consequences
**Easier**: TS types derive from one Rust source — zero hand-maintenance, zero drift surface for the
type layer. No runtime dependency, no supply-chain exposure, `cargo audit` unaffected (AC-15).
Downstream chunks vendor a known, fixed path. Codegen rides `cargo test` — no separate toolchain.

**Harder**: Six types now carry a derive that contributors must preserve. A new wire field
requires re-running `cargo test` and committing the regenerated `.ts` or CI blocks the merge (this
is the intended forcing function). ts-rs is new to the workspace with no prior in-repo evidence, so
its serde-compat fidelity on the hard cases (tagged enums, flatten) cannot be trusted from
compilation alone — ADR-002's round-trip fixtures are the safety net (SR-01).

Cross-references: ADR-002 (round-trip fixtures are the contract authority over the generated type).
