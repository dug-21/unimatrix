# C2' — HTTPS bridge driver (`scripts/bridge-cycle-driver.js`)

**Extended in place**, cumulative. ADR-005. Adds retrieval + briefing `tools/call` envelopes
through the EXISTING shipped bridge; emits them into `dimension_bundle`. NO net-new
transport/cert/spawn code (C-2 fork-smell guard — reuse the existing spawn/RPC/witness machinery
verbatim).

## Purpose

The nan-021 driver speaks stdio JSON-RPC to the shipped `mcp-bridge.js` and drives
`context_cycle(start)` -> `context_cycle(stop)` -> (inline) `context_cycle_review`. nan-022 adds
the MCP-bridge-surface retrieval and briefing captures (D1, D4) through the SAME bridge session
and emits a `dimension_bundle` fragment instead of a bare `metric_vector`. The /observe-surface
dimensions (behavioral, precompact, isolation-write) are NOT driven here — the shell gate (C5')
fires those over pinned `/observe` (this driver stays the MCP-bridge surface only).

## Consumed verbatim (do NOT re-implement)

`parseArgs`, the bridge `spawn` + `NODE_OPTIONS=--require witness`, the newline-delimited
JSON-RPC framer (`rpc`/`send`/`notify`/`ensureOk`/`resultText`), `toolCall`, the #830 self-heal
reliance (no retry/reconnect added), the `initialize` -> `notifications/initialized` handshake,
the inline-review-when-`REVIEW_INLINE=1` ordering.

## Change — add retrieval + briefing tools/call envelopes; emit a bundle fragment

```
// after context_cycle(stop), and either inline-review or a SEPARATE capture invocation:

// SEED phase: replay the manifest seed_calls as context_store tools/call (CONTENT only, R-15)
for (const s of manifest.seed_calls || []) {
    ensureOk(await rpc((id) => toolCall(id, "context_store", s.args)), "context_store(seed)");
}

// RETRIEVAL (D1): replay manifest query_calls as context_search/lookup/get tools/call.
//   Parse each result's RANKED id list + scores from the tool-result text.
//   Capture TWICE (intra double-capture) — two passes over the SAME query set.
const retrieval_1 = await driveRetrieval(manifest.query_calls);   // [{tool,args,result_ids,scores}...]
const retrieval_2 = await driveRetrieval(manifest.query_calls);   // capture_2

// PROACTIVE (D4): replay manifest briefing_calls as context_briefing tools/call.
//   Parse ranked ids + scores + injection set. Capture TWICE (intra).
const briefing_1 = await driveBriefing(manifest.briefing_calls);  // {ids,scores,injection_set}
const briefing_2 = await driveBriefing(manifest.briefing_calls);  // capture_2

// ANALYTICS (D3) MetricVector: the existing inline review (REVIEW_INLINE=1) — verbatim.
//   Plus parse informs_edges + phase_signal from the review/edge tools/call if drivable here;
//   otherwise the shell composes them (document which side owns each — single source).
```

### Helpers (new, but using ONLY the existing `rpc`/`toolCall`/`resultText`)

```
driveRetrieval(query_calls):
    for each q in query_calls:
        resp = ensureOk(await rpc(id => toolCall(id, q.tool, q.args)), "context_search/lookup/get");
        { result_ids, scores } = parseRankedResult(resultText(resp));   // ids in RANKED order
        push {tool:q.tool, args:q.args, result_ids, scores};
    return the list.

driveBriefing(briefing_calls):
    resp = ensureOk(await rpc(id => toolCall(id, "context_briefing", briefing_call.args)), "context_briefing");
    parse {ids, scores, injection_set} from resultText(resp);
    return it.

parseRankedResult(text):
    // parse the tool-result JSON/markdown into the RANKED id list + aligned scores.
    // The result is the server's ranked output (the D1/D4 capture); ids in returned order.
    // If scores are absent in the response, return scores:null (K3 degrades to membership-only,
    // documented — never a silent loosening).
```

## Output — emit the bundle fragment instead of a bare metric_vector

The driver's STDOUT JSON line widens from `{ok, metric_vector, ...}` to carry the
MCP-bridge-surface captures:

```
process.stdout.write(JSON.stringify({
    ok: true,
    metric_vector: mv,                    // existing (analytics)
    retrieval: { queries: retrieval_1, capture_2: retrieval_2 },
    proactive: { briefing_ids: briefing_1.ids, briefing_scores: briefing_1.scores,
                 injection_set: briefing_1.injection_set, capture_2: briefing_2 },
    informs_edges: <edges or [] if shell-owned>,
    phase_signal: <phase or {} if shell-owned>,
    session_id: sid, error: null,
}) + "\n");
```

The shell gate (C5') assembles the FULL `dimension_bundle` from this fragment plus the
/observe-surface captures it owns (behavioral, precompact, isolation). Single source per
dimension: this driver owns the MCP-bridge-surface captures; the shell owns the /observe-surface
captures — document the split so no dimension is captured twice or missed.

## Bridge-in-path discipline (C-2 / D-2)

- All new captures ride the SAME bridge session (session-id replay assertion + idle-window min
  preserved). No direct `mcp_url` POST.
- NO new transport/cert/credstore/spawn code — only new `toolCall` envelopes over the existing
  `rpc`. A net-new transport path is a fork smell to FLAG.
- The witness still observes the wire (BRIDGE_WITNESS lines); the shell asserts the bridge
  carried the new calls (SSE + Mcp-Session-Id replay) — see cloud-cycle-lib.md.

## Error handling

- `ensureOk` surfaces a JSON-RPC error or tool-level `isError` as a HARD failure (verbatim) —
  the driver reports `ok:false`, the shell tail-dumps bridge stderr and `fail()`s.
- A heal-exhausting #830 failure surfaces as a JSON-RPC error on the call (verbatim; no retry).
- A retrieval/briefing call returning an empty result set is NOT swallowed: emit the empty list;
  the Python side's degenerate-corpus guard (R-06) / never-empty guard (R-09) classifies it
  INFRA-ERROR, never a vacuous pass.

## Key test scenarios (hints)

- The new retrieval/briefing calls ride the EXISTING bridge (SSE + Mcp-Session-Id replay
  asserted by the shell witness — bridge-in-path, not a direct POST; integration risk).
- Each retrieval/briefing capture is taken TWICE (capture_2 present) for intra double-capture.
- A degenerate (too-short) result set is emitted as-is, not padded/hidden -> Python flags
  INFRA-ERROR (R-06).
- No net-new transport/cert/spawn code (R-16 review-flag; diff is additive envelopes only).
- The emitted bundle fragment carries the documented keys the shell expects (cross-language
  contract, R-09).
