# Security Review: crt-024-security-reviewer

## Risk Level: low

## Summary

crt-024 is a scoring formula change confined entirely to `unimatrix-server`. No new
dependencies, no new storage schema, no new MCP tools, no new trust boundaries. The
change adds six operator-controlled f64 weight fields to `InferenceConfig` and replaces
a two-pass sequential ranking pipeline with a single fused pass in `SearchService`. All
weight inputs are validated at startup before any request is served. One medium-severity
finding was identified in the config merge path that can produce an invalid weight sum
after merging two individually valid configs; this is non-blocking because the blast
radius is incorrect search ordering for co-existing global+project config operators only,
not data corruption or information disclosure. No blocking findings.

---

## Findings

### Finding 1: Config Merge Can Produce Weight Sum > 1.0

- **Severity**: medium
- **Location**: `crates/unimatrix-server/src/infra/config.rs`, `merge_configs()` at lines ~1544-1574
- **Description**: `validate_config()` is called per-file before merge. The merged result is
  never re-validated. The six new f64 weight fields inherit the existing epsilon-compare merge
  pattern: when a project-level field differs from the compiled default, it wins; otherwise the
  global-level field is used. This means a global config with high w_sim+w_nli+w_conf (e.g.,
  `w_sim=0.5, w_nli=0.4, w_conf=0.1`, sum=1.0, individually valid) combined with a project
  config that only overrides `w_coac=0.15` (valid alone) produces a merged sum of 1.15,
  violating the sum <= 1.0 invariant. The effect is `fused_score > 1.0` for high-signal
  entries, breaking the NFR-02 range guarantee. Since `ScoredEntry.final_score` is returned
  over the MCP interface, out-of-range scores corrupt agent context injection quality but do
  not expose data or escalate privilege.
- **Blast radius**: Incorrect search result ordering for operators using both global and
  project config files with weight fields that sum > 1.0 after merge. Default single-config
  deployment is unaffected.
- **Recommendation**: Add a call to `config.inference.validate(path)?` at the end of
  `merge_configs` or in `load_config` after the merge step. This is consistent with the
  existing pattern for individual config loading and requires only a few lines.
- **Blocking**: No. Operator-only trigger (requires deliberate use of both global and
  project configs), no data exposure, no privilege escalation.

---

### Finding 2: `_confidence_weight` Dead Read Retained for Lock Ordering

- **Severity**: low (informational)
- **Location**: `crates/unimatrix-server/src/services/search.rs`, lines ~429-439
- **Description**: `_confidence_weight` is read from `confidence_state` and immediately
  discarded (prefixed `_`). The comment explains this is kept to preserve the lock-ordering
  invariant. This is not a security risk, but it is dead code that future developers may
  remove without understanding the invariant, breaking the lock ordering and risking a
  deadlock. The comment is present and accurate.
- **Recommendation**: Consider replacing with an explicit lock-ordering comment or assertion
  rather than a discarded variable, to make the invariant harder to accidentally delete.
- **Blocking**: No.

---

### Finding 3: NaN Propagation Guards Verified Present

- **Severity**: informational — finding is CLEAR
- **Location**: `crates/unimatrix-server/src/services/search.rs`, lines ~757-784
- **Description**: The risk register identified three potential NaN/infinity sources:
  (1) `raw_coac / MAX_CO_ACCESS_BOOST` — guarded by `.min(1.0)`; denominator is a const
  (non-zero); no NaN possible.
  (2) `prov_norm = raw_prov / PROVENANCE_BOOST` — guarded by explicit `if PROVENANCE_BOOST == 0.0` check.
  (3) `FusionWeights::effective()` zero-denominator — guarded by `if denom == 0.0` returning
  all-zeros with a warning log.
  (4) NaN NLI entailment from model output — guarded by `if v.is_nan() { 0.0 }` cast.
  All four guard paths are present in production code and unit-tested. SeR-02 from the risk
  register is mitigated.
- **Blocking**: No.

---

### Finding 4: Input Validation at System Boundary

- **Severity**: informational — finding is CLEAR
- **Location**: `crates/unimatrix-server/src/infra/config.rs` validate() and `SecurityGateway`
- **Description**: The six new config fields arrive from operator-controlled TOML files, not
  from agent/user input. They are validated at startup (per-field range [0.0,1.0] and sum
  <= 1.0). Invalid configs fail server startup with a structured diagnostic error — no
  runtime injection risk. Agent-supplied inputs (query text, k, floors) pass through the
  existing `SecurityGateway` unchanged; no new attack surface was added by this change.
- **Blocking**: No.

---

### Finding 5: No Secrets, No New Dependencies

- **Severity**: informational — finding is CLEAR
- **Description**: Full diff scan found zero hardcoded credentials, API keys, tokens, or
  secrets. The architecture document confirms no new crate dependencies were introduced.
  All signal inputs are computed from data already in the pipeline. The `#[cfg(test)]`
  gate on `use crate::confidence::rerank_score` correctly confines the legacy function
  to test scope only.
- **Blocking**: No.

---

## OWASP Checklist

| OWASP Category | Assessment |
|---------------|------------|
| A01 Broken Access Control | Not applicable — no new access control paths |
| A03 Injection | Clear — no external input reaches the formula; query text goes through existing gateway validation unchanged |
| A04 Insecure Design | Finding 1 (medium) — merged config sum bypass, operator-only, non-blocking |
| A05 Security Misconfiguration | Finding 1 partially — bad merged config is silently accepted |
| A07 Identification/Auth Failures | Not applicable — no new auth paths |
| A08 Software/Data Integrity | Finding 1 — score range guarantee can be violated in merged-config scenario |
| A09 Logging/Monitoring Failures | Clear — all degradation paths (NLI failure, co-access timeout) log at warn/debug |
| Deserialization | Clear — TOML deserialization uses serde; malformed configs return structured errors |

---

## Blast Radius Assessment

**Worst-case scenario**: An operator with both global and project configs inadvertently
creates a merged weight sum > 1.0. The `fused_score` for high-signal entries exceeds 1.0.
`ScoredEntry.final_score` is returned via the MCP interface as a float field in the search
response. Agents consuming this field as a relevance rank-ordering signal receive incorrect
but still monotonic ordering (entries still rank by the formula; only the absolute scale is
wrong). No data is corrupted in the store. No entries are lost. No privilege escalation.
The failure mode is degraded ranking quality, detectable only by comparing scores to the
[0,1] expected range.

**Regression blast radius**: All search results pass through the new fused formula. The
pre-existing `apply_nli_sort` code path is removed. If the fused formula has a subtle
implementation bug, every `context_search` call returns worse-ranked results. The failure
mode is silent degradation in agent context quality, not hard errors. The existing
integration tests in `test_lifecycle.py` and `test_tools.py` assert finite, non-negative,
in-range scores and would catch NaN/negative/wildly-out-of-range output.

---

## Regression Risk

**Risk level**: low-medium.

The pipeline change is significant (removing `apply_nli_sort` and the old Step 8 re-sort,
replacing them with a single pass) but the existing test suite covers:
- All three `apply_nli_sort` behaviors migrated to `compute_fused_score` unit tests (R-05)
- All normalization boundary values for util_norm, prov_norm, coac_norm
- NLI-absent re-normalization and the zero-denominator guard
- AC-11 regression: NLI-dominant entry beats co-access-dominant entry
- ADR-003 Constraints 9 and 10 as named unit tests
- Integration tests for NLI-absent path producing finite in-range scores
- Co-access signal reaching the scorer via lifecycle integration test

The `BriefingService` tests unchanged and are isolated from the scoring formula change.
The `_confidence_weight` dead read is a low regression risk (it compiles, tests pass).

One pre-existing design risk: the `old-behavior.toml` eval profile uses
`w_sim=0.85, w_conf=0.15, sum=1.0` — which passes validation individually. If combined
with a global config that also sets any weight, the merge could produce sum > 1.0.
Documentation should note operators should not use eval profiles as project-level configs.

---

## PR Comments

- Posted 1 comment on PR #336 noting Finding 1 (config merge sum bypass).
- Blocking findings: no.

---

## Knowledge Stewardship

- Nothing novel to store via /uni-store-lesson. The config-merge-without-re-validation
  pattern is a pre-existing architectural property of this codebase. The new risk here
  (sum-constrained f64 fields) is feature-specific. If a second feature adds cross-field
  constraints to config fields that go through merge_configs, this pattern warrants a
  generalizable lesson — but this is the first occurrence.
