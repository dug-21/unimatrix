# Agent Report: crt-022-agent-4-config-and-removal

**Task**: Implement InferenceConfig + AsyncEmbedService removal (Wave 1 - Foundation)
**Branch**: feature/crt-022
**Commit**: 97c38ee

---

## Files Modified

| File | Change |
|------|--------|
| `crates/unimatrix-core/src/async_wrappers.rs` | Removed `AsyncEmbedService<T>` struct and `embed_entry`, `embed_entries`, `dimension` methods; removed `EmbedService` from imports |
| `crates/unimatrix-server/src/infra/config.rs` | Added `InferenceConfig` struct, `Default` impl, `validate()` method, `ConfigError::InferencePoolSizeOutOfRange` variant + Display, `inference: InferenceConfig` field on `UnimatrixConfig`, validation wire-up in `validate_config`, merge semantics in `merge_configs`, 12 unit tests |
| `Cargo.lock` | Updated to reflect `rayon`, `num_cpus`, `thiserror` additions already in `Cargo.toml` |

---

## Tests

**12 new unit tests** in `crates/unimatrix-server/src/infra/config.rs` `#[cfg(test)] mod tests`:

| Test | Covers |
|------|--------|
| `test_inference_config_valid_lower_bound` | AC-11 #5 |
| `test_inference_config_valid_upper_bound` | AC-11 #6 |
| `test_inference_config_rejects_zero` | AC-11 #7 |
| `test_inference_config_rejects_sixty_five` | AC-11 #8 |
| `test_inference_config_valid_eight` | R-07 scenario 3 |
| `test_inference_config_valid_four` | ADR-003 floor |
| `test_inference_config_default_formula_in_range` | R-07 scenario 5 |
| `test_inference_config_absent_section_uses_default` | AC-09 |
| `test_inference_config_parses_from_toml` | serde round-trip |
| `test_inference_config_error_message_names_field` | AC-09 diagnostic quality |
| `test_inference_merge_per_project_wins` | merge semantics |
| `test_inference_merge_global_wins_when_project_is_default` | merge semantics |

**Pass/fail**: 1483 passed, 0 failed (full `unimatrix-server` lib test run).

**Workspace check**: `cargo check --workspace` — zero errors.

---

## Verification

- `grep -r "AsyncEmbedService" crates/` — zero results (confirmed twice)
- `AsyncVectorStore` present and unchanged in `async_wrappers.rs`
- `cargo check --workspace` exits 0 after `AsyncEmbedService` removal
- `InferenceConfig::validate()` wired into `validate_config` (called per config file load)
- `merge_configs` extended with replace-semantics for `rayon_pool_size`
- Default formula `(num_cpus::get() / 2).max(4).min(8)` matches ADR-003 (floor 4, not 2)

---

## Issues

None. Pre-existing doctest failure in `config.rs` module-level doc comment (the `~/` path in the `# File layout` section) — not introduced by this agent; confirmed pre-existing.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-server` config patterns — found entry #646 (`Backward-Compatible Config Extension via serde(default)`), directly applicable and followed exactly.
- Stored: nothing novel to store — `InferenceConfig` follows the established `#[serde(default)]` pattern from entry #646 verbatim; `merge_configs` extension follows the replace-semantics pattern already documented in the file. No new gotchas or non-obvious traps discovered.
