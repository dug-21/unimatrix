# Test Plan — ts-rs-codegen (Deliverable 1, codegen + CI diff-gate)

> Covers AC-01, AC-02, AC-03, AC-15. Risks R-12 (runtime leak), R-14 (CI diff-gate non-functional —
> the **meta-gate** protecting every other codegen AC). Behavioral serde fidelity (R-01/R-02) is the
> sibling `contract-fixtures.md` plan's job; this plan proves the **codegen mechanism and its gate**.
> Pseudocode: `pseudocode/ts-rs-codegen.md`. Files: `Cargo.toml`, `wire.rs` (derives), `bindings/*.ts`, CI workflow.

## Scope of this component

The ts-rs dev-dependency, the `TS` + `#[ts(export, export_to = "../bindings/")]` derives on the **six**
exported types, the `cargo test`-driven generation, and the CI `git diff --exit-code` drift-gate. Plus
the proof that ts-rs stays out of the runtime dependency closure.

## Unit / build-mechanism expectations

### AC-01 — six types derive TS, ts-rs is dev-only
- **file-check** `crates/unimatrix-engine/Cargo.toml`: `ts-rs` appears under `[dev-dependencies]`, and is **absent** from `[dependencies]` and `[build-dependencies]`.
- **grep** `wire.rs`: each of `HookInput`, `HookRequest`, `HookResponse`, `ImplantEvent`, `EntryPayload`, `TranscriptDeltaPayload` carries `#[derive(... TS ...)]` and `#[ts(export, export_to = "../bindings/")]`. Assert **exactly six** export sites (a 7th or a missing one both fail).
- Assert `TranscriptDeltaPayload` is present in this set (the 6th export — the design-review delta).

### AC-02 — `cargo test` generates committed bindings for all six
- On a clean checkout, `cargo test -p unimatrix-engine` regenerates the bindings.
- **file-check**: each of `bindings/{HookInput,HookRequest,HookResponse,ImplantEvent,EntryPayload,TranscriptDeltaPayload}.ts` **exists and is non-empty** after the test run.
- Assert generation is driven by `cargo test` (the `#[ts(export)]` mechanism fires from the test binary), not a bespoke build step.

### AC-04 support — discriminated unions carry literal `type` (structure side)
- **grep** generated `HookRequest.ts` / `HookResponse.ts`: each variant carries its literal `type` field. (The *behavioral* authority is contract-fixtures' Node harness per ADR-002; this is the structural cross-check only.)

## CI drift-gate self-test (R-14 — the meta-gate, AC-03)

This is High-priority despite Med severity: if the gate is non-functional, R-01/R-02/R-08's protection
silently evaporates. The gate must be proven to **fail on real drift and pass on clean state** — not
merely present in the workflow.

- **test_ci_gate_fails_on_drift**: mutate a `wire.rs` field (e.g. rename or add a field) **without**
  regenerating → the CI step `cargo test -p unimatrix-engine && git diff --exit-code crates/unimatrix-engine/bindings/`
  exits **non-zero**. (Run in a throwaway worktree/temp copy so the repo is not left dirty.)
- **test_ci_gate_passes_on_clean**: regenerate / restore → the same step exits **zero**.
- **ordering assertion**: confirm `cargo test` (which *generates*) runs **before** the `git diff`, and
  the diff path is exactly `crates/unimatrix-engine/bindings/` (not a parent dir, not a stale path). A
  gate that diffs before generating, or targets the wrong path, passes on a dirty tree — this is the
  exact R-14 failure mode.
- Confirm the gate lives in the **existing** test job (OQ-01 — no new workflow).

## Runtime-leak / supply-chain (R-12, AC-15)

All three checks required; ts-rs proven absent from the runtime edge set:

- **test/shell: cargo tree** — `cargo tree --edges normal -p unimatrix-engine` (and for any shipped
  crate) → `ts-rs` **absent** from the normal (runtime) edge set.
- **shell: cargo metadata** — `cargo metadata --format-version 1` shows `ts-rs` only as a
  dev-dependency of `unimatrix-engine`, never a normal dependency of any crate.
- **shell: cargo audit** — `cargo audit` exits clean (AC-15). ts-rs adding a dev-only dependency must
  not introduce an advisory on the shipped graph.

## Edge cases

- A field added to a wire type but not exported (no `#[ts(export)]`) → its binding is missing → the
  contract-fixtures cross-check (R-08) catches the absence; this plan asserts the six export sites so a
  silently-unexported 7th type is also caught.
- ts-rs feature unification pulling it into a normal edge via another crate → caught by `cargo tree --edges normal`.
- Dirty working tree at gate time → the ordering assertion (generate-then-diff, correct path) is what
  prevents a false pass; explicitly tested.

## Out of scope for this plan

- Behavioral serde correctness of the emitted types (tag/flatten/None-omission/dual-sided delta) →
  `contract-fixtures.md`.
- The runtime fixtures themselves → `contract-fixtures.md`.

## Self-check
- [ ] AC-01 asserts exactly six export sites incl. `TranscriptDeltaPayload`, ts-rs dev-only in Cargo.toml.
- [ ] AC-02 asserts all six `.ts` non-empty after `cargo test`.
- [ ] AC-03 / R-14 proves the gate FAILS on drift AND PASSES on clean, with generate-before-diff ordering + correct path.
- [ ] AC-15 / R-12: cargo tree --edges normal (absent) + cargo metadata (dev-only) + cargo audit (clean).
