# vnc-050-agent-11-observe-403-client — C6c observe 403 latch

## Report
Files:
- NEW `packages/unimatrix/lib/hook-client/observe-latch.js` — isDenied / latch / guard; marker `{stateDir}/observe-denied.json` = `{cred: sha256hex(token)}`, 0600 via existing `state.atomicWrite` (already exported, already 0600 — no state.js change), no expiry, fail-open.
- `lib/hook-client/transport-http.js` — classifyResponse sets `deny: 1` only on 403 whose body contains `"token lacks SessionWrite"` (field absent otherwise).
- `lib/hook-client/index.js` — HTTP mode: marker hit → return before any network; transport wrapped by `guard` (covers carrying + queue.replay); deny carrying → no enqueue, no generic stderr; deny on sync → no "sync request failed"; delta deny → latch, no delta stderr line.
- NEW `packages/unimatrix/test/hook-client/observe-latch.test.js` — 13 spawned-hook tests per test plan.

Tests: observe-latch 13/13 pass. index/transport-http/queue/state/delta/decoration: 208 pass, 1 fail (`test_unwritable_state_dir_exit0`) caused by concurrent config.js edits (HOME-as-file now resolves `malformed` before my code runs) — not this component.

Size gate: FAIL. Total stripped 112963/110000, raw 200205/200000 (includes concurrent agent's edits). This component adds ~1325 stripped (observe-latch.js 919, index.js ~310, transport-http.js ~96). Cap not raised.

Deviations: pseudocode `denyReason` replaced by boolean `deny` (bytes); `server_db_zero_observation_rows` (real server) not implemented here — Stage 3c. Not committed per instruction.

## Knowledge Stewardship
- Queried: context_get 5826 / pattern search — skipped (budget); design taken from ADR-004 file + HD 10.
- Stored: nothing novel to store — pattern pending gate outcome.
