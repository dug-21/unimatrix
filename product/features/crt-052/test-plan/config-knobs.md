# Test Plan — C9 Config Knobs

**Component**: four knobs added to `RetentionConfig` in `infra/config.rs` (beside
`transcript_buffer_max_bytes` `:1561-1576`), with the `serde(default)` / `validate()` / merge pattern.
**Knobs**: `transcript_hold_max_sessions` (≈64), `transcript_hold_ttl_secs` (≈86400),
`transcript_candidate_session_cap_bytes` (24 KB), `transcript_candidate_cycle_cap_bytes` (≈256 KB).
**Wave**: A (cap knobs) + B (hold knobs). **Tests live in**: `infra/config.rs` `#[cfg(test)]`.

## Unit Test Expectations
- `test_config_defaults` — each knob defaults to its starting value when absent from config
  (`serde(default)`): `transcript_hold_max_sessions==64`, `transcript_hold_ttl_secs==86400`,
  `transcript_candidate_session_cap_bytes==24*1024`, `transcript_candidate_cycle_cap_bytes==256*1024`.
  (Assert against the delivery-pinned defaults; if delivery pins different numbers, update to match.)
- `test_config_serde_roundtrip` — round-trip the extended `RetentionConfig` through serde; new fields
  parse from config and serialize back.
- `test_config_merge_pattern` — partial config overlays defaults via the existing merge pattern; an
  unset knob inherits the default, a set knob overrides.
- `test_config_validate_bounds` — `validate()` rejects nonsensical values (zero/negative cap, zero
  max_sessions if invalid, zero TTL if invalid) per the validation pattern used by
  `transcript_buffer_max_bytes`.
- `test_config_knobs_drive_behavior` (cross-ref) — confirm the knobs are READ by the consumers: the
  hold cap/TTL by `transcript_hold.rs` (held-buffer-store.md tests parameterize on the knob, not the
  literal), the caps by `select.rs` (per-session) and `distill_handler.rs` (per-cycle aggregate).

## Cross-component note
- Boundary tests for cap eviction (R-02) and TTL sweep (R-02) and aggregate-cap truncation (R-15)
  parameterize on these knobs rather than hard-coded literals — so a defaults change does not break the
  behavior tests. This plan owns only the config parse/validate/merge; the behavior is owned by C8 and
  C6.

## Assertions Summary (concrete)
- All four knobs parse, default, merge, and validate per the established `RetentionConfig` pattern.
- Consumers read the knob value, not a hard-coded constant (so config is genuinely live).
