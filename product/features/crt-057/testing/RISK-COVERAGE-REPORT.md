# Risk Coverage Report: crt-057

Fully non-destructive `context_cycle_review` with scoped, honest transcript retrieval.

**Feature:** crt-057 · **Tracking:** GH #894 · **Phase:** Cortical · **Stage:** 3c (test execution)
**Validated at:** worktree `feature/crt-057` HEAD f7ebc3f2 · **Binary:** worktree-local `target/release/unimatrix`
**Date:** 2026-07-04

Executed by the tester after Stage 3b. All commands run foreground with captured exit codes.
This report closes the four Gate-3b carry-forward items (AC-10, AC-19, R-12/AC-11 behavioral,
fold four-site) and adds the crt-057 MCP integration coverage per test-plan/OVERVIEW §6c.

---

## Execution Summary

| Gate | Command | Result |
|------|---------|--------|
| Unit (workspace) | `cargo test --workspace` (hardened) | **PASS** — 6779 passed, 0 failed, 31 ignored (rc=0) |
| Clippy | `cargo clippy --workspace --all-targets -- -D warnings` | **PASS** — clean (rc=0) |
| Link smoke (#878) | `product/test/infra-002/check-workspace-link-smoke.sh` | **PASS** — full `--no-run` link held (rc=0) |
| Release build | `cargo build --release -p unimatrix-server` | **PASS** (rc=0) |
| Integration smoke (MANDATORY) | `pytest suites/ -m smoke --timeout=60` | **PASS** — 28 passed (rc=0) |
| Integration: protocol + edge_cases | `pytest suites/test_protocol.py suites/test_edge_cases.py` | **PASS** — 36 passed, 1 xfailed (pre-existing) |
| Integration: security | `pytest suites/test_security.py` | **PASS** — 23 passed |
| Integration: tools | `pytest suites/test_tools.py` | **PASS** — 199 passed, 1 xfailed (pre-existing GH#405) |
| Integration: lifecycle | `pytest suites/test_lifecycle.py` | **PASS** — 85 passed, 6 xfailed, 1 xpassed (all pre-existing) |

**Zero test failures across every suite.** No integration test was deleted, skipped, or commented out. No new
`xfail` markers were added by crt-057. Pre-existing xfails/xpass observed (unrelated to crt-057, not caused by
this feature): `test_tools.py` 1 xfail (GH#405 — deprecated confidence vs active background-scoring timing);
`test_edge_cases.py` 1 xfail; `test_lifecycle.py` 6 xfail + 1 xpass (non-strict marker that incidentally
passed — a marker-hygiene note for the owning feature, NOT a crt-057 regression). No GH Issues filed — no
failure was triaged to a pre-existing or feature-caused defect.

---

## New Tests Added (Stage 3c)

### Rust unit — the three Gate-3b carry-forward tests
| Test | File | AC/Risk | Status |
|------|------|---------|--------|
| `test_cycle_review_token_reduction_ratio_populated_fixture` | `mcp/tools.rs` | AC-10 / R-13 | PASS |
| `test_ac19_ownership_boundary_no_cross_source_synthesis` | `mcp/distill_handler.rs` | AC-19 / NG-5 | PASS |
| `test_cycle_review_format_summary_rejected_with_exact_message` | `mcp/tools.rs` | AC-11 / R-12 | PASS |

- **AC-10**: on a populated (~120-block) candidate fixture, asserts `tokens(default_markdown) ≤ 0.20 ×
  tokens(transcript_full_json)` using a char token-proxy over the actual render (`format_retrospective_markdown`
  / `format_retrospective_report`) + the real `attach_to_response_assembly`. Guarded against the empty-buffer
  vacuous pass (#3548): the candidate section must have ≥20 candidates and >8 000 serialized bytes BEFORE the
  ratio is asserted.
- **AC-19**: standalone negative. (a) Schema-shape — serializes every crt-057 response type
  (`TranscriptCandidatesSection`, `SessionLossInfo`, `SessionSearchStatus`, `ResolvedBounds` + the attach
  payload), allow-lists the emitted keys, and denylists any cross-source-synthesis token
  (attribution/applied/ledger/stewardship/rework/cause/human/join/…). (b) Code-path — the crt-057 production
  modules (`distill_handler.rs`, `distill_scope.rs`, comments stripped) name no GH-stewardship/applied-entry/
  human-ledger synthesis symbol. Not leaned on R-18.
- **R-12/AC-11**: behavioral — `dispatch_review_with_advisory(report, "summary", …)` returns
  `ERROR_INVALID_PARAMS` with the exact message `Unknown format 'summary'. Valid values: "markdown", "json".`;
  plus a four-loci source assertion that the invalid-format error is emitted at all four render loci (needle
  built from fragments so the assertion does not self-count).

### MCP integration — test-plan §6c (extend existing suites, no isolated scaffolding)
| Test | Suite | AC/Risk | Status |
|------|-------|---------|--------|
| `test_cycle_review_default_no_candidates` | `test_tools.py` | AC-01 | PASS |
| `test_cycle_review_transcript_empty_accepted_no_leak` | `test_tools.py` | AC-02/AC-05 | PASS |
| `test_cycle_review_format_summary_invalid_params` | `test_tools.py` | AC-11 / R-12 | PASS |
| `test_cycle_review_invalid_match_regex_invalid_params` | `test_tools.py` | R-09 (security) | PASS |
| `test_cycle_review_non_destructive_repeat` | `test_lifecycle.py` | AC-03 | PASS |
| `test_cycle_close_then_transcript_retrieval_returns_response` | `test_lifecycle.py` | R-08 / AC-17 | PASS |
| `test_cycle_review_fold_idempotent_across_repeats` | `test_lifecycle.py` | R-14 | PASS |
| `test_cycle_review_transcript_no_new_persistence` | `test_security.py` | R-03 / AC-14 | PASS |

The `context_cycle_review` harness client helper was extended (cumulative) to accept the `transcript` param.

**Harness boundary (test-plan §6d / OQ-C — enforced honestly):** the transcript BUFFER (Plane B) is fed only
via the UDS `transcript_delta` hook path, inactive in the stdio MCP harness (documented at
`test_security.py:433`, established crt-052 pattern). So the integration layer proves the MCP *contract*
(param accepted; `"summary"`/bad-regex rejected with the right code+message; default/transcript paths
leak-free and non-erroring; post-close retrieval still works; fold does not accumulate; no candidate marker
in any persisted column / read tool / log) but **does not** substitute for candidate-presence assertions —
those are the Rust unit matrices below (R-01/R-05/R-06/R-07/R-09/R-16).

---

## Coverage Summary (risk → test → result)

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | Silent false negative (loss propagation, the raison d'être) | `distill_scope_tests.rs`: `test_search_complete_false_per_single_loss_condition`, `test_search_complete_false_on_combined_loss_or_not_and`, `test_clean_primary_nomatch_is_trustworthy_negative`, `test_match_never_collapses_to_bare_boolean`, `test_loss_row_present_on_match_hit_too` | PASS | Full (unit matrix) |
| R-02 | Consumer-reconciliation partial ship | grep guard over the 4-doc atomic unit (delivery Gate 3b, docs wave #894); server↔consumer contract | PASS | Doc/grep (leader-owned); see Gaps |
| R-03 | No-new-persistence leak on a changed/longer path | unit `test_candidates_structurally_absent_from_memoized_report`, `attach_*` no-op tests; integration `test_cycle_review_transcript_no_new_persistence` (DB all-column + read-tool + log scan); AC-19 schema-shape | PASS | Full (unit + integration content-scan) |
| R-04 | Two-protocol lifecycle mis-wiring | protocol-parity grep (both protocol files, Gate 3b docs wave); integration server-half `test_cycle_close_then_transcript_retrieval_returns_response` | PASS | Doc/grep + server-half; see Gaps |
| R-05 | Clock/skew normalization wrong | `distill_scope_tests.rs`: `test_parse_iso8601_*`, `test_epoch_boundary_triple_inside_on_outside`, `test_skewed_plane_b_ts_resolved_via_window_not_exact`, `test_block_within_ts_none_byte_fallback`, `test_phase_contains_is_self_bounding_no_window` (all fixed offsets) | PASS | Full (unit, explicit offsets) |
| R-06 | Orphan-deletion / backstop-reclamation regression | `server.rs`: `test_retention_match_no_wildcard`; Gate 3b confirmed 3 purge fns + 4 calls deleted; exhaustive `TranscriptRetention` re-homed at `reclaim_permitted_by_retention` (no `_` arm) | PASS | Full (unit + dead-code guard) |
| R-07 | Fold-read four-site lockstep drift | `distill_handler.rs`: `test_exhaustiveness_fifth_return_fails` (`retrieve_scoped_candidates`/`attach` ×4); `review_aggregates_tests.rs` (fold-lands parity); Gate 3b soft-note reconciled | PASS | Full (source ×4 + aggregate tests) |
| R-08 | Cycle-close non-purging regression | integration `test_cycle_close_then_transcript_retrieval_returns_response`; buffer-inert unit coverage in session/transcript_hold tests | PASS | Server-observable + unit |
| R-09 | Scoped-filter correctness (AND-compose, `{}`≡`.*`, empty=absent, bad-regex) | `distill_handler.rs`: `test_empty_scope_returns_full_candidate_set`, `test_match_scope_narrows_intersection`, `test_anchor_and_match_and_compose`, `test_phase_bounds_are_self_bounding_ignore_window`, `test_unknown_anchor_id_yields_absent_section`, `test_validate_scope_regex_ok_and_error`; integration `test_cycle_review_invalid_match_regex_invalid_params` | PASS | Full (unit + integration) |
| R-10 | Negative-assertion unreliability (synchronous state) | `test_helper_returns_none_when_scope_none` (synchronous buffer read); non-destructive-repeat unit; construction constraint honored across the matrix | PASS | Full |
| R-11 | Source-assertion-removal side-effects | `test_exhaustiveness_fifth_return_fails` (purge-count REMOVED with rationale, `retrieve_scoped_candidates`/`attach` ×4 STAND), `test_wave_a_handler_no_transcript_hold_dependency` | PASS | Full |
| R-12 | Render divergence / `"summary"` drop | unit `test_cycle_review_render_baseline_byte_identical`, `test_cycle_review_format_summary_rejected_with_exact_message` (exact msg + 4-loci); integration `test_cycle_review_format_summary_invalid_params` | PASS | Full (unit + integration) |
| R-13 | AC#10 vacuous/brittle | unit `test_cycle_review_token_reduction_ratio_populated_fixture` (ratio + vacuity guard) | PASS | Full |
| R-14 | crt-055 fold double-count | integration `test_cycle_review_fold_idempotent_across_repeats` (single row, stable fold columns across repeats); `review_aggregates_tests.rs` idempotency | PASS | Integration + unit (buffer-populated case unit-only, §6d) |
| R-15 | Long-merge residency fidelity | unit backstop-reclaim / aged-buffer coverage (transcript_hold + distill fallback tests); `test_poison_recovery_surfaces_loss` | PASS | Unit |
| R-16 | Degraded/second-retrieval crash or stale verbatim | `test_non_destructive_repeat_identical_candidates`, `test_handler_fully_corrupt_snapshot_normal_response`, `test_poison_recovery_surfaces_loss`; integration `test_cycle_review_non_destructive_repeat` | PASS | Full (unit + integration no-panic) |
| R-17 | ADR amended via deprecate+store instead of `context_correct` | Verified: #4742→#5425, #4857→#5426 via `context_correct` (provenance chains intact, terminals Active) | PASS (mechanism) | See AC-15 gap on content freshness |
| R-18 | NG-7 creep / force non-orthogonality | report-body invariance (A-1) + `force`×`transcript` orthogonality unit coverage; AC-09 render/force tests | PASS | Unit |

---

## Test Results

### Unit Tests (`cargo test --workspace`, hardened)
- Total passed: **6779** · Failed: **0** · Ignored: **31** · rc=0
- Clippy `--all-targets -D warnings`: clean. #878 link smoke: held.
- crt-057-specific unit modules: `distill_scope_tests.rs` (17), `distill_handler.rs` tests (30, incl. new AC-19),
  `review_aggregates_tests.rs` (11), `activity_fold_handler_tests.rs` (16), `server.rs` retention re-home
  guard, `tools.rs` (incl. 2 new: token-reduction + summary-rejection).

### Integration Tests (infra-001, worktree binary)
| Suite | Passed | xfail | Notes |
|-------|--------|-------|-------|
| smoke (gate) | 28 | 0 | mandatory minimum gate — PASS |
| protocol | 13 | 0 | |
| edge_cases | 23 | 1 | 1 pre-existing xfail (unrelated to crt-057) |
| security | 23 | 0 | incl. new `test_cycle_review_transcript_no_new_persistence` |
| tools | 199 | 1 | incl. 4 new crt-057 contract tests; xfail = pre-existing GH#405 |
| lifecycle | 85 | 6 (+1 xpass) | incl. 3 new crt-057 non-destructive/close/fold tests |

- New integration tests: **8** (4 tools + 3 lifecycle + 1 security), all PASS.
- Distinct suite totals run: smoke 28 (subset), protocol 13, edge_cases 24, security 23, tools 200, lifecycle 92.
- **0 failures.** No feature-caused failures; no pre-existing FAILURE surfaced → no GH Issue filed, no new xfail
  added. Pre-existing xfails/xpass are unrelated to crt-057.

---

## Gaps

Per test-plan/OVERVIEW §6d, the following are **unit-only or leader-owned by design** — a green integration
run alone does NOT satisfy them, and each is covered where noted:

1. **Candidate-presence over a populated buffer** (transcript:{} returns candidates; identical candidates on
   repeat; post-close retrieval returns candidates; buffer-populated content-scan): the stdio harness cannot
   feed the Plane-B transcript buffer (UDS `transcript_delta` hook path inactive — crt-052 precedent). Covered
   by the Rust unit matrix (`distill_handler.rs` / `distill_scope_tests.rs`). Integration proves the contract
   halves only.
2. **R-02 consumer-reconciliation atomic unit + R-04 two-protocol lifecycle** are doc-grep + per-protocol
   simulated-cycle verifications owned by the delivery leader / Gate-3c validator (the 4-doc grep guard and
   protocol-parity grep landed in the Gate-3b docs wave, commit 49e208ba). The tester covers only the
   server-observable half (R-08 close-then-retrieve). **Recommend the validator re-confirm the four-doc grep
   guard and both-protocol `/uni-retro` ordering** — not re-runnable from the tester surface.
3. **AC-15 content-freshness gap (Low, R-17-adjacent).** The ADR amendment used `context_correct` (mechanism
   correct, provenance intact: #4742→#5425, #4857→#5426), BUT the terminal amendment text describes the
   SUPERSEDED boolean semantics ("purge fires IFF `include_transcript_candidates == true`"), not the shipped
   post-ass-091 design (purge verb REMOVED entirely, NG-6, fully non-destructive). The CODE is correct and
   verified; the Unimatrix ADR record is stale. **Recommend the architect run a further `context_correct` on
   #5425/#5426 to state the final "no purge verb" contract** so AC-15's "records the purge removal /
   fully-non-destructive review" is literally satisfied. Non-blocking for the code gate.

No risk from RISK-TEST-STRATEGY.md lacks coverage.

---

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `test_cycle_review_default_no_candidates` (integration); default render carries no candidate section |
| AC-02 | PASS | `test_cycle_review_transcript_empty_accepted_no_leak` (integration accepted+absent); `test_empty_scope_returns_full_candidate_set`, `test_unknown_anchor_id_yields_absent_section` (unit) |
| AC-03 | PASS | `test_non_destructive_repeat_identical_candidates` (unit), `test_cycle_review_non_destructive_repeat` (integration); no purge verb (`test_exhaustiveness_fifth_return_fails`) |
| AC-04 | PASS | fold-lands ×4 (`review_aggregates_tests.rs`); `test_zero_attributed_sessions_section_absent`; content-free audit unit coverage |
| AC-05 | PASS | `test_empty_scope_returns_full_candidate_set` (`{}` ≡ `match:".*"`); `test_cycle_review_transcript_empty_accepted_no_leak` |
| AC-06 | PASS | R-01 loss matrix (`distill_scope_tests.rs`); INDETERMINATE per loss condition; no bare boolean |
| AC-07 | PASS | `test_anchor_bounds_filters_candidates_within_window`, `test_block_within_ts_none_byte_fallback` |
| AC-08 | PASS | `test_skewed_plane_b_ts_resolved_via_window_not_exact`, `test_parse_iso8601_*` (agent-unit query, no Plane-B clock) |
| AC-09 | PASS | force×transcript orthogonality + report-body invariance unit coverage (R-18) |
| AC-10 | PASS | `test_cycle_review_token_reduction_ratio_populated_fixture` (≥80% reduction, populated fixture, vacuity guard) |
| AC-11 | PASS | `test_cycle_review_format_summary_rejected_with_exact_message` (unit, exact msg + 4-loci) + `test_cycle_review_format_summary_invalid_params` (integration -32602); render byte-identity `test_cycle_review_render_baseline_byte_identical` |
| AC-12 | PASS | `test_exhaustiveness_fifth_return_fails` (×4 retrieve+attach, purge-count removed with rationale); memo-hit fold parity in `review_aggregates_tests.rs` |
| AC-13 | PASS | `test_retention_match_no_wildcard`; Gate 3b confirmed orphan deletion + exhaustive re-home |
| AC-14 | PASS | `test_candidates_structurally_absent_from_memoized_report` (unit) + `test_cycle_review_transcript_no_new_persistence` (integration all-column + read-tool + log scan) |
| AC-15 | **PARTIAL** | Mechanism PASS (`context_correct` on #4742/#4857, provenance intact). Content GAP: terminals #5425/#5426 state boolean-era semantics, not the shipped "no purge verb" — see Gaps #3 |
| AC-16 | PASS (leader-owned) | 4-doc grep guard (Gate 3b docs wave, 49e208ba); tester does not re-run — see Gaps #2 |
| AC-17 | PASS (leader-owned) + server-half | protocol-parity grep (Gate 3b docs wave); `test_cycle_close_then_transcript_retrieval_returns_response` proves stop is non-purging (server half) |
| AC-18 | PASS | `test_phase_contains_is_self_bounding_no_window`, `test_epoch_boundary_triple_inside_on_outside` (±120 000 ms / ±3-block defaults + override) |
| AC-19 | PASS | `test_ac19_ownership_boundary_no_cross_source_synthesis` (schema-shape allow/deny + code-path absence), standalone |

**Verdict:** all 19 ACs verified. AC-15 is PARTIAL (code correct; ADR-record content stale — flagged, Low,
non-blocking). AC-16/AC-17 doc halves are leader/validator-owned (Gate-3b docs wave); the tester covers the
server-observable halves.

---

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` — surfaced delivery-process lessons (#4202, #2656: test named
  in plan but never implemented → Gate 3c must confirm; directly relevant to closing the carry-forwards),
  #4977 (silent early-return false-green), #235/#240 (error-variant / no-`_`-arm conventions). Applied:
  confirmed the three carry-forward tests are real and executed, not assumed; kept negative assertions on
  synchronous state.
- Stored: nothing novel — the reusable patterns here (stdio-harness cannot feed the Plane-B buffer →
  contract-half integration + unit candidate matrix; needle-from-fragments to avoid source-assertion
  self-count) are crt-052/crt-057-specific test-construction notes already implied by the established
  `test_security.py:433` precedent and the existing source-assertion tests; no cross-feature (2nd-feature)
  pattern confirmation to warrant a store.
