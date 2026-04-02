# Security Review: crt-039-security-reviewer

## Risk Level: low

## Summary

crt-039 refactors `run_graph_inference_tick` to split structural Informs inference (Phase 4b)
from NLI Supports inference (Phase 8). The change removes an outer `if nli_enabled` gate from
`background.rs` and restructures `nli_detection_tick.rs` so Phase 4b runs unconditionally while
Phase 8 remains behind `get_provider()`. The security profile is low: no new external input
surfaces are introduced, no user-controlled data touches the changed code paths, and the
`MAX_INFORMS_PER_TICK = 25` hard cap limits the write rate in any failure scenario. All 55
module tests pass; the critical Path A / Path B boundary tests (TC-01, TC-02) are present and
passing.

---

## Findings

### Finding 1: Dead-code suppression on NliCandidatePair and InformsCandidate

- **Severity**: low
- **Location**: `nli_detection_tick.rs:70`, `nli_detection_tick.rs:90`
- **Description**: Both `NliCandidatePair` (now single-variant after `Informs` removal) and
  `InformsCandidate` carry `#[allow(dead_code)]`. For `NliCandidatePair`, the `cosine` field
  inside `SupportsContradict` is genuinely carried through Phase 6/7/8 but never read in Phase 8
  (the weight uses `nli_scores.entailment`, not cosine). The `allow(dead_code)` was present
  pre-crt-039 and is not introduced by this PR. For `InformsCandidate`, the struct is actively
  used in the `informs_metadata` vec, but individual fields (`source_category`,
  `target_category`) are only read at write time in `format_informs_metadata`. Clippy confirms
  this surfaces as a warning but does not produce an error under CI. Not blocking — the
  suppression is conservative and prevents future regression warnings when the struct grows. No
  security impact: the fields are populated from DB reads at tick time, not from external input.
- **Recommendation**: In a follow-up, remove `#[allow(dead_code)]` from `InformsCandidate` once
  confidence that all fields are consumed. Not required for this PR.
- **Blocking**: no

### Finding 2: Test uses out-of-range `supports_candidate_threshold: 1.1` (above validate() exclusion bound)

- **Severity**: low
- **Location**: `nli_detection_tick.rs:1293`
- **Description**: TC-01 sets `supports_candidate_threshold: 1.1` to guarantee zero Supports
  candidates with cosine-1.0 identical embeddings. The `validate()` function in `config.rs:924`
  rejects any value `>= 1.0` as out-of-range. This config struct bypasses `validate()` entirely
  — it is constructed directly with `InferenceConfig { ..InferenceConfig::default() }` and never
  passed to `validate()`. There is no security risk: this is test-only code inside `#[cfg(test)]`
  that never touches production. However, the approach is semantically imprecise — the value 1.1
  is not a real cosine and could silently mask a bug where the function fails to exclude
  Supports candidates for a different reason. The test comment acknowledges this and explains
  the strategy.
- **Recommendation**: Consider using `supports_candidate_threshold: 0.999` (within range,
  below any real cosine from identical embeddings) to avoid the bypass pattern. Low priority.
- **Blocking**: no

### Finding 3: `informs_candidates_found` counter counts post-guard candidates, not truly pre-guard

- **Severity**: low
- **Location**: `nli_detection_tick.rs:367`
- **Description**: The counter is incremented after `phase4b_candidate_passes_guards()` returns
  true (line 350–360), and the comment claims it is the "pre-dedup count." This is accurate —
  it is pre-dedup but post-guard. The RISK-TEST-STRATEGY.md describes FR-14 as requiring the
  count "before dedup and cap," which matches the implementation. However, the field name
  `informs_candidates_found` does not distinguish "found before guard" from "found after guard."
  A "floor too high" diagnosis would show `informs_candidates_found = 0`, correctly indicating
  no pairs passed the cosine floor. This is not a security issue, but the diagnostic semantics
  are subtly different from what FR-14 describes as "raw count before dedup" — the count is
  raw before DB-dedup, but not raw before the cosine+category filter. No data integrity risk.
- **Recommendation**: Update the log field comment to clarify "post-guard, pre-dedup" rather
  than ambiguous "pre-dedup." Not blocking.
- **Blocking**: no

### Finding 4: Category strings from config used as Informs guard without sanitization

- **Severity**: low
- **Location**: `nli_detection_tick.rs:772-776`, `config.rs:554-562`
- **Description**: `informs_category_pairs` is loaded from `config.toml` at startup and used
  as the category pair filter in Phase 4b. The values are compared against
  `entry.category.as_str()` via `.any(|pair| pair[0] == source_category && pair[1] ==
  target_category)`. This is a string equality check — no injection surface. The config file
  is operator-controlled, not user-controlled per request. The C-07 convention ("no domain
  string literals in production code") is respected: category strings come from config, not
  hardcoded. No OWASP injection concern applies here.
- **Recommendation**: None required.
- **Blocking**: no

---

## OWASP Analysis

| OWASP Category | Applicable | Assessment |
|----------------|------------|------------|
| A01 Broken Access Control | No | No trust boundary changes; `run_graph_inference_tick` is internal tick, not user-callable |
| A02 Cryptographic Failures | No | No cryptography involved |
| A03 Injection | No | Category strings are equality-compared, not interpolated into SQL; all DB writes use parameterized queries via sqlx bind() |
| A04 Insecure Design | No | Control flow split is architecturally sound; get_provider() is the sole Path B entry point |
| A05 Security Misconfiguration | No | `nli_informs_cosine_floor` raised from 0.45 to 0.50; this is a behavioral tightening, not a loosening |
| A06 Vulnerable Components | No | No new dependencies introduced |
| A07 Auth / Identity Failures | No | Internal background tick; no authentication surface |
| A08 Data Integrity Failures | No | MAX_INFORMS_PER_TICK=25 hard cap; dedup pre-filter prevents duplicates; INSERT OR IGNORE is the DB backstop |
| A09 Logging / Monitoring Failures | No | Four structured log fields added (AC-17 FR-14); existing contradiction scan log preserved |
| A10 SSRF | No | No HTTP/network operations in changed code |

---

## Blast Radius Assessment

**Worst case if the control flow split has a subtle bug:**

Path A (Informs write loop) runs before the `get_provider()` gate. If the write loop were to
invoke Path B code (e.g., through accidental call reordering), Supports edges could be written
with zero or garbage NLI scores. This is R-01 from the risk register. The architecture
structurally prevents this: `get_provider()` is the sole entry to Phase 6/7/8, and the Path A
write loop does not call any NLI functions. TC-02 validates the absence of Supports edges when
NLI is not ready.

**Worst case if Phase 4b writes incorrect Informs edges:**

The `GRAPH_EDGES` table accumulates edges. Incorrect Informs edges degrade retrieval quality
(PPR scores are biased) but do not expose stored content to unauthorized parties, do not enable
privilege escalation, and do not cause data loss. The `MAX_INFORMS_PER_TICK = 25` hard cap
limits how many spurious edges can accumulate per tick. The `apply_informs_composite_guard`
defense-in-depth re-evaluation at write time provides a second filter. Failure mode is safe:
degraded graph quality, not silent data corruption or information disclosure.

**Worst case if the contradiction scan block was inadvertently changed:**

The diff shows that only comments were added to the contradiction scan block — the `if
current_tick.is_multiple_of(CONTRADICTION_SCAN_INTERVAL_TICKS)` condition and
`get_adapter().await` guard are byte-identical to pre-crt-039. If this were altered to run
unconditionally, the O(N) ONNX scan would fire every 15-minute tick, causing severe CPU
pressure. The diff audit confirms R-09 is not triggered.

---

## Regression Risk

**Low.** The changes are:

1. `background.rs`: Removes `if inference_config.nli_enabled` wrapper around
   `run_graph_inference_tick`. The function now runs unconditionally. The only behavioral
   change when `nli_enabled = false` (production default) is that Phase 4b now executes and
   can write Informs edges. This is the intended fix. Phase 8 remains behind `get_provider()`
   inside the function. No existing functionality is removed.

2. `nli_detection_tick.rs`: Removes `NliCandidatePair::Informs` and `PairOrigin::Informs`
   variants. Both are fully absent from the codebase — confirmed by grep. The old Phase 8b
   write loop (previously inside the NLI batch path) is replaced by the Path A write loop
   (before the Path B gate). The old test
   `test_run_graph_inference_tick_nli_not_ready_no_op` is removed (its semantics are
   invalidated) and replaced by TC-01 and TC-02. All 55 module tests pass.

3. `config.rs`: `nli_informs_cosine_floor` default raised from 0.45 to 0.50. Existing entries
   with cosine in [0.45, 0.50) will no longer generate Informs candidates until they accumulate
   more similar neighbors. This is a narrowing, not a broadening. MRR eval gate (AC-11) is the
   quantitative regression check.

**No existing Supports edge behavior is changed.** Phase 8 control flow is identical to
pre-crt-039 for the case where `get_provider()` succeeds — same text fetch, same rayon batch,
same threshold checks.

---

## Dependency Safety

No new dependencies were introduced. `Cargo.toml` and `Cargo.lock` are unchanged by this PR.
`cargo audit` is not installed in this environment; no CVE check was possible. The changed
code uses only existing crates already in the workspace: `sqlx`, `serde_json`, `rand`,
`tracing`, `tokio`, `unimatrix-store`, `unimatrix-vector`, `unimatrix-embed`.

---

## Secrets Check

No hardcoded secrets, API keys, tokens, or credentials in the diff. Confirmed by pattern scan.

---

## PR Comments

- Posted 1 comment on PR #486.
- Blocking findings: no.

---

## Knowledge Stewardship

Nothing novel to store — the patterns observed here (control-flow split risks, guard removal
requiring candidate set separation, MAX_INFORMS_PER_TICK burst control) are all already
catalogued in Unimatrix entries #4017, #4018, #3949, #3675, and #3723. No new cross-feature
anti-pattern emerged that would warrant a lesson-learned entry.
