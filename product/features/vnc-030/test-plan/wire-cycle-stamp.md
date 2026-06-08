# Test Plan — C4 `wire.rs` `cycle_stamp` field

Source: ADR-003. AC: AC-02. Risks: R-16. File: extend the col-017 `topic_signal` serde test block in `crates/unimatrix-engine/src/wire.rs` (mirror the 5-test pattern at the documented block). `cargo test -p unimatrix-engine`.

Additive: `ImplantEvent.cycle_stamp: Option<CycleStampPayload>` with `#[serde(default, skip_serializing_if = "Option::is_none")]`; `CycleStampPayload { topic: String, phase: Option<String> }`; **7th** ts-rs export. No `deny_unknown_fields` anywhere (frozen-F1, NFR-04).

## Serde trio (mirror col-017 at wire.rs:1345-1367)

### implant_event_without_cycle_stamp_deserializes_to_none
- JSON without the field → `cycle_stamp.is_none()` (old-JSON backward compat, FR-11).

### implant_event_with_cycle_stamp_deserializes
- JSON `"cycle_stamp":{"topic":"vnc-030","phase":"delivery"}` → `Some` with topic/phase populated.

### implant_event_with_cycle_stamp_null_deserializes_to_none
- `"cycle_stamp":null` → `None` (null-tolerant).

### implant_event_serialize_none_omits_field (skip_serializing_if)
- `cycle_stamp: None` → serialized JSON does **not** contain `cycle_stamp` (the field never appears via `skip_serializing_if` → pre-existing fixtures unchanged).

### implant_event_serialize_some_includes_field
- `cycle_stamp: Some(...)` → JSON contains `cycle_stamp{topic, phase}`.

### cycle_stamp_payload_phase_none_omits_phase
- `CycleStampPayload{topic, phase:None}` serializes without `phase` (omit-when-null parity with the JS client's implantEvent shape).

## Mixed-version tolerance, both directions (R-16, FR-12)

### old_server_simulation_tolerates_unknown_cycle_stamp
- A stamped frame deserialized by a struct **without** the field defined (or with `serde(default)` ignore) succeeds — old-server tolerance; no `deny_unknown_fields` rejects it.

### unstamped_rust_hook_frame_yields_none_legacy_chain
- An unstamped Rust-hook-shaped frame → `cycle_stamp: None` → legacy chain (the steady-state production mix: `hook.rs` untouched, no flag).

## Fixture / binding integrity (R-16, AC-02)

### pre_existing_wire_fixtures_byte_unchanged
- The full pre-existing parity fixture suite passes byte-unmodified (the field never appears via `skip_serializing_if`). Assert no `deny_unknown_fields` was introduced on any struct in the deserialize path.

### ts_rs_export_sentinel_recounted_to_seven
- The `test_export_bindings_all_six_written_and_nonempty`-style sentinel (wire.rs:469) is renamed/extended to **seven**: add `CycleStampPayload::export_all` + the `ImplantEvent` re-export. Assert each committed binding is written and non-empty.

### bindings_git_diff_clean_except_additive
- `git diff --exit-code crates/unimatrix-engine/bindings/` is clean **except** the expected additive `ImplantEvent.ts` field diff + the new `CycleStampPayload.ts` file. (Run as the drift check; see #4831/#4821 — adding a field to a high-traffic wire enum/struct has exhaustive-match blast radius.)

## Coverage requirement
No `deny_unknown_fields` introduced; pre-existing fixtures and all other bindings byte-identical; serde tolerant both directions; 7th export wired into the sentinel test.

## Construction-site note (blast radius, #4831)
Adding `cycle_stamp` to `ImplantEvent` forces every struct-literal construction of `ImplantEvent` in the workspace to add the field (exhaustive). The serialize tests above only pass once all constructors compile with the new field defaulted — flag for the developer that the compile-cycle surface is wider than `wire.rs`.
