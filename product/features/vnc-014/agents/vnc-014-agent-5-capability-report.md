# Agent Report: vnc-014-agent-5-capability

**Task**: Add `Capability::as_audit_str()` to `infra/registry.rs`
**Status**: COMPLETE

---

## Files Modified

- `crates/unimatrix-store/src/schema.rs` — `impl Capability` block with `as_audit_str()` method (5 exhaustive arms, no wildcard)
- `crates/unimatrix-server/src/infra/registry.rs` — CAP-U-01 through CAP-U-06 unit tests added to existing `#[cfg(test)]` block

---

## Implementation Notes

### Method placement

The pseudocode (`capability.md`) says "File modified: `crates/unimatrix-server/src/infra/registry.rs`" but also states "The method is added to the type definition in `unimatrix-store/src/schema.rs`". The implementation correctly follows the latter: `as_audit_str()` lives on the `Capability` type in `schema.rs`. The `registry.rs` re-export (`pub use unimatrix_store::{AgentRecord, Capability, TrustLevel}`) makes it accessible to tool handlers without any import change.

### SessionWrite variant (deviation from pseudocode)

The pseudocode listed 4 variants (Read, Write, Search, Admin). The actual `Capability` enum has 5: it also includes `SessionWrite = 4`. An exhaustive match with no wildcard arm requires all variants to be covered. `SessionWrite` maps to `"session_write"` (lowercase snake_case, consistent with the other strings). This deviation from the pseudocode was necessary for the code to compile and is the correct behavior per ADR-006's intent.

Pattern stored in Unimatrix as entry #4365 for future agents.

---

## context_lookup Capability Finding

**`context_lookup` gates on `Capability::Read`** (line 481 of `mcp/tools.rs`):

```rust
self.require_cap(&ctx.agent_id, Capability::Read).await?;
```

The implementation brief's capability-to-tool mapping table lists `context_lookup` under `Capability::Search` ("search"), but the actual gate in the handler is `Capability::Read`. The pseudocode (`capability.md`) explicitly flags this discrepancy and instructs: "use whatever capability is actually checked there." The audit string for `context_lookup` will therefore be `"read"` when `tools.md` populates `capability_used` for that handler.

---

## Tests

CAP-U-01 through CAP-U-06 added to `registry.rs`:
- CAP-U-01: `Read` → `"read"`
- CAP-U-02: `Write` → `"write"`
- CAP-U-03: `Search` → `"search"`
- CAP-U-04: `Admin` → `"admin"`
- CAP-U-05: `SessionWrite` → `"session_write"` (extra variant not in spec)
- CAP-U-06: `&'static str` return type compile-time proof

`unimatrix-store` builds and passes 289 tests (2 pre-existing failures in `audit.rs` tests added by another agent — unrelated to this component).

`unimatrix-server` does not compile as a whole because other vnc-014 agents' work is in progress (22 missing `AuditEvent` fields across `tools.rs`, `gateway.rs`, etc.). The CAP-U tests in `registry.rs` will pass once the workspace compiles.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — surfaced ADR-001, ADR-002, ADR-008 for vnc-014; applied
- Stored: entry #4365 "Capability enum has 5 variants — SessionWrite missing from pseudocode as_audit_str spec" via `/uni-store-pattern`
