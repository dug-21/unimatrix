# 893-agent-2-verify — Fresh Verification Report (GH #893)

**Verdict: PASS** (REWORKABLE-clear). Branch `bugfix/893-parity-gate-d5-documented-waiver`. Fresh verifier — did not author the fix.

## Scope of change (confirmed)
Diff is **test-harness-only**, 5 files under `product/test/infra-001/`, +296/−25:
- `harness/parity_outcome.py` — new `gate_disposition()` single-source waiver helper; `DimensionResult.documented_exception` structural flag; `rollup()` stays honest/blocks_c0_proof-blind.
- `harness/parity_matrix_support.py` — `evidence_table` now sources `verdict/exit_code/waived` from `gate_disposition`, adds `waived` + `gate_disposition` keys; `documented_exceptions` keys on the structural flag, not a detail-string sniff.
- `harness/parity_dimensions.py` — precompact `blocks_c0_proof` flipped `True→False` (ADR-009, Unimatrix #5648, human-signed).
- `suites/test_https_uds_parity_matrix.py` — 5 new tests + honesty-pin asserts on the existing seam test.
- `suites/test_parity_dimensions.py` — C1 rename `test_blocks_c0_proof_precompact_is_signed_documented_exception`.

No Rust production code touched → #878 workspace-link smoke not applicable (no Rust change).

## Test execution (all foreground)

| Step | Command | Result |
|------|---------|--------|
| 1. Changed parity suites off-Docker | `pytest suites/test_https_uds_parity_matrix.py suites/test_parity_dimensions.py -v` | **48 passed** (incl. all 6 new #893 tests + C1 rename) |
| 2. Rust workspace | hardened `cargo test --workspace` | **4513 passed, 1 pre-existing flake** (see below) |
| 3. Clippy | `cargo clippy --workspace -- -D warnings` | **clean, RC=0** |
| 4. Smoke gate | `pytest suites/ -v -m smoke --timeout=60` | **35 passed, 0 failed** |
| 5. Parity `-m parity` | see below | off-Docker seam proven; live cross-leg env-blocked |

### Rust flake (pre-existing, unrelated — NOT rework)
`eval::corpus::fixtures_tests::test_ac14_scenario_search_returns_non_empty_ranked_list` failed once under full-workspace parallel load ("both rank_below anchors must be present"). **Passes 3/3 in isolation.** This is the HNSW approximate-retrieval membership flip already tracked in **GH#746** (family: #833, #790). The #893 diff is Python-only and cannot cause a Rust failure. No new issue filed (already tracked). Not fixed (out of scope).

### Parity coverage — ran vs environment-blocked
- **Ran (off-Docker):** 48 matrix+dimension tests including the 6 new #893 tests, plus `test_c3_orchestrator_seam_with_fixture_https_vector` (off-Docker orchestrator wiring proof) — PASSED. These drive the **real** production functions (`assert_rollup`, `gate_disposition`, `evidence_table`, `classify_dimension`) with synthetic capture bundles — the exact seam #893 changed.
- **Environment-blocked:** the 2 live cross-leg HTTPS↔UDS drives (`test_https_uds_parity`, `test_https_uds_parity_matrix`) SKIP because `UNIMATRIX_HTTPS_SMOKE` is unset — they shell out to an external live HTTPS smoke executable (Stage 3c/cloud activity, not wired here). These exercise the transport drive, **not** the disposition/waiver logic #893 changed; that logic is fully covered off-Docker.

## End-to-end intended behavior — verified
Confirmed the disposition seam behaves as specified (assertions inspected + executed against real production functions):
- **Documented-exception-only → JOB WAIVED, artifact stays honest:** `assert_rollup` does NOT raise; `evidence_table` carries `verdict:ERROR`, `exit_code:7`, non-empty `documented_exceptions`, `waived:true`, `gate_disposition:PASS`. (the honesty pin — `test_matrix_documented_exception_only_is_waived_but_artifact_stays_error` + the seam test)
- **Undocumented infra still raises** (`test_matrix_undocumented_infra_still_raises`).
- **Documented exception on a still-`blocks_c0_proof=True` dim still raises** — keyed on the registry flag, not the id (`test_matrix_documented_exception_on_blocking_dim_still_raises`).
- **Real PARITY_FAIL alongside a documented gap still raises RED** — waiver never masks a divergence; RED checked before INFRA (`test_matrix_documented_exception_with_real_parity_fail_raises_red`).
- **`documented_exception` set ONLY by classify branch 1b** — never by empty/misroute infra, never by a comparator (`test_matrix_documented_exception_flag_set_only_by_branch_1b`).
- **precompact `blocks_c0_proof` flipped True→False** (`test_blocks_c0_proof_precompact_is_signed_documented_exception` + registry diff).

Seam integrity: `gate_disposition` is the single source of the waiver, consumed by both `evidence_table` and `assert_rollup` (no drift); `rollup` measurement verdict remains blocks_c0_proof-blind and is never rounded up. Tests drive the real gate functions directly, not proxies.

## GH issues filed
None. The one Rust flake is already tracked (GH#746). No failure attributable to #893.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` + `context_get(5648)` — ADR-009 (#5648) is the human-signed decision this fix implements; #5323/#5376 already codify the off-Docker seam-teeth verification discipline this run relied on; #746 is the pre-existing AC-14 flake.
- Stored: nothing novel to store — the verification approach (drive real disposition functions off-Docker; live cross-leg is env-blocked and wouldn't exercise the changed logic) is already covered by #5323/#5376, and the flake triage is a known GH issue (bugs are GH issues, not lessons).
