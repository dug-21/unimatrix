# Security Review: bugfix-893-security-reviewer

## Risk Level: low

## Summary
PR #939 is a test-only diff (Python parity harness under `product/test/infra-001/` + feature artifacts) — no production Rust/JS, no new dependencies, no secrets. The documented-exception waiver is provably conjunctive, single-sourced in `gate_disposition()`, fails closed on every adversarial case, and preserves the honesty invariant (`rollup()` unchanged; artifact keeps `verdict:ERROR`/`exit 7`). No blocking findings.

## Findings

### F1 — Waiver is conjunctive, single-sourced, and fails closed (confirmation, not a defect)
- **Severity**: informational
- **Location**: `harness/parity_outcome.py:392-431` (`_is_waivable_infra` / `gate_disposition`); `harness/parity_matrix_support.py:50,134` (consumers)
- **Description**: `waived=True` requires (1) >=1 INFRA_ERROR row, (2) zero PARITY_FAIL rows, (3) every INFRA row `documented_exception is True` AND `blocks_c0_proof is False`. Both `evidence_table` and `assert_rollup` read the single `gate_disposition` pure function — no recomputation drift. Verified fail-closed for: real PARITY_FAIL (clause 2, RED raised before INFRA branch), undocumented/transport INFRA (flag False), documented exception on a still-blocking dim (flag-keyed, not id-keyed), documented exception coexisting with a real PARITY_FAIL (RED), and preflight/ingest InfraError (`emit_infra_and_fail` unchanged).
- **Recommendation**: none.
- **Blocking**: no

### F2 — `documented_exception` confined to a single non-spoofable setter (confirmation)
- **Severity**: informational
- **Location**: `harness/parity_outcome.py:207-214` (branch 1b)
- **Description**: grep across the harness confirms `documented_exception=True` is set in exactly one place — the D5 `measurable=False` classifier branch. Comparators only return PARITY_PASS/PARITY_FAIL, so the flag cannot be set by a comparator. `dimension_by_id` KeyError on an orphan id fails closed to non-waivable.
- **Recommendation**: none.
- **Blocking**: no

### F3 — Pre-existing one-leg-`None` precompact `AttributeError` (out of scope)
- **Severity**: low
- **Location**: `harness/parity_outcome.py:194-195` (branch 1b `.get` on a possibly-`None` capture)
- **Description**: When a `None` precompact capture is `continue`-skipped in branch 1, branch 1b calls `cap.get("measurable")` on `None` → `AttributeError`. Pre-existing (this diff only adds the flag to the existing return path); not reached by the documented-gap fixture (both legs supply `measurable=False` dicts). Fails loud (crash), never a silent green, so it cannot produce a spurious waiver.
- **Recommendation**: track separately if desired; not for this bugfix.
- **Blocking**: no

## OWASP Assessment
- Injection / command / path traversal: N/A — no shell, no filesystem path from external input, no format strings over untrusted data.
- Deserialization: `json.dumps` of an internally-built table only (no untrusted `loads`).
- Access control / trust boundaries: N/A — off-Docker pure-Python test harness, no privilege surface.
- Vulnerable/new dependencies: none added (only intra-harness imports).
- Secrets: none in diff (scanned).

## Blast Radius Assessment
Worst case if the waiver had a subtle bug: the nan-021 release-gate job greens on an unmeasured dimension. Structurally bounded — the only waivable dimension is the human-signed `blocks_c0_proof=False` precompact; any PARITY_FAIL or undocumented/still-blocking INFRA still fails the job; the on-disk artifact retains the honest `verdict:ERROR`/`exit 7` for audit. Failure mode is fail-closed. No production code path is touched, so runtime blast radius outside CI is nil.

## Regression Risk
Minimal. `rollup()` behavior is byte-for-byte unchanged (docstring-only edit) — verified against its existing tests. `emit_infra_and_fail` and all preflight/ingest InfraError paths still raise. The single behavioral change is the intended waiver in `assert_rollup`, gated on the conjunctive `gate_disposition`. The evidence artifact gains `waived`/`gate_disposition` fields, making a waived run distinguishable from a genuine ERROR — resolving #893's core concern. `blocks_c0_proof` data flip is precompact-only (test `test_blocks_c0_proof_precompact_is_signed_documented_exception` pins the other four to True).

## PR Comments
- Posted 1 review comment on PR #939 (`--comment`, non-blocking).
- Blocking findings: no.

## Knowledge Stewardship
- Stored: nothing novel to store — the honesty/waiver invariants are ADR-governed (ADR-001/002/006/009, Unimatrix #5648) and PR-specific; per "Bugs are GH issues, not lessons" and the fresh-eyes confirmation yielded no generalizable cross-feature security anti-pattern.
