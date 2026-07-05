# Agent Report — nxs-014-agent-2-testplan (Stage 3a Test Plan Design)

## Deliverables
- `product/features/nxs-014/test-plan/OVERVIEW.md` — test strategy, risk-to-test map, integration harness plan, AC index
- `product/features/nxs-014/test-plan/chain-verify-core.md`
- `product/features/nxs-014/test-plan/correction-write-path.md`
- `product/features/nxs-014/test-plan/import-validation.md`
- `product/features/nxs-014/test-plan/verify-cli.md`
- `product/features/nxs-014/test-plan/readme-integrity.md`

Component plans map 1:1 to the IMPLEMENTATION-BRIEF §Component Map.

## Risk coverage (R-01..R-12 all mapped)
- Critical: R-01 (DB read-back killer, fails on struct-only half-fix) → correction-write-path;
  R-02 (Deprecated predecessor) covered on BOTH callers + loader guard → chain-verify-core, import-validation, verify-cli.
- High: R-03 (mixed-corpus clean + mutation loud), R-04 (existing import tests unchanged + AND-halves),
  R-05 (ROLLBACK proven by post-failure row count).
- Medium: R-06/AC-10 frozen-vector tripwire, R-07 round-trip, R-08 empty-predecessor reject,
  R-09 each ViolationKind, R-10 CLI both branches + read-only, R-11 README.
- Low: R-12 is_clean property.

All AC-01..AC-12 indexed to a component plan (OVERVIEW §AC index).

## Integration suite plan (Stage 3c)
- MANDATORY: `smoke`. Required: `lifecycle`, `tools` (context_correct regression); `protocol` via smoke.
- NO new infra-001 tests: AC-01/02 (persisted columns) are not MCP-visible → Rust DB read-back only;
  AC-09 CLI verify → Rust integration test mirroring Export/Import CLI, not the MCP harness;
  import verify (AC-04/05) → Rust import test module. A Python subprocess CLI test would need harness
  infra change → GH Issue if a stable machine surface (`--json`) is later wanted (ARCHITECTURE Open Q2).

## Open questions
1. **CLI test harness**: confirm the existing `Export`/`Import` subcommand integration tests use
   `assert_cmd` vs `run_import`-style in-process calls — verify-cli.md should reuse whichever exists.
   Dev/pseudocode should surface the exact pattern.
2. **read-only open assertion (test_verify_cli_opens_readonly)**: strongest form is a behavioral
   "DB file unmodified after run" check; a call-site/review grep is the fallback. Pseudocode should
   confirm `run_verify` uses `SqlxStore::open_readonly` so the behavioral test is feasible.
3. **`ChainReport` output surface**: v1 is human-readable text (SR-05). Tests assert id-naming in
   `Display`/`describe()`. If dev adds structured fields, keep `Display` stable for AC-09 assertions.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_search — ADR-001/002/003 (#5502/#5503/#5504),
  capability #5478 (KI-CHAIN-XV), test conventions #238/#2271/#2149 (SqlxStore test setup,
  `open_test_store`, `PoolConfig::test_default`), lesson #3611 (multi-site interface fix — reinforces
  R-01 two-site DB read-back), #4177 tautological-pass / #4473 warn+continue / #5180 green-on-skip
  (informs the fail-loud + no-green-on-skip assertions). Grounded plans against actual code
  (`write_ext.rs` struct :539 / bind :582, `query_all_entries` at read.rs:324, `validate_hashes`
  existence-check at import/mod.rs:429, `hash.rs` frozen vectors).
- Stored: nothing novel to store — the load-bearing patterns (DB read-back to defeat a two-site
  half-fix; Deprecated-predecessor loader coverage) are feature-specific and already subsumed by
  stored lessons #3611 (multi-site) and #4177/#4473/#5180 (false-green family). No cross-feature
  pattern visible across 2+ features beyond those; storing would duplicate.
