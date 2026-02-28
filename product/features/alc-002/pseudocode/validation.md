# Pseudocode: validation component

## Purpose

Add input validation and enum parsing for the enrollment tool's parameters. Follows the strict parsing approach from ADR-001.

## New Constants

```
const MAX_AGENT_ID_LEN: usize = 100;
```

## Function: validate_enroll_params

```
pub fn validate_enroll_params(params: &EnrollParams) -> Result<(), ServerError>:
    // target_agent_id: required, non-empty, max length, no control chars
    if params.target_agent_id.is_empty():
        return Err(InvalidInput { field: "target_agent_id", reason: "required" })

    validate_string_field("target_agent_id", &params.target_agent_id, MAX_AGENT_ID_LEN, false)?

    // trust_level: validated by parse_trust_level (called separately in tool handler)
    // capabilities: validated by parse_capabilities (called separately in tool handler)
    // agent_id, format: handled by existing identity/format parsing

    Ok(())
```

NOTE: trust_level and capabilities are validated by their dedicated parsing functions, not here. This function validates the target_agent_id string field only. The tool handler calls validate_enroll_params first, then parse_trust_level, then parse_capabilities in sequence.

## Function: parse_trust_level

Per ADR-001: strict exhaustive matching, case-insensitive, no fallback.

```
pub fn parse_trust_level(s: &str) -> Result<TrustLevel, ServerError>:
    match s.to_lowercase().as_str():
        "system" => Ok(TrustLevel::System)
        "privileged" => Ok(TrustLevel::Privileged)
        "internal" => Ok(TrustLevel::Internal)
        "restricted" => Ok(TrustLevel::Restricted)
        _ => Err(InvalidInput {
            field: "trust_level",
            reason: "must be one of: system, privileged, internal, restricted"
        })
```

No trimming -- input must be exact (case-insensitive). "system " with trailing space is rejected.

## Function: parse_capabilities

Per ADR-001: strict exhaustive matching, case-insensitive, no fallback. Rejects duplicates.

```
pub fn parse_capabilities(caps: &[String]) -> Result<Vec<Capability>, ServerError>:
    if caps.is_empty():
        return Err(InvalidInput { field: "capabilities", reason: "at least one capability required" })

    let mut result = Vec::new()
    let mut seen = HashSet::new()   // tracks lowercase strings for dedup

    for cap_str in caps:
        let lower = cap_str.to_lowercase()

        if !seen.insert(lower.clone()):
            return Err(InvalidInput {
                field: "capabilities",
                reason: format!("duplicate capability: {}", cap_str)
            })

        let capability = match lower.as_str():
            "read" => Capability::Read
            "write" => Capability::Write
            "search" => Capability::Search
            "admin" => Capability::Admin
            _ => return Err(InvalidInput {
                field: "capabilities",
                reason: format!("unknown capability '{}'. Valid: read, write, search, admin", cap_str)
            })

        result.push(capability)

    Ok(result)
```

## Imports Required

```
use std::collections::HashSet;
use crate::registry::{TrustLevel, Capability};
use crate::tools::EnrollParams;
```

## Error Handling

All errors are `ServerError::InvalidInput` with descriptive field/reason. This is consistent with all existing validation functions.

## Key Test Scenarios

### parse_trust_level
- "system" -> Ok(System)
- "SYSTEM" -> Ok(System) -- case insensitive
- "Privileged" -> Ok(Privileged)
- "internal" -> Ok(Internal)
- "restricted" -> Ok(Restricted)
- "admin" -> Err (not a valid trust level)
- "" -> Err
- "system " (trailing space) -> Err (strict, no trimming)
- "superadmin" -> Err

### parse_capabilities
- ["read"] -> Ok([Read])
- ["READ", "write"] -> Ok([Read, Write]) -- case insensitive
- ["read", "write", "search", "admin"] -> Ok([Read, Write, Search, Admin])
- [] -> Err (empty)
- ["read", "read"] -> Err (duplicate)
- ["read", "READ"] -> Err (case-insensitive duplicate)
- ["unknown"] -> Err
- [""] -> Err

### validate_enroll_params
- Valid target_agent_id -> Ok
- Empty target_agent_id -> Err
- target_agent_id with control chars -> Err
- target_agent_id exceeding max length -> Err
