# Agent Report: 930-investigator

Role: bug investigator | Feature: bugfix-930 | Status: complete

Diagnosis posted: https://github.com/dug-21/unimatrix/issues/930#issuecomment-4908089803

## Outcome
Root cause identified with high confidence: **registry split-brain**. The HTTP `/observe` write path uses the daemon-global `SessionRegistry` (`ObserveContext`, `main.rs:1274` ← `main.rs:846`); the per-slug `UnimatrixServer` instances that serve MCP over HTTPS (including `cycle_review`) keep the constructor-default empty `SessionRegistry::new()` (`server.rs:416`) because `http_provision.rs::build_project_server` (lines 261-273) never performs the shared-registry overwrite that both other construction paths do (`main.rs:976`, `main.rs:1695`). Deltas merge correctly under `http-{sid}` into the global registry; `cycle_review` folds an instance that never received a byte.

- Issue hypothesis 1 (per-frame cycle_stamp fold key): disconfirmed — `take_transcripts_for_feature` joins via `SessionState.feature`.
- Issue hypothesis 2 (buffer lost across HTTP requests): right family, wrong mechanism — buffers persist; the writer and reader hold different registry instances.

Proposed minimal fix: overwrite `input.server.session_registry` and `input.server.transcript_hold` with the daemon-shared pair in the per-slug loop (mirroring `main.rs:976`/`994`), plus a startup `Arc::ptr_eq`/Wave-B assertion so a future wiring split fails loud. Full trace, risk assessment (incl. cross-slug visibility caveat), and missing-test identification are in the GH comment.

## Follow-up: per-slug observe routing viability (coordinator request)
Verdict: **PER-SLUG-ROUTING-VIABLE** — addendum posted: https://github.com/dug-21/unimatrix/issues/930#issuecomment-4909018574
`observe_url` encodes the slug (`{base}/v1/{slug}/observe`, `client_bundle.rs:135`); `route_observe` already parses `ProjectKey::Slug` at `handlers.rs:51` before the registry write. Per-slug registries (built in `build_project_server`, resolved per request beside `resolve_store`) replace the shared-registry overwrite AND fix the F2 cross-slug fold leak by construction. ~5 files, no wire/client change. Sibling split flagged: `pending_entries_analysis` has the same global-write/per-slug-read shape.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — pattern #4828 (vnc-027 UDS/HTTP session-id split) and #4799 (registry drain/held-buffer lifecycle) directed the trace; ADRs #5026/#5442 confirmed the fold join key, disconfirming hypothesis 1.
- Stored: nothing novel to store — defect specifics live on GH #930 (bugs are GH issues, not lessons); the transport-split family is already covered by #4828/#4799. The generalizable guard ("constructor defaults are test-only; production paths must overwrite shared-state fields") is encoded as the proposed startup assertion instead.
