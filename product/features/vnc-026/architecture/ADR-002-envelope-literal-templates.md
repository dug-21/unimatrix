## ADR-002: Host-Envelope Stdout via Literal Templates, Never Object Serialization

### Context

SR-02 (High/Medium): AC-04 demands the SubagentStart `hookSpecificOutput` envelope be
byte-identical to `write_stdout_subagent_inject` (`hook.rs:994-1006`) — field order,
compact separators, trailing newline. `serde_json` and `JSON.stringify` differ subtly:
key ordering is insertion-order in JS but feature-dependent in serde_json (this workspace
enables `preserve_order` — verified `unimatrix-server/Cargo.toml:38` — so Rust output is
insertion-ordered: `hookSpecificOutput` → `hookEventName` → `additionalContext`), and
escaping/number formatting can diverge on adversarial content. Building the envelope as a
JS object and stringifying it makes byte parity depend on incidental serializer behavior
in two languages.

### Decision

`transform.js` emits stdout envelopes from **literal template strings**; the only
serializer call is `JSON.stringify` on the inner text scalar (for string escaping):

```js
// SubagentStart (AC-04):
process.stdout.write(
  '{"hookSpecificOutput":{"hookEventName":"SubagentStart","additionalContext":'
  + JSON.stringify(text) + '}}\n'
);

// UserPromptSubmit / PreCompact plain path: body verbatim + newline, only if non-empty:
if (body.length > 0) process.stdout.write(body + '\n');
```

Empty text/204/empty body → no stdout at all (matches `write_stdout`'s silent-skip).
There is no code path that serializes a whole envelope object. The committed parity
goldens (ADR-001), generated from the Rust hook's actual stdout bytes, are the sole byte
authority — if serde_json behavior ever changes, the goldens change and the template is
updated to match, never the reverse.

String-escaping residual risk (control chars, non-BMP) is covered by ADR-001's
adversarial corpus cases: `JSON.stringify` and serde_json agree on the JSON escaping of
all content the corpus exercises; any disagreement surfaces as a byte diff in Layer 1.

Equally: client-built **request** JSON (`build-request.js`) constructs objects in wire
declaration order and serializes with plain `JSON.stringify` — byte order there is *not*
contractual (the server parses JSON; AC-01 is JSON-equality), so no templating is needed
on the request side. Fixtures (`bindings/fixtures/`) remain the request-shape authority
(AC-14).

### Consequences

- Easier: AC-04 byte parity reduces to one template + one escaping function with golden
  coverage; the envelope can never drift via object-key reordering or serializer upgrades.
- Harder: adding a future host envelope (Codex/Gemini) means writing another literal
  template rather than reusing a generic serializer — acceptable, envelopes are ~3 lines
  each and host-specific by nature (ass-068 Q4).
- The template hard-codes the `preserve_order` key order; if the workspace ever drops
  that feature, the Rust side changes and the goldens flag it (ADR-001 drift check) —
  the failure mode is loud.
