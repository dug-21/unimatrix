## ADR-002 vnc-043: Description source-of-truth — keep the twin-literal + byte-equality guard, do not collapse

### Context
The subgraph tool contract is documented in two `&str` literals that must agree: the mirror const
`CONTEXT_GRAPH_DESCRIPTION` (`tools.rs:76`, substring-tested) and the live `#[tool(description = "…")]`
attribute literal on the `context_graph` handler (`tools.rs:~3945–3996`, what MCP clients actually
receive). Editing one and not the other re-creates the exact drift the zero-reviewer flagged and that is
the **root cause of #903** — the discoverable contract said `edge_types`/`direction` were "neighbors
only" even though subgraph honors them. vnc-043 edits the subgraph text on two axes (filter availability
+ depth-1-live staleness carve-out), i.e. two changes in each of two literals.

SR-01 asked: collapse the duplicated consts to a single source of truth, **or** add a same-body
invariant test? Investigation of the code settles it:
- rmcp 1.7.0 **cannot consume a `const` inside `#[tool(description = …)]`** — it requires a string
  literal. This is documented in-repo (`tools.rs:6264–6266`) and is the same constraint the
  `context_get` twin hit (#869). A true single-source collapse is therefore not achievable without a
  proc-macro / build-script wrapper — disproportionate for a narrow doc+dispatch fix.
- A same-body invariant test **already exists**: `test_graph_tool_attr_description_matches_const`
  (`tools.rs:6263`, #869) asserts the live macro-generated description is **byte-identical** to the
  mirror const. A single-byte divergence already fails CI.

### Decision
Keep the twin-literal + byte-equality-guard pattern; **do not** introduce a collapse mechanism. Edit
**both** literals identically for both textual changes. The existing #869 guard
(`test_graph_tool_attr_description_matches_const`) is the drift protection — no *new* invariant test is
required, but the spec MUST require (a) both literals edited identically and (b) that guard stays green.

The two textual changes (applied to both literals):
1. **Filter availability** — the subgraph section states `edge_types` and `direction` are honored on
   subgraph (they already are). The `GraphParams` schemars field docs (`graph_read.rs:82` `direction`,
   `:84` `edge_types`) are **single-source** (schemars derive, no mirror) and each edited once to drop
   "neighbors only".
2. **Staleness carve-out** — the subgraph staleness text gains a depth-1 = live / depth>1 = cache split,
   parallel to the neighbors wording (AC-09), replacing the current flat "subgraph mode uses the
   in-memory graph cache for BFS traversal … same staleness contract as neighbors mode at depth>1".

The substring assertions (`tools.rs:6198+`) are **extended** with the new filter-availability and
depth-1-live phrases (AC-13, AC-09); they assert presence, so existing assertions are unaffected. This
does **not** reopen ADR-004 vnc-019 — no `graph_rebuilt_at` field is added; staleness stays text-only.

Open Q4 / SR-04 is resolved negative in-repo: no `.snap`, `insta`, or `schema_for` snapshot pins the
description string or the `GraphParams` schema, so the doc edit cannot red-bar CI on a stale snapshot.

### Consequences
Easier: the discoverability defect that caused #903 is fixed at every surface an agent can read; the
byte-equality guard makes silent re-drift impossible; no new mechanism, no rmcp-version risk, minimal
blast radius.
Harder: two literals must still be hand-edited in lockstep — the guard catches divergence at test time,
not edit time, so a careless edit fails CI rather than being prevented; a future genuine single-source
collapse remains blocked until rmcp supports a const in the attribute (tracked as latent debt, not
in-scope here).
