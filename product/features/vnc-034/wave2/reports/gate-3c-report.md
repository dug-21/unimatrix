# Gate 3c Report: vnc-034 Wave 2 (#727)

> Gate: 3c (Final Risk-Based Validation)
> Feature: vnc-034 Wave 2 (issue #727)
> Branch: feature/vnc-034-wave2
> Date: 2026-06-11
> Result: **PASS**
> Scope: test results vs RISK-TEST-STRATEGY, SPECIFICATION (Wave-2 FRs/ACs), ACCEPTANCE-MAP (AC-W2-R1..R6, AC-CT-C4/C6), ARCHITECTURE (ADR-003/004/005), and the locked decisions D1/D3/D4/D5/D6 + funnel elimination.

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Risk mitigation proof | PASS | RISK-COVERAGE-REPORT maps every Wave-2 risk (R-01/03/04/06/10/12/13) to ≥1 passing test; verified named tests exist in source. |
| 2. Test coverage completeness | PASS | All Wave-2 AC→scenario mappings exercised; integration (10/0) + unit (4002/0) + infra-001 smoke (23/0). Cannot-drive cases each covered by a NAMED passing unit test. |
| 3. Specification compliance | PASS | AC-W2-R1..R6 + AC-CT-C4/C6 each map to verified passing tests; D1 regex exact; no scope drift (D2/D3 splits honored). |
| 4. Architecture compliance | PASS | ADR-003 single-funnel (adapter_for sole dispatch), ADR-004 allowlist, ADR-005 default alias all honored and test-proven at 3c. |
| 5. Knowledge stewardship | PASS | Tester report (`agent-6`) carries `## Knowledge Stewardship` with Queried: + Stored:"nothing novel"+reason. No WARN. |

**Result: PASS (5/5 checks; 0 warnings).** Re-ran the integration suite at 3c: `10 passed; 0 failed` (RC=0). All cannot-drive unit tests, D1 regex, RESERVED_SLUGS, and purge/re-attach logic verified directly in source.

---

## Integration Test Validation (mandatory)

### infra-001 smoke — backward-compat gate (claimed 23/0)
- Report claims 23 passed / 0 failed (351 deselected). Role is **correctly framed**: infra-001 spawns `serve --stdio` (single-project, no HTTP), exercises `ProjectKey::Default` ONLY, and **structurally cannot reach the `/v1/{slug}/` HTTP edge**. Its job is the Default-path byte-for-byte backward-compat regression gate (AC-W2-R2 / AC-CT-C4 end-to-end). This framing is **honest, not masking a gap** — slug routing is proven by the Rust HTTP integration file, which CAN reach the edge. The report states this explicitly (§44–64, cannot-drive case #4).

### project_routing_integration.rs — slug edge (claimed 10/0, RE-RUN at 3c)
Re-ran `cargo test -p unimatrix-server --test project_routing_integration`: **10 passed; 0 failed; 0 ignored** (RC=0, 0.21s). Read the full file — the assertions are **real, not vacuous**:
- **AC-W2-R1** (`test_two_slugs_route_to_distinct_stores`): alpha & beta each DISPATCH (reach the adapter, not 404/400); `Arc::ptr_eq` proves the two slug stores + default are three DISTINCT instances. The `route_mcp` `wraps_store` debug_assert (OQ-PR-4) proves dispatch is to the slug's OWN store.
- **AC-W2-R3** (`test_slug_a_write_unreadable_from_slug_b`, `test_slug_a_write_does_not_appear_in_slug_b`, `test_default_and_slug_interleaved_no_cross_contamination`): A's write is `is_err()` from B's `get`, absent from B's `query_all_entries`, and B+Default entry counts are unchanged by A's writes. Genuine data-layer isolation against the SAME handles the resolver routes to.
- **AC-W2-R2 / AC-CT-C4** (`test_v1_tools_default_unchanged_with_projects`): `/v1/tools/` dispatches to Default WITH projects registered, status IDENTICAL to the no-projects case (no re-point).
- **AC-W2-R5** (`test_n_clients_one_slug_share_store`): two distinct `mcp-session-id` headers on `/v1/alpha/` both dispatch to alpha's adapter; a write by one is visible to a read by the other (shared store); binding is URL-path-derived only.
- **AC-W2-R6** (`test_invalid_slug_path_rejected_at_edge`): 7 distinct edge cases (percent-encoded traversal, encoded relative traversal, encoded dot-dot, uppercase, underscore, leading hyphen, 64-char over-length) each → 400 `invalid project slug` at the parse edge; default+slug stores untouched (no path join).

### Cannot-drive cases — each covered by a NAMED passing unit test (spot-checked, all PRESENT)
| Cannot-drive case | Named test | Located |
|-------------------|-----------|---------|
| CLI lifecycle / D4 / D5 / D6 / OQ-CLI-7 | `test_deregister_reregister_reattaches_to_preserved_chain`, `test_purge_requires_slug_confirmation_or_no_destroy`, `test_purge_with_confirmation_removes_dir_and_deregisters`, `test_purge_then_register_is_fresh_store`, `test_delete_deregisters_and_preserves_data_dir`, `test_register_already_routing_errors_loud`, `test_register_dir_exists_deregistered_reattaches`, `test_register_two_states_distinct_messages`, `test_register_rejects_reserved_tools_shadowing`, `test_register_rejects_reserved_route_segments`, `test_register_reserved_is_separate_from_charset`, `test_register_reserved_exact_match_only` | `src/projects/tests.rs` |
| No-payload-project unrepresentability | `test_per_request_slug_rejected_at_funnel_not_default_store` | `src/http/router/tests.rs` |
| Seam-not-collapsed (AC-CT-C6) | `test_storeresolver_seam_types_present`, `test_bearer_validator_trait_valid_token`, `test_default_resolver_is_the_same_trait_as_wave2_resolver` | `src/http/router/tests.rs`, `src/http/auth/tests.rs` |
| infra-001 slug-routing (by design unreachable) | covered by integration file above | n/a |

Funnel no-bypass also has `test_no_residual_fixed_adapter_path` (`src/http/router/project_resolver/tests.rs`). All present; **none silently dropped**.

### No deleted/commented tests
`git diff main...HEAD` over `crates/unimatrix-server/src` + `tests`: zero removed `#[test]`/`#[tokio::test]`, zero commented-out test fns, zero removed `fn test_*`. RISK-COVERAGE-REPORT includes the integration counts (§25–42, the 10 named).

### Pre-existing flakes are NOT Wave-2 regressions
`git diff --name-only main...HEAD` contains **no token or eval file** — Wave 2 touches neither. `http::token::tests::test_concurrent_creation_no_corruption` (concurrency) and `eval::runner::sweep_tests::test_ac14_correlated_sweep_non_vacuous` (non-determinism) are platform/timing flakes that pass in isolation and cannot be Wave-2 regressions. Confirmed.

---

## Locked-Decision Final Check (3c evidence, not just code shape)

| Decision | Test evidence (3c) | Status |
|----------|--------------------|--------|
| **D1** exact allowlist `^[a-z0-9][a-z0-9-]{0,62}$`, reused | seam.rs:84 `is_empty() || len() > 63` + first-char alnum + lowercase-alnum-or-hyphen = exact regex; NO underscore, NO 64. `test_slug_reject_64_char_discriminator`, `test_slug_reject_underscore_discriminator`, `test_slug_accept_63_char_boundary` PASS; integration `al_pha`/`Alpha`/64-char cases → 400. | PASS |
| **D3** config-driven list, no network | RISK-COVERAGE R-12 row: no per-slug HTTP/network health; `test_list_is_config_driven_not_dir_scan` present. AC-W1-S6 intact. | PASS |
| **D4** delete=de-register, --purge re-type confirm, re-attach preserves chain (OQ-CLI-7) | projects.rs:425 `confirm != Some(slug.as_str())` refuses bare purge; State B re-attaches via non-destructive `Store::open` (projects.rs:282–306); `test_deregister_reregister_reattaches_to_preserved_chain` (chain head identical) + `test_purge_then_register_is_fresh_store` (contrast) PASS. | PASS |
| **D5** reserved incl. `tools` | config.rs:2285 `RESERVED_SLUGS = ["v1","health","observe","tools"]`; `test_register_rejects_reserved_tools_shadowing`, `test_register_reserved_is_separate_from_charset` PASS (`tools` charset-valid yet rejected). | PASS |
| **D6** two-state register | branches on `(data_exists, is_routed)` (projects.rs:268–336); `test_register_already_routing_errors_loud` + `test_register_two_states_distinct_messages` PASS — not collapsed. | PASS |
| **Funnel** adapter_for sole dispatch | `test_no_residual_fixed_adapter_path` (unit) + `test_dispatch_through_adapter_for_no_fixed_bypass` (integration, RE-RUN PASS): per-key write lands ONLY in that key's store; Wave-1 `let _store` discard path gone. | PASS |

All six were PASS at 3b on code shape; at 3c the **test evidence proves them** (re-run integration green; named unit tests confirmed present in source).

---

## Detailed Findings

### Check 1 — Risk mitigation proof
**Status**: PASS
**Evidence**: RISK-COVERAGE-REPORT §70–78 maps R-01 (Critical), R-03, R-04, R-06, R-10, R-12, R-13 each to named tests. Spot-checked the named tests exist in source (all located). Wave-1-only risks (R-02/05/07/08/09/11) are explicitly out of Wave-2 scope (tracked in Wave-1 reports) — correct, no gap.

### Check 2 — Test coverage completeness
**Status**: PASS
**Evidence**: 4002 unit (leader-verified GREEN at 3b) + 10 integration (RE-RUN GREEN at 3c) + 23 infra-001 smoke. Every Wave-2 AC→scenario mapping from the strategy is exercised. The four cannot-drive cases are each backed by a NAMED passing unit test (table above) — structural reasons stated honestly, not coverage gaps.

### Check 3 — Specification compliance
**Status**: PASS
**Evidence**: AC-W2-R1..R6 and AC-CT-C4/C6 each map to a verified passing test (RISK-COVERAGE §84–100, cross-checked against the integration file and unit-test presence). D1 regex is EXACTLY the spec/ADR-004 value, not the drifted issue-#727 `_`/64 variant. No scope additions: D2 (overlay) and D3 (network health) excluded with negative tests.

### Check 4 — Architecture compliance
**Status**: PASS
**Evidence**: ADR-003 single-funnel proven load-bearing at last — `adapter_for(key)` is the sole dispatch route; the Wave-1 `let _store` discard + fixed-adapter fallback is removed (seam.rs:259–330); `debug_assert!(adapter.wraps_store(&store))` ties resolve and dispatch to the same map. ADR-004 allowlist enforced pre-filesystem at the parse edge. ADR-005 default alias (`/v1/tools/`) byte-identical with/without projects. No architectural drift.

### Check 5 — Knowledge stewardship compliance
**Status**: PASS
**Evidence**: `wave2/agents/vnc-034-wave-2-agent-6-tester-report.md` §105–116 carries `## Knowledge Stewardship` with `Queried:` (context_briefing surfacing #4963/#4952/#4968, each with relevance) and `Stored:` ("nothing novel to store" + stated reason: the discriminator-helper/`collect_resp` fixes are local test-mechanics on a pattern already captured by #4963, no cross-feature reuse value). Reason present after "nothing novel" ⇒ no WARN.

---

## Rework Required

None.

## Scope Concerns

None.

## Knowledge Stewardship
- Queried: gate-3c check set against RISK-TEST-STRATEGY + ACCEPTANCE-MAP + locked decisions (all source documents in-context; re-ran integration suite and grep-verified named-test presence rather than querying Unimatrix).
- nothing novel to store -- clean PASS, no recurring cross-feature gate-failure pattern. One feature-specific note for the SM (already in Gate 3b §82, now resolved): the 3b report's WARN that `tests/project_routing_integration.rs` did not exist is CLOSED — the file now exists, ran 10/0, and supplies the over-the-wire AC-W2-R1/R3/R5/R6 + funnel coverage 3b deferred. This is feature history, not a generalizable lesson.
```
