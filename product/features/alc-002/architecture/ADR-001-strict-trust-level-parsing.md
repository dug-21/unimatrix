## ADR-001: Strict Trust Level Parsing

### Context

The `context_enroll` tool accepts `trust_level` as a string parameter from MCP clients. The existing `TrustLevel` enum (System, Privileged, Internal, Restricted) has specific security semantics — each level maps to different default capabilities in the trust hierarchy.

SR-04 from the scope risk assessment identified that invalid or adversarial trust_level strings could bypass the hierarchy if parsing uses a fallback default.

### Decision

Trust level parsing is strict and exhaustive:
- Case-insensitive matching: "system" | "privileged" | "internal" | "restricted"
- Unknown values return `ServerError::InvalidInput` with a clear error listing valid options
- No fallback to a default trust level on parse failure
- Same approach for capability parsing: "read" | "write" | "search" | "admin"

The `parse_trust_level()` and `parse_capabilities()` functions are placed in `validation.rs` alongside existing validators, keeping parsing logic co-located with validation.

### Consequences

- Safer: no way for an invalid string to silently map to a trust level
- Stricter: clients must pass exact strings (case-insensitive). Typos like "admin" in trust_level field are rejected, not silently mapped.
- The MCP tool schema (via JsonSchema derive) documents valid values, but runtime validation is the enforcement boundary.
