# Security Review: crt-042-security-reviewer-report

## Risk Level: low

## Summary

The crt-042 PPR Expander change is a well-contained, security-conscious implementation. The quarantine check (`SecurityGateway::is_quarantined`) is correctly positioned in Phase 0 before any expanded entry reaches `results_with_scores`, mirroring the existing Phase 5 pattern. The feature ships behind `ppr_expander_enabled = false` by default, so the attack surface addition is zero until an operator explicitly enables the flag. No new external trust boundaries are introduced, no user-controlled integers reach the BFS, no new dependencies are added, and no secrets are present in the diff.

Two findings are noted (both low severity, neither blocking): a future-caller quarantine contract gap that is partially mitigated by module-level documentation, and a latency concern (not a security risk per se) when the flag is enabled at full expansion capacity. No blocking findings.

---

## Findings

### Finding 1: Quarantine Caller Contract Gap (future-caller risk)

- **Severity**: low
- **Location**: `crates/unimatrix-engine/src/graph_expand.rs` — module-level doc comment, lines 33–38
- **Description**: `graph_expand` is a pure function with no quarantine enforcement internally. The module doc comment explicitly states "Any caller that adds returned IDs to a result set MUST independently apply `SecurityGateway::is_quarantined()` before use." The current caller (`search.rs` Phase 0) correctly applies this check (line 927). However, `graph_expand` is now `pub`-accessible from `unimatrix-engine`'s `graph` module, making it callable from any future code in the workspace. A future caller that skips the quarantine check would silently expose quarantined entries. The documentation contract alone is not enforced by the type system; no compile-time or runtime guard prevents bypass.
- **Verification**: `pub use graph_expand::graph_expand;` is re-exported from `graph.rs`. The function signature `fn graph_expand(...) -> HashSet<u64>` returns raw IDs with no quarantine annotation. The Rust type system cannot encode this obligation.
- **Recommendation**: This is an acceptable risk given the architectural decision (pure function contract is intentional per ADR-001). The doc comment obligation is correctly documented. To reduce future-caller risk, consider adding a `#[doc = "SECURITY: Caller MUST apply SecurityGateway::is_quarantined() before inserting returned IDs into result sets"]` attribute or a `// SECURITY:` inline marker at the function signature level (not just the module doc). Not blocking — current caller is correct.
- **Blocking**: no

---

### Finding 2: O(N) Embedding Scan — Latency-Based DoS Potential at High Config Values

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/services/search.rs` lines 935–938; `crates/unimatrix-server/src/infra/config.rs` lines 722–728
- **Description**: When `ppr_expander_enabled = true`, Phase 0 calls `vector_store.get_embedding()` for each expanded entry. This is an O(N) HNSW scan per entry. The operator-controllable upper bound is `max_expansion_candidates = 1000` (validated range). At 1000 expanded entries against a 70,000-entry corpus: 1000 × O(70,000) = 70M f32 comparisons per search request. This is not exploitable by an end user (the parameter is server-config, not query input), but a misconfigured deployment could produce severe search latency degradation on every request when the flag is on. The feature ships with `false` default and the ARCHITECTURE.md documents a P95 latency gate (≤50ms over baseline) required before default enablement.
- **OWASP relevance**: OWASP A05:2021 (Security Misconfiguration) — operator misconfiguration of `max_expansion_candidates` without measuring latency could create availability issues.
- **Recommendation**: The existing validation correctly rejects 0 and values above 1000. The default value (200) and the latency gate requirement are appropriate mitigations. No change needed. The follow-up for O(1) embedding lookup (filed as open issue per search.rs comment line 932) is the correct long-term fix. Not blocking.
- **Blocking**: no

---

### Finding 3: Quarantine Check Position — Verified Correct

- **Severity**: informational (no issue found)
- **Location**: `crates/unimatrix-server/src/services/search.rs` lines 920–928
- **Description**: The quarantine check sequence in Phase 0 is: (1) fetch entry via `entry_store.get()` — on error, skip; (2) `SecurityGateway::is_quarantined(&entry.status)` — if quarantined, skip; (3) `vector_store.get_embedding()` — on None, skip; (4) push to `results_with_scores`. This ordering is correct. The quarantine check fires after the entry is loaded (so we have the actual status from the database, not a stale ID) and before the entry is added to the result pool. A quarantined entry can never reach `results_with_scores` through this code path. The two integration tests (AC-13 direct 1-hop, AC-14 transitive 2-hop) confirm this invariant.
- **Recommendation**: No action needed.
- **Blocking**: no

---

### Finding 4: No Injection Risk — Seed IDs Are Database-Derived

- **Severity**: informational (no issue found)
- **Location**: `crates/unimatrix-server/src/services/search.rs` line 892
- **Description**: `seed_ids` passed to `graph_expand` are collected from `results_with_scores`, which contains entries returned by HNSW vector search. These are database-assigned `u64` entry IDs, not user-supplied integers. The user controls the query text (which drives the embedding), but cannot directly inject specific entry IDs into the BFS seed set. There is no SQL construction, shell invocation, or format string in the BFS path. No injection risk.
- **Recommendation**: No action needed.
- **Blocking**: no

---

### Finding 5: No Secrets, No New Dependencies

- **Severity**: informational (no issue found)
- **Description**: The full diff contains no hardcoded API keys, tokens, passwords, or credentials. `Cargo.toml` files were not modified — no new dependencies introduced. `tracing-test = "0.2.6"` is a pre-existing dev-dependency on `main`. The eval profile (`ppr-expander-enabled.toml`) contains only configuration values, no sensitive data.
- **Recommendation**: No action needed.
- **Blocking**: no

---

## Blast Radius Assessment

**Worst case if Phase 0 has a subtle bug (flag-off path)**: Zero blast radius. The outer guard `if self.ppr_expander_enabled` is the very first statement inside `if !use_fallback`. When `ppr_expander_enabled = false` (the default), execution does not enter the Phase 0 block at all. No `Instant::now()`, no BFS, no fetch, no push. The flag-off path is bit-identical to pre-crt-042.

**Worst case if Phase 0 has a subtle bug (flag-on path)**: A bug in `graph_expand` BFS (e.g., cycle not terminated) could cause unbounded CPU usage on one search request, capped only by `max_candidates` (1000 absolute max, 200 default). The result would be one slow search request, not a crash or data corruption. The BFS visited set prevents infinite loops on any finite graph. Even in the cycle-termination failure mode, `max_candidates` is a hard counter that breaks the loop.

A bug in the quarantine check ordering (moving the check after `results_with_scores.push`) could allow a quarantined entry to appear in search results. The blast radius is information disclosure of a quarantined entry's content to a search caller — not privilege escalation or storage corruption. The current code is correct.

A bug in the in-pool deduplication check could cause the same entry to appear twice in `results_with_scores`, giving it double personalization mass in Phase 1. This is a retrieval-quality issue, not a security issue. The `in_pool` set is constructed from `seed_ids` (not from `results_with_scores` at post-Phase-0 state), which creates a subtle window: if an entry was added to `results_with_scores` between seed collection and the expanded loop (not possible in the current synchronous Phase 0 logic), the deduplication could miss it. In practice this window cannot open because Phase 0 is entirely within one `await` chain with no interleaving.

**Storage blast radius**: Zero. `graph_expand` is read-only over a cloned `TypedRelationGraph`. Phase 0 writes only to `results_with_scores` (a local `Vec`). No SQLite writes, no HNSW mutations, no global state.

---

## Regression Risk

**Flag-off regression (R-01)**: Low risk. The feature flag guard is the first statement in Phase 0. Test AC-01 (`test_search_flag_off_pool_size_unchanged`) explicitly asserts pool length is unchanged with `ppr_expander_enabled = false`. The existing full search test suite must pass with the default flag.

**Config struct hidden test sites (R-08)**: The three new `InferenceConfig` fields use `..InferenceConfig::default()` spread syntax in all test literal constructions (confirmed by reviewing the test additions in `config.rs`). The serde default functions (`default_ppr_expander_enabled`, `default_expansion_depth`, `default_max_expansion_candidates`) are consistent with `Default::default()` values. Tests `test_inference_config_expander_serde_fn_matches_default` and `test_inference_config_expander_fields_defaults` verify this alignment. The four-site pattern (struct body, Default impl, serde fn, validate) is followed.

**Config merge regression**: The merge logic uses the `if project_value != default_value { project_wins } else { global_wins }` pattern, consistent with all other InferenceConfig fields. `test_inference_config_merged_propagates_expander_fields` covers this.

**PPR algorithm regression**: `graph_expand` is called before Phase 1. When the flag is off, Phase 1 receives `results_with_scores` identical to pre-crt-042. When the flag is on, Phase 1 receives a wider pool — this is the intended behavior. The `personalized_pagerank` function itself is unchanged.

---

## OWASP Checklist

| Concern | Assessment |
|---------|-----------|
| A01 Broken Access Control | Not applicable. `graph_expand` returns entry IDs; quarantine check is in place before any expanded entry reaches results. Phase 0 does not change which caller has access to which entries — it only widens the pool subject to the same quarantine gate as Phase 5. |
| A02 Cryptographic Failures | Not applicable. No cryptographic operations. |
| A03 Injection | Not present. Seed IDs are database-derived u64 values. No SQL, shell, or format string injection risk in the BFS path. |
| A04 Insecure Design | Not applicable. The caller quarantine obligation is an acknowledged design tradeoff (pure function contract). It is documented at the module level and enforced in the only current caller. |
| A05 Security Misconfiguration | Low risk. `max_expansion_candidates = 1000` at full config could cause latency issues if the flag is enabled without latency measurement. The feature flag default of `false` mitigates this. Validation is unconditional. |
| A06 Vulnerable Components | Not applicable. No new dependencies introduced. `tracing-test 0.2.6` is pre-existing. |
| A07 Authentication/Authorization Failures | Not applicable. No auth path changes. |
| A08 Data Integrity Failures | Not applicable. Phase 0 is read-only on both store and graph. |
| A09 Logging Failures | Not applicable. Timing instrumentation uses `debug!` level (correct). No sensitive data logged — only entry counts and elapsed milliseconds. |
| A10 SSRF | Not applicable. No outbound HTTP. |

---

## PR Comments

- Posted 1 comment on PR #496 (comment below).
- Blocking findings: no.

---

## Knowledge Stewardship

- Stored: nothing novel to store — the quarantine caller contract gap (pure function with external security obligation) is already captured as a generalizable pattern risk in the RISK-TEST-STRATEGY. The finding that `graph_expand` is `pub` with no type-system enforcement of the quarantine contract is specific to crt-042 and does not represent a new cross-feature anti-pattern beyond what is already documented in ADR-007 (Enforcement Point Architecture for Security, entry #83) and the SR-07 entries. No lesson-learned is novel enough to store.
