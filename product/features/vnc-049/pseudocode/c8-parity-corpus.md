# C8 — Parity corpus (OpenCode cases)

**Location:** `crates/unimatrix-server/src/uds/parity_corpus_uds.rs`
**ADRs:** ADR-004 (corpus is the AC-02 gate + split-brain guard). **Risks:** R-03 (split-brain drift).
**Wave:** 4 (needs C2 + C3 both present). **col-022:** lands with C2 + C3.

## Purpose

Extend the cross-language regression fixture so it asserts the Rust normalizer (`hook.rs` +
`hook/opencode.rs`, C2) and the JS normalizer (`normalize.js`, C3) canonicalize OpenCode identically.
The Rust hook is the oracle (vnc-026 #4751). This is the AC-02 gate and the permanent drift sentinel
for the col-022 split-brain (NFR-04).

## Edits — add opencode cases across the 7 canonical events

Following the existing corpus structure (oracle-generated goldens + drift check), add cases:
```
opencode_cases = [
  { provider:"opencode", event_in:"SessionStart",     expect_canonical:"SessionStart",     model_id: None },
  { provider:"opencode", event_in:"UserPromptSubmit",  expect_canonical:"UserPromptSubmit",  model_id: Some("ollama/qwen3-coder") },
  { provider:"opencode", event_in:"PreToolUse",        expect_canonical:"PreToolUse",        model_id: Some("ollama/qwen3-coder") },
  { provider:"opencode", event_in:"PostToolUse",       expect_canonical:"PostToolUse",       model_id: Some("ollama/qwen3-coder") },
  { provider:"opencode", event_in:"SubagentStart",     expect_canonical:"SubagentStart",     extra:{ agent_type:"<validated>" } },
  { provider:"opencode", event_in:"PreCompact",        expect_canonical:"PreCompact" },       # experimental leg
  { provider:"opencode", event_in:"Stop",              expect_canonical:"Stop",              degraded:true },
]
```
Each case asserts:
1. **Rust ↔ JS identical** canonicalization of `event_in` → `expect_canonical` (the split-brain guard).
2. **provider** stamped `"opencode"` on both sides.
3. **model_id** carried identically (present cases + absent/None case) — covers OQ-5 (carrier gets
   corpus coverage) and lesson #5670 (frame-field parity).
4. **subagent** case carries validated agent on `extra.agent_type`, NOT tool args (R-07 parity).

## Drift-failing property (R-03.2)

The corpus MUST fail if either arm is edited without the other (single-source the CONTRACT, not just
DATA — #5302). Assert on the full mapping contract (name + provider + model_id + charset behavior), so a
one-sided edit to `hook/opencode.rs` or `normalize.js` breaks the test loudly.

## Data flow

Compares C2 output (oracle) against C3 output for the same opencode-shaped inputs. Pure fixture test;
no DB, no UDS runtime.

## Error handling

Test-only; a mismatch is a hard test failure (the intended gate). No runtime error paths.

## Key test scenarios (hints for tester)

1. **All 7 opencode events** covered; corpus green (R-03.1/AC-02b).
2. **Drift sentinel** (R-03.2): mutate one arm → corpus fails.
3. **model_id parity** (OQ-5, #5670): present + None cases match across languages.
4. **provider parity** (AC-02c): `"opencode"` on both sides.
5. **subagent observe-channel parity** (R-07): agent on `extra.agent_type` both sides.
</content>
