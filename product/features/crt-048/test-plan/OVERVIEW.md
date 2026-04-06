# Test Plan Overview: crt-048 — Drop Freshness from Lambda

## Feature Summary

crt-048 removes the `confidence_freshness` dimension from Lambda, leaving a
3-dimension structural integrity metric (graph quality, contradiction density,
embedding consistency) with re-normalized weights (0.46 / 0.31 / 0.23). The
change spans one crate (`unimatrix-server`) across four files, deletes two
functions, updates two signatures, removes two struct fields, and cleans
up ~11 unit tests and 8 test fixture sites.

---

## Overall Test Strategy

### Tiers

| Tier | Scope | Mechanism |
|------|-------|-----------|
| Build gate | Compile-time correctness; partial field removals are compile errors | `cargo build --workspace` |
| Unit tests | Pure math in `infra/coherence.rs`; struct correctness in `mcp/response/` | `cargo test --workspace` |
| Integration smoke | MCP JSON-RPC protocol conformance; `context_status` tool round-trip | `pytest -m smoke` |
| Integration suite | JSON output key absence; per-source Lambda path; `confidence` suite | `suites/test_tools.py`, `suites/test_confidence.py` |
| Grep assertions | Static analysis: residual symbol detection, call-site count, constant presence | shell one-liners |
| Manual checks | Unimatrix knowledge state (AC-12); PR release notes (C-07, NFR-06) | `context_get` on entries #179, #4199 |

### Test Priority by Risk Priority

| Priority | Risks | Primary Mechanism |
|----------|-------|------------------|
| Critical | R-01, R-02, R-03, R-06 | Distinct-value unit tests + build gate + grep count |
| High | R-04, R-07 | Test body inspection + re-derived expected floats |
| Medium | R-05, R-08 | JSON key-absence integration test |
| High (process) | R-10 | post-delivery `context_get` on Unimatrix entries |
| Low | R-09 | Build gate only |

---

## Risk-to-Test Mapping

| Risk | Description | Test(s) | Component Plan |
|------|-------------|---------|----------------|
| R-01 | `compute_lambda()` positional arg transposition | `lambda_specific_three_dimensions` (distinct inputs), `lambda_single_dimension_deviation` (per-slot isolation) | coherence.md |
| R-02 | Partial fixture-site removal in `mod.rs` (8 sites, 16 refs) | Build gate; `make_coherence_status_report()` explicit check | response-mod.md |
| R-03 | `DEFAULT_STALENESS_THRESHOLD_SECS` accidentally deleted | `grep` assertion (exactly 1 definition); build implies `run_maintenance()` compiles | coherence.md |
| R-04 | `lambda_weight_sum_invariant` uses exact `==` instead of epsilon | Test body inspection: must use `(sum - 1.0_f64).abs() < f64::EPSILON` | coherence.md |
| R-05 | Breaking JSON change surprises callers | JSON key-absence integration test via `tools` suite | response-status.md |
| R-06 | `coherence_by_source` per-source call site not updated | Grep count: exactly 2 `compute_lambda(` in `status.rs`, each 4 args; per-source consistency test | status.md |
| R-07 | Re-normalization test expected values not updated for new weights | `lambda_renormalization_without_embedding` with trivial and non-trivial inputs; re-derived expected values | coherence.md |
| R-08 | `From<&StatusReport>` retains stale field assignments | Build gate; JSON key-absence test | response-status.md |
| R-09 | `generate_recommendations()` retains stale reference | Build gate | coherence.md |
| R-10 | ADR-003 (#179) not superseded before merge | `context_get` on #179 (deprecated) and new ADR entry (active, all 4 data points) | (manual) |

---

## Acceptance Criteria Coverage

| AC-ID | Coverage Mechanism | Component |
|-------|-------------------|-----------|
| AC-01 | grep: `confidence_freshness` → 0 matches in `crates/` | all |
| AC-02 | `lambda_weight_sum_invariant` unit test with epsilon guard | coherence.md |
| AC-03 | grep: `confidence_freshness_score` → 0 matches; build gate | coherence.md |
| AC-04 | grep: `oldest_stale_age` → 0 matches | coherence.md |
| AC-05 | Build gate; function signature inspection | coherence.md |
| AC-06 | grep in `mcp/`; integration JSON key-absence test | response-status.md |
| AC-07 | `lambda_all_ones` / direct unit assertion | coherence.md |
| AC-08 | `lambda_renormalization_without_embedding` — trivial all-1.0 case | coherence.md |
| AC-09 | Build gate; signature inspection | coherence.md |
| AC-10 | `cargo test --workspace` zero unexpected failures | all |
| AC-11 | grep: exactly 1 definition with updated comment | coherence.md |
| AC-12 | `context_get` on #179 and new entry | (manual, Stage 3c) |
| AC-13 | grep count (2 sites, 4 args each); per-source test | status.md |
| AC-14 | Build gate — compile error indicates partial removal | response-mod.md |

---

## Cross-Component Test Dependencies

1. `mcp/response/status.rs` field removal (Component C) is a prerequisite for
   any test that constructs `StatusReport` — this blocks Component D tests if
   incomplete. Build gate is the detection mechanism (R-02).

2. `infra/coherence.rs` signature changes (Component A) cascade to Component B
   (`services/status.rs`). All call sites in Component B must be updated before
   `cargo build` succeeds.

3. Integration tests that exercise `context_status` through the MCP wire protocol
   depend on the binary being built from the post-crt-048 code. Run
   `cargo build --release` before any `pytest` invocation.

---

## Integration Harness Plan

### Suite Selection

This feature touches:
- `context_status` tool logic (Component B assembles StatusReport)
- JSON output format (Component C removes two keys)
- Coherence / Lambda computation (Component A)

Applicable suites from the selection table:

| Feature Touch | Suites |
|--------------|--------|
| Any tool logic change | `tools`, `protocol` |
| Confidence/coherence system | `confidence` |
| Smoke gate (mandatory) | `smoke` |

**Suites to run in Stage 3c:**
1. `python -m pytest suites/ -v -m smoke --timeout=60` (mandatory gate)
2. `python -m pytest suites/test_tools.py -v --timeout=60` (covers `context_status` tool params and response)
3. `python -m pytest suites/test_confidence.py -v --timeout=60` (covers Lambda / coherence behavior)

### Existing Suite Coverage of crt-048 Risks

| Risk | Existing Suite Coverage |
|------|------------------------|
| R-05 (JSON key absence) | `test_tools.py` likely has `context_status` response shape assertions — verify `confidence_freshness_score` is absent |
| R-07 (re-normalization correctness) | `test_confidence.py` Lambda tests — verify any expected floats are updated |
| R-01, R-06 (arg order) | Not directly testable through MCP wire protocol; covered by unit tests only |
| R-02, R-03 (struct / constant) | Build gate; not MCP-visible |

### New Integration Tests Required

**test_tools.py — add one test:**

```python
def test_status_json_no_freshness_fields(server):
    """R-05, AC-06: JSON output must not contain removed keys."""
    response = server.call_tool("context_status", {"format": "json"})
    # Parse the JSON content from the MCP response
    import json
    payload = json.loads(response["content"][0]["text"])
    assert "confidence_freshness_score" not in payload, \
        "confidence_freshness_score must be absent from context_status JSON"
    assert "stale_confidence_count" not in payload, \
        "stale_confidence_count must be absent from context_status JSON"
```

This test is necessary because JSON key absence is not testable at the unit level
(the struct fields are gone, but serde derives the serialization; we need an end-to-end
MCP call to confirm the wire format is correct).

**test_confidence.py — verify no expected-float drift:**
No new test is required, but all existing tests that assert specific Lambda float values
from `context_status` responses must be audited in Stage 3c. If any test hardcodes an
expected Lambda that was computed with the 4-dimension weights, it must be updated with
the 3-dimension value (R-07).

### When NOT to Add Integration Tests

- `compute_lambda()` positional argument correctness (R-01) — pure internal math;
  unit tests with distinct dimension values are the right tool.
- `DEFAULT_STALENESS_THRESHOLD_SECS` presence (R-03) — constant existence; grep suffices.
- ADR supersession (R-10) — Unimatrix knowledge state; `context_get` manual check.

---

## Deleted Tests Inventory

These tests must be absent from the post-delivery codebase. Their absence is the
positive signal that freshness code was fully removed.

**`infra/coherence.rs` — must be deleted:**
`freshness_empty_entries`, `freshness_all_stale`, `freshness_none_stale`,
`freshness_uses_max_of_timestamps`, `freshness_recently_accessed_not_stale`,
`freshness_both_timestamps_older_than_threshold`, `oldest_stale_no_stale`,
`oldest_stale_one_stale`, `oldest_stale_both_timestamps_zero`,
`staleness_threshold_constant_value`, `recommendations_below_threshold_stale_confidence`

**`mcp/response/mod.rs` — must be deleted:**
`test_coherence_json_all_fields`, `test_coherence_json_f64_precision`,
`test_coherence_stale_count_rendering`, `test_coherence_default_values`
