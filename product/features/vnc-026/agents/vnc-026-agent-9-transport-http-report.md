# Agent Report — vnc-026-agent-9-transport-http

## Task
Implement `packages/unimatrix/lib/hook-client/transport-http.js` from validated pseudocode
(pseudocode/transport-http.md) + unit suite per test-plan/transport-http.md.

## Files Created
- `packages/unimatrix/lib/hook-client/transport-http.js` (232 lines) — `post()`, `pingForInit()`, `DEFAULT_TIMEOUTS`, `BODY_LIMIT_BYTES`
- `packages/unimatrix/test/helpers/stub-server.js` (147 lines) — shared cumulative stub-server helper per test-plan OVERVIEW (scriptable status/contentType/body/delayMs/destroy + request log; also `startSilentTcpServer`, `refusedPort`)
- `packages/unimatrix/test/hook-client/transport-http.test.js` (425 lines) — 28 tests

Committed: `impl(transport-http): HTTP POST /observe with ADR-005 timeouts and failure classification (#679)` (f0091428, branch feature/vnc-026).

## Implementation Notes
- `post(config, frame, opts)` always resolves a SendResult — never throws/rejects (ADR-005 fail-open; ADR-007 allSettled independence degenerates to values).
- Headers: `Authorization: Bearer`, `Content-Type: application/json`, `Accept: text/plain` on sync / `application/json` on FNF (the #4703 canary).
- ADR-005 timeouts caller-supplied (defaults 750/2,000/3,000 ms), connect timer cleared on `connect`/`secureConnect`, both timers `.unref()`ed, once-guarded settle.
- C-02 1 MiB post-serialization guard (backstop for delta.js `bodyBuf` passthrough); response bodies capped at 1 MiB without hang.
- Classification: 401/403→auth, 4xx→http_4xx, 5xx→http_5xx, ETIMEDOUT→timeout, all else→connect; URL forms: trailing slash, path prefix, port, IPv6 literal (brackets stripped for `http.request`).
- `agent: false` — fresh socket per request (per-event process semantics; makes connect events deterministic in-process too).
- One interpretation beyond the pseudocode letter: on oversized response bodies the pseudocode destroys the stream but never resolves until the total deadline; I resolve immediately with the capped body (satisfies "capped without hang" + ok classification). No other deviations.
- Module emits zero stdout/stderr and never logs token/URL/body (R-16) — enforced by a grep-gate test.

## Tests
- 28/28 pass (`node --test test/hook-client/transport-http.test.js`).
- Full package suite: 222 pass / 6 fail — all 6 failures are PRE-EXISTING on main (`lib/merge-settings.js:112` writes `LD_LIBRARY_PATH=`-prefixed hook commands; `merge-settings.test.js`/`init.test.js` expect plain commands). Confirmed by running those suites alone (they do not import any new file). Not touched per triage rule (never fix unrelated failures); flagging for the init-remote agent who owns merge-settings.js in this feature.

## Issues / Blockers
- None blocking. Note above re: 6 pre-existing init/merge-settings failures — relevant to agent owning init-remote.
- IPv6 test self-skips when `::1` unavailable (passes in this environment).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — surfaced #4703 (remote /observe returns JSON without content negotiation — confirmed the Accept: text/plain sync-arm tests are the canary) and ass-014 transport decisions (context only). context_search for HTTP transport patterns: no directly applicable prior pattern.
- Stored: entry #4768 "node:test HTTP-client timeout testing: TLS-stall trick for connect deadlines; grep-gate not stdout spy; abort-safe stub server" via /uni-store-pattern.
