## ADR-001: `follow_supersessions: bool = true` and the accepted default divergence from `context_graph`

### Context

vnc-042 changes the default behavior of the most-used read tool: `context_get` will
resolve a requested deprecated id to its active terminal **by default** (AC-06). This
requires a new parameter on `GetParams`. Two decisions must be ruled (C-6):

1. **The parameter name and default.** GH #843 originally proposed a `version` enum
   (`"current"` | `"exact"`); the human rejected "version" as implying multiple versions
   of the *command*, and the enum's only cited future value (`"chain"`) is explicitly Out
   of Scope for `context_get` (NG-2 — chain lookback lives in `context_graph` mode
   `chain`). With the third mode ruled out, the behavior is exactly two-valued.

2. **The cross-tool consistency tension (SR-06, must be stated loudly).**
   `context_graph` already exposes `resolve_supersessions: Option<bool>` with **default
   `false`** (`graph_read.rs:84`; unwrap-defaults in subgraph/path handlers; tool docs at
   `tools.rs:96,104`). vnc-042 introduces the *same underlying concept* (follow
   supersession to the active terminal) but with the **opposite default (`true`)**.
   Same concept, near-identical name, opposite default across two tools — a documented
   footgun risk. NG-4 forbids modifying `context_graph`, so the divergence cannot be
   erased by changing the graph tool in this feature.

### Decision

**Add `follow_supersessions: Option<bool>` to `GetParams`, `#[serde(default)]`, semantic
default `true`.**

```rust
/// Resolve a requested deprecated id to its active terminal (vnc-042).
/// - None (omitted) / Some(true) ⇒ DEFAULT-ON: follow superseded_by to the Active terminal.
/// - Some(false) ⇒ escape hatch: return the entry exactly as stored (any status).
#[serde(default)]
pub follow_supersessions: Option<bool>,
```

- Three-state serde mirrors `include_edges` (`None | Some(true) | Some(false)`);
  `#[serde(default)]` makes pre-vnc-042 callers deserialize to the default-on path (C-2,
  AC-06). Keep it a plain `Option<bool>` — no string coercion — so the MCP
  integer/string-serialization fragility class (#3728) is not reintroduced.

**The default divergence from `context_graph` is correct on principle: each tool's default
serves its own dominant intent.** A supersession default should give the caller what the
tool is *for*, and the two tools are for genuinely different things:

- `context_get` is **knowledge retrieval** — the caller wants the *current truth* about an
  entry. The right default is **follow (`true`)**: return the active terminal.
- `context_graph` is **structure / provenance traversal** — the caller wants the graph's
  actual *as-stored* nodes and their real edges. The right default is **as-stored
  (`false`)**: do not silently substitute terminals into the topology.

This is the same "default should serve the caller's intent" principle the human raised on
the original default question, applied per-tool. The opposite defaults are therefore the
*correct* outcome for each tool independently — not accidental debt to be reconciled.

- **Do NOT "unify" this by flipping either default.** Making `context_graph` default to
  follow would silently rewrite provenance topology; making `context_get` default to
  as-stored would reintroduce exactly the silent-stale-read hazard this feature removes.
  Either "consistency fix" is a regression. If a future refactor touches both, it must
  preserve the per-intent defaults.
- The distinct verb `follow_*` (vs `resolve_*`) is a deliberate signal that the default
  differs; the shared noun `supersessions` keeps the concept greppable across the MCP
  surface. Matching the graph name *exactly* (`resolve_supersessions`) while flipping the
  default `false → true` would be the worst outcome: identical name, opposite behavior when
  omitted. A distinct-but-related verb avoids the same-name/opposite-default trap while
  preserving vocabulary linkage.
- The rejected close runner-up — `resolve_supersessions: bool = true` (exact graph
  name-match) — is recorded and declined: it maximizes vocabulary identity at the cost of
  the same-name/opposite-default footgun.
- NG-4 (this feature does not modify `context_graph`) is a secondary scope note, **not** the
  justification for the divergence — the justification is the per-intent principle above.
- The `context_get` tool description (`tools.rs:947-948`) must state the new default and the
  escape hatch (C-5); a description that lies to agents is a known hazard (#4303).

### Consequences

- **Easier:** durable-id callers (memory files, stored docs, edges, prior sessions) get
  current content with zero code change (AC-01/AC-06). The escape hatch
  (`follow_supersessions=false`) keeps audit/lookback/provenance reads exact (AC-03).
- **Harder / risk:** callers that *intentionally* passed a deprecated id expecting
  as-stored content now silently receive the terminal (SR-01). Mitigated by the
  discoverable escape hatch in the tool description. Non-code consumers are outside any
  test harness — accepted as a product bet (SCOPE assumption, LOCKED via #843).
- **Reviewer confusion risk (SR-06):** two tools, one concept, opposite defaults. Mitigated
  by the distinct verb and by documenting the divergence here rather than hiding it.
- See ADR-002 (dead-end path) and ADR-003 (response construction) for how the resolved
  entry is selected and rendered.
