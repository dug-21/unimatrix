# Test Plan — C3 Provider Normalization Arm (JS mirror)

`packages/unimatrix/lib/hook-client/normalize.js` — byte-parity port of the C2 Rust arm
(col-022 split-brain). C1 (plugin) + C3 (this) + C8 (parity corpus) change together.

Risks owned: **R-03** (split-brain — shared with C2/C8). ACs: AC-02b.

Test surface: JS unit (extend the existing normalize.js test file — do not fork). The authoritative
cross-language parity assertion lives in C8; this plan covers the JS arm's own canonicalization.

## Unit expectations

### Canonicalization mirror (FR-04, AC-02, R-03)
For each of the 7 canonical events, assert `normalize.js` produces the identical
`(canonical_name, provider)` result as the Rust arm (C2):
- `test_normalize_opencode_sessionstart`
- `test_normalize_opencode_userpromptsubmit`
- `test_normalize_opencode_pretooluse`
- `test_normalize_opencode_posttooluse`
- `test_normalize_opencode_subagentstart`
- `test_normalize_opencode_precompact`
- `test_normalize_opencode_stop`

Each asserts the FULL contract (name + provider + model_id passthrough), not just the name.

### Non-regression
- `test_normalize_existing_providers_unchanged` — claude-code/gemini-cli/codex-cli normalization
  unchanged; the pre-existing normalize.js tests re-run green.

## Split-brain guard (col-022, R-03, C-1)
The single-source-of-truth for parity is C8's corpus. This plan asserts:
- The JS arm's opencode output is derivable from the same oracle/corpus as the Rust arm — no
  independent hand-authored expectations that could drift (#5302).
- Editing C2 without C3 (or vice versa) is caught by C8's drift check — this component MUST land in
  the same change as C2 and C8.

## Cross-references
- C8 `parity_corpus_uds.rs` is the drift sentinel that fails if the Rust and JS arms diverge.
- C1 plugin builds the frame that this normalizer canonicalizes.
