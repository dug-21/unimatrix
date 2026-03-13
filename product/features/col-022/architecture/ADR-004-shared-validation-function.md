## ADR-004: Shared Validation Function for MCP Tool and Hook Handler

### Context

SR-07 identifies a split-brain validation risk: the MCP tool (`context_cycle`) validates parameters on the server side, while the hook handler validates on the hook process side. Two independent validation implementations can diverge over time, leading to cases where the hook accepts parameters the MCP tool rejects (or vice versa).

The MCP tool and hook handler run in different processes (hook is synchronous, server is async tokio), but they share the same `unimatrix-server` crate codebase.

### Decision

Extract a single `validate_cycle_params()` function in `unimatrix-server/src/infra/validation.rs` (where existing validation helpers live). Both the MCP tool handler and the hook `build_request()` function call this same function.

```rust
pub fn validate_cycle_params(
    type_str: &str,
    topic: &str,
    keywords: Option<&[String]>,
) -> Result<ValidatedCycleParams, String>
```

The function:
1. Validates `type_str` is "start" or "stop", maps to `CycleType` enum
2. Validates `topic` via `sanitize_metadata_field()` + structural check (non-empty, max 128 chars, valid feature ID characters)
3. Validates `keywords`: each string max 64 chars (truncate individual strings), max 5 items (truncate array), filter empty strings
4. Returns `ValidatedCycleParams { cycle_type, topic, keywords }` on success

The hook handler calls this function and, on validation failure, falls through to the generic `RecordEvent` path (the event still gets recorded as an observation, but without the cycle_start special handling). This preserves the hook's "never fail" contract.

The MCP tool calls this function and, on validation failure, returns a structured error to the agent.

### Consequences

**Easier:**
- SR-07 fully resolved: single source of truth for validation rules.
- Adding new validation rules (e.g., keyword character restrictions) requires one change.
- Testable in isolation with unit tests on the validation function.

**Harder:**
- The hook handler must depend on `validation.rs` module, creating a compile-time coupling. This is acceptable: the hook handler already lives in the same crate.
- The hook process must not pull in heavy async dependencies through this path. `validate_cycle_params` is pure computation with no I/O, so this is safe.
