# Agent Report: crt-039-agent-3-config

**Component:** `crates/unimatrix-server/src/infra/config.rs`
**Feature:** crt-039 — Tick Decomposition: Decouple Structural Graph Inference from NLI Gate
**Task:** Raise `nli_informs_cosine_floor` default from 0.45 to 0.50 (ADR-003)

---

## Files Modified

- `crates/unimatrix-server/src/infra/config.rs`
- `crates/unimatrix-server/src/services/nli_detection_tick.rs`

---

## Changes Applied

### config.rs

1. `default_nli_informs_cosine_floor()` return value: `0.45` → `0.5` (with ADR-003 comment)
2. Doc comment on `nli_informs_cosine_floor` field: updated both the inclusive-floor example value (0.45 → 0.50) and the Default line
3. `test_inference_config_default_nli_informs_cosine_floor`: Updated to assert `0.5_f32` for both the backing fn (TC-06a) and `InferenceConfig::default()` field (TC-06b)
4. `test_validate_nli_informs_cosine_floor_valid_value_is_ok`: Updated nominal valid value from `0.45` to `0.5`

No change to `InferenceConfig::default()` calling pattern — it already delegates to the backing fn. No change to validation logic (C-09: 0.5 is within `(0.0, 1.0)` exclusive).

### nli_detection_tick.rs (TC-U — hidden site)

5. `test_phase4b_uses_nli_informs_cosine_floor_not_supports_threshold`: Updated from `cosine_in_band = 0.47` (old band `[0.45, 0.50)`) to `cosine_at_floor = 0.50` (new band `[0.50, supports_threshold)`). Added `assert_eq!(config.nli_informs_cosine_floor, 0.5_f32)` sanity check. Updated assertion messages to reflect AC-18 (updated).

---

## Test Results

```
test result: ok. 2570 passed; 0 failed; 0 ignored
```

All workspace tests pass. The `test_phase4b_uses_nli_informs_cosine_floor_not_supports_threshold` test was the only failure before the nli_detection_tick.rs fix — it compiled cleanly with the old band cosine literal but failed at runtime after the floor raise.

---

## Issues / Blockers

None. All changes within scope. No compilation warnings from the two modified files.

---

## Knowledge Stewardship

- **Queried:** `mcp__unimatrix__context_search` — found entries #2730, #3817, #4013, #646 (patterns), and #4017, #4018, #4019 (ADRs). Entry #4013 is directly relevant: "spec only names a subset of test sites." Applied: grepped config.rs AND nli_detection_tick.rs for old literal before marking complete.
- **Stored:** Attempted to supersede entry #4013 to extend it with the cross-file nli_detection_tick.rs hidden site (crt-039 adds a new concrete example where the failing test was in a different file than the spec-scoped change). `context_correct` returned `-32003` (agent lacks Write capability). Pattern content prepared but not persisted — delivery leader should store via admin agent if warranted.

  Proposed extension to #4013:
  > When changing InferenceConfig defaults, grep BOTH config.rs AND nli_detection_tick.rs for every old literal value. crt-039: test_phase4b_uses_nli_informs_cosine_floor_not_supports_threshold in nli_detection_tick.rs used cosine=0.47 (old band floor), compiled cleanly, failed only at runtime after floor raised to 0.50. This is a cross-file hidden site — spec scoped the change to config.rs only.
