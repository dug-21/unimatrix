# Component 2 — response formatter (`ResolutionNote` + `format_single_entry_with_note`)

**File:** `crates/unimatrix-server/src/mcp/response/entries.rs`
**Precedent:** `format_store_success_with_note` (`:189-231`), `format_single_entry` (`:24-64`)

## Purpose

Render a resolved-get response that carries a resolution message, WITHOUT touching the
byte-identity-critical `format_single_entry`. Adds the `ResolutionNote` enum (handler→formatter
contract) and a note-carrying wrapper that delegates entry-body rendering to the exact same
internals as the base formatter, then attaches the note per format.

## 2a. `ResolutionNote` enum (NEW)

```
/// What resolution did to the requested id (vnc-042). Handler builds it; formatter renders it.
/// Clean passthrough carries NO ResolutionNote (handler routes to format_single_entry instead).
enum ResolutionNote {
    Followed          { from: u64, to: u64 },                        // AC-02 hop
    DeadEnd           { requested: u64 },                            // AC-04 no active successor
    AsStoredDeprecated{ requested: u64, superseded_by: Option<u64> },// AC-03 / AC-08
}
```

Placement: alongside `format_single_entry` in `entries.rs`, `pub` (crossed from `tools.rs`).
Re-export path as the handler expects (mirror how `format_single_entry` is imported in `tools.rs`).

## 2b. `format_single_entry_with_note` (NEW)

Signature (mirrors base + note, per Integration Surface):
```
pub fn format_single_entry_with_note(
    entry:  &EntryRecord,
    format: ResponseFormat,
    edges:  Option<&EdgesView>,
    note:   &ResolutionNote,
) -> CallToolResult
```

**Delegation rule (R-06, AC-01 body-equivalence):** the entry body + edges must be rendered by
the SAME code path as `format_single_entry`, so the returned body is identical modulo the note.
Two acceptable implementations — pick one and keep it strictly additive:

- **(preferred) text/markdown:** call the shared body builders (`format!("#{id} | ...")`,
  `format_entry_markdown_section`, `render_summary_digest`, `render_markdown_related`) —
  i.e. reuse the exact expressions `format_single_entry` uses — then prepend/append the note
  string. For json: build `entry_to_json(entry)` (+ edges insert, identical to base) and add
  the `resolution` object.
- Do NOT re-derive the body differently; drift here reintroduces R-06.

### Text render — Summary and Markdown

Placement of the note per variant (SCOPE one-liners):

| Variant | placement | exact text |
|---------|-----------|-----------|
| `Followed{from,to}` | **prepend** | `↻ Requested #{from} (deprecated) → returning current version #{to}.` |
| `DeadEnd{requested}` | **prepend** | `⚠ Requested #{requested}: no active successor found (chain dead-ends on a non-active entry).` |
| `AsStoredDeprecated{_, Some(z)}` | **append** | `deprecated; superseded by #{z} (omit follow_supersessions to follow).` |
| `AsStoredDeprecated{_, None}` | **append** | `deprecated; no recorded successor.` |

```
render_note_text(note) -> (prefix: Option<String>, suffix: Option<String>):
    Followed{from,to}            => (Some("↻ Requested #{from} (deprecated) → returning current version #{to}."), None)
    DeadEnd{requested}           => (Some("⚠ Requested #{requested}: no active successor found (chain dead-ends on a non-active entry)."), None)
    AsStoredDeprecated{_,Some(z)}=> (None, Some("deprecated; superseded by #{z} (omit follow_supersessions to follow)."))
    AsStoredDeprecated{_,None}   => (None, Some("deprecated; no recorded successor."))   // R-08: NO #{} , NO unwrap

Summary:
    body = <same line format_single_entry builds, incl. edges digest>
    line = join_nonempty([prefix, body, suffix], separator = "\n")   // prefix above body, suffix below
    CallToolResult::success(text(line))

Markdown:
    body = <same markdown format_single_entry builds, incl. ### Related>
    text = ""
    if prefix: text += "{prefix}\n\n"
    text += body
    if suffix: text += "\n\n> {suffix}"      // blockquote, matching format_store_success_with_note style
    CallToolResult::success(text(text))
```

The `superseded_by: Option<u64>` is matched, never unwrapped — the `None` arm emits the
well-formed pointerless footer. No `#{}`, no `#null`, no panic (R-08, AC-08, C-4).

### JSON render — structured `resolution` object (ADR-003, OQ-3)

Build the base object exactly as `format_single_entry` does (`entry_to_json` + optional
`edges`/`edge_totals` inserts), then insert ONE `resolution` key:

```
Json:
    obj = entry_to_json(entry)
    if let Some(view) = edges:                 // identical to base formatter
        obj["edges"]       = render_json_edges(view)
        obj["edge_totals"] = render_json_edge_totals(view)
    obj["resolution"] = match note:
        Followed{from,to}             => { "status":"followed",             "requested_id":from, "returned_id":to }
        DeadEnd{requested}            => { "status":"no_active_successor",   "requested_id":requested }
        AsStoredDeprecated{req,Some(z)}=> { "status":"as_stored_deprecated", "requested_id":req, "superseded_by":z }
        AsStoredDeprecated{req,None}  => { "status":"as_stored_deprecated",  "requested_id":req, "superseded_by":null }
    CallToolResult::success(text(to_string_pretty(obj)))
```

**Critical (R-07):** this function is only ever called on NON-clean paths, so the `resolution`
key appears only when there is a real note. Clean passthrough never reaches here — it routes to
`format_single_entry`, which never emits `resolution`. This is what preserves json byte-identity
for the common active-entry case.

## What stays UNCHANGED (SR-04 / R-01 / TS-01 / TS-02)

`format_single_entry` (`:24-64`) is NOT modified — no note param, no `resolution` key, no
prefix/suffix. The byte-identity canary (`test_none_json_byte_identical_to_base_object`,
`response/mod.rs:~367`) and the ~15 shape tests pass unchanged because the clean route still
calls the untouched base formatter. Editing either test = FLAG event, not a fix (#5099).

## Data Flow

- **Inputs:** `&EntryRecord`, `ResponseFormat`, `Option<&EdgesView>`, `&ResolutionNote`.
- **Output:** `CallToolResult` — body byte-identical to `format_single_entry` for the same
  entry+edges, PLUS the note (text prefix/suffix) or the `resolution` json key.
- No I/O, no async, no store access — pure rendering.

## Error Handling

Pure function; no fallible ops except `serde_json::to_string_pretty`, handled with
`.unwrap_or_default()` exactly as the existing formatters do (`:60`, `:182`, `:227`). No panics.

## Key Test Scenarios (hints)

- Strip-and-compare (R-01 sc.4): output of `_with_note` with the note region removed equals
  `format_single_entry` output for the same entry across all three formats (proves additivity).
- Each variant × each format renders the exact string / json shape in the tables above.
- `AsStoredDeprecated{_, None}` → footer is the pointerless form, contains no `#`, no panic (AC-08/R-08).
- json `resolution.status` present with correct discriminant + fields for all three variants (R-07 sc.2-4).
- `format_single_entry` output unchanged — TS-01 canary + TS-02 shape tests green (R-01, NFR-05).
