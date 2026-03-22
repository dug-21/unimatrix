# Test Plan: Validation Layer (Component 1)

File: `crates/unimatrix-server/src/infra/validation.rs`
Risks: R-06 (Critical coverage), AC-02, AC-03

---

## Unit Test Expectations

All tests are inline `#[cfg(test)]` functions in `validation.rs`. No I/O, no async.

### `validate_cycle_params` — Type Acceptance (AC-02, FR-03)

**`test_validate_cycle_params_type_start_accepted`**
- Arrange: `type_str = "start"`, `topic = "crt-025"`, all optionals `None`
- Act: call `validate_cycle_params`
- Assert: returns `Ok(ValidatedCycleParams { cycle_type: CycleType::Start, .. })`

**`test_validate_cycle_params_type_phase_end_accepted`**
- Arrange: `type_str = "phase-end"`, valid topic, all optionals `None`
- Assert: returns `Ok(ValidatedCycleParams { cycle_type: CycleType::PhaseEnd, .. })`

**`test_validate_cycle_params_type_stop_accepted`**
- Arrange: `type_str = "stop"`, valid topic, all optionals `None`
- Assert: returns `Ok(ValidatedCycleParams { cycle_type: CycleType::Stop, .. })`

**`test_validate_cycle_params_type_invalid_pause_rejected`**
- Arrange: `type_str = "pause"`, valid topic
- Assert: returns `Err(msg)` where `msg` contains all three valid values
  ("start", "phase-end", "stop")

**`test_validate_cycle_params_type_empty_rejected`**
- Arrange: `type_str = ""`, valid topic
- Assert: returns `Err(_)`

**`test_validate_cycle_params_type_restart_rejected`**
- Arrange: `type_str = "restart"`, valid topic
- Assert: returns `Err(_)` with descriptive message

### `validate_cycle_params` — Phase Normalization (R-06, AC-03, FR-02)

**`test_validate_phase_lowercase_normalization`**
- Arrange: `phase = Some("Scope")`, valid type/topic
- Assert: `Ok(params)` where `params.phase == Some("scope")`

**`test_validate_phase_uppercase_normalization`**
- Arrange: `phase = Some("IMPLEMENTATION")`, valid type/topic
- Assert: `params.phase == Some("implementation")`

**`test_validate_phase_mixed_case_normalization`**
- Arrange: `phase = Some("Design")`, valid type/topic
- Assert: `params.phase == Some("design")`

**`test_validate_next_phase_normalization`**
- Arrange: `next_phase = Some("Design")`, valid type/topic
- Assert: `params.next_phase == Some("design")`

**`test_validate_phase_none_always_valid`** (FR-02.5)
- Arrange: `phase = None`, any event type
- Assert: `Ok(_)` with `phase = None`

### `validate_cycle_params` — Phase Format Rejection (R-06, FR-02)

**`test_validate_phase_space_rejected`** (FR-02.2)
- Arrange: `phase = Some("scope review")`
- Assert: `Err(msg)` containing "phase"

**`test_validate_phase_leading_space_trimmed_internal_space_rejected`** (edge case)
- Arrange: `phase = Some("a b")` (internal space after trim)
- Assert: `Err(_)` — internal space rejected

**`test_validate_phase_leading_trailing_space_trimmed_passes`** (edge case from Risk Strategy)
- Arrange: `phase = Some(" scope ")` (leading + trailing space only)
- Assert: `Ok(params)` where `params.phase == Some("scope")` — trim removes spaces, result has no space

**`test_validate_phase_empty_rejected`** (FR-02.4)
- Arrange: `phase = Some("")`
- Assert: `Err(msg)` containing "phase"

**`test_validate_phase_64_char_boundary_accepted`** (FR-02.3 boundary)
- Arrange: `phase = Some(&"a".repeat(64))`
- Assert: `Ok(_)` with `params.phase == Some("a".repeat(64))`

**`test_validate_phase_65_char_rejected`** (FR-02.3)
- Arrange: `phase = Some(&"a".repeat(65))`
- Assert: `Err(msg)` containing "64" or "phase"

**`test_validate_phase_underscore_accepted`** (R-06 clarification — no space, passes format)
- Arrange: `phase = Some("gate_review")` — underscore is not a space
- Assert: `Ok(params)` where `params.phase == Some("gate_review")`

**`test_validate_outcome_max_512_chars_accepted`** (FR-02.6, WARN resolved)
- Arrange: `outcome = Some(&"x".repeat(512))`
- Assert: `Ok(_)`

**`test_validate_outcome_513_chars_rejected`** (FR-02.6)
- Arrange: `outcome = Some(&"x".repeat(513))`
- Assert: `Err(msg)` containing "outcome"

**`test_validate_outcome_none_always_valid`** (FR-02.6)
- Arrange: `outcome = None`
- Assert: `Ok(_)`

### `CYCLE_PHASE_END_EVENT` constant (FR-03.7 dependency)

**`test_cycle_phase_end_event_constant_value`**
- Assert: `CYCLE_PHASE_END_EVENT == "cycle_phase_end"`

### `keywords` removal (FR-03.5)

**`test_validate_cycle_params_no_keywords_parameter`** (compile-time, but document intent)
- Verify function signature has no `keywords` parameter
- Assert: function compiles without `keywords`; any call site with `keywords` would fail to compile

---

## Integration Test Expectations

The validation layer is pure (no I/O), so no integration tests are needed beyond what's covered
by server-level integration tests that call `context_cycle` through the MCP protocol.

The infra-001 `tools` suite covers type rejection and phase rejection at the MCP level
(`test_cycle_invalid_type_rejected`, `test_cycle_phase_with_space_rejected` — new tests).

---

## Edge Cases from Risk Strategy

| Edge Case | Test | Expected Outcome |
|-----------|------|-----------------|
| `" scope"` (leading space) | `test_validate_phase_leading_trailing_space_trimmed_passes` | Trim to `"scope"`, pass |
| `"scope "` (trailing space) | same test (symmetric) | Trim to `"scope"`, pass |
| `"a b"` (internal space after trim) | `test_validate_phase_leading_space_trimmed_internal_space_rejected` | Rejected |
| `"gate_review"` (underscore) | `test_validate_phase_underscore_accepted` | Accepted (not a space) |
| `phase = None` on stop event | `test_validate_phase_none_always_valid` | Always valid |
| `outcome = None` | `test_validate_outcome_none_always_valid` | Always valid |
| 64-char boundary | `test_validate_phase_64_char_boundary_accepted` | Accepted |
| 65-char boundary | `test_validate_phase_65_char_rejected` | Rejected |
