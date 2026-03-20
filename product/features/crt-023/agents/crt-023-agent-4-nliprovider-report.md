# Agent Report: crt-023-agent-4-nliprovider

**Component**: NliProvider (`unimatrix-embed`)
**Feature**: crt-023 — NLI + Cross-Encoder Re-ranking
**Date**: 2026-03-20

## Summary

Implemented the NLI cross-encoder provider infrastructure in `unimatrix-embed`. All files follow
the architecture spec and validated pseudocode exactly.

## Files Created / Modified

| File | Action |
|------|--------|
| `crates/unimatrix-embed/src/cross_encoder.rs` | Created — `CrossEncoderProvider` trait, `NliScores` struct, `NliProvider` impl |
| `crates/unimatrix-embed/src/cross_encoder_tests.rs` | Created — test module split (500-line limit) |
| `crates/unimatrix-embed/src/model.rs` | Modified — added `NliModel` enum |
| `crates/unimatrix-embed/src/download.rs` | Modified — added `ensure_nli_model` |
| `crates/unimatrix-embed/src/error.rs` | Modified — added `InferenceFailed(String)` variant |
| `crates/unimatrix-embed/src/lib.rs` | Modified — exported new module and public types |

## Tests

**98 passed, 0 failed, 26 ignored** (ignored tests require model on disk — `--include-ignored`).

Unit tests that do NOT require a model (all pass):
- `test_softmax_sum_invariant_typical` — sum ≈ 1.0 for typical logits
- `test_softmax_sum_invariant_extreme_logits` — no NaN/inf for [100, -50, -50]
- `test_softmax_all_equal_logits` — uniform distribution for equal logits
- `test_softmax_no_nan_no_inf` — four extreme logit cases
- `test_truncate_input_*` (6 tests) — truncation at 2000 chars, UTF-8 boundary safety, CJK
- `test_nli_provider_send_sync` — compile-time Send+Sync check
- `test_cross_encoder_provider_object_safe` — dyn CrossEncoderProvider usable
- `test_from_config_name_*` (3 tests) — minilm2/deberta/unknown
- `test_nli_model_*` (7 tests) — model_id, onnx_filename, cache_subdir, derives

## Implementation Notes

### Pseudocode Deviations

None. Followed pseudocode exactly. One implementation decision made explicit:

**Softmax label order**: The pseudocode noted "verify from model config at implementation time".
Verified: `cross-encoder/nli-MiniLM2-L6-H768` config.json "id2label" is
`{"0":"contradiction","1":"entailment","2":"neutral"}`. Constants `LOGIT_IDX_CONTRADICTION=0`,
`LOGIT_IDX_ENTAILMENT=1`, `LOGIT_IDX_NEUTRAL=2` are defined with a doc comment citing the source
URL.

### Bug Found During Implementation

The pseudocode's softmax pattern included a subtle double-exp bug: the final `NliScores` assignment
was written as `logits[IDX].exp() / sum` which re-computes `exp()` on the original (non-shifted)
logit, producing `sum >> 1.0`. Fixed by storing all exp values in an array first and indexing into
it. This is now documented as a pattern in Unimatrix (#2729).

### File Split

`cross_encoder.rs` reached 583 lines before split. Moved `#[cfg(test)] mod tests` to
`cross_encoder_tests.rs` using `#[path = "..."]`. Required making `truncate_input`,
`softmax_3class`, and `PER_SIDE_CHAR_LIMIT` `pub(crate)` so the external test module can access
them via `use super::*`.

### Other Agent Work

The workspace had `InferenceConfig` with all 10 NLI fields already added by another agent
(config agent), and `InferenceConfig` test initializers in `config.rs` that were already updated
with `..InferenceConfig::default()` struct update syntax. No changes needed to server crate.

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-embed` — found entry #67 (ADR-001: Mutex Session
  for ONNX Inference Concurrency) which confirmed the `Mutex<Session>` pattern.
- Stored: entry #2729 "NliProvider softmax gotchas: double-exp bug, pub(crate) for path-test
  modules, label order" via `/uni-store-pattern`

## Self-Check

- [x] `cargo build --workspace` passes (zero errors)
- [x] `cargo test -p unimatrix-embed` passes (98 passed, 0 failed)
- [x] No `todo!()`, `unimplemented!()`, `TODO`, `FIXME`, or `HACK` in non-test code
- [x] All modified files within scope defined in brief
- [x] Error handling uses `EmbedError` with context, no `.unwrap()` in non-test code
- [x] `NliProvider`, `NliScores`, `NliModel` all have `#[derive(Debug)]`
- [x] Code follows validated pseudocode — only the softmax bug corrected, label order verified
- [x] Test cases match component test plan expectations
- [x] No source file exceeds 500 lines (`cross_encoder.rs`: 321, `cross_encoder_tests.rs`: 261)
- [x] Knowledge Stewardship report block included
