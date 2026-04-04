# Security Review: crt-046-security-reviewer (bugfix #515)

## Risk Level: low

## Summary

This bugfix PR addresses both findings from the original crt-046 delivery security review
(PR #512). The three missing `InferenceConfig::validate()` range checks are correctly
implemented with proper NaN guards. The unbounded `store.get()` loop is capped at 50.
No new security concerns were introduced. No injection, access control, deserialization,
or secrets findings. The changes are minimal and targeted.

---

## Findings

### Finding 1 — NaN guard on pre-existing f32 threshold checks (observation, not blocking)

- **Severity**: low (pre-existing, not introduced by this PR)
- **Location**: `crates/unimatrix-server/src/infra/config.rs`, lines 1088–1308
- **Description**: The three new fields added by this bugfix correctly use `!v.is_finite()`
  as a prefix guard before comparison (pattern documented in Unimatrix entry #4133). However,
  the older threshold fields — `supports_candidate_threshold`, `supports_edge_threshold`,
  `nli_entailment_threshold`, `nli_contradiction_threshold`, `ppr_alpha`,
  `ppr_inclusion_threshold`, and `nli_informs_cosine_floor` — do NOT have the
  `!v.is_finite()` guard. A TOML file containing `nli_entailment_threshold = inf` would pass
  validation and propagate into NLI scoring.
  - This is a PRE-EXISTING condition not introduced by this PR.
  - f32/f64 comparison semantics: NaN satisfies neither `<= 0.0` nor `>= 1.0`, so NaN
    bypasses the existing guards silently. `f32::INFINITY` satisfies `>= 1.0` and is correctly
    rejected by exclusive upper-bound checks (e.g., `>= 1.0`). The only actual NaN bypass
    risk exists for the older fields.
  - The fix in this PR uses the correct pattern. No remediation needed in this PR's scope.
  - Tracking: a follow-up issue to add `!v.is_finite()` guards to the 8 older float fields
    would reduce this residual risk. Not a blocker for this PR.
- **Blocking**: no.

### Finding 2 — CLUSTER_ID_CAP truncation drops newest IDs (accepted design decision)

- **Severity**: informational
- **Location**: `crates/unimatrix-server/src/mcp/tools.rs`, line 1178
- **Description**: The cap truncates after `sort_unstable()` ascending by u64 value, so the
  numerically highest (most recently created) IDs are dropped first. The comment in the code
  correctly documents this. For a normal deployment this is acceptable: recently-created
  entries are not yet established knowledge. However, in a regression scenario where many new
  high-value entries are created rapidly within a single cycle, briefing blending may exclude
  them from cluster consideration.
  - This is a documented, intentional trade-off. The comment is accurate.
  - No security concern; failure mode is silent omission of recent entries from cluster-based
    briefing only. Semantic path still includes them.
  - The cap of 50 (vs the original security review's recommendation of 100) is more
    conservative and therefore more secure against latency amplification.
- **Blocking**: no.

---

## Fix Verification

### Fix 1: InferenceConfig::validate() range checks

The three added guard blocks are verified correct against the field documentation:

| Field | Documented range | Guard condition | NaN safe? |
|-------|-----------------|-----------------|-----------|
| `goal_cluster_similarity_threshold` | (0.0, 1.0] | `!v.is_finite() \|\| v <= 0.0 \|\| v > 1.0` | Yes |
| `w_goal_cluster_conf` | finite, non-negative | `!v.is_finite() \|\| v < 0.0` | Yes |
| `w_goal_boost` | finite, non-negative | `!v.is_finite() \|\| v < 0.0` | Yes |

The `!v.is_finite()` prefix is in the correct position — it evaluates before the comparison
operators, catching both NaN and Infinity before they reach the range check.

The `goal_cluster_similarity_threshold` uses `<= 0.0` (exclusive lower) and `> 1.0`
(inclusive upper, i.e., 1.0 is valid). This matches the documented range `(0.0, 1.0]` and
is consistent with the ADR-005 field semantics where an exact cosine match of 1.0 is valid.

Note: previous validation code for similar float fields (e.g., `ppr_blend_weight`, lines
1250–1254) uses `*value < 0.0 || *value > 1.0` — which is the same inclusive-both-ends
pattern, and does NOT have the `!is_finite()` prefix. The new code is strictly more correct.

### Fix 2: CLUSTER_ID_CAP in context_briefing

The truncation is inserted in the correct position: after `dedup()` and before the
`entry_max_sim` HashMap construction and the sequential `store.get()` loop. This means:

1. The cap applies to the deduplicated set — no inflation from duplicate IDs across clusters.
2. The `entry_max_sim` HashMap is still built by iterating all `top_clusters` rows (not the
   capped list), but entries for IDs above the cap are never looked up. Dead map entries.
   This is a minor inefficiency, not a correctness issue and not a security concern.
3. The sequential `store.get()` loop is bounded by 50 iterations maximum.

The `const CLUSTER_ID_CAP: usize = 50` definition is local to the function scope. The test
module duplicates this value correctly as a module-level const for test isolation. There is
no shared definition that could drift — a follow-up could unify them, but this is cosmetic.

---

## OWASP Checks (diff-scoped)

| Check | Finding |
|-------|---------|
| SQL Injection | **CLEAR.** No new SQL in the diff. The config validation and vec truncation touch no database layer. |
| Injection (general) | **CLEAR.** No format strings, shell commands, or path operations in the changed code. |
| Broken access control | **CLEAR.** The cap and validation changes are purely internal. No new trust boundaries. |
| Security misconfiguration | **RESOLVED.** The three previously unvalidated fields now have correct range guards. |
| Deserialization | **CLEAR.** No new deserialization in the diff. `serde_json::from_str::<Vec<u64>>` (pre-existing) is bounded integer parsing with error handling. |
| Input validation | **RESOLVED.** Config values from operator-supplied TOML files are now validated correctly at startup for all three new fields. |
| Hardcoded secrets | **CLEAR.** No secrets, tokens, keys, or credentials in the diff. |
| Unsafe Rust | **CLEAR.** No unsafe blocks introduced. Gate report confirms no unsafe in either changed file. |
| New dependencies | **CLEAR.** No new Cargo dependencies. |
| Error handling | **CLEAR.** `ConfigError::NliFieldOutOfRange` propagates cleanly to server startup failure — correct behavior for config validation errors. |
| Denial of service | **RESOLVED (mitigated).** The unbounded loop cap converts a potential latency cliff to bounded O(50) store.get() calls per briefing. |

---

## Blast Radius Assessment

**If the fix has a subtle bug — worst cases named:**

1. Off-by-one in the threshold guard: if `goal_cluster_similarity_threshold = 1.0` were
   rejected (e.g., if the guard used `>= 1.0` instead of `> 1.0`), valid operator configs
   with threshold=1.0 would fail at server startup. Impact: startup failure requiring config
   correction. Non-silent, non-data-corrupting. The test `test_validate_goal_cluster_similarity_threshold_one_passes` directly catches this.

2. If CLUSTER_ID_CAP were set to 0 instead of 50: `truncate(0)` would produce an empty
   cluster list, silently falling through to `cluster_entries_with_scores.is_empty()` check
   and returning pure semantic results. No crash, no data corruption, but cluster-based
   blending would be silently disabled. The value 50 is correct and tested.

3. If truncation ran before dedup (wrong ordering): a 100-element pre-dedup list that dedupes
   to 20 unique IDs would be truncated to 50 pre-dedup, yielding potentially fewer than 20
   unique IDs post-dedup. The test `test_cluster_id_cap_dedup_then_truncate` catches the
   correct ordering.

**Worst-case blast radius** for a subtle regression in this fix:
- Config domain: server startup fails for operator with a legitimately-configured field value
  caught by an incorrectly-written guard. Recoverable by config correction. No data loss.
- Briefing domain: cluster-based blending returns fewer or different entries than expected
  (silent quality degradation, not corruption). Pure semantic fallback remains active.
- No escalation paths, no data corruption, no privilege violation, no information disclosure.

---

## Regression Risk

**Low.** The two changes are narrowly scoped:

1. The validate() guards run at server startup, failing fast on invalid config. Existing
   deployments with default configs (0.80, 0.35, 0.25) are well within all new ranges and
   are unaffected. The changes add guards after all existing checks — no interaction risk
   with earlier validation logic.

2. The truncate() call modifies `cluster_entry_ids_raw` before any downstream use. The cap
   is generous (50 entries vs. typical 5–10). Normal deployments will not hit the cap. The
   only risk is new behavior for edge-case deployments with very large historical cycles.
   In that edge case, the behavior improves (bounded latency) rather than degrades.

The 17 targeted tests (12 + 5) and the 4499-test full suite pass confirms no regressions.

---

## PR Comments

- Posted 1 comment on PR #516 via `gh pr review 516 --comment`.
- Blocking findings: no.

---

## Knowledge Stewardship

- Stored: nothing novel to store — the NaN bypass gap in older f32 threshold fields is a
  generalizable finding, but entry #4133 already captures the correct pattern ("always prefix
  with !v.is_finite()"). A follow-up tracking issue would be appropriate, but it does not
  constitute a new Unimatrix lesson. The security-reviewer severity calibration lesson (#3766)
  remains relevant: where an established procedure explicitly mandates a step and that step
  was omitted, treat it as blocking. Both fixes in this PR address exactly that class of
  omission and are now correctly implemented.
