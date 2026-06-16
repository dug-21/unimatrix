## ADR-003 (vnc-037): Serializer seam — get-only edge rendering with a `None ⇒ key absent` byte-identity invariant

### Context

The single-entry rendering helpers — `format_single_entry` (`response/entries.rs:13`),
`entry_to_json` (`response/mod.rs:121`), `format_entry_markdown_section`
(`response/mod.rs:160`) — are **shared** by `context_search`, `context_lookup`,
`context_store`, and `context_correct`. D-07 mandates that only `context_get` surfaces
edges; the four list-view tools' payloads must remain **byte-identical** to pre-vnc-037
(no `edges` key, no `### Related` section). SR-01 (High) flags exactly the failure mode:
#3449 — a formatter that silently omits an `Option` field with `if let Some(..)` / no
else produces invisible regressions. The risk is that threading an `edges: Option<…>`
argument through the shared helpers lets a future edit, or a `Some` accidentally passed by
a list-view caller, change those four payloads.

### Decision

Make `None ⇒ key absent` **structural**, not a runtime convention that a later edit can
break, by keeping the edge rendering on the get path and leaving the shared helpers'
existing output paths untouched:

1. **`entry_to_json` signature is UNCHANGED.** It keeps emitting exactly its pre-vnc-037
   object for all callers. The get path calls `entry_to_json(entry)` to get the base
   object, then — *only when edges were surfaced* — inserts the `edges` array and the
   `edge_totals` object onto that JSON value. No list-view caller ever reaches that
   insertion. There is no `edges` key to omit because for list views the key is never
   added in the first place.

2. **`format_entry_markdown_section` is UNCHANGED.** The `### Related` section is appended
   by the get handler *after* the shared helper returns, only when edges were surfaced.

3. **`format_single_entry` gains `edges: Option<&EdgesView>`** as a third parameter and is
   the *only* place the three formats branch on edges. `None` (the value every list-view
   path effectively produces, and the value the get path passes on opt-out) yields the
   pre-vnc-037 result verbatim for all three formats. `Some(view)` adds the summary digest
   / markdown `### Related` / json `edges`+`edge_totals`.

4. **Byte-identity is a tested invariant, not a comment.** The spec/tester must assert that
   `context_search` / `context_lookup` / `context_store` / `context_correct` payloads (all
   three formats) are byte-identical to a pre-vnc-037 snapshot, and that an opted-out get
   (`include_edges: Some(false)`) is itself edge-free and indistinguishable from a list-view
   single-entry render.

The DB neighbor query lives in the **handler**, not the serializer (D-07): on opt-out the
handler passes `None` and never runs the query.

### Consequences

- **Easier:** zero blast radius on the four list-view tools — their output is produced by
  the same unchanged code paths as before; the `edges` key/section can only appear on the
  get-surface path. The invariant is enforced by *structure* (the key is never added, the
  section is never appended) rather than by a conditional that could be mis-edited (#3449).
- **Harder:** the get path now owns three small format-specific render branches
  (summary digest, markdown `### Related`, json `edges`/`edge_totals`) rather than pushing
  all rendering into the shared helper — a deliberate trade of slight duplication for a
  guaranteed-inert list-view path. This may push `tools.rs`/`entries.rs` toward the
  500-line limit, motivating a sibling render module (see ARCHITECTURE OQ-B).
- **Cross-ref:** the edge *shape* rendered here is the projection defined in ADR-002; the
  totals JSON shape is fixed in ADR-005.
