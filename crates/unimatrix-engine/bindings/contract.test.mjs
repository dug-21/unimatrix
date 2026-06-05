// vnc-024 Component 2 — Node round-trip harness (ADR-002). `node --test`.
//
// The committed Rust-emitted fixtures (fixtures/*.json) are the contract authority. The ts-rs
// `.ts` bindings are erased at runtime (pure `export type`), so this harness asserts on the
// parsed-JSON object shape against the documented contract — the consuming-language (TS) side of
// every behavior the Rust suite asserts on the source side. Both runtimes read the SAME fixtures.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const DIR = join(dirname(fileURLToPath(import.meta.url)), "fixtures");
const load = (name) => JSON.parse(readFileSync(join(DIR, name), "utf8"));

// Expected literal `type` discriminant per tagged fixture (HookRequest + HookResponse).
const TAGGED = {
  "request_ping.json": "Ping",
  "request_session_register.json": "SessionRegister",
  "request_session_close.json": "SessionClose",
  "request_record_event.json": "RecordEvent",
  "request_record_event_omitted.json": "RecordEvent",
  "request_record_events.json": "RecordEvents",
  "request_context_search.json": "ContextSearch",
  "request_context_search_no_source.json": "ContextSearch",
  "request_briefing.json": "Briefing",
  "request_compact_payload.json": "CompactPayload",
  "request_compact_payload_no_excerpt.json": "CompactPayload",
  "response_pong.json": "Pong",
  "response_ack.json": "Ack",
  "response_error.json": "Error",
  "response_entries.json": "Entries",
  "response_briefing_content.json": "BriefingContent",
};

test("every request/response fixture narrows to the correct tagged variant (literal type)", () => {
  for (const [file, variant] of Object.entries(TAGGED)) {
    const obj = load(file);
    assert.equal(typeof obj.type, "string", `${file} must carry a literal type`);
    assert.equal(obj.type, variant, `${file} must narrow to ${variant}`);
  }
  // member-specific field present on a representative variant (AC-04: narrows, not just parses).
  assert.equal(load("response_entries.json").items[0].id, 42);
});

test("None-vs-omission: omitted-key fixtures LACK the key (not null); present carry the value", () => {
  // ImplantEvent.topic_signal + .provider — absent on the omitted fixture.
  const omitted = load("request_record_event_omitted.json");
  assert.ok(!("topic_signal" in omitted), "topic_signal absent, not null");
  assert.ok(!("provider" in omitted), "provider absent, not null");
  const present = load("request_record_event.json");
  assert.equal(present.topic_signal, "vnc-024");
  assert.equal(present.provider, "claude-code");
  // ContextSearch.source
  assert.ok(!("source" in load("request_context_search_no_source.json")));
  assert.equal(load("request_context_search.json").source, "SubagentStart");
  // CompactPayload.transcript_excerpt
  assert.ok(!("transcript_excerpt" in load("request_compact_payload_no_excerpt.json")));
  assert.equal(load("request_compact_payload.json").transcript_excerpt, "prior excerpt text");
});

test("flatten: HookInput unknown keys preserved at top level; named field wins on collision", () => {
  const hi = load("request_hookinput_flatten.json");
  assert.equal(hi.hook_event_name, "PreToolUse");
  assert.equal(hi.session_id, "sess-fixture"); // named field, not duplicated
  assert.equal(hi.unknown_extra_key, "extra-value"); // extras ride at top level on the wire
  assert.equal(hi.another_extra.nested, 7);
});

test("transcript_delta payload round-trips (TS→Rust): exactly {offset,bytes}, lossless offset", () => {
  const p = load("transcript_delta_payload.json");
  assert.deepEqual(Object.keys(p).sort(), ["bytes", "offset"]); // exactly the binding's fields
  assert.equal(typeof p.offset, "number"); // wire form; ts-rs bigint is a compile-time type only
  assert.ok(Number.isSafeInteger(p.offset), "offset stays within 2^53 for lossless round-trip");
  assert.equal(p.offset, 4294967296);
  assert.equal(typeof p.bytes, "string");
  assert.ok(p.bytes.length > 0);
});
