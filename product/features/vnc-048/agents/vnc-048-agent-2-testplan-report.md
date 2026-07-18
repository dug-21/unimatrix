# Agent Report — vnc-048-agent-2-testplan (Stage 3a: Test Plan Design)

## Deliverables
- product/features/vnc-048/test-plan/OVERVIEW.md — strategy, risk→test→AC map, gate non-negotiables, deploy-shape axis, integration harness plan, WARN-1 carry.
- product/features/vnc-048/test-plan/resolve_slug_store.md — funnel unit tests (ordering, base derivation, validation edge, existence gate, host-base-miss).
- product/features/vnc-048/test-plan/export.md — AC-09 disagreement seam (TOP), divergence guard, stderr summary, no-slug parity, stray-hash boundary.
- product/features/vnc-048/test-plan/import.md — AC-10 round-trip A→B (TOP), AC-12 served-vector-from-start (TOP), live-PID gate, non-empty-audit refusal, vector redirect.
- product/features/vnc-048/test-plan/main-dispatch.md — clap wiring + help contract.
- product/features/vnc-048/test-plan/readme.md — canonical restore-sequence file-check.

## Gate non-negotiables covered
- AC-09 / R-01 S1: seed A via runtime literal-slug layout at X/<slug>, seed disjoint non-empty B at X/<hash>, read via run_export_with_base(slug=..); assert emitted==A ∧ ∩B==∅; N=1 same-path explicitly called ceremonial (#4974). Paired divergence guard (no-slug emits B).
- AC-12 / R-03 S2: full register→stop→import--slug→start CLI sequence + served vector query, proven from start not disk state.

## Integration suite plan
- No new infra-001 pytest tests: feature is CLI-only (export/import subcommands), no MCP-visible behavior (C-9). Falls under "do NOT plan integration tests" (no MCP surface).
- infra-001 smoke (`pytest -m smoke`) runs as MANDATORY non-regression gate (proves visibility raises / signature changes leaked nothing onto tool surface; note tool-count assertion is 15 per #942).
- Primary functional coverage = Rust cargo integration tests in existing export_integration.rs / import_integration.rs (extend, cumulative) + projects.rs units.
- #878 full-workspace LINK smoke MANDATORY (new test binaries added).

## Open questions
- AC-12 served-vector-query surface: the exact query entry point the functional test hits post-`start` (in-process client vs. MCP call vs. CLI) is left to Stage 3b pseudocode/impl — plan specifies "served vector search through the running daemon," implementation picks the concrete surface. Flag for import rust-dev + tester to align (col-030 divergence risk).
- Live-PID test needs a controllable live PID that passes is_process_alive + is_unimatrix_process (/proc/{pid}/cmdline). Plan suggests the test process's own PID or a controlled child; feasibility of the is_unimatrix_process cmdline match in-test to confirm in 3b.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing (vnc-048 test plan) — surfaced #5693 ADR-001 reuse triad, #5694 ADR-002 pre-open existence gate, #5696 ADR-004 vector rebuild target, #4725 two-transport-single-dispatch test pattern, #3644 col-030 parallel-pseudocode/test-plan divergence lesson, #5667 validation lesson. All applied (seam-must-drive-real-entry-point discipline, divergence guard, interface-drift caution).
- Declined: nothing novel to store — the operative patterns (ceremonial-seam-unless-value #4974, two-resolver-disagreement #5507, seed-one/read-other) already exist; this feature's plan is a specific interpretation, not a new cross-feature pattern. Promotable at retro only if the four-deploy-shape-as-coverage-axis recurs in the sibling-CLI slug-awareness work.
</content>
