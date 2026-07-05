# Agent Report — vnc-044-agent-6-tester (Stage 3c, Test Execution)

## Deliverable
`product/features/vnc-044/testing/RISK-COVERAGE-REPORT.md` — R-01..R-13 mapped to results,
unit + integration counts, AC-02..AC-09 verification.

## Results (real counts)

| Layer | Result |
|-------|--------|
| Unit — `cargo test -p unimatrix-server --lib` | 4482 passed, 0 failed, 1 ignored |
| Unit — hardened `cargo test --workspace` (setsid -w + timeout) | rc=0, all crates pass |
| #878 workspace LINK smoke | PASS (rc=0) — also satisfies R-06 `--no-run` guard |
| Integration smoke (`pytest -m smoke`) — MANDATORY gate | 30 passed, 0 failed |
| New vnc-044 integration tests | 26 node-ids, all PASS (18 tools + 8 lifecycle) |
| Modified existing graph tests (`detail="full"`) | 5, all PASS |
| Regression (`test_protocol.py`, `test_get_edges.py`) | green |
| xfail / GH Issues | 0 new xfail, 0 GH Issues filed |
| **Gate-3c rework — flake stability** | 11 hardened tests **× 3 repeats under full-core CPU load = 33/33 pass** (the condition that forced the original 100% failure) |

## Gate-3c REWORKABLE FAIL — resolved (test-only)
Gate 3c flagged `test_graph_legacy_summary_alias_equivalent` as ~50% flaky (100% under load):
it byte-compared two sequential `context_graph` reads whose summary payload carries
`confidence`, which the server's background scoring (GH#405) mutates between the two calls.
Production code is correct — this was a byte-compare-of-a-mutable-field test defect. Fix (no
`crates/**` touched): all two-sequential-read comparisons in the new vnc-044 tests now compare
**structurally** with background-mutable fields (`confidence`, `access_count`,
`last_accessed_at`) normalized to a constant while KEYS are retained (field-set coverage
unweakened; the alias test also asserts the exact 8-field node key set). Hardened 11 tests total
(2 in test_tools.py equality + 2 edge-only 3-way + 5 lifecycle default-mode + 2 lifecycle
golden). Re-verified 3× under injected CPU load — all pass. RISK-COVERAGE-REPORT.md corrected
(the earlier "no flakes / Gaps: None" claim superseded).

## What I changed (test/harness only — NO production `crates/**` edits)
- `harness/client.py::context_graph(...)` — added `detail: str | None = None` + arg-marshalling
  (the required cumulative harness extension; all new tests depend on it).
- `suites/test_tools.py` — new vnc-044 axis section (18 node-ids): axis threading, node/edge
  field-set, markdown-reject ×7 modes, legacy alias + conflict, accept-and-ignore ×2 + bogus,
  live-attr description advertises `detail`. Added `detail="full"` to 3 existing tests that
  assert the full `EntryRecord`/`EdgeRecord` shape.
- `suites/test_lifecycle.py` — new vnc-044 section (8 node-ids): 5-mode default-summary +
  envelope-metadata preservation, `detail=full` golden (chain + subgraph, complete-key-set +
  byte-stability fallback), #913 size-win. Added `detail="full"` to 2 existing subgraph tests.

## Golden method used (R-04/AC-04)
Pre-vnc-044 binary capture is impractical in-harness → the plan's documented fallback:
`detail=full` output parses to the **complete `EntryRecord`/`EdgeRecord` key set** (content,
hashes, timestamps, direction, metadata all present) and is **byte-stable across two identical
runs**. Stated in the report.

## Triage notes
- The 2 initial integration failures were **defects in my own new fixtures**, not production:
  near-duplicate node content tripped the server's >0.9 semantic dedup, collapsing the two
  subgraph endpoints into one entry so no edge existed. Fixed in-place with cross-domain
  distinct content (triage category 3 — bad test assertion). Both now pass. No production bug.
- The 1 `xfailed` in the lifecycle run is **pre-existing and unrelated** (an ONNX/tick-model
  test matched by the `-k` filter). Not introduced by vnc-044; no GH Issue warranted.

## Flagged production concerns
- **None.** No test revealed a production defect. Implementation matches the plan; the
  default→summary flip behaves exactly as the ACs require (full reachable byte-for-byte via
  `detail=full`, summary lean and correct).
- Carried-forward (not a defect, disclosed by design): R-11 — the summary projection carries
  **lifecycle** `EntryRecord.status` only; a capability subgraph shows `"active"` for every
  node. #913's delivery-status tally is NOT delivered (named follow-up #3). Asserted as an
  illustration in `test_graph_summary_shrinks_payload_913`, never as a failing expectation.

## Self-Check
- [x] Unit tests executed (workspace summary captured), all green
- [x] Integration smoke passed (30/30)
- [x] Relevant suites executed (tools/lifecycle/protocol/get_edges per OVERVIEW)
- [x] No new xfail markers; no GH Issues needed
- [x] No integration tests deleted or commented out
- [x] RISK-COVERAGE-REPORT.md maps every risk + integration counts + AC verification
- [x] Gaps: none
- [x] No production `crates/**` modified

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` (task: vnc-044 Stage 3c execution) — surfaced
  #5510/#5509 (ADR-002/001 two-axis contract), #4502/#4503/#4490 (GraphParams layout lock +
  line budget), #5389 (behavioral unit tests for rmcp `#[tool]` handlers). All consistent with
  the implemented resolver/projection.
- Stored: entry #5521 "Two-axis detail=summary default-flip breaks graph integration tests
  asserting full-record shape; distinct fixture content needed for dedup" via
  `/uni-store-pattern` (topic `testing`, tags incl. `ADR-001`) — a forward-looking migration +
  fixture trap the next ADR-001 adopter (`context_get`/`context_search`) will hit.
