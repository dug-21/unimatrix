# Test Plan: config-knob (`infra/config.rs` `transcript_buffer_max_bytes`)

Covers R-11; FR-10, ADR-006. Unit tests in the existing `config.rs` test module (`:3786`),
plus one cross-layer integration test (the only scenario that catches wiring gaps).

## §1 Serde + validation

- `test_transcript_buffer_max_bytes_default_when_absent` — config without the field
  deserializes to `4_194_304`.
- `test_transcript_buffer_max_bytes_explicit_value_respected` — round-trips a non-default.
- `test_validate_rejects_below_floor` — `65_535` rejected with a clear error naming the field
  and the floor; `65_536` accepted (boundary).
- `test_validate_accepts_default` — default passes `validate()` (guard against a floor typo).

## §2 Project-wins merge

- `test_transcript_buffer_max_bytes_project_overrides_global` — global sets one value,
  project another: project wins (same merge behavior as sibling `transcript_retention` arm;
  mirror that existing test's shape).
- `test_transcript_buffer_max_bytes_global_used_when_project_absent`.

## §3 Cap chain — R-11.5 (the load-bearing scenario)

- `test_config_cap_reaches_session_buffer` (integration, lives where registry construction is
  testable): config with 128 KiB cap → `SessionRegistry::with_transcript_cap(...)` → register
  session → stream past 128 KiB → overflow (ring-tail) occurs at 128 KiB, NOT 4 MiB. This is
  the config→registry→buffer three-layer chain tested once end-to-end.
- Grep/review gate: all three production construction sites (`server.rs:335`,
  `main.rs:645/:1068`) call `with_transcript_cap(cfg.retention.transcript_buffer_max_bytes)`;
  no production site uses `SessionRegistry::new()` (test-only ctor).
