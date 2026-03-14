# Security Review: crt-019-security-reviewer

## Risk Level: medium

## Summary

The crt-019 diff implements the Bayesian confidence formula, adaptive blend weight state, implicit helpful vote injection, and doubled access weight for `context_lookup`. The security fundamentals are sound: no injection surfaces, no exposed privileged fields, no hardcoded secrets. However, two functional defects affecting correctness and one hardcoded value issue were found. One defect (the background tick handle shadowing) is blocking because it silently defeats the entire adaptive blend feature on every deployment — the adaptive `confidence_weight` will never update beyond the initial default.

---

## Findings

### Finding 1 — Background tick shadows the shared ConfidenceStateHandle (BLOCKING)

- **Severity**: high
- **Location**: `crates/unimatrix-server/src/background.rs`, `run_single_tick`, line 235
- **Description**: `run_single_tick` receives the shared `confidence_state: &ConfidenceStateHandle` (the same `Arc` held by `SearchService`) as a function parameter. At line 235, it immediately creates a new local handle and shadows the parameter:
  ```rust
  let confidence_state = crate::services::confidence::ConfidenceState::new_handle();
  let status_svc = StatusService::new(..., Arc::clone(&confidence_state));
  ```
  The `StatusService` is wired to the throwaway local handle, not the shared one. The maintenance tick computes and writes the empirical prior and observed spread into the throwaway handle which is dropped at the end of every tick cycle. `SearchService` reads from the original shared handle, which retains its initial default value (`confidence_weight = 0.18375`, `alpha0 = 3.0`, `beta0 = 3.0`) for the lifetime of the server process.
- **Blast radius**: The adaptive blend feature never activates. Search re-ranking uses the fixed initial weight permanently. Entries with updated empirical priors never benefit from the background tick's prior computation. This is a silent correctness failure — no error is logged, no panic occurs.
- **Recommendation**: Remove the local `let confidence_state = ...` line at line 235. Use the parameter (`Arc::clone(confidence_state)`) when constructing `StatusService`.
- **Blocking**: yes

---

### Finding 2 — Hardcoded `0.18375` in `uds/listener.rs` (non-blocking)

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/uds/listener.rs`, line 1017
- **Description**: `rerank_score(*sim, entry.confidence, 0.18375)` uses the initial default weight as a hardcoded literal in the UDS injection log path. This value is not read from `ConfidenceState` — it will not adapt even if Finding 1 is fixed. The UDS path is a secondary transport (not the primary MCP path), but this creates an inconsistency: the MCP path uses the adaptive weight while the UDS log path uses a permanent literal.
- **Recommendation**: Pass `confidence_weight` from a `ConfidenceState` read here, consistent with the MCP `search.rs` path.
- **Blocking**: no

---

### Finding 3 — `allow(dead_code)` on SearchService.confidence_state suppresses useful warning (non-blocking)

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/services/search.rs`, line 89
- **Description**: `#[allow(dead_code)]` is applied to `confidence_state`. The field is actually read at line 128 (the `confidence_weight` snapshot before search). The annotation is likely a carry-over from initial scaffolding. It suppresses compiler feedback that could detect future accidental non-use.
- **Recommendation**: Remove the `#[allow(dead_code)]` annotation since the field is actively used.
- **Blocking**: no

---

## SEC-01: Bayesian Prior Manipulation Attack Surface (addressed, accepted risk)

The prior clamp `[0.5, 50.0]` is implemented and tested. At maximum clamp (`alpha0=50, beta0=0.5`), an unvoted entry scores `50/50.5 ≈ 0.99` on the helpfulness component. With `W_HELP = 0.12`, the maximum helpfulness-driven contribution to confidence is `0.12 * 0.99 ≈ 0.119`. This limits the blast radius of prior manipulation to under 12 percentage points of confidence score. The zero-variance degeneracy case (all entries at identical rate) correctly falls back to cold-start defaults. The threshold of 10 voted entries provides an additional barrier — an attacker needs 10 biased entries in the active-and-voted population before empirical estimation engages.

Assessment: acceptable risk per RISK-TEST-STRATEGY SEC-01. The clamp and threshold are implemented and tested.

---

## SEC-02: access_weight Not Exposed as External Input (verified safe)

`access_weight` does not appear in any MCP parameter schema (`LookupParams`, `GetParams`, `SearchParams`). It is constructed exclusively in `tools.rs` with server-hardcoded values (1 or 2). External callers cannot inject an arbitrary weight.

---

## SEC-03: Implicit helpful vote injection (verified safe)

The `context_get` handler injects `helpful: params.helpful.or(Some(true))`. This folds into the existing `UsageContext.helpful` field and is processed by the existing `UsageDedup` one-vote-per-agent-per-entry gate. No second `spawn_blocking` task is spawned. `helpful: Some(false)` from the caller correctly overrides the implicit true. The implementation matches the architecture spec (Component 5).

---

## R-11: store.record_usage_with_confidence duplicate-ID deduplication (verified, functional)

The store's `write_ext.rs:80` converts `access_ids` to a `HashSet`, which deduplicated the `flat_map` repeat of `access_ids`. However, the outer loop iterates over `all_ids` — also repeated via `multiplied_all_ids`. Since `access_set.contains(&id)` is true for the id, each iteration of the outer loop adds `+1`. With `[id, id]` in `all_ids`, the loop runs twice and the id is incremented twice. The approach works correctly. The store also adds `increment_access_counts` as an explicitly documented R-11 fallback. Test coverage exists in `test_context_lookup_access_weight_2_increments_by_2` and `test_context_lookup_dedup_before_multiply_second_call_zero`.

---

## RwLock Poison Recovery (verified consistent)

All lock acquisitions across `confidence.rs`, `status.rs`, `usage.rs`, and `search.rs` use `unwrap_or_else(|e| e.into_inner())`. This is consistent with the existing `CategoryAllowlist` convention and guards against FM-03.

---

## NaN Propagation Guards (verified adequate)

`helpfulness_score` has an explicit `if score.is_nan() { return 0.5; }` guard. `compute_empirical_prior` checks `variance <= 0.0` and `ratio <= 1.0` before computing the method-of-moments, returning cold-start defaults in degenerate cases. `compute_observed_spread` uses `.max(0.0)` for non-negative guarantee.

---

## Blast Radius Assessment

If Finding 1 (the blocking defect) has a subtle secondary bug or is not fixed: the confidence system degrades to a fixed static weight with cold-start priors permanently. No crash, no data loss — purely a functionality regression. The feature appears to work, all tests pass, but no adaptation occurs. This is the worst-case silent failure mode.

If the access_weight path has a subtle bug: `access_count` increments for `context_lookup` are either silently dropped or doubled incorrectly. This affects search ranking signal quality but does not affect data integrity or security.

---

## Regression Risk

The changes touch core formula constants (`W_BASE`, `W_USAGE`, `W_HELP`, `W_TRUST`) and the `compute_confidence`/`rerank_score`/`helpfulness_score` signatures. These affect every confidence score in the system. The golden value assertions in `pipeline_regression.rs` have been updated, which is the expected procedure (SR-06). The T-REG-01 ordering (`good > auto`) is preserved by the `base_score(Proposed, "auto") = 0.5` constraint (R-10 verified correct). All 4 `rerank_score` call sites in `search.rs` have been updated. The `UsageContext.access_weight` field is struct-construction-complete — any missed site would cause a compile error (confirmed: no `Default` impl with `access_weight: 0`).

---

## OWASP Assessment

| Check | Finding |
|-------|---------|
| Injection | No SQL injection risk — parameterized queries throughout `write_ext.rs` |
| Broken access control | `access_weight` is server-internal, not externally injectable |
| Security misconfiguration | No new configuration surfaces introduced |
| Insecure deserialization | No new deserialization of untrusted data |
| Input validation | `helpfulness_score` clamps NaN inputs; `compute_empirical_prior` guards zero variance |
| Sensitive data exposure | No secrets, tokens, or keys in the diff |
| Vulnerable components | No new dependencies added |

---

## PR Comments

- Posted 1 blocking comment on PR #256 (Finding 1: handle shadowing)
- Posted 2 non-blocking comments (Finding 2: hardcoded weight in UDS; Finding 3: dead_code annotation)
- Blocking findings: yes

---

## Knowledge Stewardship

Stored: nothing novel to store — the `let x = new_value()` shadowing of a function parameter is a general Rust anti-pattern already known; the security-relevant pattern (server-internal fields not exposed in public schemas) is correct practice documented in the existing ADR set. Nothing generalizable beyond this PR.
