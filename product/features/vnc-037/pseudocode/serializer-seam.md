# Component: serializer-seam

## Purpose

Extend the shared single-entry serializer with an **optional** `edges` argument so only
`context_get` renders edges, and make `None ⇒ key/section absent` a **structural** invariant —
the four list-view tools stay byte-identical to pre-vnc-037 (ADR-003, FR-13/C-4, SR-01). Provide
the per-format render of `EdgesView` (summary digest / markdown `### Related` / json
`edges`+`edge_totals`) with the `↔` glyph and the `…N more` pointer referencing
`GET_EDGE_DISPLAY_LIMIT` (no literal 3) (ADR-005, FR-12/FR-14).

## Location

- `crates/unimatrix-server/src/mcp/response/entries.rs` — `format_single_entry` signature change
  + format branching (`:13`).
- `crates/unimatrix-server/src/mcp/response/edges.rs` (new) — the 3 render helpers (keeps
  `entries.rs` < 500 lines, OQ-B).
- **UNCHANGED**: `entry_to_json` (`response/mod.rs:121`) and `format_entry_markdown_section`
  (`response/mod.rs:160`) — signatures fixed (byte-identity invariant).

## Signature Change (the seam)

```
// before:  fn format_single_entry(entry: &EntryRecord, format: ResponseFormat) -> CallToolResult
// after:
fn format_single_entry(
    entry: &EntryRecord,
    format: ResponseFormat,
    edges: Option<&EdgesView>,        // NEW — only context_get passes Some(..)
) -> CallToolResult
```

All other callers (the four list-view tools' single-entry path, and `context_get` on opt-out)
pass `None`. Every existing call site is updated to pass `None` (additive third arg).

## Body (pseudocode) — `None ⇒ unchanged` is structural

```
fn format_single_entry(entry, format, edges):
    match format:
      Summary:
        let mut line = format!("#{} | {} | {} | [{}]", entry.id, entry.title, entry.category, tags_str(entry.tags))
        if let Some(view) = edges:                      // get-only; list views pass None ⇒ no digest
            line.push_str(&render_summary_digest(view)) // appends " | edges: …"
        success(text(line))
      Markdown:
        let mut text = format_entry_markdown_section(1, entry, None)   // UNCHANGED helper
        if let Some(view) = edges:                      // append ### Related AFTER the footer
            text.push_str("\n\n")
            text.push_str(&render_markdown_related(view))
        success(text(text))
      Json:
        let mut obj = entry_to_json(entry)              // UNCHANGED helper — base object only
        if let Some(view) = edges:                      // inject keys ONLY when surfaced
            obj["edges"] = render_json_edges(view)              // array of the 5-field objects
            obj["edge_totals"] = json!({ "inbound": view.totals.inbound,
                                         "outbound": view.totals.outbound })
        success(text(to_string_pretty(obj)))
```

> Structural invariant (ADR-003): for `None`, the `edges` key is **never inserted** and the
> `### Related` section is **never appended** — not guarded-omitted. There is no `edges` key to
> drop because for list views it is never added. The DB queries live in the handler, never here.

## Render helpers (edges.rs)

### render_json_edges(view) -> serde_json::Value

```
json array of, per edge:
  { "edge_type": e.edge_type, "direction": e.direction,
    "target_id": e.target_id, "target_title": e.target_title,   // null when None
    "authored": e.authored }
// zero-edge: view.edges is empty ⇒ "edges": []  (paired with edge_totals {0,0}) — FR-12
```

### render_markdown_related(view) -> String  (ADR-005)

```
"### Related\n"
if view.edges is empty:
    + "No related entries."                                    // FR-12 explicit empty state
else:
    for each e in view.edges:                                  // flat ranked ≤cap list — NO author/inferred sub-split
        glyph = match e.direction { "both" => "↔", "outbound" => "→", "inbound" => "←" }
        + format!("- {} {} #{} \"{}\"\n", e.edge_type, glyph, e.target_id,
                                          e.target_title.unwrap_or("(untitled)"))   // dangling: no panic
    let total = view.totals.inbound + view.totals.outbound      // uncapped total (↔ already once)
    if total > (GET_EDGE_DISPLAY_LIMIT as usize):               // "more than displayed" — references the constant
        let n = total - (GET_EDGE_DISPLAY_LIMIT as usize)       // N = total − cap — references the constant
        + format!("_…and {} more — use context_graph_\n", n)
```

> No literal `3` at the threshold or arithmetic (C-12/FR-18/AC-13). The author/inferred sub-split
> is **dropped** (ranking front-loads authored — ADR-005 OQ-02); a flat ranked list only.

### render_summary_digest(view) -> String  (ADR-005, OQ-02 form)

```
if view.edges is empty AND view.totals are 0:
    return " | edges: none"                                     // FR-12
// digest from the UNCAPPED totals (the honest split), distinguishing asymmetric vs symmetric:
//   ↑ = asymmetric outbound count, ↓ = asymmetric inbound count, ↔N = symmetric count, (K authored)
// Proposed form (OQ-02 — architect's chosen form, consistent with the existing entry-line style):
return format!(" | edges: {}↑ {}↓ ↔{} ({} authored)", out_asym, in_asym, sym, authored_tally)
```

> **OQ-02 detail (flagged):** the summary digest needs the asymmetric-out / asymmetric-in /
> symmetric counts SEPARATELY, but `EdgeTotals{inbound, outbound}` only carries the post-bucket
> split (with `↔` folded into inbound per the count convention). To render `5↑ 2↓ ↔3` the digest
> needs the symmetric count split out from inbound. **Resolution options for the implementer:**
> (a) widen `EdgeTotals` / `EdgeCountSplit` with a `symmetric: usize` third count (a `SUM(CASE
>     WHEN direction='both' …)` in store-split-count) — cleanest, keeps the digest honest; OR
> (b) render the simpler `edges: N↑ M↓ (K authored)` form without a separate `↔` tally.
> **Recommendation: (a)** — the spec's proposed digest explicitly shows `↔N`, and SR-04/NFR-7 make
> the string an acceptance surface. If (a) is taken, `store-split-count` adds the third aggregate
> and `EdgeCountSplit`/`EdgeTotals` gain `symmetric: usize` (still uncapped, still ↔-once). The
> `authored` tally counts the displayed-≤cap set unless the architect's OQ-02 form says otherwise —
> the tester pins the exact byte form. **See Open Questions in the return summary.**

## Constraints honored

- **C-4/ADR-003**: `entry_to_json` / `format_entry_markdown_section` signatures UNCHANGED; `None ⇒
  absent` structural.
- **C-12/FR-18**: `…N more` threshold + arithmetic reference `GET_EDGE_DISPLAY_LIMIT`, no literal 3.
- **ADR-005**: nested `edge_totals` object; flat ranked markdown; `↔` glyph; sub-split dropped.
- **C-11**: `entries.rs`/`edges.rs` each < 500 lines (render helpers in `edges.rs`).

## Data Flow

- **Inputs**: `entry`, `format`, `Option<&EdgesView>`.
- **Outputs**: `CallToolResult` — base payload (`None`) or base + edges (`Some`).

## Error Handling

- No fallible I/O in the serializer (the queries already ran in the handler). A `None` title renders
  a placeholder (`"(untitled)"`) — never `.unwrap()` on the Option (R-15, no panic on dangling).

## Key Test Scenarios

- **byte-identity via REAL producer (R-07, AC-07, #1268)** — `context_search`/`lookup`/`store`/
  `correct` single-entry payloads (all 3 formats) byte-identical to a pre-vnc-037 baseline: no
  `edges` key, no `### Related`, no `edges:` digest. Produced through the genuine serializer path.
- **None ⇒ key absent structural** — `entry_to_json` unchanged; passing `None` yields a
  byte-identical payload; opt-out get is edge-free and indistinguishable from a list view.
- **markdown ### Related (R-17, AC-08)** — flat ranked ≤cap list after the footer; `↔` glyph on
  symmetric lines; the dropped author/inferred sub-split is asserted ABSENT; single `…N more — use
  context_graph` pointer on overflow; zero-edge ⇒ "No related entries".
- **json shape (R-17, AC-08)** — `edges` array of the exact 5-field objects + nested `edge_totals`
  `{inbound, outbound}` (symmetric-once); zero-edge ⇒ `edges: []`, `edge_totals: {0,0}`.
- **summary digest (R-17, OQ-02)** — the `↔` split form; zero-edge ⇒ `edges: none`. Exact byte
  form pinned per the architect's OQ-02 decision.
- **cap-isolation (AC-13b)** — overriding `GET_EDGE_DISPLAY_LIMIT` (e.g. 2) shrinks the rendered set
  and the `…N more` arithmetic, while `edge_totals` stay byte-unchanged.
- **dangling title no-panic (R-15)** — `target_title: None` renders across all 3 formats without panic.
