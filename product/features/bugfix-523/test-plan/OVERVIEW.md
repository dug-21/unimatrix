# Test Plan Overview: bugfix-523 — Server Hardening Batch

## Scope

Four independent unit-test-only defect fixes in `unimatrix-server`. All tests live in
`#[cfg(test)]` modules co-located with their implementation file. No new integration test
suites are added for this batch (see Integration Harness section below).

---

## Risk-to-Test Mapping

| Risk ID | Priority | Mitigating Tests | Component File |
|---------|----------|-----------------|----------------|
| R-01 | Critical | `test_nli_gate_path_a_informs_edges_still_written_nli_disabled`, `test_nli_gate_path_c_cosine_supports_edges_still_written_nli_disabled` | nli-tick-gate.md |
| R-02 | Critical | Same as R-01 (structural proof Path C runs after gate) + code inspection of `// === PATH B entry gate ===` | nli-tick-gate.md |
| R-03 | Critical | 19 NaN tests AC-06..AC-24, 2 Inf tests AC-25/AC-26 — all via `assert_validate_fails_with_field` | nan-guards.md |
| R-04 | Critical | `test_dispatch_rework_candidate_invalid_session_id_rejected` (AC-28) + code inspection of insertion order | session-sanitization.md |
| R-05 | High | `test_cosine_supports_path_skips_missing_category_map_src`, `test_cosine_supports_path_nonfinite_cosine_handled` + code review that non-finite cosine site is still `warn!` | log-downgrade.md |
| R-06 | High | Gate 3a presence-count check: Gate 3a reviewer must verify all named test functions exist by grep/search before marking delivery complete | All components |
| R-07 | High | Spot-check field name strings for loop-group fields (AC-17 through AC-24) — verify error string would fail with a wrong name | nan-guards.md |
| R-08 | High | `test_dispatch_rework_candidate_valid_path_not_regressed` (AC-29) | session-sanitization.md |
| R-09 | Med | `test_nli_gate_nli_enabled_path_not_regressed` (AC-03) | nli-tick-gate.md |
| R-10 | Med | `cargo test -p unimatrix-server -- infra::config` clean run; boundary-value tests for `w_sim` (AC-27) | nan-guards.md |
| R-11 | Med | Gate report must explicitly state behavioral-only per ADR-001(c)/entry #4143 | log-downgrade.md, nli-tick-gate.md |
| R-12 | Low | Covered upstream by AC-07 + AC-08; no additional test | nan-guards.md |

---

## AC-04 / AC-05 Behavioral-Only Coverage — Authoritative Decision

**Gate 3b reviewers must accept AC-04 and AC-05 as behavioral-only coverage.**

The architecture (ADR-001(c), Unimatrix entry #4143) commits to behavioral-only coverage for
all log-level ACs in Items 1 and 2. Log level is NOT asserted in tests. This supersedes
SPECIFICATION.md's "Option A preferred" language.

Rationale: lesson #3935 documents that `tracing-test` / `tracing_subscriber` harnesses in this
codebase cause subscriber state leakage and initialization conflicts across parallel tests,
leading to Gate 3b failures. Adding `tracing-test` as a dev-dependency for two assertions in
one batch is not justified.

The gate report at Stage 3c MUST include the following statement verbatim:

> "AC-04 and AC-05 log-level assertions are behavioral-only per ADR-001(c) (Unimatrix entry
> #4143). Log level verified by code review. No `tracing-test` harness used."

Any gate feedback requesting log-level assertions must be escalated to the Bugfix Leader, not
unilaterally resolved by adding the `tracing-test` harness.

---

## Test Strategy by Item

| Item | File Under Test | Test Location | Test Count | Test Type |
|------|----------------|---------------|------------|-----------|
| 1 — NLI Tick Gate | `services/nli_detection_tick.rs` | `nli_detection_tick.rs` `#[cfg(test)]` | 4 | `#[tokio::test]` async unit |
| 2 — Log Downgrade | `services/nli_detection_tick.rs` | same as Item 1 | 3 | `#[tokio::test]` async unit |
| 3 — NaN Guards | `infra/config.rs` | `config.rs` `#[cfg(test)]` | 21 new + pre-existing | `#[test]` sync unit |
| 4 — Session Sanitization | `uds/listener.rs` | `listener.rs` `#[cfg(test)]` | 2 | `#[test]` or `#[tokio::test]` |

Total new tests: 30 (4 + 3 + 21 + 2).

---

## Must-Not-Skip Scenarios

Three scenarios from RISK-TEST-STRATEGY.md that Gate 3c must treat as non-negotiable:

**1. R-01/R-02 — Path A and Path C unconditional (AC-02)**

Both `test_nli_gate_path_a_informs_edges_still_written_nli_disabled` AND
`test_nli_gate_path_c_cosine_supports_edges_still_written_nli_disabled` are required.
Passing AC-01 (Path B skipped) alone is insufficient — the gate is only correct when
both sides of the predicate are independently verified. If the gate is placed before
`run_cosine_supports_path`, Path C silently stops producing edges with no crash or error.

**2. R-03 — All 19 NaN fields individually tested (AC-06 through AC-24)**

Count of NaN tests must be exactly 19. The loop-group dereference pattern (Groups B and C)
is different from the inline pattern (Group A) and must be tested independently. A passing
test using a wrong field name string passes vacuously — spot-check is required for
AC-17 through AC-24 (fusion/phase weight loop fields).

**3. R-04 — Guard insertion order verified by code inspection (AC-28)**

AC-28 runtime test confirms the guard fires. Code inspection must independently confirm
no use of `event.session_id` appears between the capability check and the guard block.
Both verification methods are required.

---

## Cross-Component Dependencies

All four items are independent. No data flows between them at runtime. The only
cross-component dependency is a diff-level constraint: Items 1 and 2 both modify
`nli_detection_tick.rs` and must be assigned to the same implementation agent/wave (C-08).

Stage 3c tester must verify:
1. The final diff for `nli_detection_tick.rs` contains both the gate insertion (Item 1)
   and the two `warn!`→`debug!` changes (Item 2) with no extraneous changes.
2. `background.rs` is unchanged in the diff (C-01 constraint — outer call stays unconditional).

---

## Integration Harness Plan (infra-001)

**No new integration test suite files are needed for this batch.**

Rationale: All four items are internal to `unimatrix-server` with no MCP-visible behavior
changes. The fixes are:
- An internal early-return inside `run_graph_inference_tick` (not invocable via MCP tools)
- Internal log level changes (not MCP-visible)
- Startup validation changes (server fails fast before MCP tools become available — cannot
  exercise via the running server test harness)
- A UDS dispatch guard (UDS is not exercised by the infra-001 MCP JSON-RPC harness)

**Applicable existing suites**: None add coverage for this batch.

**Mandatory gate**: Smoke tests remain mandatory as a minimum regression gate.

At Stage 3c, the executor must run:
```bash
cd /workspaces/unimatrix/product/test/infra-001
python -m pytest suites/ -v -m smoke --timeout=60
```

A smoke test pass confirms the MCP server still starts and responds — which is the only
infra-001-observable property affected by Item 3 (a NaN in config prevents startup). The
test suite itself uses valid configs, so Item 3's guards will not fire during smoke tests;
their coverage is purely via the unit tests in `config.rs`.

---

## Regression Scope

The following pre-existing test subsets must remain passing after delivery:

| Scope | Command fragment | Relevance |
|-------|-----------------|-----------|
| `infra::config` module | `-- infra::config` | Item 3 changes; all existing boundary tests |
| `services::nli_detection_tick` module | `-- services::nli_detection_tick` | Items 1 and 2 |
| `uds::listener` module | `-- uds::listener` | Item 4 |
| Full workspace | `cargo test --workspace 2>&1 \| tail -30` | Global regression gate |

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned entries #4143 (ADR-001 for this
  batch), #3548 (test-plan assertion coverage gap lesson), #3766 (InferenceConfig NaN lesson
  from bugfix-444), #4133 (NaN guard pattern), #4142 (log-level AC pattern). All directly
  applicable.
- Queried: `context_search` for bugfix-523 ADRs — returned #4143 (full ADR). Retrieved via
  `context_get`. All architectural decisions incorporated.
- Stored: nothing novel to store at this stage — patterns already captured in #4133 and
  #4142. No new cross-feature patterns visible from this test plan design.
