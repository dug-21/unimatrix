# Test Plan: Feature Flag (Cargo.toml)

## Test Scenarios

### T-FF-01: Default build has context_briefing tool (AC-26)
```
Run: cargo build -p unimatrix-server
Assert: Build succeeds
Assert: context_briefing tool method present in binary

Run: cargo test -p unimatrix-server
Assert: All tests pass (including MCP briefing tests)
```

### T-FF-02: --no-default-features build succeeds without context_briefing (AC-25, AC-27)
```
Run: cargo build --no-default-features -p unimatrix-server
Assert: Build succeeds with zero errors
Assert: context_briefing method compiled out

Run: cargo test --no-default-features -p unimatrix-server
Assert: Tests pass (MCP-specific briefing tests gated out)
```

### T-FF-03: BriefingService available in both configurations (AC-27)
```
Both build configurations must have BriefingService available.
BriefingService is NOT gated behind feature flag.
The UDS transport uses BriefingService regardless of feature flag.

Verify: grep -n "cfg.*mcp.briefing" services/briefing.rs
Assert: No feature gate on BriefingService
```

### T-FF-04: mcp-briefing feature defined correctly (AC-17)
```
Method: grep -A2 "mcp-briefing" Cargo.toml
Assert: Feature defined in [features] section
Assert: Feature listed in "default" array
Assert: Feature has no dependency activations (empty: [])
```

### T-FF-05: context_briefing method gated (AC-16)
```
Method: grep -n "cfg.*mcp.briefing" tools.rs
Assert: #[cfg(feature = "mcp-briefing")] present on context_briefing method
```

## Build Matrix

| Configuration | Command | Expected |
|--------------|---------|----------|
| Default | `cargo build -p unimatrix-server` | Success, all tools |
| Default tests | `cargo test -p unimatrix-server` | All pass |
| No features | `cargo build --no-default-features -p unimatrix-server` | Success, no context_briefing |
| No features tests | `cargo test --no-default-features -p unimatrix-server` | Pass (gated tests skipped) |
| Workspace build | `cargo build --workspace` | Success |
| Workspace test | `cargo test --workspace` | All pass |

## Risk Coverage

| Risk | Test(s) | Status |
|------|---------|--------|
| R-03 (Feature flag compatibility) | T-FF-01, T-FF-02, T-FF-03 | Covered |
