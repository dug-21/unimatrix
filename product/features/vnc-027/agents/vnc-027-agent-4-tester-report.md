# vnc-027 Agent Report — Tester (Stage 3c, Execution)

Agent: vnc-027-agent-4-tester · GH #680 · branch `feature/vnc-027`

## Scope

Executed the integration/live layers deferred from Stage 3b plus full regression:
live UDS listener round-trip, FNF truncation, cross-transport replay (both
directions), delta-over-UDS buffer merge, frozen Rust hook end-to-end, UDS
latency — then ran the Stage 3b unit/fixture suites, the parity drift gate, and
the infra-001 MCP harness. UDS is Unix-only; this is Linux, so all live layers ran.

## Deliverables

- `product/features/vnc-027/testing/RISK-COVERAGE-REPORT.md` — R-01..R-18 mapping, AC-01..AC-12 status, unit + integration counts, xfail references.
- `packages/unimatrix/test/helpers/real-server.js` — EXTENDED (cumulative): socket readiness poll + `socketPath`, `udsPost(frame, opts)` (drives shipped `transport-uds`), `udsConnectRaw()`.
- `packages/unimatrix/test/hook-client/parity-layer2-uds.test.js` — NEW live UDS Layer 2 suite (16 tests).

## Results

| Layer | Result |
|-------|--------|
| `cargo test -p unimatrix-server --lib` | 3617 passed, 0 failed, 1 ignored |
| `cargo test -p unimatrix-engine --lib wire` (AC-11) | 101 passed, 0 failed |
| `scripts/regen-parity.sh` drift gate (AC-11) | zero git diff |
| Node hook-client incl. Layer 2 | 559 passed, 0 failed, 1 skipped (FR-22 todo) |
| — of which NEW live UDS Layer 2 | 16 passed |
| infra-001 `smoke` (mandatory gate) | 23 passed |
| infra-001 `protocol` | 13 passed |
| infra-001 `tools` | 185 passed, 3 xfailed |

Integration totals: 221 passed, 3 xfailed, 0 failed. All R-01..R-18 covered
(R-16 post-merge by design); all AC-01..AC-12 PASS.

Headline proofs landed: AC-11/R-08 s4 frozen Rust hook PreCompact stdout is
byte-identical to the TS client against the same daemon (the deployed-hook safety
proof); AC-07 buffer CONTENT asserted for UDS deltas; AC-04 bidirectional replay
+ session-id split pinned; AC-05 p95 sync≈0.14 ms / fnf≈0.08 ms over the live socket.

## Triage / GH Issues filed

None. No new integration failures. The 3 `tools` xfails are pre-existing markers
(GH#405, GH#305, GH#575) carried from prior features — not caused by vnc-027; no
new xfail markers added, no tests deleted/commented. The additive `accept` field
+ `HookResponse::Text` variant caused zero MCP-surface regression.

## Risk coverage gaps

- R-16 dogfood drop-detector — post-merge obligation by design (FR-32), documented procedure, not an F4a gate item.
- R-17 mixed-client double-prepend — only the supported one-client-per-project row tested; unsupported row documented by SCOPE design.
- R-04 TaskCompleted end-to-end keying — unreachable-but-tested by design (ADR-006 age-prune authoritative); covered by unit branch + Stop-negative.
- No gap on any High/Critical risk.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — returned ADR-001 (accept↔Text), ADR-003 (socket lifecycle), the shared-injection-core pattern (#4798), and the fail-open drop-detector pattern (#4800); all applied.
- Stored: entry #4828 "UDS live-listener Layer 2: session-id split makes HTTP precompact() blind to UDS-written buffers" via context_store (topic testing/hook-client, category pattern) — captures the session-id split read-surface trap, the JSONL-turn sentinel requirement, the `hook Ping` CLI-stdout divergence, and the real-server.js UDS extension recipe.
