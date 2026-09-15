# C3 — Provider normalization arm (JS mirror)

**Location:** `packages/unimatrix/lib/hook-client/normalize.js`
**ADRs:** ADR-004 (JS mirror of C2). **Risks:** R-03 (split-brain / col-022). **Wave:** 3 (lands with C2 + C8).

## Purpose

Byte-parity port of the C2 opencode canonicalization arm. `hook.rs` and `normalize.js` are a
split-brain pair (col-022, lesson #5670): editing one side without the other is a known defect class.
This component adds the identical opencode arm to the JS normalizer and is guarded by the C8 parity corpus.

## Edits (mirror C2 exactly)

### 1. KNOWN_PROVIDERS mirror
Wherever `normalize.js` holds the provider allowlist (mirror of `hook.rs:158`), add `"opencode"`.

### 2. Canonicalization arm (mirror `uds/hook/opencode.rs::canonicalize`)
```
// vnc-049 C3 — mirror of hook/opencode.rs (col-022). Rust hook.rs is the oracle (#4751).
function canonicalizeOpencode(event) {
  switch (event) {
    case "SessionStart":
    case "UserPromptSubmit":
    case "PreToolUse":
    case "PostToolUse":
    case "SubagentStart":
    case "PreCompact":
    case "Stop":
      return event;                 // C1 emits canonical names; identity map
    default:
      return "__unknown__";         // caller preserves raw string (matches Rust)
  }
}
```

### 3. model_id passthrough
The JS normalizer must carry `model_id` through the frame it builds (mirror of the HookInput.model_id
field, C4), and mirror `is_valid_model_id`:
```
function isValidModelId(s) {
  return typeof s === "string" && s.length >= 1 && s.length <= 128 && /^[a-z0-9._/-]+$/.test(s);
}
// frame.model_id = isValidModelId(model) ? model : undefined   // undefined ⇒ omitted (matches skip_serializing_if)
```

### 4. Wiring into the normalize entry point
The existing normalize function that maps event-name + provider must route `provider === "opencode"`
through `canonicalizeOpencode` and attach `provider: "opencode"` + validated `model_id`, exactly
parallel to the Rust hint path.

## Contract-single-sourcing (#5302, R-03)

Do NOT copy only the *data*; mirror the full *contract* (name map + charset + provider allowlist + the
"emit canonical, __unknown__ for unrecognized" behavior). The C8 parity corpus asserts identical output
for all 7 canonical events and fails if either side drifts.

## Data flow

IN: OpenCode-shaped frame from the plugin (C1) via the JS edge client path. OUT: canonical frame with
`provider:"opencode"`, validated `model_id` — must canonicalize IDENTICALLY to Rust (C2).

## Error handling

- Unknown event → `"__unknown__"`, raw string preserved (matches Rust).
- Invalid `model_id` → omitted (undefined), never forwarded raw.
- Never throw into the OpenCode session path (fail-open, NFR-02) — the plugin (C1) owns fail-open, but
  normalize must not throw on malformed input.

## Key test scenarios (hints for tester)

1. **Parity (primary, R-03.1/AC-02b):** for each of the 7 events, `canonicalizeOpencode(event)` ===
   Rust `canonicalize(event)` — enforced via C8 corpus.
2. **Drift sentinel (R-03.2):** editing the Rust arm without this one (or vice-versa) fails the corpus.
3. `isValidModelId` matches Rust `is_valid_model_id` on the same inputs (accept `ollama/qwen3-coder`,
   reject illegal/oversized).
4. `provider:"opencode"` attached; `model_id` omitted when invalid/absent (serde parity).
</content>
