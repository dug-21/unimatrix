# Agent Report — vnc-024-agent-4-contract-fixtures (Stage 3b, Wave 2, Component 2)

## Scope
Component 2 — round-trip fixtures + node harness (Deliverable 1, ADR-002). The fixture (not the
generated `.ts`) is the contract authority; fixtures assert serde BEHAVIOR in both directions and
both runtimes. Committed `a7b1772a`.

## Files modified / created
- `crates/unimatrix-engine/src/wire.rs` `#[cfg(test)]` (extended in place; Component 1 derives/struct/const/codegen-test untouched): fixture emitter + 5 round-trip assertion tests + 2 helpers.
- `crates/unimatrix-engine/bindings/fixtures/*.json` — 18 Rust-emitted fixtures (per HookRequest + HookResponse variant; present/absent pairs for the 4 skip fields; flatten + collision; typed delta).
- `crates/unimatrix-engine/bindings/contract.test.mjs` — `node --test` harness.

## Tests
- Rust `cargo test -p unimatrix-engine`: 435 passed / 0 failed (6 new: `test_emit_fixtures`, `test_round_trip_request_fixtures`, `test_round_trip_response_fixtures`, `test_none_vs_omission_dual_direction_all_four_fields`, `test_flatten_extra_and_collision`, `test_transcript_delta_payload_round_trip`).
- `node --test contract.test.mjs`: 4/4, exit 0. Malformed fixture → node exits 1 (restored).
- Binding diff empty (no new wire variant, Constraint 3).

## Confirmed
- `.ts` consumption: pure JSON-shape assertion (ts-rs bindings are erased `export type`s; harness `JSON.parse`s fixtures and asserts shape). No tsc / type-stripping needed.
- `u64`/offset: ts-rs maps `u64 → bigint` in the `.ts` (compile-time only); on the wire it is a JSON integer. Fixture offset = 2^32, within 2^53, asserted via `Number.isSafeInteger` for lossless round-trip.
- AC-11 dual-sided: Rust→TS (Rust emits from typed struct, node asserts `{offset,bytes}`) + TS→Rust (`test_transcript_delta_payload_round_trip` parses the same fixture into `TranscriptDeltaPayload`). Rust-emit-only would NOT satisfy AC-11.

## Issues found & fixed
Emitter and reader tests share one lib test binary and ran concurrently; a non-atomic `std::fs::write` let a reader observe a half-written fixture (1/435 flake). Fixed via atomic temp-file+rename (`write_fixture_atomic`); stable over 5 consecutive `--lib` runs.

## Knowledge Stewardship
- Queried: `context_search` — surfaced #4722 (ts-rs 12 dev-only codegen: u64→bigint, serde-json-impl, cfg(test) gating) and #4719/#4720/#4721 (ADR-002 dual-sided contract authority).
- Stored: entry **#4724** (pattern) — "Rust fixture-emitter + reader tests in one lib binary race: write atomically (temp+rename)"; captures the timing-only concurrent-write race and the bigint/safe-integer offset trap, neither visible in source.
