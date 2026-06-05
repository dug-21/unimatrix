# Component 2 — Round-Trip Fixtures + Node Harness (Deliverable 1)

> ADR-002. The fixture — not the generated `.ts` — is the contract authority. Codegen captures
> structure; fixtures assert serde **behavior** (None-vs-omission, tagged discriminant, flatten),
> in both directions, in both runtimes.

## Purpose

Prove the Rust↔TS contract on the wire, not just at the type level. A Rust test emits JSON fixtures
for every request/response variant + serde edge cases; a standalone `node --test` harness imports
the committed bindings and deserializes the same fixtures, asserting behavior. Ships in F1 — a
deferred check would freeze the contract unverified (FR-06).

## Files

| File | Action |
|------|--------|
| `crates/unimatrix-engine/src/wire.rs` `#[cfg(test)]` (:379+) | Modify — add fixture-emit test + Rust round-trip assertions incl. dual-sided delta |
| `crates/unimatrix-engine/bindings/fixtures/*.json` | Create — Rust-emitted, one per variant + edge cases + delta |
| `crates/unimatrix-engine/bindings/contract.test.mjs` | Create — `node --test` harness (~dozen lines) |

## Fixture set (emitted by the Rust test → committed JSON)

One fixture per `HookRequest` variant and per `HookResponse` variant (FR-05 / AC-05), PLUS the serde
edge cases (FR-07 / AC-06) and the dual-sided delta (FR-11 / AC-11):

```
fixtures/
  request_ping.json
  request_session_register.json
  request_session_close.json
  request_record_event.json            // ImplantEvent with topic_signal + provider PRESENT (non-trivial)
  request_record_event_omitted.json    // topic_signal + provider ABSENT (None-vs-omission, parse-default side)
  request_record_events.json
  request_context_search.json          // source PRESENT
  request_context_search_no_source.json// source ABSENT (skip_serializing_if dual-direction)
  request_briefing.json
  request_compact_payload.json         // transcript_excerpt PRESENT
  request_compact_payload_no_excerpt.json // transcript_excerpt ABSENT
  request_hookinput_flatten.json       // extra top-level keys → land under `extra`
  response_pong.json
  response_ack.json
  response_error.json
  response_entries.json
  response_briefing_content.json
  transcript_delta_payload.json        // { "offset": <u64>, "bytes": "<non-trivial text>" } — AC-11
```

> The four `skip_serializing_if` fields each need BOTH a "present, non-trivial value" fixture and an
> "absent key" fixture so a partial wiring (emit-only or parse-only) cannot pass on an all-`None`
> path (R-02 / #3557). Named fields: `ImplantEvent.topic_signal`, `ImplantEvent.provider`,
> `ContextSearch.source`, `CompactPayload.transcript_excerpt`.

## Rust-side fixture emitter + round-trip (wire.rs `#[cfg(test)]`)

```
FUNCTION test_emit_fixtures():               // #[test]
    // Build one value per variant + edge case (non-trivial field values, not all-None).
    cases = [ HookRequest::Ping,
              HookRequest::RecordEvent { event: implant_event_with(topic_signal=Some, provider=Some) },
              ... all variants ...,
              HookResponse::Entries { items: [non_trivial_entry], total_tokens: N },
              ... ]
    FOR (name, value) IN cases:
        json = serde_json::to_string_pretty(value)
        write("../bindings/fixtures/" + name + ".json", json)
    // The dual-sided delta fixture is emitted from the TYPED struct, not hand-written:
    delta = TranscriptDeltaPayload { offset: <within-2^53>, bytes: "tok sk-NOTAKEY example span" }
    write("fixtures/transcript_delta_payload.json", serde_json::to_string_pretty(delta))

FUNCTION test_round_trip_each_variant():     // #[test] — Rust side authority half
    FOR fixture IN read_dir("../bindings/fixtures/"):
        raw = read(fixture)
        IF fixture is a request fixture:
            decoded: HookRequest = serde_json::from_str(raw)
            re = serde_json::to_string(decoded)
            ASSERT semantic_eq(raw, re)        // structural round-trip identity
        IF fixture is a response fixture: ... same with HookResponse ...

FUNCTION test_none_vs_omission_dual_direction():  // #[test] — R-02 / AC-06
    // EMIT side: None must serialize to an ABSENT key, not null.
    ev = ImplantEvent { topic_signal: None, provider: None, ... }
    out = serde_json::to_value(ev)
    ASSERT NOT out.contains_key("topic_signal")
    ASSERT NOT out.contains_key("provider")
    // PARSE side: an omitting fixture must deserialize to default (None).
    parsed: ImplantEvent = from_str(read("fixtures/request_record_event_omitted.json"))
    ASSERT parsed.topic_signal == None AND parsed.provider == None
    // Repeat for ContextSearch.source and CompactPayload.transcript_excerpt.

FUNCTION test_flatten_extra():               // #[test] — R-01 scenario 2
    parsed: HookInput = from_str(read("fixtures/request_hookinput_flatten.json"))
    ASSERT named fields populated AND parsed.extra contains the unknown keys
    // collision case: a key matching a named field → named field wins, extras isolated.

FUNCTION test_transcript_delta_payload_round_trip():  // #[test] — AC-11 Rust half
    parsed: TranscriptDeltaPayload = from_str(read("fixtures/transcript_delta_payload.json"))
    ASSERT parsed.offset == expected AND parsed.bytes == expected
    re = serde_json::to_string(parsed); ASSERT semantic_eq(re, fixture)
```

## Node harness (`bindings/contract.test.mjs`) — `node --test`

Standalone, no TS client package. Imports the committed `.ts` (compiled/loaded per the chosen
runner; delivery confirms whether a tsc step or direct `.ts` import via node's type-stripping is
used) and the Rust-emitted fixtures.

```
import { test } from "node:test";
import assert from "node:assert";
// import generated types from bindings/*.ts  (structure-only; TS types are erased at runtime,
//   so the harness asserts on the parsed JS object shape against the documented contract)

test("every request/response fixture parses to the correct tagged variant", () => {
    for (const f of listFixtures()) {
        const obj = JSON.parse(readFixture(f));
        if (isTagged(f)) assert.ok(typeof obj.type === "string");  // discriminant present (AC-04/SR-01)
        // assert the discriminant value matches the expected variant for this fixture
    }
});

test("None-vs-omission: omitted-key fixtures lack the key (not null)", () => {
    const ev = JSON.parse(readFixture("request_record_event_omitted.json")).event;
    assert.ok(!("topic_signal" in ev));   // absent, not null  (AC-06 parse side)
    assert.ok(!("provider" in ev));
    // present fixture: the key IS present with a non-trivial value
    const ev2 = JSON.parse(readFixture("request_record_event.json")).event;
    assert.equal(ev2.topic_signal, EXPECTED);
});

test("flatten: HookInput extra keys preserved", () => {
    const hi = JSON.parse(readFixture("request_hookinput_flatten.json"));
    assert.ok(hi.unknown_extra_key !== undefined);  // extras live at top level on the wire
});

test("transcript_delta payload round-trips dual-sided into TranscriptDeltaPayload", () => {
    // TS→Rust direction: harness CONSTRUCTS the payload object and writes it; a Rust test
    //   (test_transcript_delta_payload_round_trip) parses it into the typed struct. The two
    //   together prove both directions. The .mjs side asserts the {offset,bytes} shape the TS
    //   binding declares, so a client-side shape drift on offset/bytes is caught (R-01 scenario 4).
    const p = JSON.parse(readFixture("transcript_delta_payload.json"));
    assert.equal(typeof p.offset, "number" /* or bigint */);
    assert.equal(typeof p.bytes, "string");
    assert.deepEqual(Object.keys(p).sort(), ["bytes", "offset"]);  // exactly these fields
});
```

### Dual-sided requirement (AC-11, ADR-002)
A Rust-emit-only check does NOT satisfy AC-11. The fixture must round-trip:
- **Rust→TS**: Rust emits `transcript_delta_payload.json` from the typed struct; node parses it and
  asserts the `{offset, bytes}` shape the binding declares.
- **TS→Rust**: the node harness's constructed/echoed payload deserializes into `TranscriptDeltaPayload`
  on the Rust side without loss.

## Error handling

- A malformed fixture must make `node --test` exit non-zero (FR-06 verification) — the harness
  asserts structure, so a missing/renamed field fails the assertion.
- The Rust round-trip asserts `from_str` succeeds AND re-serialization is structurally equal; a
  serde drift (e.g. `null` emitted instead of omitted) fails `test_none_vs_omission_dual_direction`.

## Sequencing

Runs after Component 1 (needs the committed `.ts` and the same `cargo test` run that emits them).
The CI order (Component 1): `cargo test` → `git diff --exit-code` → `node --test`.

## Key test scenarios (hints — full plan in test-plan/contract-fixtures.md)

- **AC-05**: Rust test + `node --test` both deserialize every variant fixture; malformed fixture
  fails node. (R-01.)
- **AC-06**: all four `skip_serializing_if` fields dual-direction, both runtimes, non-trivial value.
  (R-02 — the single most-omitted category, #885/#3557.)
- **AC-04**: each tagged variant fixture parses to the correct union member keyed on `type`. (R-01.)
- **AC-11**: `transcript_delta_payload.json` round-trips dual-sided into `TranscriptDeltaPayload`.
- **Flatten**: extra keys preserved; collision → named field wins. (R-01 scenario 2.)

## Open questions / gaps

- **`.ts` consumption in node**: whether the harness imports `.ts` via node's experimental
  type-stripping, a `tsc` pre-step, or asserts purely on parsed-JSON shape (types erased at
  runtime). Delivery confirms; the behavioral assertions hold regardless. Non-blocking.
- **u64 precision**: the delta `offset` fixture value stays within 2^53 (or uses ts-rs's chosen
  bigint mapping) so JSON round-trip is lossless. Coordinated with Component 1's open question.
