# Test Plan: validation component

## Unit Tests

### parse_trust_level

#### test_parse_trust_level_system
- Input: "system" -> Ok(TrustLevel::System)

#### test_parse_trust_level_privileged
- Input: "privileged" -> Ok(TrustLevel::Privileged)

#### test_parse_trust_level_internal
- Input: "internal" -> Ok(TrustLevel::Internal)

#### test_parse_trust_level_restricted
- Input: "restricted" -> Ok(TrustLevel::Restricted)

#### test_parse_trust_level_case_insensitive
- Input: "SYSTEM" -> Ok(TrustLevel::System)
- Input: "Privileged" -> Ok(TrustLevel::Privileged)

#### test_parse_trust_level_invalid_admin
- Input: "admin" -> Err(InvalidInput) -- R-04: "admin" is a capability, not a trust level

#### test_parse_trust_level_empty
- Input: "" -> Err(InvalidInput)

#### test_parse_trust_level_trailing_space
- Input: "system " -> Err(InvalidInput) -- R-04: strict matching

#### test_parse_trust_level_unknown
- Input: "superadmin" -> Err(InvalidInput)

### parse_capabilities

#### test_parse_capabilities_single
- Input: ["read"] -> Ok([Capability::Read])

#### test_parse_capabilities_all_four
- Input: ["read", "write", "search", "admin"] -> Ok([Read, Write, Search, Admin])

#### test_parse_capabilities_case_insensitive
- Input: ["READ", "Write"] -> Ok([Read, Write])

#### test_parse_capabilities_empty_vec
- Input: [] -> Err(InvalidInput) -- R-09

#### test_parse_capabilities_duplicate
- Input: ["read", "read"] -> Err(InvalidInput) -- R-05

#### test_parse_capabilities_case_insensitive_duplicate
- Input: ["read", "READ"] -> Err(InvalidInput) -- R-05

#### test_parse_capabilities_unknown
- Input: ["unknown"] -> Err(InvalidInput)

#### test_parse_capabilities_empty_string
- Input: [""] -> Err(InvalidInput) -- R-09

### validate_enroll_params

#### test_validate_enroll_params_valid
- Valid params -> Ok(())

#### test_validate_enroll_params_empty_target
- target_agent_id: "" -> Err(InvalidInput)

#### test_validate_enroll_params_control_chars
- target_agent_id: "agent\x00bad" -> Err(InvalidInput) -- R-11

#### test_validate_enroll_params_max_length
- target_agent_id of 100 chars -> Ok
- target_agent_id of 101 chars -> Err(InvalidInput)

## Risk Coverage

| Risk | Tests |
|------|-------|
| R-04 | parse_trust_level_* (all 9 tests) |
| R-05 | parse_capabilities_duplicate, parse_capabilities_case_insensitive_duplicate |
| R-09 | parse_capabilities_empty_vec, parse_capabilities_empty_string |
| R-11 | validate_enroll_params_control_chars, validate_enroll_params_empty_target, validate_enroll_params_max_length |
