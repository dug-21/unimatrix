# Component: doc-surfaces

Files: `crates/unimatrix-server/src/mcp/graph_read.rs`, `crates/unimatrix-server/src/mcp/tools.rs`

## Purpose

Correct the discoverable `context_graph` contract so agents can find that `edge_types`/`direction`
filtering is available on `subgraph` mode (it already ships since vnc-019; the docs mis-state it as
"neighbors only" — the literal root cause of #903), and disclose the depth-1-live / depth>1-cache
staleness split in the subgraph description (parallel to neighbors). Text-only. No `graph_rebuilt_at`
field (ADR-004 vnc-019 preserved).

Four edit points across two files (ADR-002 / SR-01). The two `tools.rs` literals are byte-identical
twins guarded by `test_graph_tool_attr_description_matches_const` (#869) — edit both identically.

---

## Edit point 1 — `direction` schemars doc (`graph_read.rs:82`)

Current:
```
/// chain: "forward"|"backward"|"both"; neighbors: "incoming"|"outgoing"|"both".
pub direction: Option<String>,
```

Change: add `subgraph` to the modes `direction` applies to (subgraph accepts incoming/outgoing/both,
same value set as neighbors). Drop any implication it is chain/neighbors-only. Single-source (schemars
derive) — edited once, no mirror, no byte-equality guard (FR-2 / AC-13).

Intent (exact wording is the implementer's; must convey): `direction` applies to chain, neighbors,
AND subgraph; for neighbors/subgraph the values are incoming/outgoing/both.

---

## Edit point 2 — `edge_types` schemars doc (`graph_read.rs:84`)

Current:
```
/// neighbors only: edge types to traverse (absent/[] = all except Supersedes).
pub edge_types: Option<Vec<String>>,
```

Change: drop "neighbors only"; state `edge_types` applies to neighbors AND subgraph (and path, which
already documents it). Keep the "absent/[] = all except Supersedes" semantics unchanged. Single-source
(schemars derive) — edited once (FR-1 / AC-13).

Intent: `edge_types` applies to neighbors, subgraph, and path; absent/`[]` = all except Supersedes.

---

## Edit points 3 & 4 — twin description literals (`tools.rs`), EDITED IDENTICALLY

Two literals hold the same body and MUST stay byte-identical (ADR-002 / SR-01):
- `CONTEXT_GRAPH_DESCRIPTION` mirror const — `tools.rs:76` (subgraph section `:89–100`).
- live `#[tool(description = "…")]` attribute literal on `context_graph` — `tools.rs:~3944`
  (subgraph section `:3957–3968`).

The subgraph section currently:
- says nothing about `edge_types`/`direction` being honored on subgraph;
- has a flat staleness line: "subgraph mode uses the in-memory graph cache for BFS traversal. The
  cache is rebuilt each tick (typically 30-60 seconds). Edges written within the current tick interval
  may not appear in the result. This is the same staleness contract as neighbors mode at depth>1."

### Two textual changes to the subgraph section (apply to BOTH literals, byte-identically)

1. **Filter availability (FR-3 / AC-13).** State that `subgraph` accepts an `edge_types` filter and a
   `direction` (incoming/outgoing/both) filter, honored on traversal — mirroring the neighbors wording.
   Keep the existing `direction:"outgoing"` canonical-label paragraph (`:89–93`) intact; it explains
   the label invariant and must remain.

2. **Staleness carve-out (FR-4 / AC-09).** Replace the flat staleness line with the depth-1-live /
   depth>1-cache split, parallel to the neighbors wording:
   - subgraph `max_depth == 1` reads the live database and reflects all committed writes immediately;
   - subgraph `max_depth > 1` reads the in-memory graph cache, which may lag recent writes by up to one
     tick interval (typically 30-60 seconds).

### Hard constraints on the edit

- Both literals get the **identical** new body. Escaped inner quotes (`\"outgoing\"`), line
  continuations (`\`), and `\n\` mode separators must match the surrounding literal's style in each file.
- Do NOT add or reference a `graph_rebuilt_at` field (assertion at `tools.rs:6252` forbids it).
- Preserve every existing subgraph fact the substring test already asserts (`direction:"outgoing"`,
  `depth_reached`, `truncated`, `empty result`, `200`, `values above 200 are rejected`) — the edit
  ADDS text, it must not delete these phrases.
- `test_graph_tool_attr_description_matches_const` (`tools.rs:6263`, #869) must stay green after both
  edits — this is the drift guard; it is not modified.

---

## Edit point 5 (test) — extend substring assertions (`tools.rs:6198+`)

In the substring test (the `assert!(description.contains(...))` block spanning `:6199–6259`), ADD
assertions for the two new semantic facts so their presence is CI-pinned (AC-13 / AC-09, ADR-002):

```
# filter availability on subgraph (FR-3/AC-13)
assert!(description mentions edge_types + direction honored on subgraph)
# depth-1 live / depth>1 cache staleness split for subgraph (FR-4/AC-09)
assert!(description states subgraph max_depth==1 reads live / all committed writes visible)
assert!(description states subgraph max_depth>1 reads the cache / tick-window lag)
```

Assert on stable substrings of the exact wording chosen for edits 3&4. These assert presence only —
existing assertions are unaffected. Since the test reads `CONTEXT_GRAPH_DESCRIPTION` (the const) and
the #869 guard proves the live literal is byte-identical, these substrings transitively pin BOTH
literals.

The two schemars docs (edits 1&2) are NOT covered by the byte-equality guard — verify them explicitly
with a doc-string-presence or schema-doc check (R-07 coverage requirement; the tester places it).

---

## Data flow

None (compile-time string constants + schemars-derived field docs). No runtime behavior; the wire
schema shape is unchanged (schemars descriptions are metadata only). No `GraphParams` shape change.

## Error handling

N/A — documentation. The only failure surface is CI: a divergence between the twin literals fails
`test_graph_tool_attr_description_matches_const`; a missing new phrase fails the extended substring test.

## Key test scenarios (hints for tester)

- `test_graph_tool_attr_description_matches_const` (#869) green — two literals byte-identical after
  both edits (R-07/AC-13).
- Extended substring test asserts BOTH the filter-availability text AND the depth-1-live/depth>1-cache
  staleness text are present (R-07/AC-09/AC-13).
- Explicit presence check on the `edge_types` (`graph_read.rs:84`) and `direction` (`:82`) schemars
  docs — these two are NOT byte-equality-guarded (R-07).
- Negative: description still does NOT contain `graph_rebuilt_at` (ADR-004, existing assert `:6252`).
- All four edit points changed — no point left to manual sync (R-07 coverage requirement).
