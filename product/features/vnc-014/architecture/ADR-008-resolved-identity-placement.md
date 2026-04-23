## ADR-008: ResolvedIdentity crate placement — unimatrix-server only

### Context

`build_context_with_external_identity()` accepts `Option<&ResolvedIdentity>` as its Seam 2
forward-compatibility parameter. The open question (OQ-A from the specification, WARN-1 from
the vision guardian) was: should `ResolvedIdentity` live in `unimatrix-server` or be promoted
to `unimatrix-core` for potential cross-crate sharing in the Wave 2-3 bearer-auth feature?

`ResolvedIdentity` already exists at `crates/unimatrix-server/src/mcp/identity.rs` — it was
placed there during an earlier design pass. The question is whether to move it before vnc-014
ships or leave it in place.

W2-3 (bearer-auth / Seam 2 activation) is not yet scoped. There is no concrete evidence today
that W2-3 will require `ResolvedIdentity` from outside `unimatrix-server`. The only use of
this type is on the server's tool-dispatch path (`build_context_with_external_identity`), which
is inherently a server-layer concern.

### Decision

`ResolvedIdentity` stays in `unimatrix-server` (`mcp/identity.rs`). It is not moved to
`unimatrix-core` for vnc-014.

If W2-3 requires it from another crate, the migration path is a one-line `pub use` re-export
from `unimatrix-server` into `unimatrix-core`, with no API break for existing callers. That
migration can be a W2-3 task if and only if a concrete cross-crate caller materialises.

Minimal scope is the correct default. Promoting a type to a shared crate before any concrete
second consumer exists creates unnecessary coupling and forces all crates that depend on
`unimatrix-core` to compile-time-depend on bearer-auth concepts that are irrelevant to them.

### Consequences

**Easier**:
- vnc-014 has no crate boundary changes; `unimatrix-core` is untouched.
- `ResolvedIdentity` can evolve freely in `unimatrix-server` without affecting the core crate's
  stability guarantees.
- The Seam 2 parameter (`Option<&ResolvedIdentity>`) compiles and links immediately — no
  refactoring needed to unblock delivery.

**Harder**:
- If W2-3 needs `ResolvedIdentity` from a non-server crate, a `pub use` re-export migration
  is required at that time (low effort, no API break).
- Future developers must remember that the type lives in the server crate, not in core, which
  may be mildly surprising if they expect all shared domain types in `unimatrix-core`.

**OQ-A status**: Closed. `unimatrix-server/mcp/identity.rs` is the authoritative location.
