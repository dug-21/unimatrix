# Agent Report: vnc-026-agent-2-testplan (Stage 3a — Test Plan Design)

## Deliverables

All under `product/features/vnc-026/test-plan/`:

| File | Covers |
|---|---|
| OVERVIEW.md | Strategy (unit / Layer 1 parity / Layer 2 vs merged F2 / Rust generator / CI / benchmark), full R-01..R-20 risk-to-test map, AC-01..16 coverage map, cross-component deps, infra-001 integration harness plan, binding gate decisions honored |
| index.md | fd-0 stdin (incl. Windows, R-14), exit-0/no-stdout matrix, sync/FNF dispatch, sync-path fs spy (AC-08 amended), AC-13 benchmark, ass-071 SubagentStop dump freebie |
| config.md | FR-06 8-row resolution matrix (R-09), nested-.git split-brain, hash parity goldens vs `project.rs`, partial env pair, no-network proofs |
| normalize.md | Canonical map, Gemini aliases, `__unknown__` sentinel, golden-driven defensive cases |
| build-request.md | Full ADR-001 inventory as Layer 1 cases (R-01), unknown-stdin-field preservation (ass-071 carry-in), AC-14 contract round-trip, ppid collision (R-19) |
| transcript.md | 12,000 B window mechanics, malformed JSONL, multi-byte window-edge splits, truncate_utf8 units, MAX_PRECOMPACT_BYTES |
| transport-http.md | Per-sync-arm `Accept: text/plain` (#4703 shape), timeout structure tests (values ACCEPTED — not flagged), failure-class table, R-15 content-type defense, URL forms |
| transform.md | Byte goldens (AC-04), adversarial escaping incl. U+2028/29 (R-03), single-JSON.stringify grep-gate (ADR-002) |
| delta.md | R-04 UTF-8 property test, end-anchored elision frame shape (ADR-008, asserts NOT span-start), amended AC-15 delta arm (offset-non-advance + NO queue file), R-07 livelock, Layer 2 with the four pinned ADR-008 server-state assertions, AC-10 8-session attribution |
| queue.md | ADR-003 lifecycle, bounds/eviction/prune, 32-frame/256 KiB budget, poison-pill, delta-never-queued directory scan, R-16 frame content scan |
| state.md | R-10 breadcrumb truthfulness matrix + W4 transitions, content-free scan, atomic rename, key sanitization traversal corpus, 7-day prune / SessionClose delete |
| init-remote.md | AC-11 matrix incl. spaced-path regex table (carries the ONLY open gate note), double-fire check, R-18 auth-Ping, commandSource back-compat byte-identity, FR-21/AC-16/R-12 blast-radius gate |
| parity-corpus.md | Corpus manifest (R-02 every-arm audit), Rust generator + branch-coverage assert, comparison rules, CI gates incl. R-20 fail-not-skip non-vacuity guards, Node×OS matrix (AC-12 expansion) |

## Risk Coverage

Every R-01..R-20 risk maps to named scenarios (OVERVIEW.md table). Critical (R-01, R-14) get the
deepest coverage: full ADR-001 inventory + OS-matrix CI + Windows POST smoke. All binding gate
decisions honored: amended AC-15, accepted timeouts, closed gate-note 1, OS-matrix expansion,
four pinned Layer-2 assertions, ass-071 freebie planned as advisory artifact capture.

## Integration Harness Plan (infra-001)

`smoke` only (mandatory minimum gate). Rationale: C-07 — zero server-side production changes; the
sole Rust change is an additive dev-test, MCP-invisible. Feature integration coverage lives in the
feature's own `node:test` Layer 2 suites against the merged F2 server per C-04/NFR-06 cumulative
infra. No new infra-001 tests; an HTTP-hook-client harness dimension would be a GH Issue.

## Open Questions

1. Ownership-regex spaced-path fix (WARN 2) — init-remote.md tests the FIXED pattern; pseudocode
   for init-remote must land the fix before the AC-11 table freezes (only remaining open gate note).
2. `test_ping_unreachable` ordering (does failed Ping leave settings.local.json written?) —
   pseudocode must define; plan asserts whatever is documented, flagged for Stage 3a alignment.
3. Misconfigured-vs-unconfigured breadcrumb distinction (config.md) — partial env pair is class
   `auth` per FR-06; absent-config breadcrumb shape needs pseudocode confirmation.

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing — surfaced #4714 (text/plain negotiation), #4740
  (vnc-025 buffer ADR), #4725 (dual-transport dispatch-guard test pattern — informed the
  delta-never-queued directory-scan assertion), #4743 (shared transcript-block core);
  context_search surfaced #1204 (test plans must cross-reference pseudocode for edge cases —
  applied: plans defer ordering/breadcrumb-shape specifics to pseudocode and flag them as OQs),
  #4751 (ADR-001 corpus decision).
- Stored: nothing novel to store — the plans synthesize existing ADRs (001–008), the risk
  strategy, and already-stored lessons (#2984, #4452, #1203, #4703); no new reusable fixture or
  harness technique was invented at planning time (stub-server helper is standard node:test
  practice; candidates may emerge during Stage 3c execution).
