# Agent Report: crt-037-agent-4-config

**Feature**: crt-037 — Informs Edge Type
**Component**: InferenceConfig extension (`crates/unimatrix-server/src/infra/config.rs`)
**Agent ID**: crt-037-agent-4-config
**Status**: COMPLETE

---

## Work Completed

Extended `InferenceConfig` with three new fields for Informs edge detection:

### Fields Added

| Field | Type | Default | Range |
|-------|------|---------|-------|
| `informs_category_pairs` | `Vec<[String; 2]>` | 4 SE pairs | — (empty is valid) |
| `nli_informs_cosine_floor` | `f32` | `0.45` | `(0.0, 1.0)` exclusive |
| `nli_informs_ppr_weight` | `f32` | `0.6` | `[0.0, 1.0]` inclusive |

### Changes Made

1. **Struct fields** — three new fields added after `ppr_max_expand` in `InferenceConfig`, with section comment and doc comments per architecture spec.

2. **Default functions** — `default_informs_category_pairs()`, `default_nli_informs_cosine_floor()`, `default_nli_informs_ppr_weight()` added in a new section after existing inference tick default functions.

3. **`Default` impl** — three fields added to `InferenceConfig::default()` struct literal (both sites updated atomically per pattern #3817).

4. **`validate()` range checks** — two checks added before `Ok(())`:
   - `nli_informs_cosine_floor`: `<= 0.0 || >= 1.0` → `Err(NliFieldOutOfRange)`
   - `nli_informs_ppr_weight`: `< 0.0 || > 1.0` → `Err(NliFieldOutOfRange)`
   - No check on `informs_category_pairs` (empty list is valid — disables detection)

5. **Config merge/fusion path** — all three fields added to the project-wins merge struct literal using matching patterns (`!=` for Vec, `.abs() > f32::EPSILON` for f32 fields).

6. **Domain vocabulary constraint (C-12)** — the four category strings ("lesson-learned", "decision", "pattern", "convention") appear ONLY in `default_informs_category_pairs()`.

### Tests Added (14 total)

| Test | AC Coverage |
|------|-------------|
| `test_inference_config_default_informs_category_pairs` | AC-07 |
| `test_inference_config_default_nli_informs_cosine_floor` | AC-08 |
| `test_inference_config_default_nli_informs_ppr_weight` | AC-09 |
| `test_inference_config_default_passes_validate` | AC-12 |
| `test_inference_config_toml_override_informs_fields` | serde round-trip |
| `test_validate_nli_informs_cosine_floor_zero_is_error` | AC-10 lower |
| `test_validate_nli_informs_cosine_floor_one_is_error` | AC-10 upper |
| `test_validate_nli_informs_cosine_floor_valid_value_is_ok` | AC-10 nominal |
| `test_validate_nli_informs_cosine_floor_near_boundaries` | AC-10 boundary sweep |
| `test_validate_nli_informs_ppr_weight_zero_is_ok` | AC-11 lower inclusive |
| `test_validate_nli_informs_ppr_weight_one_is_ok` | AC-11 upper inclusive |
| `test_validate_nli_informs_ppr_weight_negative_is_error` | AC-11 below lower |
| `test_validate_nli_informs_ppr_weight_above_one_is_error` | AC-11 above upper |
| `test_validate_empty_informs_category_pairs_is_ok` | empty list valid |

---

## Test Results

- **Config tests**: 261 passed, 0 failed (all new 14 tests pass)
- **Workspace build**: clean (zero errors)
- **Pre-existing failure**: `col018_topic_signal_null_for_generic_prompt` — embedding model not initialized in test environment; unrelated to this component; pre-existing flakiness

---

## Files Modified

- `crates/unimatrix-server/src/infra/config.rs`

---

## Self-Check

- [x] `cargo build --workspace` passes (zero errors)
- [x] `cargo test -p unimatrix-server --lib -- infra::config` passes (261/261)
- [x] No `todo!()`, `unimplemented!()`, `TODO`, `FIXME`, or `HACK` in non-test code
- [x] All modified files within scope defined in the brief
- [x] Error handling uses `NliFieldOutOfRange` (existing project error type) — no `.unwrap()` in non-test code
- [x] New fields have `#[derive(Debug)]` (inherited from struct)
- [x] Code follows validated pseudocode — no silent deviations
- [x] Test cases match component test plan (AC-07 through AC-12 all covered)
- [x] No source file exceeded 500-line limit (config.rs is a pre-existing large file; no new file created)
- [x] Domain vocabulary strings appear ONLY in `default_informs_category_pairs()` (C-12)
- [x] Both serde default function AND `Default` impl struct literal updated atomically (pattern #3817)

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — surfaced entry #3817 (dual-site atomic change pattern for InferenceConfig serde+Default), entry #3937 (NLI neutral score tap pattern for cross-category edges). Both directly applicable and applied.
- Stored: nothing novel to store — the dual-site atomic change pattern (#3817) was already in Unimatrix and was exactly the gotcha encountered. The config extension followed a well-established pattern with no new deviations. No new gotchas discovered.
