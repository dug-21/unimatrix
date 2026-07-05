# Agent Report — crt-058-agent-2-testplan (Stage 3a, Test Plan Design)

## Deliverables
- `product/features/crt-058/test-plan/OVERVIEW.md` — strategy, risk-to-test + AC-to-component mapping, integration harness plan
- `product/features/crt-058/test-plan/eager-delete-helper.md`
- `product/features/crt-058/test-plan/deprecate-handler.md`
- `product/features/crt-058/test-plan/response-formatter.md`
- `product/features/crt-058/test-plan/audit-emit.md`

## Coverage
All 11 risks (R-01…R-11) mapped to named behavioral tests; all 11 ACs + 6 delivery-time closure items assigned to a component plan. Keystone assertions:
- **AC-10** subset test invokes BOTH real functions (`delete_agent_edges_for_entry` + `run_orphaned_edge_compaction`) over shared-seeded parallel fixtures: `R ⊆ T` AND `R == exactly the two agent edges`; PLUS chokepoint-exclusion against the real `context_correct` handler (R-01 closure) and pinned predicate string (R-02).
- **AC-04** per-source matrix (all 7 `source` values, only `agent` removed) + per-format matrix (Json parses integer field, Summary/Markdown assert rendered value).
- **AC-05 vs AC-06** `Some(0)`→literal `0` behaviorally distinguished from `None`→omitted, all three formats.
- **AC-11** tuple set-equality in audit `metadata`, asserted NOT the `"{}"` sentinel (grounded in `audit.rs:35` empty-substitution guard).
- **R-03** atomic single-statement RETURNING; **R-10** self-loop counted once; **R-05** quarantine/restore byte-identical.

## Grounding (existing infra reused, cumulative)
- Seed helpers `insert_graph_edge_with_source`, `deprecate_entry_with_successor`, `run_orphaned_edge_compaction`, `count_graph_edges`, `total_graph_edges` in `background.rs` test mod (`:1911`) — subset test extends this module.
- Formatter tests extend `mcp/response/mod.rs` `mod tests` (`:209`).
- Audit read-back reuses `server.rs:2205` pattern, extended to project `metadata`.
- Helper DB tests: new split module in `edge_write.rs` (pure `mod tests` at `:420` kept).

## Integration suite plan (infra-001)
smoke (gate) + tools + protocol + lifecycle + edge_cases; volume/security selective. Four new Python tests planned (edges_removed count via wire, zero→`0`, full deprecate→audit chain, no-edge success). AC-10 subset test is a Rust in-process test, NOT Python — the compaction is not tool-invocable and both predicates must run in one process.

## Open questions
- None blocking. R-04 zero-case is RESOLVED (ADR-004: `Some(0)`→`0`); test plans encode the resolved contract.
- Confirm at Stage 3b whether `insert_graph_edge_with_source` should be promoted to `pub(crate)` so the eager-helper DB module and the subset test share ONE seeding helper (R-02 fixture-identity) rather than a drift-prone copy. Flagged in OVERVIEW + eager-delete-helper plans.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — surfaced ADR-003 (#5460, eager⊆tick executable invariant), ADR-001 (#5458), plus test-discipline lessons #3386 (implementors omit edge-case tests listed lower in the plan → I front-loaded edge cases into named tests per component), #3548 (test plan must specify the asserted VALUE, not just presence → all format assertions specify the parsed value), #3644 (parallel pseudocode+test-plan signature divergence → plans pin the exact `edges_removed`-before-`format` position), #4904 (split-module test placement pattern → applied for the edge_write DB module).
- Stored: nothing novel — the load-bearing patterns (multi-pass same-table subset enforced by a behavioral test over both real predicates; parse-based per-format assertions; front-loading edge-case tests) already exist as #3910/#5417/#5427/#3548/#3386. The feature-specific nuance (subset test must exercise the successor-bearing break case via chokepoint-exclusion, not just successor-less fixtures) is captured in the deprecate-handler plan and RISK-TEST-STRATEGY R-01, not generalizable beyond this successor-less/successor-bearing split.
