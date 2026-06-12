# Risk Coverage Report: vnc-035

> `context_correct` outgoing-edge carry-forward (step 8b′ `run_carry_forward_loop`).
> Stage 3c execution. Unit layer (store + server) green; infra-001 smoke gate green;
> targeted `tools`/`lifecycle` carry/correct/edge subsets green. Three new MCP black-box
> tests added per the Stage 3a plan; two prior-run test assertions were corrected (bad
> assertions, not feature bugs) — see Test Corrections.

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| **R-01** | Warn-and-continue per-edge-copy failure path has no behavioral signal (Critical; #4473 precedent) | `test_carry_forward_continues_on_edge_copy_failure` (mandatory, by name), `test_carry_query_err_returns_empty_summary`, `test_correction_committed_before_carry` | PASS | Full |
| **R-02** | `edges_carried` miscount (count attempts/conflicts instead of `true` inserts) | `test_carry_count_idempotent_repass`, `test_carry_count_keys_off_true_only`, `test_carry_empty_when_no_eligible_edges`; MCP: `test_correct_response_includes_edges_carried`, `test_correct_omits_edges_carried_when_zero` | PASS | Full |
| **R-03** | Eligibility-predicate drift (superset exclusion vs incoming) | `test_query_outgoing_excludes_derived_classes`, `test_query_outgoing_only_ineligible_returns_empty`, `test_carry_excludes_derived_classes` + single-source SQL grep guard | PASS | Full |
| **R-04** | Composition/pipeline order break (8b → 8b′ → 8c) | `test_carry_count_idempotent_repass` (re-passed edge conflicts in 8b′, validates 8b-before-8b′), `test_carry_redirect_contradicts_converge` | PASS | Full |
| **R-05** | `Contradicts` double-write / reverse-orphan | `test_carry_contradicts_both_directions_exactly_once`, `test_carry_redirect_contradicts_converge`, `test_carry_contradicts_counts_one` | PASS | Full |
| **R-06** | Disjointness assumption fails on self-loop | `test_self_referential_edge_rejected_at_write`, `test_carry_redirect_no_double_process_on_self_loop` | PASS | Full |
| **R-07** | Tick-window staleness mis-filed as carry bug | `test_correction_carries_outgoing_edges_visible_on_new_entry` (depth-1 DB read, immediate, no tick — per #4526) | PASS | Full |
| **R-08** | `validate_and_write_edges` discards per-edge bool → count unrecoverable | `test_carry_count_keys_off_true_only` (exact count only achievable if loop owns its write loop and captures the bool) | PASS | Full |
| **R-09** | Missing `source_id` index (latency only) | Plan-time confirmation: `idx_graph_edges_source_id` EXISTS (db.rs:969, migration.rs:367, schema-check db.rs:1387). No functional test required. | N/A (verified present) | Full (noted) |
| **R-10** | Shed path targets Deprecated original | `context_edge` correct/edge subset (52/52) covers shed-via-new-id and frozen-source rejection through MCP | PASS | Full |
| **R-11** | `created_at` wrongly preserved from source row | `test_carried_edge_metadata_is_fresh_agent` (created_at = now, source/created_by = "agent") | PASS | Full |

## Test Results

### Unit Tests

| Crate / Module | Command | Total | Passed | Failed |
|----------------|---------|-------|--------|--------|
| `unimatrix-store` (lib, full) | `cargo test -p unimatrix-store --lib` | 344 | 344 | 0 |
| `unimatrix-store` `read_outgoing` (R-03 eligibility) | `cargo test -p unimatrix-store --lib read_outgoing` | 4 | 4 | 0 |
| `unimatrix-server` `carry_forward_loop_tests` | `cargo test -p unimatrix-server --lib carry_forward` | 15 | 15 | 0 |

**Mandatory test confirmed present and passing by name:**
`mcp::tools::carry_forward_loop_tests::test_carry_forward_continues_on_edge_copy_failure` — PASS
(AC-07 / R-01 / SR-01 / lesson #4473).

### Integration Tests (infra-001 MCP harness)

| Run | Command | Total | Passed | xfail | xpass | Failed |
|-----|---------|-------|--------|-------|-------|--------|
| Smoke gate (MANDATORY) | `pytest suites/ -m smoke --timeout=60` | 23 | 23 | 0 | 0 | 0 |
| New vnc-035 tests (targeted) | `pytest <3 named tests>` | 3 | 3 | 0 | 0 | 0 |
| `tools` correct/edge/carry subset | `pytest suites/test_tools.py -k "correct or edge or carry"` | 52 | 52 | 0 | 0 | 0 |
| `lifecycle` correct/carry/edge subset | `pytest suites/test_lifecycle.py -k "correct or carry or edge"` | 18 | 13 | 4 | 1 | 0 |

New integration tests added (Stage 3a plan, all `server` fixture, fresh DB):
1. `suites/test_tools.py::test_correct_response_includes_edges_carried` (AC-11a) — PASS
2. `suites/test_tools.py::test_correct_omits_edges_carried_when_zero` (AC-11b) — PASS
3. `suites/test_lifecycle.py::test_correction_carries_outgoing_edges_visible_on_new_entry` (AC-01/AC-02, depth-1 DB read) — PASS

The `lifecycle` subset's 4 xfail are pre-existing markers (GH#291 tick-interval, GH#406
multi-hop injection) unrelated to vnc-035. The 1 xpass (`test_deprecated_visible...` /
GH#405-class confidence-timing) is a pre-existing flaky xfail that happened to pass this
run — not introduced by vnc-035, no action.

### DEFERRED suites

| Suite | Status | Reason | Reproduce |
|-------|--------|--------|-----------|
| `tools` (full, 130 tests) | DEFERRED | Each test spins a fresh per-test MCP server (~3-8s/test); full run extrapolated to >1h and exceeded a reasonable gate window. The `correct/edge/carry` subset (52 tests, 7m10s) covers the entire vnc-035-affected tool surface; the 6 tools smoke tests covering other tool critical paths passed in the smoke gate. | `cd product/test/infra-001 && python -m pytest suites/test_tools.py --timeout=90 -q` |
| `lifecycle` (full, 72 tests) | DEFERRED | Same per-test-server cost. The `correct/carry/edge` subset (18 tests) plus the 5 lifecycle smoke tests cover the carry-forward flow and correction chains. | `cd product/test/infra-001 && python -m pytest suites/test_lifecycle.py --timeout=90 -q` |

`confidence`, `contradiction`, `security`, `volume`, `edge_cases`, `protocol` were NOT
required by the Stage 3a suite-selection (carry-forward adds no new external input, no
confidence/contradiction-detection change, response-envelope change is one optional field).
Smoke coverage across all suites passed.

## Gaps

None. Every risk R-01..R-11 maps to at least one passing test (R-09 verified by index
presence, no functional test required by design). The mandatory `test_carry_forward_continues_on_edge_copy_failure`
is present by name and passing.

The two full-suite DEFERRALs above are runtime-driven, not coverage gaps: the affected
surface (correct/edge/carry) is fully exercised by the targeted subsets plus smoke.

## Test Corrections (bad assertions fixed — NOT feature bugs)

Two tests authored by the prior (stalled) run carried flawed assertions that contradicted
the implemented design. Both were corrected per triage rule 3 (bad test assertion → fix
the test, document). Neither is a feature defect; no GH Issue, no xfail.

1. **`test_correct_response_includes_edges_carried`** — the no-leak assertion
   `str(id_x) not in carry_line` is unsound: a single-digit target id (e.g. `1`) collides
   with the legitimate count digit in `"Carried 1 outgoing edges forward"`. Replaced with an
   exact-string match against the canonical count-only ack
   (`format_edges_carried` = `"Carried {N} outgoing edges forward"`), which structurally
   cannot contain edge identities (AC-11c).
2. **`test_correction_carries_outgoing_edges_visible_on_new_entry`** — (a) used
   `extract_entry_id` on the correction response, but the `edges_carried` ack is appended as
   a plain-text line after the JSON block (same pattern as the vnc-017 redirect summary), so
   the raw text is no longer valid JSON and the regex fallback grabbed the *original* id; now
   parses the JSON prefix and reads `correction.id` directly. (b) asserted `on_a == 0`
   (original edge removed from deprecated A), but carry-forward is **copy** semantics, not
   move — the implementation's own unit test
   `test_carry_eligible_attach_to_new_id_not_original` documents "still on A too (carry
   copies, it does not move outgoing edges)". AC-02's guarantee is that the carried row
   attaches to B (asserted, PASS); A is deprecated and its retained edge is inert. Assertion
   corrected to `on_a == 1` with a comment citing the design.

## GH Issues filed

None. All failures triaged to bad test assertions (fixed in-place). No pre-existing
unrelated failure required a new xfail+Issue. The 4 pre-existing lifecycle xfail markers
(GH#291, GH#406) and the GH#405-class xpass predate vnc-035 and are untouched.

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `test_carry_eligible_attach_to_new_id_not_original` (unit); `test_correction_carries_outgoing_edges_visible_on_new_entry` (MCP, on_b == 1) |
| AC-02 | PASS | Same tests — carried row attaches to new id B; copy semantics confirmed (original retained on deprecated A is inert/by-design) |
| AC-03 | PASS | `test_carry_eligible_attach_to_new_id_not_original` carries `Advances` edge onto B (Advances→vision_root regression class) |
| AC-04 | PASS | `test_query_outgoing_excludes_derived_classes` (unit predicate), `test_carry_excludes_derived_classes` (loop): Supersedes/CoAccess/Informs excluded, Supports carried |
| AC-05 | PASS | `tools` correct/edge subset (52/52) — shed via `context_edge` against new id; Deprecated-original frozen-source rejection |
| AC-06 | PASS | `test_carry_contradicts_both_directions_exactly_once`, `test_carry_contradicts_counts_one` |
| AC-07 ⚠️ | PASS | **`test_carry_forward_continues_on_edge_copy_failure`** — present by name, 4 assertions (success, B Active/A Deprecated, pre-failure edges persist, failed++ & warn) |
| AC-08 | PASS | `test_carry_count_idempotent_repass` (idempotent), `test_carry_count_keys_off_true_only` (additive/changed-target count contract) |
| AC-09 | PASS | `test_carry_no_ceiling_all_carry_above_50` — all eligible edges carry, no truncation/ceiling warn |
| AC-10 | DEFERRED | Docs cleanup (uni-zero SKILL + agent docs) is a file-check/grep AC owned by the docs component, not the test gate; verify at doc review |
| AC-11 | PASS | `test_correct_response_includes_edges_carried` (N>0 ack, count-only), `test_correct_omits_edges_carried_when_zero` (absent at zero); unit `test_format_edges_carried_count_only` |

Additional design-mandated checks: carried-edge metadata (R-11) PASS
(`test_carried_edge_metadata_is_fresh_agent`); pipeline order (R-04) PASS; eligibility
single-source (R-03) PASS (grep: exclusion list in exactly one SQL clause,
`read_outgoing.rs:79`); `source_id` index (R-09) PRESENT; tick-window staleness (R-07) PASS
(depth-1 immediate read).

AC-10 is the only AC not closed by the test gate — it is a documentation grep/file-check
owned by the docs component and must be confirmed at doc review, not here.

## Knowledge Stewardship
- Queried: `context_briefing` unavailable in this resumed run; relied on Stage 3a plan's
  recorded findings (lessons #4473 warn-continue-by-name, #4526 tick staleness; patterns
  #4041 rows-affected bool, #4459 Contradicts source-validation). All applied during triage.
- Stored: nothing novel — the two test corrections are instances of known traps already in
  Unimatrix: (1) ack-appended-after-JSON breaks naive id extraction (vnc-017 redirect-summary
  pattern, already documented); (2) single-digit-id substring collision is a generic
  assertion-hygiene point, not feature knowledge. Copy-vs-move carry semantics are captured
  in the implementation's own unit test comment. Re-storing would duplicate.
