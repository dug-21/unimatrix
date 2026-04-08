# crt-050 Security Review Report

Agent: crt-050-security-reviewer
PR: #544
Feature: crt-050
GH Issue: #542

## Result

**Risk Level: LOW**
**Blocking Findings: No**

## Findings

### Finding 1 — Separator collision in `COUNT(DISTINCT phase || '|' || session_id)`
- **Severity**: Low
- **Location**: `crates/unimatrix-store/src/query_log.rs`, `count_phase_session_pairs`
- **Description**: String concatenation with fixed separator `'|'` to form a synthetic DISTINCT key is fragile. If `phase` contains `'|'`, different `(phase, session_id)` pairs produce the same concatenated string, silently undercounting distinct pairs. SQLite supports `COUNT(DISTINCT col_a, col_b)` natively.
- **Recommendation**: Replace `COUNT(DISTINCT (phase || '|' || session_id))` with `COUNT(DISTINCT phase, session_id)`.
- **Blocking**: No — phase strings do not contain `'|'` in practice; worst-case impact is a spurious `use_fallback=true` from an artificially low count. Lesson stored as Unimatrix #4241.

### Finding 2 — No index on `observations.hook` or `observations.tool`
- **Severity**: Low (performance, not security)
- **Location**: `crates/unimatrix-store/src/db.rs`, `observations` table schema
- **Description**: Query A filters on `hook = 'PreToolUse'` and `tool IN (...)`. No index on these columns. The `ts_millis` index narrows the window first. Already documented as R-11 in RISK-TEST-STRATEGY.md.
- **Recommendation**: Follow-up issue for composite index `ON observations(hook, tool, ts_millis)`.
- **Blocking**: No.

### Finding 3 — `PhaseOutcomeRow` re-exported with `#[doc(hidden)]`
- **Severity**: Low (architectural hygiene)
- **Location**: `crates/unimatrix-store/src/lib.rs`
- **Description**: Architecture specified `PhaseOutcomeRow` as internal. The `#[doc(hidden)]` re-export makes it technically public API. No security risk.
- **Blocking**: No.

## OWASP Assessment

| Concern | Verdict |
|---------|---------|
| SQL Injection | Clear — all SQL parameterized |
| Broken Access Control | Clear — no new API surface |
| Security Misconfiguration | Clear — both new config fields validated at startup |
| Deserialization | Clear — no new external deserialization |
| Input Validation | Clear — startup `validate()` rejects out-of-range values |
| Secrets / Credentials | Clear — none introduced |
| Unsafe code | Clear — none |
| New dependencies | Clear — none |

## Blast Radius

Worst case: subtle bug in `apply_outcome_weights` silently shifts rank ordering within `(phase, category)` buckets. Effect: degraded search relevance in phase affinity scoring (weight 0.05 in fused pipeline). No panic, no data corruption, no information disclosure.

## Merge Readiness

**READY** — no blocking findings.
