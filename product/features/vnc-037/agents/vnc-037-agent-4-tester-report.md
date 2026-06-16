# Agent Report: vnc-037-agent-4-tester (Stage 3c — Test Execution)

## Outcome: PASS

All unit tests green; mandatory integration smoke gate green; all new + relevant
integration suites green. No feature bugs. AC-12 latency baseline recorded as OPEN
(no measured baseline produced in this environment) — escalation per brief, not a
silent pass.

## Deliverable
- `product/features/vnc-037/testing/RISK-COVERAGE-REPORT.md` — full risk→test map
  (R-01..R-20), unit + integration counts, AC-01..AC-14 verification, AC-12 OPEN.

## Tests executed
| Layer | Command / suite | Result |
|-------|-----------------|--------|
| Unit | `cargo test -p unimatrix-store --lib` | 389 passed |
| Unit | `cargo test -p unimatrix-server --lib` | 4184 passed, 1 ignored |
| Unit | `cargo build --workspace` | PASS |
| Integ | `pytest -m smoke` (MANDATORY GATE) | 23 passed |
| Integ | `suites/test_get_edges.py` (NEW, vnc-037) | 17 passed |
| Integ | lifecycle carry-forward (1 NEW) | 2 passed |
| Integ | contradiction + confidence (regression) | 26 passed, 1 xfailed (pre-existing GH#405) |
| Integ | protocol + tools (`-k get/store/correct/search/lookup`) | 67 passed, 1 xfailed (pre-existing GH#405) |
| Unit | `graph_read_neighbors` + store `graph_queries` (R-08 unedited) | 6 + 46 passed |

Per the env constraint (`cargo test --workspace` OOMs the linker), unit tests were
run per-package with the hardened runner (`setsid -w timeout … > logfile`).

## New test infrastructure (cumulative)
- Extended the harness `context_get` client with the additive `include_edges` kwarg
  (`harness/client.py`; absent ⇒ default-on).
- New suite `suites/test_get_edges.py` (15 functions, 17 cases) — reuses the `server`
  fixture and the established `_compute_db_path` direct-SQLite seeding pattern.
- New lifecycle test `test_correct_then_get_carried_edge_classifies_authored`
  (R-05/DNB-2, discriminating: carried authored edge outranks higher-confidence
  inferred).

## Failure triage (per USAGE-PROTOCOL.md)
4 initial `test_get_edges` failures → diagnosed as **bad-test bugs**, NOT feature
defects: the MCP `context_store` **semantic dedup** (cosine ≥ ~0.93) collapsed
near-identical seeded entries to one id, so edges pointed at a shared target. The
equivalent store/server unit tests for the same behaviors were already green,
ruling out a feature bug. Fixed by seeding target entries via direct SQL
(`_seed_target`, pinned ids/confidence/title). No feature code changed; no
integration tests deleted/commented out.

## xfail / GH Issues
- No new `xfail` markers introduced. No GH Issues filed (no pre-existing failures
  surfaced by vnc-037).
- The 1 observed xfail (`test_confidence.py` GH#405) is pre-existing and unrelated.

## Open items
- **AC-12 latency baseline: OPEN.** No measured edge-free baseline on a high-degree
  node produced here; provisional ≤5 ms p50 / ≤15 ms p95 remain unconfirmed (C-9 /
  OQ-C). Hub bound is proven (store-boundary + MCP). Escalation path (relax / mandate
  OQ-03 opt-out / revisit default-on) goes to the human per the brief.
- **AC-13b cap-isolation override:** not runtime-expressible for a compile-time
  `const`; the load-bearing single-source invariant (AC-13a) is fully proven.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- MCP disconnected (non-blocking per
  spawn note); proceeded without. Applied #3886 (proof-outside-cap) and #1268 (real
  producer) from the prior risk-strategy queries.
- Stored: nothing novel to store -- #3886 already captures the ranking-test lesson
  applied here; the "MCP store semantic-dedup vs many-distinct-seed-entries" harness
  gotcha is a single-occurrence candidate, deferred to retro for confirmation.
