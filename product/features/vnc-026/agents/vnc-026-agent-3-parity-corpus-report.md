# Agent Report — vnc-026-agent-3-parity-corpus (Wave 1: Rust generator + goldens)

## Files Created/Modified

- `crates/unimatrix-server/src/uds/hook.rs` — additive `#[cfg(test)] #[path]` wiring only (C-07 held; no production change)
- `crates/unimatrix-server/src/uds/parity_corpus_gen.rs` — engine: oracle pipeline, volatile normalization, stdout reconstruction (pinned hook.rs:963-1028), staged generation, MANIFEST writer, `#[ignore]`'d `generate_parity_corpus`
- `crates/unimatrix-server/src/uds/parity_corpus_gen_tests.rs` — 7 non-ignored guard tests incl. `test_generator_branch_coverage` (R-02 scenario 2)
- `crates/unimatrix-server/src/uds/parity_corpus_cases.rs` — events/aliases/stdin-shape cases
- `crates/unimatrix-server/src/uds/parity_corpus_cases_tools.rs` — UserPromptSubmit/PostToolUse/PostToolUseFailure cases
- `crates/unimatrix-server/src/uds/parity_corpus_cases_b.rs` — context_cycle + SubagentStart tail cases
- `crates/unimatrix-server/src/uds/parity_corpus_cases_stdout.rs` — stdout-golden cases
- `crates/unimatrix-server/src/uds/parity_corpus_transcripts.rs` — transcript JSONL builders
- `packages/unimatrix/test/fixtures/parity/` — committed corpus: 83 case dirs + MANIFEST.json (104 arm keys, zero empty)
- `scripts/regen-parity.sh` — regen entry point (referenced by the generator's panic message)
- `.gitattributes` — `parity/** -text` (golden bytes CRLF-protected)

Committed on `feature/vnc-026`: `impl(parity-corpus): Rust oracle golden generator + committed corpus (#679)`.

## Tests

- 7/7 guard tests pass (default pass); generator runs green under `--ignored`
- Full `cargo test -p unimatrix-server --lib`: 3608 passed / 0 failed; other workspace crates clean
- `cargo fmt` clean, `cargo clippy --tests` zero warnings in parity files; all files ≤ 500 lines
- Corpus: 83 cases; drift-checked — generator run twice into temp dirs AND regenerated in place: byte-identical (diff = zero)

## Conventions for Layer 1/2 implementers (later wave)

1. Env var is `UNIMATRIX_PARITY_DIR` (pseudocode letter; test-plan's `UNIMATRIX_PARITY_OUT` is stale).
2. Relative `transcript_path` in stdin.json resolves against the case directory (documented in MANIFEST.json `conventions`).
3. Volatile normalization has a THIRD rule beyond ADR-001's two: top-level `cwd` equal to the running process's cwd → `"<process-cwd>"`. Required because `build_request`'s missing-cwd fallback embeds process state; rule is self-contained so the JS runner applies it with its own `process.cwd()`.
4. Generator prunes stale case *directories* but never touches files (coexists with `project-hash-goldens.json`).

## Known JS-divergence risks pinned by goldens (for the build-request/transform agents)

- `stdin-lone-surrogate-escape`: serde_json rejects lone surrogates (whole parse → defensive default); JS `JSON.parse` accepts them. Golden pins Rust truth.
- `stdout-subagent-envelope-adversarial`: serde_json emits U+2028/U+2029 raw; `JSON.stringify` escapes them (` `). Byte-golden pins serde truth — transform.js must account for this or document the deviation.

## Issues / Blockers

None. One scope note: `mcp/tools.rs` has pre-existing non-fmt-clean code (not touched).

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing — surfaced ADR-001 (#4751, corpus oracle design, applied as spec), #4724 (emit/read fixture test coupling), #2611 (Serialize on fixture types); context_search for vnc-026 ADRs (#4757/4758/4759, delta-side, not directly applicable to Wave 1)
- Stored: entry #4770 "Golden-fixture generators must normalize process-state fallbacks with self-contained rules both runners can apply" via /uni-store-pattern
