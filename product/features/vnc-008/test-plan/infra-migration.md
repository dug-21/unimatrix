# Test Plan: infra-migration

## Risk Coverage: R-01, R-06, R-10

## Tests

### T-INFRA-01: Compilation after move
- **Type**: Build verification
- **Command**: `cargo check --workspace`
- **Expected**: Zero errors
- **Risk**: R-01

### T-INFRA-02: Test count preserved
- **Type**: Test count comparison
- **Command**: `cargo test --workspace`
- **Expected**: >= 1,664 tests passed, 0 failed
- **Risk**: R-06

### T-INFRA-03: Re-exports resolve correctly
- **Type**: Compilation verification
- **Expected**: `use unimatrix_server::audit::*` still resolves via lib.rs re-export
- **Risk**: R-10

### T-INFRA-04: Module tests pass in new location
- **Type**: Targeted test run
- **Command**: `cargo test -p unimatrix-server audit:: registry:: session:: scanning:: validation:: categories:: contradiction:: coherence:: pidfile:: shutdown:: embed_handle:: usage_dedup:: outcome_tags::`
- **Expected**: All module-internal tests pass
- **Risk**: R-06

### T-INFRA-05: No infra module imports from services/mcp/uds
- **Type**: Grep verification
- **Command**: `grep -r 'use crate::services' src/infra/ && grep -r 'use crate::mcp' src/infra/ && grep -r 'use crate::uds' src/infra/`
- **Expected**: No matches (zero output)
- **Risk**: R-07
