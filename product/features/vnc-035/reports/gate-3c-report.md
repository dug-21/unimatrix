# Gate 3c Report: vnc-035

> Gate: 3c (Final Risk-Based Validation)
> Date: 2026-06-12
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Risk mitigation proof (R-01..R-11) | PASS | RISK-COVERAGE-REPORT maps every risk to ≥1 passing test; R-09 verified by index presence (no functional test required by design). |
| 2. Test coverage completeness | PASS | All 27 risk scenarios exercised across store-unit (4), carry-loop-unit (15), formatter (1), and infra-001 integration (3 new + 52 tools + 18 lifecycle subsets + 23 smoke). |
| 3. Specification compliance (FR-01..FR-12) | PASS | Every FR has passing coverage; AC-01..AC-09, AC-11 tested; AC-10 is the docs file-check (landed in uni-zero SKILL.md). |
| 4. Architecture compliance (ADR-001..005) | PASS | Handler pipeline order 8→8b→8b′→8c confirmed in code (tools.rs:1133-1172); single-source SQL predicate; own-loop count; re-stamp metadata; Contradicts bidirectional counted once. |
| 5. Knowledge stewardship | PASS | Tester report (agent-4) has `## Knowledge Stewardship` with `Queried:` + `Stored: nothing novel -- {reason}`. |

**Independent re-verification (this gate, not relying on report claims):**
- `cargo test -p unimatrix-store --lib read_outgoing` → 4 passed.
- `cargo test -p unimatrix-server --lib carry_forward` → **15 passed**, incl. mandatory `test_carry_forward_continues_on_edge_copy_failure`.
- `pytest suites/ -m smoke` → **23 passed**, 0 failed (199.51s).
- 3 new vnc-035 integration tests → **3 passed** (24.87s).

## Detailed Findings

### 1. Risk mitigation proof — PASS
Every risk R-01..R-11 maps to at least one passing test in RISK-COVERAGE-REPORT.md §Coverage Summary, independently re-confirmed:

- **R-01 (Critical, #4473 precedent)** — `test_carry_forward_continues_on_edge_copy_failure` PRESENT BY NAME and PASSING. It uses a genuine per-edge Nth-call fault seam (`carry_fault::arm_fail_on_nth(2)`), seeds 2 eligible edges, forces the 2nd write to SQL-error, and asserts all four required outcomes: (1) correction success (B Active / A Deprecated), (2) transaction intact, (3) **exactly one** pre-failure edge persists on B, (4) `summary.failed >= 1` + `tracing::warn!` fired (`logs_contain("fault-injected, vnc-035 AC-07")`). This is the strong "edges before the failure persist" assertion lesson #4473 demands — not a weaker all-writes-fail variant. Plus `test_carry_query_err_returns_empty_summary` (R-01 #2) and `test_correction_committed_before_carry` (R-01 #3).
- **R-02/R-08 (count contract)** — `test_carry_count_keys_off_true_only`, `test_carry_count_idempotent_repass` (re-passed triple → UNIQUE conflict → not counted), `test_carry_empty_when_no_eligible_edges`; MCP `test_correct_response_includes_edges_carried` / `test_correct_omits_edges_carried_when_zero`. The loop owns its write loop and counts `Inserted` only — R-08 satisfied.
- **R-03 (predicate drift)** — `test_query_outgoing_excludes_derived_classes` (unit), `test_carry_excludes_derived_classes` (loop). Single-source SQL predicate confirmed in code: exclusion list `('Supersedes','CoAccess','Informs')` appears in exactly one clause (`read_outgoing.rs:79`); no parallel Rust filter; superset rationale documented inline (read_outgoing.rs:50-55, 86-90).
- **R-04 (pipeline order)** — confirmed in handler (tools.rs:1133 8b → :1155 8b′ → :1167 8c). `test_carry_count_idempotent_repass` pins 8b-before-8b′ via the UNIQUE-conflict mechanism.
- **R-05 (Contradicts)** — `test_carry_contradicts_both_directions_exactly_once`, `test_carry_redirect_contradicts_converge`, `test_carry_contradicts_counts_one`.
- **R-06 (self-loop disjointness)** — `test_self_referential_edge_rejected_at_write`, `test_carry_redirect_no_double_process_on_self_loop`.
- **R-07 (tick staleness)** — `test_correction_carries_outgoing_edges_visible_on_new_entry` asserts depth-1 DB read, immediate, no tick (per #4526).
- **R-09 (index)** — `idx_graph_edges_source_id` verified present (db.rs:969, migration.rs:367); latency-only, no functional test required.
- **R-10 (shed against new id)** — covered by the `tools` correct/edge subset (52/52): shed via new id + Deprecated-original frozen-source rejection.
- **R-11 (no preservation)** — `test_carried_edge_metadata_is_fresh_agent` asserts `created_at = now`, `source`/`created_by = "agent"`.

### 2. Test coverage completeness — PASS
**Integration validation (mandatory):**
- Smoke gate independently re-run: **23/23 passed** (matches report).
- 3 new vnc-035 integration tests exist and pass:
  - `test_tools.py::test_correct_response_includes_edges_carried` (AC-11a)
  - `test_tools.py::test_correct_omits_edges_carried_when_zero` (AC-11b)
  - `test_lifecycle.py::test_correction_carries_outgoing_edges_visible_on_new_entry` (AC-01/AC-02)
- Relevant integration subsets run (tools correct/edge/carry 52/52; lifecycle correct/carry/edge 18 with 4 pre-existing xfail).
- **No integration tests deleted or commented out** — `git diff` shows pure additions (test_tools.py +76/-0, test_lifecycle.py +96/-0).
- **xfail hygiene**: vnc-035 added **zero** xfail markers (`git diff | grep "^+.*xfail"` empty). The 4 lifecycle-subset xfail are pre-existing (GH#291 tick-interval, GH#406 multi-hop injection, ONNX-model-absent env) — not new, not masking feature bugs.
- RISK-COVERAGE-REPORT.md includes integration test counts (§Integration Tests table).
- The xpass (`test_deprecated_visible…`, GH#405-class) is a pre-existing flaky xfail, untouched by vnc-035.

**DEFERRAL assessment (reasonable — no coverage gap):** the tester deferred the full `tools` (130) and `lifecycle` (72) suites for per-test MCP-server spin-up cost (~3-8s/test, >1h extrapolated). The vnc-035-affected surface — `context_correct` carry-forward, `context_edge` shed, the `edges_carried` ack — is fully exercised by the 52-test tools subset, the 18-test lifecycle subset, the 3 new tests, and the all-suite smoke gate. Carry-forward adds no new external input, no confidence/contradiction change, and only one optional response field, so the unrun tests cover orthogonal surfaces. The deferral is runtime-driven, not a coverage gap.

### 3. Specification compliance (FR-01..FR-12) — PASS
- FR-01 carry-by-default, FR-02 attach-to-new-id, FR-03 `Advances→vision_root` regression, FR-04 eligibility, FR-05 shed-via-new-id, FR-06 Contradicts bidirectional, FR-07 warn-and-continue, FR-08 additive-on-triple, FR-09 no-ceiling, FR-10 ack, FR-11 attribution — all have passing tests (see §1).
- FR-12 / AC-10 docs cleanup landed in `.claude/skills/uni-zero/SKILL.md` (file-check AC, owned by docs component, correctly NOT a code test — not failed here).

### 4. Architecture compliance (ADR-001..005) — PASS
Re-confirmed in code (not merely in report):
- ADR-001: handler inserts 8b′ between 8b (`validate_and_write_edges`, :1133) and 8c (`run_redirect_loop`, :1167); shared `now` timestamp hoisted to :1129.
- ADR-002: single SQL predicate; warn-and-continue (`query_outgoing_edges` Err → `CarrySummary::default()` + warn; per-edge SQL error → `failed++` + warn + continue).
- ADR-003: loop owns its writes via `carry_write_edge`, counts `Inserted` only; does NOT delegate counting to bool-discarding `validate_and_write_edges`.
- ADR-004: `created_at = now`, `weight = 1.0`, `source`/`created_by = "agent"`, `metadata = ""` — no preservation, no provenance marker.
- ADR-005: Contradicts forward counted + reverse inline not counted (one logical edge = one `carried`); carry (A-outgoing) and redirect (A-incoming) read disjoint sets.

### 5. Knowledge stewardship — PASS
`vnc-035-agent-4-tester-report.md` contains `## Knowledge Stewardship` with `Queried:` (Stage 3a recorded findings — context_briefing unavailable in resumed run, an acceptable documented reason) and `Stored: nothing novel to store -- {reason}` (both corrections are instances of already-captured traps). Compliant.

## Test Corrections Assessment (bad-test fixes, NOT feature-bug masking)

Both corrections confirmed legitimate against the implemented design:

1. **`test_correct_response_includes_edges_carried`** — the original `str(id_x) not in carry_line` no-leak check is genuinely unsound: a single-digit target id collides with the count digit in `"Carried 1 outgoing edges forward"`. Replaced with an **exact-string match** against the canonical count-only format (`format_edges_carried` = `"Carried {N} outgoing edges forward"`, entries.rs:305-306), which structurally cannot contain edge identities. This is a **stronger** AC-11c assertion, not a weakened one.

2. **`test_correction_carries_outgoing_edges_visible_on_new_entry`** — (a) the JSON-prefix parse replaces a regex fallback that grabbed the *original* id because the `edges_carried` ack is appended as plain text after the JSON block (matching the vnc-017 redirect-summary pattern) — correct against the implemented ack shape. (b) `on_a == 1` (copy semantics) is correct against the design: the carry loop (tools.rs:5054-5105) only writes onto B and never deletes from A; the implementation's own unit test `test_carry_eligible_attach_to_new_id_not_original` documents copy-not-move. The prior `on_a == 0` assertion was simply wrong about the semantics. The load-bearing AC-02 guarantee — the carried row attaches to B — is still asserted (`on_b == 1`, PASS).

Neither correction masks a feature bug; no GH Issue or xfail was warranted.

## Gaps
None. Every risk R-01..R-11 maps to at least one passing test. The mandatory `test_carry_forward_continues_on_edge_copy_failure` is present by name, passing, and asserts pre-failure-edge persistence. The two full-suite deferrals are runtime-driven, not coverage gaps.

## Notes (pre-existing / out of scope)
- The `eval::runner::sweep_tests::test_ac14_correlated_sweep_non_vacuous` flake is unrelated to vnc-035.
- 4 working-tree files (bindings fixtures, projects/tests.rs, project_routing_integration.rs) are pre-existing vnc-034 changes, not part of this feature.
- `tools.rs` (11,236 lines) is pre-existing large; carry additions are cohesive — a Gate 3b code-quality concern already adjudicated PASS, not a 3c risk item.

## Rework Required
None.
