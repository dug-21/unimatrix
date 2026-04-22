# Component: Capability::as_audit_str (infra/registry.rs)

## Purpose

Add a new method `as_audit_str()` to the `Capability` enum that returns
the canonical lowercase string used in `AuditEvent.capability_used`. This
method is the single source of truth for these strings, preventing silent
drift across 12+ call sites.

`Capability` is defined in `unimatrix-store/src/schema.rs` (where `AgentRecord`,
`TrustLevel`, and `Capability` live). It is re-exported from
`unimatrix-server/src/infra/registry.rs` via `pub use unimatrix_store::{AgentRecord, Capability, TrustLevel}`.

The method is added to the type definition in `unimatrix-store/src/schema.rs`
(ADR-006). The re-export in `infra/registry.rs` makes it accessible to tool
handlers without an import change.

**File modified:** `crates/unimatrix-store/src/schema.rs`

---

## New Method

### `Capability::as_audit_str` (schema.rs — on the Capability enum)

```
impl Capability {
    /// Returns the canonical lowercase string for AuditEvent.capability_used.
    ///
    /// Exhaustive match — no wildcard arm. Adding a new Capability variant
    /// without updating this method produces a compile error (ADR-006, SR-05).
    pub fn as_audit_str(&self) -> &'static str {
        match self {
            Capability::Search => "search",
            Capability::Read   => "read",
            Capability::Write  => "write",
            Capability::Admin  => "admin",
        }
    }
}
```

**No wildcard arm.** This is load-bearing: the compile error on a missing
arm is the mechanism that prevents silent `capability_used` drift when
new variants are added in future features.

The `#[deny(unreachable_patterns)]` attribute MAY be added to the match
for belt-and-suspenders enforcement, but the exhaustive match itself is
sufficient.

---

## Capability-to-Tool Mapping (for reference by tools.md)

| `Capability` variant | `as_audit_str()` | Tools that use it |
|----------------------|-----------------|-------------------|
| `Capability::Search` | `"search"` | `context_search`, `context_lookup`, `context_briefing` |
| `Capability::Read`   | `"read"`   | `context_get`, `context_status`, `context_retrospective` |
| `Capability::Write`  | `"write"`  | `context_store`, `context_correct`, `context_deprecate`, `context_quarantine`, `context_cycle` |
| `Capability::Admin`  | `"admin"`  | `context_enroll` |

Note: `context_lookup` uses `Capability::Read` for its capability gate check
(see `require_cap(&ctx.agent_id, Capability::Read)` in the current handler).
The specification maps it to `Capability::Search` ("search"). Verify against
the current `require_cap` call in `context_lookup` — use whatever capability
is actually checked there. If the gate is `Read`, the audit string is `"read"`.
Flag this discrepancy for the delivery agent to confirm.

---

## Error Handling

This method cannot fail — it is a pure `&'static str` match returning a
compile-time constant. No error paths.

---

## Key Test Scenarios

1. **Exhaustive coverage (AC-11, R-09)**: Unit test each of the four variants:
   - `assert_eq!(Capability::Search.as_audit_str(), "search")`
   - `assert_eq!(Capability::Read.as_audit_str(), "read")`
   - `assert_eq!(Capability::Write.as_audit_str(), "write")`
   - `assert_eq!(Capability::Admin.as_audit_str(), "admin")`

2. **No wildcard (R-09)**: Add `#[deny(unreachable_patterns)]` to the match
   in the implementation to make this enforced at compile time. Alternatively,
   document that the match is exhaustive in a code comment.

3. **Compile enforcement**: Confirm the method is accessible from `tools.rs`
   via the existing `use crate::infra::registry::Capability` import (no new
   imports needed because the method is on the type itself).
