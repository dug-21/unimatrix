# Test Plan — C2 Provider Normalization Arm (Rust)

`crates/unimatrix-server/src/uds/hook.rs` (`KNOWN_PROVIDERS` @158; `normalize_event_name` /
`map_to_canonical` @66-105) + new `crates/unimatrix-server/src/uds/hook/opencode.rs` (thin-wiring
target keeping the hook.rs delta minimal — over-cap file, minimal wiring only per ADR-005).

Risks owned: **R-03** (split-brain — shared with C3/C8), **R-17** (over-cap wiring). ACs: AC-02a.

Test surface: Rust `#[cfg(test)]` in `hook/opencode.rs` (new module owns its own tests) + the
existing hook.rs normalizer test module (extend, do not fork). Parity is proven in C8; this plan
covers the Rust arm's canonicalization correctness.

## Unit expectations

### KNOWN_PROVIDERS (FR-04, AC-02a, R-03.3)
- `test_known_providers_contains_opencode` — `KNOWN_PROVIDERS.contains(&"opencode")`. This is a
  necessary-but-not-sufficient check (per #5427, membership/count assertions are blind to argument
  threading — the behavioral proof is the stored `provider="opencode"` in c5).

### Event-name canonicalization (FR-01, AC-02)
For each of the 7 canonical events, assert `hook/opencode.rs` canonicalization maps the
OpenCode-origin event to the correct canonical name and stamps `provider=opencode`:
- `test_opencode_canonicalize_sessionstart`
- `test_opencode_canonicalize_userpromptsubmit`
- `test_opencode_canonicalize_pretooluse`
- `test_opencode_canonicalize_posttooluse`
- `test_opencode_canonicalize_subagentstart`
- `test_opencode_canonicalize_precompact`
- `test_opencode_canonicalize_stop`

Each asserts `(canonical_name, inferred_provider)` — the full return contract, not just the name
(#5302: single-source the CONTRACT). A model_id-bearing case asserts model_id survives
canonicalization into the `ImplantEvent` build.

### Delegation / thin-wiring (ADR-005, R-17)
- `test_hook_rs_delegates_opencode_to_module` — the hook.rs arm delegates to `hook/opencode.rs`
  rather than inlining logic (keeps hook.rs delta minimal, over-cap file).
- `test_non_opencode_providers_unchanged` — claude-code/gemini-cli/codex-cli canonicalization
  paths are unaffected by the added arm (adjacent-code non-regression, R-17). Run the pre-existing
  hook.rs normalizer tests green, unchanged.

## Edge cases
- Unknown/invalid provider string → existing rejection path unchanged.
- An opencode event whose event name is unrecognized → handled per existing unknown-event behavior,
  not silently coerced.

## Cross-references
- Parity with the JS arm (C3) is asserted in C8 (`parity_corpus_uds.rs`) — the drift sentinel.
- The stored `provider="opencode"` behavioral proof is in c5 (assembled path), NOT here.
