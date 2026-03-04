# Pseudocode: Feature Flag (Cargo.toml)

## Overview

Add a Cargo feature `mcp-briefing` (default on) to gate the `context_briefing` MCP tool. BriefingService itself is NOT gated — it is always available for UDS transport.

## Cargo.toml Changes

```toml
# AFTER [dev-dependencies] section, add:

[features]
default = ["mcp-briefing"]
mcp-briefing = []
```

The feature is empty (no dependency activation). It is purely a compilation gate for the `context_briefing` method in tools.rs.

## tools.rs Gate

```rust
#[cfg(feature = "mcp-briefing")]
#[tool(
    name = "context_briefing",
    description = "..."
)]
async fn context_briefing(...) -> ... { ... }
```

### rmcp Compatibility Assessment

The rmcp `#[tool]` macro generates tool registration at the method level. The `#[cfg(feature = "mcp-briefing")]` attribute placed BEFORE the `#[tool]` attribute should compile out the entire method when the feature is disabled, which means the tool registration code is also absent.

**Verification needed during implementation**: Build with `cargo build --no-default-features -p unimatrix-server` and confirm:
1. Compilation succeeds
2. The generated tool list has one fewer entry (no context_briefing)

**Fallback approach (per ADR-001)**: If rmcp does not support `#[cfg]` on `#[tool]` methods, create a wrapper:
```rust
#[tool(name = "context_briefing", description = "...")]
async fn context_briefing(&self, ...) -> Result<CallToolResult, rmcp::ErrorData> {
    #[cfg(not(feature = "mcp-briefing"))]
    {
        return Ok(CallToolResult::error(vec![Content::text(
            "context_briefing tool is not available in this build"
        )]));
    }
    #[cfg(feature = "mcp-briefing")]
    {
        // ... actual implementation ...
    }
}
```

## Compilation Configurations

| Configuration | context_briefing | BriefingService | UDS Briefing |
|--------------|-----------------|-----------------|-------------|
| `cargo build` (default) | Available | Available | Available |
| `cargo build --no-default-features` | Absent | Available | Available |

## Test Configurations

Both configurations must compile and pass their respective tests:
- Default: all tests pass including context_briefing MCP tests
- No default features: all tests except MCP-specific briefing tests pass

MCP-specific test code that references `context_briefing` should be gated with `#[cfg(feature = "mcp-briefing")]` if needed.

## format_briefing and Briefing struct

These are NOT gated. They are used by the MCP tool but could also be useful for other formatting contexts. If dead-code warnings appear when mcp-briefing is disabled, they can be gated with `#[cfg(feature = "mcp-briefing")]` but this is a secondary concern.

Alternatively, if `format_briefing` and `Briefing` are only used by `context_briefing`, gate them too:
```rust
#[cfg(feature = "mcp-briefing")]
pub struct Briefing { ... }

#[cfg(feature = "mcp-briefing")]
pub fn format_briefing(...) -> CallToolResult { ... }
```

This is the cleanest approach and should be preferred if no other code uses these types.
