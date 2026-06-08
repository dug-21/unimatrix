# wire.rs — cycle_stamp Wire Field

**Source**: `crates/unimatrix-engine/src/wire.rs` (extend). **ADR**: ADR-003.
**Constraints**: C-01 frozen-F1 additive (skip_serializing_if, no
deny_unknown_fields anywhere), C-16 wire additivity, #4726 ts-rs codegen
discipline (7th export, drift-checked).

## Purpose

Add the typed declared-attribution carrier `CycleStampPayload` and the additive
optional `ImplantEvent.cycle_stamp`. Presence of the field IS the declared flag —
the server's precedence becomes structural (presence-gated), not re-orderable.

## New struct (beside `TranscriptDeltaPayload`, ~wire.rs:307)

```rust
/// F4b: client-declared cycle attribution (contract, not inference).
/// Presence on an ImplantEvent means the row attributes from the stamp, not the
/// heuristic chain (ADR-004). The 7th ts-rs export.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../bindings/"))]
pub struct CycleStampPayload {
    pub topic: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
}
```

## New field on `ImplantEvent` (append AFTER `provider`, ~wire.rs:277)

```rust
    /// F4b client-declared cycle attribution (ADR-003). Some => this row
    /// attributes from the stamp (topic_source='declared'); None => legacy
    /// heuristic chain. Additive/frozen-F1: skip_serializing_if so it never
    /// appears in pre-existing fixtures; no deny_unknown_fields anywhere.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cycle_stamp: Option<CycleStampPayload>,
```

Placement after `provider` mirrors the JS attach order and keeps the struct's
existing fields byte-stable. The `#[derive]` list on `ImplantEvent` is unchanged
(Serialize, Deserialize, Debug, Clone + test ts-rs); no new bounds.

## 7th ts-rs export (`wire.rs:469` test, "all six")

The sentinel test currently force-exports six types and checks six binding files.
Add the seventh and rename/recount:
```rust
    // rename: test_export_bindings_all_six_written_and_nonempty → ..._all_seven_...
    CycleStampPayload::export_all(&cfg).expect("export CycleStampPayload bindings");
    // and add "CycleStampPayload" to the `for name in [ ... ]` existence list (~:479).
```
`ImplantEvent.ts` regenerates with the new optional field (an expected additive
binding diff). `CycleStampPayload.ts` is new. All other committed bindings stay
byte-identical (the field is `skip_serializing_if` and the struct is new). CI
`git diff --exit-code bindings/` must be re-baselined with exactly these two diffs.

## Tolerance Matrix (FR-12, binding)

- Old server + stamping client → unknown `cycle_stamp` field IGNORED (no
  `deny_unknown_fields` on `HookRequest`/`ImplantEvent` deserialize path — verify
  none is introduced).
- New server + Rust hook / old client → field absent → `cycle_stamp: None` →
  legacy chain (ADR-004). No feature flag; presence is per-event self-describing.
- All pre-existing wire fixtures pass byte-UNMODIFIED — the field never appears in
  them via `skip_serializing_if`.

## Round-Trip Discipline (#3486 / SR-03)

Field-exists-on-struct is INSUFFICIENT evidence. The binding requirement (verified
in listener-stamp-read.md + tests):
1. serde unit trio on `CycleStampPayload` / `ImplantEvent.cycle_stamp` — mirror
   the col-017 `topic_signal` trio (`wire.rs:1345-1367`):
   - None → field absent in serialized JSON (skip_serializing_if);
   - Some{topic, phase:Some} → both keys present;
   - Some{topic, phase:None} → `phase` absent; deserialize-tolerant of absent.
2. End-to-end row assertion: a stamped client event → server reads
   `event.cycle_stamp` → observation row lands `topic_signal=stamp.topic`,
   `phase=stamp.phase`, `topic_source='declared'` — asserted at ALL THREE record
   sites (the #3486 failure mode is one site forgetting the read).

## Data Flow / Type Transformations

```
JS attach:  cycle_stamp: {topic, phase?}              (omit-when-null)
   → JSON over wire (HTTP body / UDS frame, byte-identical — ADR-002 §7)
   → Rust deserialize: ImplantEvent.cycle_stamp: Option<CycleStampPayload>
   → listener read: stamp.topic (String), stamp.phase (Option<String>)
   → ObservationRow.topic_signal/phase/topic_source
```

## Error Handling

Deserialization of a malformed `cycle_stamp` (e.g. `topic` missing) fails the
whole `ImplantEvent` parse exactly as any other required-field violation does —
this is the existing serde contract; no new error path. `topic: String` is
required (a stamp with no topic is meaningless); `phase` is optional.

## Key Test Scenarios

- All pre-existing wire fixtures byte-unchanged (NFR-04).
- serde trio (None-absent / Some-present / phase-null-tolerant).
- No `deny_unknown_fields` on the deserialize path (grep-assert).
- ts-rs sentinel exports seven; `git diff --exit-code bindings/` clean except the
  expected `ImplantEvent.ts` additive diff + new `CycleStampPayload.ts`.
- Tolerance both directions (old-server-simulated via a struct without the field;
  unstamped Rust-hook frame → None → legacy chain).

## Open Questions / Gaps

None. The struct shape, serde attrs, and export count are fully specified by
ADR-003. The three read sites and round-trip assertion live in
listener-stamp-read.md.
