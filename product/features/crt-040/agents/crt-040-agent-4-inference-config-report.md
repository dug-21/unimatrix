# Agent Report: crt-040-agent-4-inference-config

**Agent ID:** crt-040-agent-4-inference-config
**Task:** Implement InferenceConfig changes for crt-040

---

## Files Modified

- `crates/unimatrix-server/src/infra/config.rs`

---

## Changes Made

### Added: `supports_cosine_threshold: f32` (dual-site, ADR-002)

1. **Struct field** — added after `nli_informs_ppr_weight` with doc comment and `#[serde(default = "default_supports_cosine_threshold")]`
2. **Serde backing function** — `fn default_supports_cosine_threshold() -> f32 { 0.65 }` added after `default_nli_informs_ppr_weight`
3. **impl Default** — `supports_cosine_threshold: default_supports_cosine_threshold()` added (calls backing fn, not literal 0.65)
4. **validate()** — range check `(0.0, 1.0)` exclusive added after `nli_informs_ppr_weight` block; validate() doc comment updated
5. **Config merge** — f32 epsilon pattern entry added after `nli_informs_ppr_weight` merge block

### Removed: `nli_post_store_k` (all 6 sites)

1. Struct field declaration + doc comment removed
2. `default_nli_post_store_k()` backing function removed
3. `nli_post_store_k: 10` from impl Default removed
4. validate() range check block removed
5. Config merge block removed
6. Test assertions removed from 3 existing tests; 2 dedicated test functions deleted; 1 AC-19 independence test deleted

Additional: `graph_inference_k` doc comment updated to remove stale `nli_post_store_k` cross-reference. `nli_top_k` doc comment updated to remove stale reference.

### Tests Added (TC-01 through TC-11)

| Test | Covers |
|------|--------|
| `test_default_supports_cosine_threshold_fn` | TC-01: backing fn returns 0.65 (AC-16, R-03) |
| `test_inference_config_default_supports_cosine_threshold` | TC-02: impl Default returns 0.65 (AC-10, AC-16, R-03) |
| `test_inference_config_toml_empty_supports_cosine_threshold` | TC-03: serde empty TOML returns 0.65 (AC-16, R-03) |
| `test_inference_config_toml_override_supports_cosine_threshold` | TC-04: TOML override propagates (FR-08) |
| `test_validate_supports_cosine_threshold_zero_fails` | TC-05: rejects 0.0 (AC-09) |
| `test_validate_supports_cosine_threshold_one_fails` | TC-06: rejects 1.0 (AC-09) |
| `test_validate_supports_cosine_threshold_default_is_ok` | TC-07: accepts 0.65 (AC-09) |
| `test_validate_supports_cosine_threshold_near_bounds_ok` | TC-08: accepts 0.001 and 0.999 (AC-09) |
| `test_config_merge_supports_cosine_threshold_project_overrides` | TC-09: project 0.70 wins over global 0.65 (R-13) |
| `test_config_merge_supports_cosine_threshold_global_when_not_overridden` | TC-10: global 0.75 wins when project == default (R-13) |
| `test_inference_config_toml_with_nli_post_store_k_succeeds` | TC-11: serde forward-compat, removed field silently ignored (AC-18, R-04) |

TC-12 (grep gate) verified: `grep "nli_post_store_k" config.rs` returns only TC-11 test body lines — zero non-test references.

---

## Test Results

- **config unit tests: 269 passed, 0 failed**
- Full workspace: 2579 passed, 1 failed (`col018_long_prompt_truncated` — pre-existing intermittent timing failure unrelated to this change; passes on baseline branch)

---

## Self-Check

- [x] `cargo build --workspace` passes (zero errors)
- [x] `cargo test --package unimatrix-server --lib infra::config` passes (269/269)
- [x] No `todo!()`, `unimplemented!()`, `TODO`, `FIXME`, or `HACK` in non-test code
- [x] Only `crates/unimatrix-server/src/infra/config.rs` modified
- [x] No `.unwrap()` added in non-test code
- [x] New struct field has doc comment
- [x] Code follows validated pseudocode — no deviations
- [x] Test cases match test plan TC-01 through TC-11
- [x] `grep "nli_post_store_k" config.rs` returns zero non-test lines (AC-17)
- [x] impl Default calls backing fn, not literal 0.65 (ADR-002)
- [x] `cargo fmt` applied before commit

---

## Issues / Blockers

None.

---

## Knowledge Stewardship

- **Queried:** `mcp__unimatrix__context_briefing` — surfaced ADR-002 (#4028, dual-site requirement), pattern #3817 (atomic dual-site change), pattern #4013 (hidden test sites). All applied.
- **Stored:** entry #4036 "InferenceConfig field removal: grep all 6 sites before touching anything — neighboring doc comments are a hidden 7th" via `/uni-store-pattern`. Captures that spec removal checklists enumerate the structural 6 sites but miss cross-references in neighboring field doc comments — a silent stale-doc trap.
