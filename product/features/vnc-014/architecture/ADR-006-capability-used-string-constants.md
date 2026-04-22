## ADR-006: capability_used Field — Capability Enum Derived String Constants

### Context

The `capability_used` column records the capability gate evaluated for each tool call. SR-05
identifies a risk: if each tool handler supplies a free-form string, values will diverge across
tools over time (e.g., one handler writes `"Write"`, another writes `"write"`, another `"WRITE"`).

The `Capability` enum in `infra/registry.rs` is the canonical source of capability names. The
options for deriving `capability_used` strings are:

(A) Free-form strings at each call site — fragile, diverges over time. Rejected.

(B) `Capability::as_str()` method on the enum — derivable, consistent, but adds a method to the
    enum that doesn't exist today.

(C) Module-level string constants in a shared location (e.g., `infra/registry.rs` or a new
    `mcp/capability_names.rs`) — explicit, searchable, auditable.

(D) `format!("{:?}", capability)` or `Display` on the enum — works but ties the audit string
    to the Debug/Display representation, which can change independently.

Option (B) is the most direct: add `fn as_audit_str(&self) -> &'static str` to the `Capability`
enum. Each variant returns its lowercase string form (`Read` -> `"read"`, `Write` -> `"write"`,
`Search` -> `"search"`, `Admin` -> `"admin"`). All call sites use
`Capability::Write.as_audit_str()` — no free-form strings at call sites.

The string form is lowercase-hyphenated to match the `clientInfo.name` convention and existing
agent_id conventions in the codebase.

### Decision

Add `fn as_audit_str(&self) -> &'static str` to the `Capability` enum:

```rust
impl Capability {
    pub fn as_audit_str(&self) -> &'static str {
        match self {
            Capability::Read   => "read",
            Capability::Write  => "write",
            Capability::Search => "search",
            Capability::Admin  => "admin",
        }
    }
}
```

All `AuditEvent` construction sites in `tools.rs` use `Capability::X.as_audit_str()` for the
`capability_used` field. Non-tool-call construction sites (background, UDS listener) that do not
gate on a capability use `""` (empty string sentinel, consistent with the column DEFAULT).

### Consequences

Easier:
- Canonical, consistent audit strings across all tools
- Refactoring `Capability` variant names does not silently change audit strings (the `as_audit_str`
  match would also need updating — a compile-time-guided change)
- Human-readable in SQL queries and compliance exports

Harder:
- All 12 tool handlers must supply this at AuditEvent construction (O(n) change, same as the
  other four-field additions)
- If new `Capability` variants are added in future features, `as_audit_str` must be extended
  (the exhaustive match will fail to compile, which is the desired enforcement)
