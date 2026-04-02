# Agent Report: crt-041-agent-4-config

**Component:** `config`
**Feature:** crt-041 — Graph Enrichment: S1, S2, S8 Edge Sources
**Agent ID:** crt-041-agent-4-config

---

## Work Completed

Modified `crates/unimatrix-server/src/infra/config.rs` across all five required sites:

1. **Struct field declarations** — Five new fields added to `InferenceConfig` after `supports_cosine_threshold`, with `#[serde(default = "...")]` attributes and full doc comments including range bounds and the S2 vocabulary recommendation.

2. **`impl Default` struct literal** — Five fields added with identical values to the backing functions (dual-site invariant per ADR-005).

3. **`default_*()` backing functions** — Five private functions added in a new `// Graph enrichment tick default value functions (crt-041)` section.

4. **`validate()` range checks** — Four numeric fields validated with `[1, N]` lower bound (1 not 0), rejecting the zero case that causes `% 0` panic (`s8_batch_interval_ticks`) or `LIMIT 0` silent disable. `s2_vocabulary` has no range check (empty vec is valid).

5. **`merge_configs()` entries** — Five fields added following the project-wins-over-global pattern. `s2_vocabulary` uses `!=` (Vec equality), numeric fields use `!=` (integer equality), consistent with the existing integer-field merge pattern.

---

## Tests Written

All tests in the `config.rs::tests` module. 17 new tests added:

| Test | Coverage |
|------|----------|
| `test_inference_config_s1_s2_s8_defaults_match_serde` | MANDATORY dual-site guard (R-03, ADR-005) |
| `test_inference_config_s2_vocabulary_empty_by_default` | W0-3 domain-agnostic default (R-03) |
| `test_inference_config_numeric_defaults` | Default value correctness (R-03) |
| `test_inference_config_s1_s2_s8_validate_rejects_zero` | max_s1_edges_per_tick = 0 rejected (R-17, AC-24) |
| `test_inference_config_validate_rejects_zero_s2_cap` | max_s2_edges_per_tick = 0 rejected |
| `test_inference_config_validate_rejects_zero_s8_interval` | s8_batch_interval_ticks = 0 rejected (panic guard) |
| `test_inference_config_validate_rejects_zero_s8_pair_cap` | max_s8_pairs_per_batch = 0 rejected |
| `test_inference_config_validate_accepts_minimum_values` | Lower bound = 1 passes (R-17) |
| `test_inference_config_validate_accepts_maximum_values` | Upper bounds pass (R-17) |
| `test_inference_config_validate_rejects_above_max_s1` | 10_001 rejected |
| `test_inference_config_validate_rejects_above_max_s8_interval` | 1_001 rejected |
| `test_inference_config_s2_vocabulary_parses_from_toml` | TOML list deserialization |
| `test_inference_config_s2_vocabulary_explicit_empty_toml` | Explicit `[]` is valid |
| `test_inference_config_partial_toml_uses_defaults` | Partial TOML — absent fields use defaults |
| `test_merge_configs_project_overrides_s1_cap` | Project wins (T-CFG-05) |
| `test_merge_configs_global_fallback_s1_cap` | Global wins when project == default (T-CFG-06) |
| `test_merge_configs_project_overrides_s2_vocabulary` | Vec field project-wins merge |

---

## Test Results

```
test result: ok. 286 passed; 0 failed; 0 ignored
```

(269 pre-existing + 17 new)

---

## Commit

`0ba353ad` — `impl(config): add S1/S2/S8 InferenceConfig fields for graph enrichment tick (#487)`

Branch: `feature/crt-041`

---

## Issues / Blockers

None. All five sites updated, all tests pass, build clean.

The mandatory `test_inference_config_s1_s2_s8_defaults_match_serde` test uses `serde_json::from_str("{}")` per spawn instructions (portable zero-config serde input). The TOML-based variant from the pseudocode (`toml::from_str("")`) would also work; both are present conceptually in the partial TOML test.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_search` — found entry #3817 (dual-site pattern) and #4028 (crt-040 ADR). Both directly relevant; pattern is well-established.
- Stored: attempted `context_correct` on #3817 to add `serde_json::from_str` recommendation — blocked by missing Write capability for agent identity `anonymous`. Nothing novel lost: the crt-041 ADR (#4035) already documents the dual-site requirement. Existing entry #3817 remains accurate.
