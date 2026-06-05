# Component 1 — ts-rs Codegen + CI Diff-Gate (Deliverable 1)

> ADR-001. Derive machine-generated TS bindings for **6** exported types, committed and CI-gated
> against drift. Dev-only — zero runtime footprint.

## Purpose

Make the Rust serde wire types the single authoritative source of the TS wire contract. `cargo test`
emits `bindings/*.ts`; CI fails the build if committed bindings drift from freshly-generated output.
F2/F5 vendor `bindings/` without re-typing anything by hand.

## Files

| File | Action |
|------|--------|
| `crates/unimatrix-engine/Cargo.toml` | Modify — add `ts-rs` under `[dev-dependencies]` only |
| `crates/unimatrix-engine/src/wire.rs` | Modify — add `TS` derive + `#[ts(export)]` on 6 types; new `TranscriptDeltaPayload` struct; `TRANSCRIPT_DELTA_EVENT` const; precedence doc-comment; codegen test in `#[cfg(test)]` (:379+) |
| `crates/unimatrix-engine/bindings/*.ts` | Create — 6 committed generated bindings |
| existing CI workflow | Modify — fold `cargo test` + `git diff --exit-code` into the existing test job |

## Cargo.toml change

```
[dev-dependencies]
ts-rs = "<current>"   # serde-compat feature is default-on; handles tag/flatten/skip annotations
```
- MUST be `[dev-dependencies]`, NOT `[dependencies]`. (Constraint 1 / NFR-01 / AC-15.)
- The `#[derive(TS)]` macro is inert outside `cfg(test)` because `#[ts(export)]` only writes files
  when the test binary runs — no runtime code path touches ts-rs.

## wire.rs derive changes (the 6 exported types)

Add `TS` to the existing derive list and a `#[ts(export, export_to = "../bindings/")]` attribute on
each. **Do NOT change any existing serde annotation** — shapes are frozen (NFR-03; binding diff for
pre-existing types must be empty).

```
// 5 existing wire types — derive bump only, shapes unchanged:
#[derive(Deserialize, Debug, Clone, ts_rs::TS)]                 // HookInput (:44)
#[ts(export, export_to = "../bindings/")]

#[derive(Serialize, Deserialize, Debug, Clone, ts_rs::TS)]      // HookRequest (:93)
#[serde(tag = "type")] #[ts(export, export_to = "../bindings/")]

#[derive(Serialize, Deserialize, Debug, Clone, ts_rs::TS)]      // HookResponse (:175)
#[serde(tag = "type")] #[ts(export, export_to = "../bindings/")]

#[derive(Serialize, Deserialize, Debug, Clone, ts_rs::TS)]      // ImplantEvent (:200)
#[ts(export, export_to = "../bindings/")]

#[derive(Serialize, Deserialize, Debug, Clone, ts_rs::TS)]      // EntryPayload (:233)
#[ts(export, export_to = "../bindings/")]

// 6th NEW type — the typed delta payload (shared shape, see OVERVIEW):
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, ts_rs::TS)]
#[ts(export, export_to = "../bindings/")]
pub struct TranscriptDeltaPayload { pub offset: u64, pub bytes: String }
```

### serde-shape notes ts-rs must preserve (verified by Component 2, not here)
- `HookRequest`/`HookResponse`: `#[serde(tag = "type")]` ⇒ ts-rs emits a discriminated union with a
  literal `type` field per variant (FR-04 / AC-04).
- `HookInput`: `#[serde(flatten)] extra: serde_json::Value` ⇒ extra keys captured (FR-07).
- `ImplantEvent.topic_signal` / `.provider`: `skip_serializing_if = "Option::is_none"` ⇒ optional
  in TS (FR-07 / AC-06).
- `TranscriptDeltaPayload`: `offset: u64`, `bytes: String` — ts-rs emits `bigint`/`number` per its
  u64 mapping and `string`. The point is it emits a **named typed binding**, not `any`/`JsonValue`
  (closes R-01/R-08 highest-drift field).

## New constant + precedence doc

```
// wire.rs constants block (near :13-36)
/// event_type value carrying a client-streamed raw transcript span. Routed by the
/// accept-and-drop guard (vnc-024 ADR-004). Carried in ImplantEvent.payload as
/// TranscriptDeltaPayload { offset, bytes } — NOT a new wire variant.
pub const TRANSCRIPT_DELTA_EVENT: &str = "transcript_delta";
```

```
// doc-comment ADDED to ImplantEvent (:200) — FR-15 / SR-06 precedence note, documentary only:
/// transcript_delta precedence: when both a streamed `transcript_delta`
/// (event_type == TRANSCRIPT_DELTA_EVENT, payload = TranscriptDeltaPayload) and a legacy
/// CompactPayload.transcript_excerpt are present, the streamed delta is the authoritative
/// forward path and supersedes the excerpt (ass-069). F1 documents this; the merge logic is #670.
```
No merge code is added (reviewer check, SR-06).

## Codegen test (wire.rs `#[cfg(test)]` module, :379+)

ts-rs auto-exports on any `cargo test` run for types carrying `#[ts(export)]`. Add an explicit
sentinel test so intent is visible and a clean-checkout `cargo test` reliably writes all six files
before the fixture emitter (Component 2) runs.

```
FUNCTION test_export_bindings():            // #[test]
    // ts-rs writes each #[ts(export)] type to export_to on first reference in the test binary.
    // Force-reference all six so a partial build cannot skip one:
    TranscriptDeltaPayload::export_all()    // or ts_rs::export!{ ... } / per-type ::export()
    HookInput::export_all()
    HookRequest::export_all()
    HookResponse::export_all()
    ImplantEvent::export_all()
    EntryPayload::export_all()
    // Assert each expected bindings/<Name>.ts now exists and is non-empty (FR-02 / AC-02).
    FOR name IN [HookInput, HookRequest, HookResponse, ImplantEvent, EntryPayload, TranscriptDeltaPayload]:
        path = "../bindings/" + name + ".ts"
        ASSERT file_exists(path) AND file_len(path) > 0
```
> Delivery confirms the exact ts-rs export API for the pinned version (`export_all` vs `export` vs
> the `export!` macro). The contract is: after `cargo test`, all six `.ts` exist and are non-empty.

## CI diff-gate (existing test job — no new workflow)

Append to the existing test job, in this order (R-14: order is load-bearing):
```
STEP cargo test --workspace          # generates bindings/*.ts and fixtures/*.json
STEP git diff --exit-code crates/unimatrix-engine/bindings/   # FAILS (non-zero) on any drift
STEP node --test crates/unimatrix-engine/bindings/contract.test.mjs   # Component 2
```
- `cargo test` MUST run **before** the diff (generation precedes comparison).
- The diff path MUST be `crates/unimatrix-engine/bindings/` (covers both `.ts` and `fixtures/`).
- Do NOT pre-clean the tree in a way that hides a dirty commit; the diff is against committed truth.

## Initialization / sequencing

`TranscriptDeltaPayload` + `TRANSCRIPT_DELTA_EVENT` must compile before Components 2 and 4. This
component lands the type definitions; Component 4 only references the constant.

## Data flow

- Input: the 6 serde type definitions in `wire.rs`.
- Output: `bindings/HookInput.ts`, `HookRequest.ts`, `HookResponse.ts`, `ImplantEvent.ts`,
  `EntryPayload.ts`, `TranscriptDeltaPayload.ts` (committed).
- Transformation: serde annotations → TS structural types (discriminated unions, optional fields,
  flatten). Behavior (None-omission, discriminant) is NOT captured here — Component 2 owns that.

## Error handling

- ts-rs export is infallible at runtime (test-only). A failed file write panics the test → CI red.
- The diff-gate's only "error" is drift: non-zero exit fails the build (the intended behavior).

## Key test scenarios (hints — full plan in test-plan/ts-rs-codegen.md)

- **AC-01/AC-15**: `cargo metadata` shows ts-rs only under `[dev-dependencies]`; `cargo tree
  --edges normal` shows it absent; `cargo audit` passes. (R-12.)
- **AC-02**: clean checkout → `cargo test` → all six `bindings/*.ts` exist, non-empty.
- **AC-03 / R-14**: mutate a `wire.rs` field without regenerating → diff step exits non-zero;
  restore → passes. Confirm `cargo test` runs before the diff and the diff path is the bindings dir.
- **AC-04**: inspect `HookRequest.ts`/`HookResponse.ts` — each variant carries its literal `type`.
- **AC-11 (emit side)**: `TranscriptDeltaPayload.ts` is emitted, typed `{ offset, bytes }`, not `any`.

## Open questions / gaps

- **Exact ts-rs export API** for the pinned major version (`export` vs `export_all` vs `export!`)
  is delivery-confirmed. Non-blocking — the post-`cargo test` file-existence contract is stable.
- **u64 → TS representation**: ts-rs may emit `bigint` for `u64` (`offset`). Component 2's dual-sided
  fixture must use a value the chosen JSON-parse path round-trips without precision loss; delivery
  confirms whether a `#[ts(type = "number")]` override or a within-2^53 fixture value is used.
