# Agent Report: nan-007-agent-1-pseudocode

**Feature**: nan-007 (W1-3 Evaluation Harness)
**Stage**: Session 2 Stage 3a — Pseudocode
**Agent ID**: nan-007-agent-1-pseudocode

## Work Completed

Produced all nine pseudocode files for the eight nan-007 components:

| File | Component | Status |
|------|-----------|--------|
| `pseudocode/OVERVIEW.md` | Cross-cutting data flow, shared types, integration surface | Complete |
| `pseudocode/snapshot.md` | snapshot.rs (D1) — VACUUM INTO, live-DB guard | Complete |
| `pseudocode/eval-profile.md` | eval/profile.rs — EvalProfile, EvalServiceLayer, EvalError | Complete |
| `pseudocode/eval-scenarios.md` | eval/scenarios.rs (D2) — query_log scan, JSONL | Complete |
| `pseudocode/eval-runner.md` | eval/runner.rs (D3) — replay, metrics | Complete |
| `pseudocode/eval-report.md` | eval/report.rs (D4) — Markdown, five sections | Complete |
| `pseudocode/uds-client.md` | uds_client.py (D5) — UnimatrixUdsClient | Complete |
| `pseudocode/hook-client.md` | hook_client.py (D6) — UnimatrixHookClient | Complete |
| `pseudocode/cli-wiring.md` | main.rs + eval/mod.rs — CLI dispatch | Complete |

## Source Documents Read

- `product/features/nan-007/IMPLEMENTATION-BRIEF.md` — constraints, resolved decisions, function signatures
- `product/features/nan-007/architecture/ARCHITECTURE.md` — component breakdown, integration surface, ADR rationale
- `product/features/nan-007/specification/SPECIFICATION.md` — FR/NFR/AC requirements
- `product/features/nan-007/RISK-TEST-STRATEGY.md` — 18 risks, test scenarios, failure mode table
- `product/features/nan-007/architecture/ADR-001-vacuum-into-rusqlite-sync.md` — sqlx + block_export_sync decision
- `product/features/nan-007/architecture/ADR-002-analytics-mode-suppression.md` — AnalyticsMode::Suppressed design
- `product/features/nan-007/architecture/ADR-005-nested-eval-subcommand-clap4.md` — clap nested enum
- `crates/unimatrix-server/src/export.rs` — block_export_sync pattern
- `crates/unimatrix-server/src/test_support.rs` — TestHarness construction model
- `crates/unimatrix-server/src/main.rs` — existing Command enum and dispatch
- `product/test/infra-001/harness/client.py` — UnimatrixClient framing reference

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for "evaluation harness patterns conventions" (category: pattern) — found #426 (Shadow Mode Evaluation Pipeline), #724 (Behavior-Based Ranking Tests), #1042 (Pure Computation Engine Module). None directly applicable; pseudocode follows existing export.rs + TestHarness patterns instead.
- Queried: `/uni-query-patterns` for "nan-007 architectural decisions" (category: decision, topic: nan-007) — found all five ADRs (#2585–#2588, #2602) confirming design decisions used in pseudocode.
- Deviations from established patterns: none. Block_export_sync pattern from export.rs is followed exactly. TestHarness construction model from test_support.rs is followed exactly for EvalServiceLayer.

## Open Questions Flagged for Implementer

These were found during pseudocode design and are documented in `eval-profile.md`:

**OQ-A (eval/profile.rs)**: `VectorIndex::new()` requires `Arc<Store>`. The eval engine
cannot call `SqlxStore::open()` on the snapshot (C-02). Implementer must inspect
`VectorIndex::new()` signature to determine whether it can accept a raw `SqlitePool`
or requires a `Store`-compatible wrapper. A minimal read-only Store wrapper may need
to be created in `eval/profile.rs` (not a new crate). This is an internal API inspection
task, not a design decision.

**OQ-B (eval/profile.rs)**: `AuditLog::new()` currently accepts `Arc<Store>`. Same
resolution path as OQ-A: use the Store wrapper from OQ-A if needed, or confirm AuditLog
accepts a pool. Writes are expected to fail silently since pool is read-only.

**OQ-C (eval/profile.rs)**: Confirm `ServiceLayer::with_rate_config()` parameter order
matches `TestHarness::new()`. The pseudocode uses the TestHarness parameter order from
`test_support.rs`; the actual signature should be verified against `services/mod.rs`.

**OQ-D (eval/scenarios.rs)**: Exact `query_log` column names must be verified against
the migration file in `crates/unimatrix-store/src/migrations/`. The pseudocode assumes
`query_text`, `result_entry_ids`, `similarity_scores` — these must match the actual schema.

These are not design gaps; they are internal API verification tasks that implementation
agents should resolve by reading the relevant source files before coding.

## Self-Check

- [x] Architecture output was read before writing any pseudocode
- [x] No invented interface names — every name traced to architecture Integration Surface table or existing source files
- [x] Output is per-component (OVERVIEW.md + one file per component), not monolithic
- [x] Each component file includes function signatures, error handling, and test scenarios
- [x] No TODO, placeholder functions, or TBD sections — open questions are implementation-inspection tasks, not design gaps
- [x] Shared types defined in OVERVIEW.md match usage in component files
- [x] All output files within `product/features/nan-007/pseudocode/`
- [x] Knowledge Stewardship report block included
