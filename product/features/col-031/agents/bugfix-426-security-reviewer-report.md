# Security Review: bugfix-426-security-reviewer

## Risk Level: low

## Summary

This is a pure numeric constant change: `FRESHNESS_HALF_LIFE_HOURS` is raised from 168.0
(1 week) to 8760.0 (1 year) in one authoritative location in `unimatrix-engine`. The change
flows into `ConfidenceParams::default()` and from there into all serving-path callers. No new
code paths, no new inputs, no new trust boundaries. The fix is correct, minimal, and safe.

## Findings

### Finding 1 — Division by zero latent exposure in freshness_score (not introduced by this PR)
- **Severity**: low (pre-existing, not introduced here)
- **Location**: `crates/unimatrix-engine/src/confidence.rs:208`
- **Description**: `freshness_score` performs `(-age_hours / params.freshness_half_life_hours).exp()`.
  If `freshness_half_life_hours` were 0.0 the result is `f64::NEG_INFINITY.exp() == 0.0`
  (IEEE 754 well-defined; not a panic). However, config-layer validation in
  `crates/unimatrix-server/src/infra/config.rs:1254` already rejects `v <= 0.0`, `NaN`, and
  `Inf` before the value reaches the engine. The compiled default (8760.0) and all named
  presets are positive. There is a struct-level test (AC-27 at line 1060) that constructs
  `freshness_half_life_hours: 0.0` for exhaustive-field coverage — this is a compile-time
  completeness check and does not reach the formula. No action needed for this PR.
- **Recommendation**: Pre-existing. Not introduced by this fix.
- **Blocking**: no

### Finding 2 — Test sentinel values in services/confidence.rs changed to 336.0 (not the new default)
- **Severity**: informational
- **Location**: `crates/unimatrix-server/src/services/confidence.rs:191, 272`
- **Description**: Two test `ConfidenceParams` literals previously set `freshness_half_life_hours:
  168.0` (which matched the old compiled default) and are now set to 336.0. The change is
  correct: these tests construct explicit non-default params to verify that `ConfidenceService`
  stores the supplied params rather than `::default()`. Using 336.0 keeps the sentinel distinct
  from both the old default (168.0) and the new default (8760.0), which preserves the test's
  intent of verifying non-default params are honoured.
- **Recommendation**: No action required. The change is correct.
- **Blocking**: no

### Finding 3 — stale_deprecated fixture ages extended from 90 days / 180 days to 2 years / 3 years
- **Severity**: informational
- **Location**: `crates/unimatrix-engine/src/test_scenarios.rs:158-160`
- **Description**: With a 1-year half-life, a 90-day-old entry scores exp(-90/365) ≈ 0.78 (still
  fresh-looking). Moving to 2 years / 3 years gives exp(-2) ≈ 0.135, preserving the "stale"
  semantics needed for `standard_ranking` to produce the correct ordering. This is a necessary
  recalibration, not a change in test coverage or logic.
- **Recommendation**: No action required.
- **Blocking**: no

## OWASP Assessment

| Check | Verdict |
|---|---|
| Injection (SQL, command, path traversal) | Not applicable — no query or path construction in the diff |
| Broken access control | Not applicable — no access-control logic changed |
| Security misconfiguration | Not applicable — default tightened (slower decay = more knowledge retained) |
| Vulnerable components | No new dependencies introduced |
| Data integrity failures | No serialization or data-write paths changed |
| Deserialization risks | No deserialization in the diff |
| Input validation gaps | Config-layer validation for `freshness_half_life_hours` is pre-existing and unchanged; NaN/Inf/<=0.0/overflow guards are all present and verified |
| Secrets / credentials | No hardcoded secrets found in any changed file |

## Blast Radius Assessment

If the fix introduced a subtle regression, the worst-case outcome is: confidence scores for
knowledge entries older than a few days trend toward zero, effectively hiding all but the
most recently-accessed entries from search results. This would be a **quality degradation**
(information retrieval failure), not a security event. It would produce no data corruption,
no privilege escalation, and no information disclosure. The regression test
`freshness_score_30day_old_entry_under_default_params_exceeds_floor` explicitly guards
against the 168h reversion scenario.

The constant is used only in confidence computation, which is fire-and-forget from the hot
path. A wrong value here cannot cause a panic, crash, or data loss.

## Regression Risk

Low. The change:
- Affects all confidence scores via `ConfidenceParams::default()` — scores for entries
  older than a few days will be higher than before. This is the intended fix.
- The `collaborative` preset (the default serving preset) picks up the new constant via
  `ConfidenceParams::default()`, confirmed by the SR-10 invariant test.
- Existing tests that were coupled to the 168.0 default were correctly decoupled to use
  explicit params, so the test suite is not fragile to future constant changes.
- The `stale_deprecated` fixture was recalibrated; ordering assertions in `standard_ranking`
  are preserved.
- No integration test xfail markers were introduced.

## PR Comments
- Posted 1 comment on PR #427
- Blocking findings: no

## Knowledge Stewardship
- nothing novel to store — this is a single-constant bugfix with pre-existing config
  validation and no new security surface. The pattern of decoupling tests from compiled
  defaults was already stored by the fix agent (entry #3698).
