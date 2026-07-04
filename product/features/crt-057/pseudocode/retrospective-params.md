# Component: `RetrospectiveParams`

File: `unimatrix-server/src/mcp/tools.rs:~431`

## Purpose

Deserialize `context_cycle_review` tool params. Add the read-only scoped-retrieval axis; keep
render/recompute axes; drop the `"summary"` render alias (doc + behavior via render-dispatch.md).

## GROUNDING NOTE (accuracy — read before editing)

The current `RetrospectiveParams` in this worktree has fields: `feature_cycle`, `agent_id`,
`evidence_limit`, `format`, `force`, `auto_close`. It does **NOT** contain
`include_transcript_candidates` — the boolean-era field was never merged here. Therefore the brief's
"remove `include_transcript_candidates`" is a **no-op in this worktree**; verify with a grep and, if
truly absent, the only change is the ADD of `transcript` plus the `format` doc-comment update. Flag to
delivery if a grep surfaces it anywhere (it should not).

## Modified struct

```
#[derive(Debug, Deserialize, JsonSchema)]
struct RetrospectiveParams {
    feature_cycle: String
    agent_id: Option<String>
    evidence_limit: Option<usize>          // UNCHANGED (col-010b, JSON path)

    // Render axis — doc-comment updated: exactly "markdown" (default) | "json".
    // "summary" is DROPPED → ERROR_INVALID_PARAMS (see render-dispatch.md). (vnc-011)
    format: Option<String>

    force: Option<bool>                    // UNCHANGED recompute axis (None ≡ false)
    auto_close: bool  #[serde(default)]    // UNCHANGED (crt-055)

    // [NEW] read-only scoped retrieval axis (ADR-002). Omit = summary only (lean default).
    #[serde(default)]
    transcript: Option<TranscriptScope>    // see transcript-scope.md
}
```

## Data flow / transformations

- `transcript` deserialized directly to `Option<TranscriptScope>`; `#[serde(default)]` makes an absent
  key deserialize to `None` (FR-6: omit = no candidate block, buffer untouched).
- `format` string is parsed downstream at render dispatch, not here (see render-dispatch.md).
- No new required field; a pre-crt-057 caller omitting `transcript` behaves exactly as the lean default.

## Error handling

- Malformed `transcript` object (wrong types) → serde deserialization error surfaced as the standard
  param-parse error path already used by the tool (no new error variant introduced here).
- Invalid `match` regex and unknown `format` are validated later (transcript-scope.md / render-dispatch.md),
  not at deserialization.

## Key test scenarios

- `transcript` absent → `None` (drives AC-01 default-no-candidates).
- `transcript: {}` present, all-None → `Some(TranscriptScope{None,None,None,None})` (drives AC-05 full dump).
- `transcript: { match: "..." }` → `r#match` populated (serde rename `match` honored — see transcript-scope.md).
- `format: "summary"` still deserializes to `Some("summary")` here (the DROP is at dispatch, not parse) —
  assert it reaches `ERROR_INVALID_PARAMS`, not a parse error.
- Params doc-comment at `tools.rs:~445` no longer implies a `"summary"` value; and the tool description
  documents the three orthogonal axes (consumer-reconciliation.md).
