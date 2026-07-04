# Component: `attach_to_response_assembly` [UNCHANGED core] + `attach_search_status` [NEW]

File: `unimatrix-server/src/mcp/distill_handler.rs:281`

## Purpose

Append response-transient content to the already-built `CallToolResult` AFTER the report is computed
and memoized — structurally OUTSIDE `RetrospectiveReport` (secrets-critical, ADR-004/#4850, AC-14).
No candidate/loss/status content reaches any SQL/file/log write.

## `attach_to_response_assembly` — UNCHANGED

Signature and body stay byte-identical (ARCH §12 marks it UNCHANGED). It already no-ops on `None` and
on `Err(_)`:

```
fn attach_to_response_assembly(result: &mut Result<CallToolResult, ErrorData>,
                               section: Option<TranscriptCandidatesSection>):
    let (Ok(call_result), Some(section)) = (result.as_mut(), section) else: return   # no-op on None/Err
    if let Ok(json) = serde_json::to_string(&section):
        call_result.content.push(Content::text("\ntranscript_candidates: {json}"))
```

No change required. The `section` now arrives already scope-filtered (distill-before-purge.md), but the
attach step is agnostic to how it was produced.

## `attach_search_status` [NEW] — response-transient honesty projection

Adds the per-session `matched`/`search_complete` rows and anchor/phase `ResolvedBounds` (FR-14-16).
Same secrets discipline: response-transient content item, never persisted, no-op on `Err`/empty.

```
fn attach_search_status(result: &mut Result<CallToolResult, ErrorData>,
                        rows: Vec<SessionSearchStatus>,
                        bounds: Option<ResolvedBounds>):
    let Ok(call_result) = result.as_mut() else: return          # no-op on Err
    if rows.is_empty() and bounds.is_none(): return             # no-op when nothing to report
    let payload = json!({ "search": rows, "resolved_bounds": bounds })
    if let Ok(json) = serde_json::to_string(&payload):
        call_result.content.push(Content::text("\ntranscript_search: {json}"))
```

`SessionSearchStatus` / `ResolvedBounds` are NEW serializable, response-transient types (OVERVIEW).
They carry NO transcript byte content — only session ids, booleans, counters, epoch bounds — so the
content-scan (R-03) sees no verbatim/secret-shaped run. They are never fields on any persisted struct.

## Data flow

- IN: `Option<TranscriptCandidatesSection>` (candidates + loss) and `(Vec<SessionSearchStatus>, Option<ResolvedBounds>)`.
- OUT: 0..2 additive `Content::text` items on the in-flight `CallToolResult`. Nothing else mutated.

## Error handling

- `Err(_)` result → both attach fns are no-ops (an error response is never rewritten — preserves the
  existing `test_attach_on_err_is_noop`).
- Serialization failure → emit nothing (keep the response valid), same as the existing `if let Ok(json)`.

## Key test scenarios

- `None` section + empty status → response byte-identical to the no-transcript default (AC-01 golden).
- Some section → one `transcript_candidates` item; non-empty status → one `transcript_search` item.
- Attach on `Err` → no rewrite (existing test preserved).
- Struct-shape guard (AC-14): `RetrospectiveReport` serialized still has NO `transcript_candidates` /
  `transcript_search` key; the status content lives only on the response.
- Content-scan (R-03): the `transcript_search` payload contains no 64+ hex run / no verbatim delta text.
