## ADR-003: graph_expand Security Comment — Inline // SECURITY: at Signature

### Context

`graph_expand.rs` already contains a full quarantine obligation doc block in the module header (lines 12-18, "Caller Quarantine Obligation (FR-06)"). The crt-042 security review identified Finding 1: the obligation is invisible when hovering over the function in an IDE, because the module-header doc block is not attached to the function signature.

Three approaches were considered:

1. **`#[doc]` attribute on the function** with quarantine obligation text. Rejected by SR-04 analysis: a `#[doc]` attribute containing the security obligation text is visible in rustdoc but is still not prominently visible at call sites in an IDE without explicitly hovering. It also duplicates the existing module-level doc block, increasing maintenance surface.

2. **Unit test asserting the quarantine call exists at the call site** (search.rs). Rejected: this tests search.rs behavior, not the function contract. A future caller in a different file would not benefit from a test scoped to search.rs. The test would also be brittle (name-matching on function calls).

3. **Inline `// SECURITY:` comment at the function signature**. Selected. This places the obligation directly at the call site in IDE navigation (hovering the function name in any caller shows the leading comments). The `// SECURITY:` prefix is already established in this codebase at `graph_enrichment_tick.rs:155` for SQL injection prevention. Using the same prefix makes security obligations consistently discoverable. No logic change. No new maintenance surface — the comment is a one-line obligation marker, not a documentation duplicate.

SR-04 risk: the comment text could diverge from actual SecurityGateway call sites over time. This risk is accepted: the comment establishes an obligation contract, not an implementation assertion. The existing module-header doc block and the call site implementation in search.rs together enforce the actual behavior. The inline comment is a visibility aid, not the sole obligation carrier.

### Decision

Add a two-line `// SECURITY:` comment immediately before the `pub fn graph_expand(` signature in `graph_expand.rs`:

```rust
// SECURITY: caller MUST apply SecurityGateway::is_quarantined() before inserting
// returned IDs into result sets. graph_expand performs NO quarantine filtering.
pub fn graph_expand(
```

No change to any other line. No change to function behavior, BFS logic, or module-level doc block.

### Consequences

- The quarantine obligation is visible at every IDE call site via hover — Finding 1 from the crt-042 security review is resolved.
- The `// SECURITY:` prefix is now used for two different obligation types in this codebase (SQL injection prevention in run_s2_tick; quarantine obligation in graph_expand). The prefix functions as a searchable marker for security-relevant obligations regardless of type.
- No test can verify the comment text remains accurate — this is accepted as per the SR-04 analysis above. The doc block and call-site implementation remain the authoritative obligation carriers.
- Zero risk of behavioral regression: the change is documentation-only.
