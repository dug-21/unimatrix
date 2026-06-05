# Agent Report — vnc-024-agent-8-tester (Stage 3c Test Execution)

## Outcome: PASS — all 15 ACs green; AC-12 gate green on all 3 arms

## What I ran

### Unit / component (Rust, summaries only)
- `cargo test -p unimatrix-engine` — full suite green (wire 90 incl. fixtures R-01/02/08/11 Rust side).
- `node --test contract.test.mjs` — 4/4 (AC-04/05/06/11 dual-sided delta TS→Rust).
- `cargo test -p unimatrix-server --lib` — **3502 passed, 1 pre-existing failure** (GH#684).
  - transcript_delta guard 5/5, observe_text mapper 7/7, retention 11/11, uds::hook 196/196.
- AC-03/R-14 CI gate self-test: mutate binding → `git diff --exit-code` exit 1; restore → exit 0.
- AC-15/R-12: `cargo tree --edges normal` → ts-rs ABSENT from runtime; dev-only (v12.0.1).

### New integration tests I wrote (cumulative — extended `http/router/tests.rs`)
10 HTTP-boundary tests (`test_observe_http_*`), all green. These close the gap Stage 3b left:
the existing AC-07/08/09 tests call the mapper directly (bypassing R-07). Mine build real
`http::Request`s and replay the handler's exact Accept-read-before-`into_parts` ordering, plus
the HTTP arm of AC-12 (prefix_session_id preserves the delta drop-routing key — RecordEvent +
batch). No new infra-001 suite added (per OVERVIEW plan; AC-07/08/09/10/12 are out-of-band of
the stdio harness).

### infra-001 smoke gate
**BLOCKED by pre-existing harness drift (GH#685)** — `harness/client.py` launches the binary with
a stale `serve --stdio` subcommand the current build rejects; every test errors at init. With the
launch corrected locally (committed harness reverted, verified clean), 19/23 smoke pass — vnc-024
non-regressive on the MCP stdio surface. The 4 failures are all pre-existing/unrelated:
2× rmcp-1.7 string-int id (GH#684), 1× category-allowlist drift, 1× missing `context_cycle` tool.

## AC-12 status (the gate)
| Arm | Result |
|-----|--------|
| UDS dispatch (zero rows, direct DB count) | PASS |
| RecordEvents batch (delta dropped, N persist) | PASS |
| HTTP /observe (prefix transform preserves drop-routing) | PASS |

## GH Issues filed
- **GH#684** — rmcp-1.7 `context_lookup`/`_get`/`_deprecate` integer-id schema+coercion regression
  (unit `test_schema_integer_type_preserved...` + integration `test_get/deprecate_with_string_id`).
  Root cause vnc-023 #674; vnc-024 changed `server.rs` zero lines. (Unit test → no xfail marker; tracked by issue.)
- **GH#685** — infra-001 harness `serve --stdio` launch drift; blocks whole suite. vnc-024 touched no CLI.

## Risk coverage gaps
None for vnc-024's surface. All Critical/High risks (R-01..R-09, R-14) have passing specific tests.

## Files
- Report: `product/features/vnc-024/testing/RISK-COVERAGE-REPORT.md`
- New tests: `crates/unimatrix-server/src/http/router/tests.rs` (10 `test_observe_http_*` fns)

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` — #4452 (gate-fix test must exercise dropped path),
  #4515 (gate-3b all-code-no-tests failure mode — not present), ADR-003 #4714 (content negotiation).
- Stored: entry #4725 "Transport-convergence guard testing: assert the per-transport transform
  preserves the routing key, not duplicate the row-count test" via context_store (pattern, testing).
