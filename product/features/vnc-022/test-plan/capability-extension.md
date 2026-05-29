# Test Plan: capability-extension

Component: `crates/unimatrix-server/src/http/auth.rs` (StaticTokenValidator) + `crates/unimatrix-server/src/uds/mod.rs` (UDS_CAPABILITIES)

Covers: AC-06 (indirectly), R-06

## Unit Tests

Location: `crates/unimatrix-server/src/http/auth/tests.rs` (extend existing module)

### test_static_token_validator_includes_session_write

Arrange: `StaticTokenValidator::new(test_token_bytes())`
Act: `validator.validate(&test_token_hex()).await`
Assert:
- `identity.capabilities` contains `Capability::SessionWrite`
- Full set is `[Read, Write, Search, SessionWrite]`
- `identity.capabilities.len() == 4`

Note: Existing test `test_bearer_validator_trait_valid_token` asserts `capabilities == vec![Read, Write, Search]` and `caps:3`. This test MUST be updated to expect 4 capabilities. This is intentional — it documents the capability set change.

### test_static_token_validator_capabilities_complete_set

Arrange: `StaticTokenValidator::new(test_token_bytes())`
Act: `validator.validate_sync(&test_token_hex())`
Assert:
- `identity.capabilities.contains(&Capability::Read)`
- `identity.capabilities.contains(&Capability::Write)`
- `identity.capabilities.contains(&Capability::Search)`
- `identity.capabilities.contains(&Capability::SessionWrite)`
- `!identity.capabilities.contains(&Capability::Admin)` — Admin must NOT be present

### test_identity_extension_caps_count_updated

The existing test `test_valid_token_inserts_resolved_identity_into_extensions` (T-STA-08) asserts `"caps":3`. After adding SessionWrite, this MUST be updated to assert `"caps":4`. If not updated, it will fail — catching the missing capability addition.

## UDS Capability Verification

Location: `crates/unimatrix-server/src/uds/mod.rs` (existing tests)

### Existing tests (must pass unchanged)

- `test_uds_capabilities_exact_set` — asserts `[Read, Search, SessionWrite]`
- `test_uds_has_capability_session_write` — asserts `true`
- `test_uds_has_capability_write_false` — asserts `false`

These tests verify UDS capabilities are NOT modified by the HTTP capability change. UDS still does NOT have `Capability::Write`.

## Risk Trace

| Risk | Scenario | Test |
|------|----------|------|
| R-06 | SessionWrite missing from HTTP capabilities | test_static_token_validator_includes_session_write |
| R-06 | Admin accidentally granted | test_static_token_validator_capabilities_complete_set |
| R-02 | UDS capabilities accidentally modified | Existing UDS tests unchanged |
