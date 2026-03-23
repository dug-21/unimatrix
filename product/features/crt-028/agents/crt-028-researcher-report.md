# crt-028 Researcher Report

**Agent ID**: crt-028-researcher
**Feature**: crt-028 WA-5 PreCompact Transcript Restoration

## Deliverable

SCOPE.md written to: `product/features/crt-028/SCOPE.md`

## Key Findings

### PreCompact path (hook.rs)

The `"PreCompact"` arm in `build_request()` currently produces a bare
`HookRequest::CompactPayload` and ignores `input.transcript_path` entirely, even though it
is fully parsed into `HookInput.transcript_path: Option<String>`. No changes to `HookInput`
or the wire protocol are needed — the field is already there.

The hook process runs with no tokio runtime (ADR-002, synchronous std::io only). All
transcript reading must use `std::fs::File` + `std::io::BufReader`.

### CompactPayload server side (listener.rs — handle_compact_payload)

crt-027 has already migrated `handle_compact_payload` from `BriefingService` to
`IndexBriefingService`. The function returns `HookResponse::BriefingContent { content, .. }`
where `content` is a flat indexed table produced by `format_index_table`. The `write_stdout`
handler in `hook.rs` prints `content` if non-empty. WA-5 inserts the transcript block into
this pipeline: read transcript before `transport.request()`, then prepend to the response
before printing.

### WA-5 contract surface (crt-027 ADR-005)

`IndexEntry` struct + `format_index_table` function are the stable WA-5 contract established
by crt-027. Both are always compiled (not gated by `mcp-briefing` feature flag). WA-5 does
not parse the rendered table — it prepends the transcript block as a string.

### GH #354 write site

Single site in `listener.rs`, `dispatch_request` ContextSearch arm, `ObservationRow`
construction:
```rust
hook: source.as_deref().unwrap_or("UserPromptSubmit").to_string(),
```
Fix: allowlist `{"UserPromptSubmit", "SubagentStart"}` with fallback to `"UserPromptSubmit"`.

### GH #355 gaps

Two gaps in `index_briefing.rs`:
1. No test verifying quarantine exclusion through the `index()` post-filter (`se.entry.status
   == Status::Active`). The deleted `BriefingService` had T-BS-08 for this.
2. No doc comment on `index()` documenting that query validation is delegated to
   `SearchService.search()` → `gateway.validate_search_query()`.

### Constants landscape

- `MAX_INJECTION_BYTES = 1400` (hook.rs, general injection budget)
- `MAX_COMPACTION_BYTES = 8000` (listener.rs, server-side compaction budget)
- `MAX_PRECOMPACT_BYTES` = new ~3000 (hook.rs, transcript block budget — must be added)
- `SNIPPET_CHARS = 150` (mcp/response/briefing.rs, entry snippet length from crt-027)

## Open Questions for Human

**OQ-1 (non-blocking)**: Should session-injection affinity be re-introduced as a ranking
tie-breaker at compaction time, or is pure fused score from `IndexBriefingService` sufficient?
crt-027 ADR-004 explicitly removed the injection history path. This would be a server-side
change to `handle_compact_payload` — out of scope for WA-5 transcript extraction but could
be a follow-up. Current decision: pure fused score.

**OQ-3 (non-blocking, implementation detail)**: Key param identification for compact tool
representation (D-2, D-3). Recommend hardcoded map for common Claude Code tools with
fallback to first string field. Needs confirmation before spec writing.

## Proposed Scope Boundaries

**In scope**: transcript read + extraction in hook.rs, `MAX_PRECOMPACT_BYTES` constant,
prepend to BriefingContent, GH #354 allowlist fix, GH #355 test + doc comment.

**Out of scope**: server protocol changes, schema migrations, PostCompact, configurable k,
session-injection affinity ranking (OQ-1 follow-up).

**Hard dependency**: crt-027 must be merged before crt-028 delivery.

## Risks

1. **Transcript file format drift**: Claude Code may change the JSONL schema. Graceful
   degradation (D-6, AC-08) mitigates this — unknown record types are skipped.
2. **Large transcript files**: Reverse-scan reads the whole file into memory as lines. For
   very long sessions this could be MBs. Mitigation: cap read at a reasonable size (e.g.,
   limit to last N bytes with `seek` from end), or read lines in reverse with a bounded
   buffer. This is an implementation decision for the spec/architecture phase.
3. **crt-027 merge dependency**: Any delay to crt-027 blocks crt-028. The scope assumes
   crt-027 is already merged on the target branch.

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for "precompact compaction hook transcript IndexBriefingService"
  — results confirmed ADR-004 crt-027 (CompactPayload flat index migration), ADR-002 crt-027
  (SubagentStart routing), BriefingService caller map, SessionRegistry patterns. No prior
  entry specifically covered the PreCompact transcript extraction pattern.
- Stored: entry #3331 "PreCompact hook: read transcript_path locally before server
  round-trip, prepend to BriefingContent" via `/uni-store-pattern`
