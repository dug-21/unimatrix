# Test Plan — wire-accept-text (`wire.rs` additive)

Component 4 / ADR-001 / FR-18, NFR-7 / **AC-11 (frozen contract)** / Risks R-07 (Med), R-08 (Med).
Additive only: `accept: Option<String>` with `#[serde(default, skip_serializing_if = "Option::is_none")]` on
`ContextSearch`/`CompactPayload`; new `HookResponse::Text { body: String }` (serde tag `"type"`). Rust `cargo test`.

## Unit expectations — additivity (`cargo test -p unimatrix-engine`)

- `test_context_search_without_accept_serializes_byte_unchanged` — a `ContextSearch` with `accept: None` serializes to the exact bytes the pre-feature struct produced (no `accept` key present). Same for `CompactPayload`. This is the core `skip_serializing_if` proof (R-07).
- `test_context_search_with_accept_text_plain_roundtrips` — `accept: Some("text/plain")` serializes WITH the key and deserializes back equal.
- `test_accept_default_on_missing_field` — a JSON body with no `accept` key deserializes to `accept: None` (serde default), proving old-client frames still parse.
- `test_no_deny_unknown_fields` — a frame with an unrecognized extra key still deserializes (no `deny_unknown_fields` regression).
- `test_response_text_variant_roundtrips` — `HookResponse::Text { body }` serializes with `"type":"Text"` and roundtrips; body bytes preserved verbatim (incl. a `--- Unimatrix Context ---\n` prefix and multibyte content).
- `test_ping_briefing_have_no_accept_field` — `Ping`/`Briefing` request variants gain no `accept` field (ADR-001 §1; Pong stays JSON).
- `test_existing_response_variants_unchanged` — `Ack`/`Error`/`Pong`/`Entries`/`BriefingContent` serialize byte-unchanged (new variant addition does not reorder/retag existing variants).

## Frame round-trip (framing authority `wire.rs:16,349,372`)

- Existing `write_frame`/`read_frame` round-trip tests remain green unmodified (4-byte BE u32 + JSON, `MAX_PAYLOAD_SIZE=1_048_576`, zero-length + oversized reject) — the new variant rides through framing unchanged.

## AC-11 frozen-contract suite (run UNMODIFIED — the strongest additivity proof)

- `cargo test -p unimatrix-server --lib parity` passes byte-unchanged.
- `scripts/regen-parity.sh` produces a ZERO diff after the wire additions (incl. the mechanical `accept: None` edits at `hook.rs` construction sites — approved variance).
- ts-rs binding drift check: regenerated bindings are additive (new optional field + new variant), existing type shapes byte-unchanged.

## R-08 — accept↔Text coupling (the only protection for frozen Rust hooks)
The `Text` variant is unsafe to send to a caller that did not send `accept` (frozen Rust hooks cannot deserialize it).
This component owns the variant existence; the coupling enforcement is asserted at the listener seam
(see listener-preformatted.md) and end-to-end against the real frozen binary (see parity-corpus-uds.md, R-08 s4).

## Edge cases
- `accept` with an unrecognized value (not `"text/plain"`) → treated as absent at the listener (asserted in listener-preformatted.md); at the wire layer it simply deserializes as `Some(other)` — no panic.
- Empty `body` in `Text` → valid (though the empty-injection path returns `Ack`, not `Text` — ADR-001 §4).
