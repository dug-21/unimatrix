## ADR-003: Resolved-get response construction — notice injection point, resolved-terminal edges, json shape

### Context

A resolved `context_get` must attach a message (hop notice AC-02, dead-end flag AC-04, or
deprecated footer AC-03) and must decide **whose edges** it returns. Three coupled
response-construction risks:

- **SR-04 — injection point.** The byte-identity invariant
  `test_none_json_byte_identical_to_base_object` (`response/mod.rs:367`, ADR-003 vnc-038)
  and ~15 `format_single_entry` shape tests assert the exact bytes of a formatted entry.
  Injecting a notice *inside* `format_single_entry` breaks all of them.
- **SR-03 — which entry's edges.** The handler builds edges from the **original** id
  (`build_edges_view(&self.store, id)`, `tools.rs:991`). If resolution swaps the returned
  entry to the terminal but edges stay keyed on the original id, the response shows terminal
  content with the wrong entry's edges. NG-1 defers *neighbor-target* resolution but does
  not say which entry's edge **list** a resolved get returns.
- **OQ-3 — json rendering.** For `format="json"`, programmatic callers need a stable
  contract for the notice/flag rather than a prepended human string.

Precedent: `format_store_success_with_note` (`response/entries.rs:189`) already models a
note across all three formats — inline for summary, a `> blockquote` for markdown, and a
structured `"note"` field for json — without touching the base `format_store_success`.

### Decision

**1. Inject the note in a handler-side formatter variant, never in `format_single_entry`.**
Add `format_single_entry_with_note(entry, format, edges, note: &ResolutionNote)` alongside
`format_single_entry`, mirroring the `format_store_success` / `_with_note` split. The
handler routes:

```
mode == CleanPassthrough  → format_single_entry(&entry, fmt, edges)            // unchanged bytes
otherwise                 → format_single_entry_with_note(&entry, fmt, edges, &note)
```

`format_single_entry` is not modified, so the byte-identity canary and the ~15 shape tests
stay green (SR-04, SR-02a). Clean passthrough (requested id already the active terminal, or
`follow_supersessions=false` on an active entry) carries **no note** (AC-02).

**2. Resolved gets rebuild edges on the resolved-terminal id.** Call
`build_edges_view(&self.store, effective_id)`, where `effective_id` is whatever id
resolution selected (terminal on a hop; requested id on dead-end or as-stored). The edge
**list** always belongs to the entry actually returned — coherent with AC-01 ("identical in
shape to a direct get of that terminal"). This does **not** resolve the edge **targets**
inside the list; deprecated neighbors still show their old id+title (NG-1 / SR-07). The
notice makes that asymmetry legible.

**3. JSON renders the message as a structured `resolution` object, present only when
non-clean.** Following the `_with_note` json precedent but typed for a stable contract:

| Case | JSON `resolution` field |
|------|-------------------------|
| Followed (AC-02) | `{"status":"followed","requested_id":X,"returned_id":Y}` |
| Dead-end (AC-04) | `{"status":"no_active_successor","requested_id":X}` |
| As-stored deprecated, pointer present (AC-03) | `{"status":"as_stored_deprecated","requested_id":X,"superseded_by":Z}` |
| As-stored deprecated, no successor (AC-03 / R-08) | `{"status":"as_stored_deprecated","requested_id":X,"superseded_by":null}` |
| Clean passthrough | *(no `resolution` key)* |

Text formats (summary/markdown) render the human one-liners from SCOPE (`↻ …` hop notice
prepended; loud `⚠ …` dead-end line prepended; `deprecated; superseded by #Z …` footer
appended). Omitting the `resolution` key in the clean case keeps the common active-entry
json byte-identical to today.

**4. Escape hatch on an orphaned/quarantined deprecated entry with no successor (R-08).**
This is the intersection of ADR-001 (escape hatch) and ADR-002 (dead-end): with
`follow_supersessions=false`, a deprecated entry is returned as-stored, but if its
`superseded_by IS NULL` (orphaned deprecated or quarantined terminal) there is no `#{X}`
to name in FR-07's `deprecated; superseded by #{X} …` footer. **Ruling:** the footer is
built from `entry.superseded_by: Option<u64>`, never from an unwrap. When it is `Some(Z)`,
emit `deprecated; superseded by #{Z} (omit follow_supersessions to follow).`; when it is
`None`, emit the well-formed pointerless footer `deprecated; no recorded successor.` —
**no panic, no malformed `#{}`**. In JSON the `superseded_by` field is `null` in the
no-successor case (the `resolution.status` stays `as_stored_deprecated`). The escape hatch
never walks the chain, so this branch is decided solely by the fetched entry's own
`superseded_by`, independent of ADR-002's `follow_to_current` path.

### Consequences

- **Easier:** byte-identity and shape tests are untouched on the common path; the new
  variant is additive and independently testable. Programmatic callers branch on
  `resolution.status` — a typed discriminant, not a parsed string.
- **Harder:** a second formatter entry point to maintain (mitigated — it delegates to the
  same rendering internals as `format_single_entry`). Resolved-terminal edge keying means
  ~18 `get_edges_tests.rs` assertions need a review pass for the resolved case (tracked in
  the acceptance map, SR-02/SR-03).
- **Accepted asymmetry (SR-07):** terminal content + terminal edge *list*, but unresolved
  edge *targets*. Legible via the notice; neighbor-target resolution is the deferred
  follow-up (NG-1).
- Depends on ADR-002 for the `DeadEnd` variant and ADR-001 for the `AsStoredDeprecated`
  (escape-hatch) variant. One remaining spec toggle: if the human prefers flat-`"note"`
  parity with `format_store_success_with_note` over the typed `resolution` object, that is
  the only open sub-choice; recommendation is the structured object.
