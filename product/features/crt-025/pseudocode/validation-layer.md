# Component 1: Validation Layer
## File: `crates/unimatrix-server/src/infra/validation.rs`

---

## Purpose

Centralizes all cycle parameter validation for both the MCP tool path and the hook path. The current `validate_cycle_params` accepts only `type_str`, `topic`, and `keywords`. This component extends it to handle `phase`, `outcome`, `next_phase`, adds `CycleType::PhaseEnd`, and adds `CYCLE_PHASE_END_EVENT` constant. The `keywords` parameter is removed from the signature; `ValidatedCycleParams.keywords` field is dropped.

---

## New / Modified Declarations

### Constants (additions)

```
// Existing (unchanged):
const CYCLE_START_EVENT: &str = "cycle_start";
const CYCLE_STOP_EVENT:  &str = "cycle_stop";

// New:
const CYCLE_PHASE_END_EVENT: &str = "cycle_phase_end";

// New validation limits:
const MAX_PHASE_LEN:   usize = 64;
const MAX_OUTCOME_LEN: usize = 512;
```

### `CycleType` enum (modified)

```
enum CycleType {
    Start,
    PhaseEnd,   // NEW: maps from "phase-end"
    Stop,
}
```

### `ValidatedCycleParams` struct (modified)

```
struct ValidatedCycleParams {
    cycle_type:  CycleType,
    topic:       String,
    phase:       Option<String>,      // NEW; normalized: trim + lowercase
    outcome:     Option<String>,      // NEW; max 512 chars; stored as-is after length check
    next_phase:  Option<String>,      // NEW; normalized: trim + lowercase
    // keywords: Vec<String>  REMOVED
}
```

---

## Modified Function: `validate_cycle_params`

### Signature (modified)

```
pub fn validate_cycle_params(
    type_str:   &str,
    topic:      &str,
    phase:      Option<&str>,     // NEW
    outcome:    Option<&str>,     // NEW
    next_phase: Option<&str>,     // NEW
    // keywords parameter REMOVED
) -> Result<ValidatedCycleParams, String>
```

Note: Returns `Result<_, String>` not `Result<_, ServerError>`. This contract is fixed (C-02, FR-03.4). Hook path cannot use `ServerError`.

### Pseudocode Body

```
FUNCTION validate_cycle_params(type_str, topic, phase, outcome, next_phase):

    // Step 1: Validate type string
    cycle_type = match type_str:
        "start"     → CycleType::Start
        "phase-end" → CycleType::PhaseEnd     // NEW case
        "stop"      → CycleType::Stop
        other       → return Err("invalid type '{}': must be 'start', 'phase-end', or 'stop'")

    // Step 2: Validate and sanitize topic (unchanged logic)
    IF topic is empty → return Err("topic must not be empty")
    clean_topic = topic filtered to ASCII non-control chars, truncated at 128
    IF clean_topic is empty → return Err("topic contains only invalid characters")
    IF NOT is_valid_feature_id(clean_topic) → return Err("topic is not a valid feature cycle identifier")

    // Step 3: Validate phase (NEW)
    validated_phase = validate_phase_field("phase", phase)?

    // Step 4: Validate next_phase (NEW)
    validated_next_phase = validate_phase_field("next_phase", next_phase)?

    // Step 5: Validate outcome (NEW)
    validated_outcome = match outcome:
        None → None
        Some(s) →
            IF s.len() > MAX_OUTCOME_LEN (512)
                → return Err("outcome exceeds 512 characters")
            → Some(s.to_string())

    // Step 6: Return (keywords dropped)
    return Ok(ValidatedCycleParams {
        cycle_type,
        topic: clean_topic,
        phase:      validated_phase,
        outcome:    validated_outcome,
        next_phase: validated_next_phase,
    })
```

### Helper: `validate_phase_field` (new private function)

```
FUNCTION validate_phase_field(field_name: &str, value: Option<&str>) -> Result<Option<String>, String>:
    match value:
        None → return Ok(None)
        Some(s) →
            trimmed = s.trim()
            IF trimmed is empty → return Err("{field_name} must not be empty when provided")
            normalized = trimmed.to_lowercase()
            IF normalized.len() > MAX_PHASE_LEN (64) → return Err("{field_name} exceeds 64 characters")
            IF normalized.contains(' ') → return Err("{field_name} must not contain spaces")
            return Ok(Some(normalized))
```

Note on edge cases from RISK-TEST-STRATEGY:
- `" scope"` → trim to `"scope"` → lowercase → `"scope"` → passes (no internal space)
- `"scope "` → trim to `"scope"` → passes
- `"a b"` (space after trim) → fails
- `"Scope"` → lowercase to `"scope"` → passes
- `""` (empty) → fails
- 64-char string → passes; 65-char string → fails

---

## Removed Logic

Remove the `keywords` validation block entirely:
```
// REMOVE: MAX_KEYWORD_LEN, MAX_KEYWORDS_COUNT constants (no longer used)
// REMOVE: keywords parameter from validate_cycle_params signature
// REMOVE: keywords: Vec<String> field from ValidatedCycleParams
// REMOVE: keywords validation loop (steps 3 and 4 in current implementation)
```

The `MAX_CYCLE_TOPIC_LEN` constant is retained (still used for topic validation).

---

## Error Handling

All errors are `String` values, not `ServerError`. Pattern follows existing validation error messages: descriptive, naming the field and constraint violated.

| Condition | Error String |
|-----------|-------------|
| Unknown type | `"invalid type '{other}': must be 'start', 'phase-end', or 'stop'"` |
| Empty topic | `"topic must not be empty"` |
| Invalid feature ID format | `"topic is not a valid feature cycle identifier"` |
| Empty phase when provided | `"phase must not be empty when provided"` |
| Phase contains space | `"phase must not contain spaces"` |
| Phase exceeds 64 chars | `"phase exceeds 64 characters"` |
| Same rules for next_phase | field name = `"next_phase"` |
| Outcome exceeds 512 chars | `"outcome exceeds 512 characters"` |

---

## Key Test Scenarios

1. `type_str = "phase-end"` → returns `CycleType::PhaseEnd`
2. `type_str = "start"` and `type_str = "stop"` → unchanged behavior
3. `type_str = "pause"` → Err with all three valid values named
4. `phase = Some("Scope")` → `Some("scope")` in result
5. `phase = Some("IMPLEMENTATION")` → `Some("implementation")`
6. `phase = Some("scope review")` → Err (contains space after trim check)
7. `phase = Some(" scope")` (leading space) → trimmed to `"scope"` → Ok
8. `phase = Some("")` → Err (empty after trim)
9. `phase = Some("a".repeat(64))` → Ok
10. `phase = Some("a".repeat(65))` → Err
11. `phase = None` → `None` regardless of cycle_type
12. `outcome = Some("x".repeat(512))` → Ok
13. `outcome = Some("x".repeat(513))` → Err
14. `next_phase` receives identical normalization to `phase`
15. Old callers passing `keywords` in JSON → silently discarded (no field on struct)
16. `validate_cycle_params` still compiles and behaves correctly for the hook path (no `ServerError` used)
