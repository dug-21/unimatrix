# Component Test Plan: source-domain-derivation
## Sites A, B, C across listener.rs, background.rs, services/observation.rs

Validating ACs: **AC-06, AC-07(a), AC-07(b), AC-07(c), AC-08**
Risk coverage: **R-04, R-06, R-07 (guard placement)**

---

## Component Responsibility

Three production sites hardcode `source_domain = "claude-code"`. This component
replaces all three with dynamic derivation:

- **Site A** (`listener.rs:1894`): live write path — has `ImplantEvent.provider` available
  directly. Derivation: `event.provider.clone().unwrap_or_else(|| "claude-code".to_string())`
- **Site B** (`background.rs:1330`): DB read path in `fetch_observation_batch()` — no
  provider in DB; uses Approach A registry-with-fallback.
- **Site C** (`services/observation.rs:585`): DB read path in `parse_observation_rows()`
  — same Approach A pattern; `_registry` parameter prefix removed.

All three tests verify: no literal `"claude-code"` assignment remains at the target
line (AC-07 grep check), and the correct `source_domain` is derived.

The `DEFAULT_HOOK_SOURCE_DOMAIN` constant placement is decided by the implementer
(ARCHITECTURE.md OQ-A). Tests import it wherever it is defined.

---

## Unit Test Expectations

### Site A: listener.rs (AC-06, AC-07a, R-02)

Site A tests require a running DB or a test helper that exercises `dispatch_request()`
for a `RecordEvent` with a specific `ImplantEvent.provider`. The existing `observation.rs`
tests use `SqlxStore` with `open_test_store`. Site A tests should follow the same pattern.

**`test_site_a_gemini_event_writes_gemini_source_domain`** (AC-06)

```rust
// Arrange: RecordEvent with ImplantEvent { provider: Some("gemini-cli"), event_type: "PreToolUse" }
// Feed through dispatch_request() or the relevant listener path.
// Then read the written ObservationRecord from the DB.

// Assert
assert_eq!(record.source_domain, "gemini-cli",
    "Site A must derive source_domain from ImplantEvent.provider");
```

If `dispatch_request()` is too integrated to call directly in a unit test, this test
can be covered by the infra-001 lifecycle suite integration test (AC-02 covers the
source_domain assertion for the cycle_start path).

---

**`test_site_a_claude_code_event_writes_claude_code_source_domain`** (AC-06)

```rust
// Arrange: RecordEvent with ImplantEvent { provider: Some("claude-code"), event_type: "PreToolUse" }
// or ImplantEvent { provider: None, event_type: "PreToolUse" } — both must produce "claude-code"

// Assert
assert_eq!(record.source_domain, "claude-code");
```

---

**`test_site_a_provider_none_falls_back_to_claude_code`** (R-02 scenario 3, AC-06)

```rust
// Arrange: ImplantEvent with provider: None (pre-vnc-013 Claude Code event on wire)
// This documents the known degraded case — provider absent → "claude-code" fallback.

// Assert
assert_eq!(record.source_domain, "claude-code",
    "None provider must fall back to claude-code via unwrap_or_else");
```

---

### Site C: services/observation.rs (AC-07c, R-04, R-06)

Site C tests extend the existing `parse_observation_rows()` test suite. The existing
tests use `sqlx::sqlite` directly. New tests add rows and call through
`load_feature_observations()` or `parse_observation_rows()` directly.

**`test_parse_rows_unknown_event_type_passthrough`** (R-06 — EXISTING TEST, COMMENT UPDATE ONLY)

This test already exists and asserts `source_domain == "claude-code"` for
`"UnknownEventType"`. Under Approach A, `resolve_source_domain("UnknownEventType")`
returns `"unknown"`, and the fallback restores `"claude-code"`. The assertion remains
correct.

Required change: Update the test comment only. Replace:
> "All hook-path records get source_domain = 'claude-code' (FR-03.3)"

With:
> "registry returns 'unknown' for unregistered types; Approach A fallback to
> DEFAULT_HOOK_SOURCE_DOMAIN restores 'claude-code' to preserve hook-path invariant
> (FR-06.4)."

Also add a second assertion within the same test:

```rust
// Verify the constant value is stable — this test doubles as a regression sentinel
assert_eq!(DEFAULT_HOOK_SOURCE_DOMAIN, "claude-code",
    "DEFAULT_HOOK_SOURCE_DOMAIN must be 'claude-code' (Approach A contract)");
```

---

**`test_parse_rows_hook_path_always_claude_code`** (R-04, AC-07c — EXISTING TEST, NO CHANGE)

This test asserts `source_domain == "claude-code"` for `"PreToolUse"`. Under Approach
A, `resolve_source_domain("PreToolUse")` returns `"claude-code"` (PreToolUse IS in the
builtin claude-code pack). The assertion and test body are unchanged — just verify it
passes after Site C implementation.

---

**`test_approach_a_fallback_for_stop_event`** (R-04 scenario 2)

```rust
// Arrange: observation row with event_type = "Stop"
// "Stop" is NOT in the builtin claude-code pack's 4-event list.
// Approach A: registry returns "unknown" → fallback to DEFAULT_HOOK_SOURCE_DOMAIN.
insert_observation(&store, "sess-1", 1700000000000, "Stop", None, None, None, None).await;

// Act: parse through observation.rs path
let records = ...; // call load_feature_observations or parse_observation_rows directly

// Assert
assert_eq!(records[0].event_type, "Stop");
assert_eq!(records[0].source_domain, "claude-code",
    "Stop is not in builtin pack; Approach A fallback must restore claude-code");
```

---

**`test_approach_a_fallback_for_session_start`** (R-04 — additional Stop/SessionStart coverage)

```rust
// "SessionStart" is also not in the 4-event builtin pack list.
insert_observation(&store, "sess-1", 1700000000001, "SessionStart", None, None, None, None).await;

let records = ...;
assert_eq!(records[0].source_domain, "claude-code");
```

---

**`test_approach_a_fallback_for_cycle_events`** (R-04 scenario 3)

```rust
// cycle_start and cycle_stop are Category 2 events — not in the registry's
// claude-code pack event_types list. Approach A must return "claude-code".
for event in &["cycle_start", "cycle_stop", "cycle_phase_end"] {
    // Insert observation and parse
    // Assert source_domain == "claude-code"
}
```

This is critical for `context_cycle_review` correctness — wrong source_domain on the
DB read path would produce incorrect retrospective attribution.

---

**`test_registry_prefix_removed_from_parameter`** (AC-07c implementation check)

Not a behavioral test — this is a structural assertion:

```rust
// After implementation, verify the registry parameter is used (not prefixed with _).
// The compiler would warn on an unused non-underscored variable.
// This test documents the contract:
// A compile-time error if _registry prefix is restored would be equivalent.
// Behavioral proxy: run a Stop event through parse_observation_rows() and verify
// source_domain == "claude-code" (if _registry were still unused, the old hardcode
// would return "claude-code" too — this test alone doesn't distinguish.
// The AC-07 grep check is the definitive verification for this item.)
```

See AC-07 grep check below for definitive verification.

---

### Site B: background.rs (AC-07b, R-04)

**`test_background_fetch_observation_batch_approach_a`** (AC-07b)

Background.rs tests typically require a running DB and a session with observations.
The existing observation.rs test pattern (`setup_test_store` + `insert_session` +
`insert_observation`) applies.

```rust
// Arrange: insert observations for "Stop" and "PreToolUse" event types
// Act: call fetch_observation_batch() (or the function that calls it at line 1330)
// Assert:
//   - "PreToolUse" rows → source_domain == "claude-code" (registry resolves directly)
//   - "Stop" rows → source_domain == "claude-code" (Approach A fallback)
//   - "cycle_start" rows → source_domain == "claude-code" (Approach A fallback)
```

If `fetch_observation_batch()` is private or difficult to test directly, the behavioral
assertion can be made at the level of the function that calls it (e.g., the retrospective
analysis function). Document the indirect path in the test comment.

---

## AC-07 Grep Check (Code Review Gate)

The following literal must NOT appear as a `source_domain` assignment in any of the
three sites after implementation:

```
"claude-code".to_string()
```

at the target lines:
- `listener.rs:1894` — now uses `event.provider.clone().unwrap_or_else(|| "claude-code".to_string())`
  (this form IS acceptable — it uses the string as a fallback argument, not as a direct assignment)
- `background.rs:1330` — must use `DEFAULT_HOOK_SOURCE_DOMAIN.to_string()` or `registry_with_fallback`
- `services/observation.rs:585` — same as Site B

**Precise AC-07 grep contract**: The literal string `"claude-code"` must not appear
as the right-hand side of a direct `source_domain: "...".to_string()` assignment in
any of the three target locations. Use of `"claude-code"` as the `unwrap_or_else`
fallback argument in Site A is acceptable — it is an expression, not a hardcoded
`source_domain = "claude-code"` assignment.

At Stage 3c execution, the tester should run:
```bash
grep -n '"claude-code"' \
  crates/unimatrix-server/src/uds/listener.rs \
  crates/unimatrix-server/src/background.rs \
  crates/unimatrix-server/src/services/observation.rs
```
and verify that any remaining matches are NOT direct source_domain assignments.

---

## debug_assert Guard Placement (AC-16, R-07)

The `debug_assert!` in `listener.rs extract_observation_fields()` is tested in
`normalization.md` (test_rework_candidate_guard_fires_in_debug). Document here that
the guard must be placed **before** the `match event_type { ... }` in
`extract_observation_fields()`, not inside the `"PostToolUse" | "post_tool_use_rework_candidate"`
arm. The match arm is the actual normalization enforcement; the `debug_assert!` is
an earlier canary.

---

## Assertions Summary

| Test | Risk | AC |
|------|------|----|
| `test_site_a_gemini_event_writes_gemini_source_domain` | R-02 | AC-06 |
| `test_site_a_claude_code_event_writes_claude_code_source_domain` | — | AC-06 |
| `test_site_a_provider_none_falls_back_to_claude_code` | R-02 | AC-06 |
| `test_parse_rows_unknown_event_type_passthrough` (updated comment) | R-06 | AC-07(c), AC-08 |
| `test_parse_rows_hook_path_always_claude_code` (unchanged) | R-04 | AC-07(c) |
| `test_approach_a_fallback_for_stop_event` | R-04 | AC-07(b), AC-07(c) |
| `test_approach_a_fallback_for_session_start` | R-04 | AC-07(b), AC-07(c) |
| `test_approach_a_fallback_for_cycle_events` | R-04 | AC-07(b), AC-07(c) |
| `test_background_fetch_observation_batch_approach_a` | R-04 | AC-07(b) |
| AC-07 grep check (runtime verification) | R-09 | AC-07 |
