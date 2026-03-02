# Test Plan: tiered-output

Component: Evidence-Limited Retrospective Output (P1)
Covers: AC-15, AC-16
Risks: R-09 (BLOCKING audit required before implementation)

---

## BLOCKING Pre-Implementation Audit (R-09)

Before implementing Component 6, run the R-09 audit:

```bash
grep -rn "evidence" product/test/infra-001/suites/
```

For each test asserting exact evidence array length in `hotspots[N]["evidence"]`:
1. Add `evidence_limit=0` to the request, OR
2. Update assertion to `<= 3`.

Document results in agent report.

---

## Unit Tests

### apply_evidence_limit

```
test_apply_evidence_limit_truncates
  - Report with 1 hotspot containing 10 evidence items
  - apply_evidence_limit(report, 3)
  - Assert: hotspot.evidence.len() == 3

test_apply_evidence_limit_zero_means_no_cap
  - Report with 1 hotspot containing 10 evidence items
  - apply_evidence_limit(report, 0)
  - Assert: hotspot.evidence.len() == 10  (unchanged)

test_apply_evidence_limit_above_count_is_noop
  - Report with 1 hotspot containing 2 evidence items
  - apply_evidence_limit(report, 5)
  - Assert: hotspot.evidence.len() == 2  (unchanged)

test_apply_evidence_limit_zero_evidence_noop
  - Report with 1 hotspot containing 0 evidence items
  - apply_evidence_limit(report, 3)
  - Assert: hotspot.evidence.len() == 0  (no error)

test_apply_evidence_limit_multiple_hotspots
  - 3 hotspots with 10, 5, 2 evidence items respectively
  - apply_evidence_limit(report, 3)
  - Assert: lengths are 3, 3, 2

test_apply_evidence_limit_no_hotspots
  - Empty hotspots vec
  - apply_evidence_limit(report, 3)
  - No error; report returned unchanged
```

### evidence_limit parameter parsing

```
test_evidence_limit_absent_defaults_to_3
  - Request JSON without "evidence_limit" field
  - Assert: parsed request.evidence_limit == None
  - And: when applied in tools.rs, None defaults to 3

test_evidence_limit_zero_parses
  - Request JSON with "evidence_limit": 0
  - Assert: parsed as Some(0)

test_evidence_limit_negative_rejected
  - Request JSON with "evidence_limit": -1
  - Assert: parse error (JSON schema minimum: 0)
```

---

## Integration Tests (Rust)

### Payload size (AC-15)

```
test_evidence_limit_3_reduces_payload
  - Build synthetic RetrospectiveReport with 13 hotspots, 10 evidence items each
  - Apply evidence_limit = 3 (default)
  - Serialize to JSON
  - Assert: serialized len <= 10240 bytes (10KB)
  - Assert: each hotspot.evidence.len() <= 3

test_evidence_limit_0_backward_compatible  (AC-16)
  - Build report with 13 hotspots, 10 evidence each
  - Apply evidence_limit = 0
  - Serialize
  - Assert: each hotspot.evidence.len() == 10  (full arrays)
```

---

## Integration Tests (infra-001 MCP harness)

These tests are added in Stage 3c. Listed here for planning:

```
test_retrospective_evidence_limit_default
  - Call context_retrospective for a feature cycle with hotspots (no evidence_limit)
  - Assert: each hotspot's evidence array has <= 3 items

test_retrospective_evidence_limit_zero
  - Call context_retrospective with evidence_limit=0
  - Assert: evidence arrays are full (same as pre-col-010 behavior)

test_retrospective_evidence_limit_custom
  - Call context_retrospective with evidence_limit=5
  - Assert: each hotspot has <= 5 evidence items
```

---

## Regression Tests (AC-24 — R-09 resolution)

After R-09 audit, update the following integration tests in infra-001:
- Any test in `test_tools.py` asserting `len(hotspots[N]["evidence"]) == N` → add `evidence_limit=0`
- Any test in `test_lifecycle.py` checking hotspot evidence count → add `evidence_limit=0`

Run `cargo test --workspace` after completing implementation to verify AC-24.
