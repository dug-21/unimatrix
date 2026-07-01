# Test Plan — response formatter (`format_single_entry_with_note`, `ResolutionNote`)

**Component:** `crates/unimatrix-server/src/mcp/response/entries.rs` — new
`format_single_entry_with_note(entry, format, edges, note: &ResolutionNote)` + `ResolutionNote`
enum. `format_single_entry` UNCHANGED. Tests exercised via `response/mod.rs` `#[cfg(test)]`.
**Owns risks:** R-01 (Critical), R-06 (Med), R-07 (High), R-08 (Med).

---

## Regression guards — MUST stay green, ZERO edits, edits are FLAG events

- **TS-01 byte-identity canary** `test_none_json_byte_identical_to_base_object`
  (`response/mod.rs:~367`). Clean passthrough (`format=null`) ⇒ byte-for-byte identical
  `CallToolResult` vs the base object. `format_single_entry` is not touched, so this passes
  unchanged. **Any edit to this test = FLAG event, never a silent fix (#5099, NFR-05).**
- **TS-02 ~15 `format_single_entry` shape tests** (`response/mod.rs:296-469`). Base formatter
  untouched ⇒ all stay green. Breakage here is the signal that the notice was mis-injected inside
  `format_single_entry` (FR-09/C-7 violated).

---

## New unit test expectations — `format_single_entry_with_note`

### Additivity / no-drift (R-01 scenario 4, R-06)
- `test_with_note_stripped_equals_base_formatter` — for the same `EntryRecord`, take
  `format_single_entry_with_note(entry, fmt, edges, note)`, strip the note region (the `↻`/`⚠`
  prepended line or the appended footer / the json `resolution` key), and assert the remainder
  equals `format_single_entry(entry, fmt, edges)` byte-for-byte. Proves the note is **purely
  additive / outside** the base body. Run across `fmt ∈ {null, markdown, json}`.
- `test_with_note_body_matches_base_across_formats` (**AC-01 body-equivalence, R-06**) — the entry
  body rendered by `_with_note` == the base-formatter body (same id, fields, content), differing
  only by the note. Guards formatter drift between the two entry points.

### `ResolutionNote::Followed{from,to}` (AC-02 hop)
- `test_note_followed_summary_prepends_hop_line` — summary/markdown output **prepends**
  `↻ Requested #{from} (deprecated) → returning current version #{to}.` (exact string, X=from,
  Y=to).
- `test_note_followed_json_resolution_object` — `format="json"` ⇒ `resolution` object ==
  `{"status":"followed","requested_id":from,"returned_id":to}`.

### `ResolutionNote::DeadEnd{requested}` (AC-04 fail-loud flag)
- `test_note_deadend_summary_prepends_loud_line` — prepends
  `⚠ Requested #{requested}: no active successor found (chain dead-ends on a non-active entry).`
- `test_note_deadend_json_resolution_object` — json ⇒
  `{"status":"no_active_successor","requested_id":requested}`. Result non-empty.

### `ResolutionNote::AsStoredDeprecated{requested, superseded_by}` (AC-03 / AC-08, R-08)
- `test_note_asstored_with_successor_appends_footer` (**TS-06 baseline**) — `superseded_by=Some(Z)`
  ⇒ appends `deprecated; superseded by #{Z} (omit follow_supersessions to follow).`; json ⇒
  `{"status":"as_stored_deprecated","requested_id":X,"superseded_by":Z}`.
- `test_note_asstored_null_successor_wellformed_footer` (**AC-08, R-08 — no panic**) —
  `superseded_by=None` ⇒ footer == `deprecated; no recorded successor.` — **no panic, no `#{}`, no
  `#null` substring**; json ⇒ `superseded_by: null`, `status` stays `as_stored_deprecated`. Footer
  built from `Option<u64>`, never an `.unwrap()` (C-4). This is the ADR-001 escape-hatch ∩ ADR-002
  dead-end intersection.

### JSON `resolution`-key presence/absence (R-07, TS-05/TS-08 — all four ADR-003 cases)
- `test_json_clean_passthrough_has_no_resolution_key` — clean passthrough MUST NOT emit a
  `resolution` key (routed through `format_single_entry`, so this ties directly to TS-01
  byte-identity). Guards against the handler/formatter ever attaching an empty/`clean` variant.
- `test_json_followed_has_resolution_key`, `test_json_deadend_has_resolution_key`,
  `test_json_asstored_has_resolution_key` — the three non-clean cases each emit the correct typed
  `resolution` object. Programmatic callers branch on `resolution.status` (typed discriminant, not
  a parsed string).

### Edges on the note path (R-03 boundary — formatter side)
- `test_with_note_renders_provided_edges` — `_with_note` renders whatever `Option<&EdgesView>` it
  is handed (the handler keys them on `effective_id`); `None` ⇒ no `edges` key. Formatter does not
  re-key or resolve edge targets (NG-1 asymmetry accepted).

---

## Coverage requirements (from RISK-TEST-STRATEGY §R-01/06/07/08)
- TS-01 + TS-02 green with zero edits; the note appears **only** via
  `format_single_entry_with_note`.
- Strip-and-compare proves additivity across all three formats.
- `resolution`-key presence/absence asserted for all four ADR-003 cases; clean case ties back to
  TS-01.
- NULL-`superseded_by` as-stored footer is well-formed/absent with no panic.

## Not in scope for this component
- Neighbor/edge **target** resolution (NG-1) — the formatter leaves deprecated targets with old
  id+title; the notice makes the asymmetry legible (R-12 accepted).
