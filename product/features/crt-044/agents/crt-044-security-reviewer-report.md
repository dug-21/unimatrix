# Security Review: crt-044-security-reviewer

## Risk Level: low

## Summary

crt-044 back-fills bidirectional edges for S1/S2/S8 graph sources via a schema v19→v20 migration and adds symmetric second `write_graph_edge` calls in three tick functions. All SQL uses parameterized queries or string literals only — no external input reaches the migration or tick write paths. The quarantine obligation on `graph_expand` is addressed by an existing call-site check in the sole caller (`search.rs`) and is now made visible with a `// SECURITY:` comment. No new dependencies were introduced. No secrets were found. No blocking findings.

---

## Findings

### Finding 1: NOT EXISTS sub-query omits source filter — deliberate but worth documenting
- **Severity**: low
- **Location**: `crates/unimatrix-store/src/migration.rs`, lines 721-726 and 751-756
- **Description**: The `NOT EXISTS` guard in Statement A checks `rev.relation_type = 'Informs'` but does NOT filter by `rev.source`. If a reverse `Informs` edge for a pair already exists with `source='nli'`, the migration silently skips inserting the S1/S2 reverse for that pair. The pair ends up with a reverse edge, but the reverse edge carries `source='nli'` rather than `source='S1'` or `source='S2'`. In a clean DB this is unlikely, but on a DB that had ad-hoc NLI edges written, the back-fill silently leaves those specific pairs with a source mismatch. The `INSERT OR IGNORE` safety net is the actual deduplication guard — this is a correctness nuance rather than a security gap. The ARCHITECTURE.md documents this as a design choice (`INSERT OR IGNORE is the correctness safety net`).
- **Recommendation**: Accept as-is: the outer `INSERT OR IGNORE` prevents any duplicate, and the UNIQUE constraint is `(source_id, target_id, relation_type)` — it does not include `source`. A reverse `Informs` edge with source='nli' already satisfies the bidirectionality requirement for traversal. The correctness nuance is: `source` tag would be 'nli' rather than 'S1'/'S2' for those specific pairs. This is unlikely to occur in production and is not a security issue. No action required.
- **Blocking**: no

### Finding 2: graph_expand quarantine obligation — single call site, verified satisfied
- **Severity**: low
- **Location**: `crates/unimatrix-engine/src/graph_expand.rs:68-69` (comment), `crates/unimatrix-server/src/services/search.rs:927` (obligation fulfillment)
- **Description**: `graph_expand` returns a `HashSet<u64>` without quarantine filtering. The security comment added by this feature correctly states this. Verified: the sole caller in `search.rs` applies `SecurityGateway::is_quarantined()` at line 927 before inserting any graph-expanded IDs into the result set. The function has one call site — `search.rs` — confirmed by grep across all crates. The `// SECURITY:` comment is a forward-looking guard for future call sites.
- **Recommendation**: No action required. Obligation is satisfied at the only call site. Comment correctly documents the contract.
- **Blocking**: no

### Finding 3: write_nli_edge Ok branch returns true for both insert and conflict — inconsistent with write_graph_edge
- **Severity**: low
- **Location**: `crates/unimatrix-server/src/services/nli_detection.rs:49-51`
- **Description**: `write_nli_edge` returns `true` for any Ok result (including UNIQUE conflict with `rows_affected = 0`), while `write_graph_edge` returns `query_result.rows_affected() > 0`. These sibling functions have different return semantics. This is pre-existing, not introduced by crt-044. crt-044 does not call `write_nli_edge` — it only adds second calls to `write_graph_edge`. Callers of `write_nli_edge` that interpret the return as "edge newly written" could silently miscount — but no such caller exists in the changed code.
- **Recommendation**: Pre-existing issue, out of scope for this PR. Flag for a future cleanup sprint. Not a regression introduced by crt-044.
- **Blocking**: no

---

## OWASP Assessment

| Check | Result |
|-------|--------|
| SQL Injection | No risk. Migration SQL uses string literals only. `write_graph_edge` uses parameterized binds (`?1` through `?7`). No user-supplied strings reach SQL composition. |
| Broken Access Control | Not applicable. Migration runs at server startup under the process's own SQLite handle. Tick functions run as internal background services. |
| Security Misconfiguration | No risk. `CURRENT_SCHEMA_VERSION` correctly bumped to 20. `INSERT OR IGNORE` + `NOT EXISTS` double guard for idempotency. |
| Deserialization | Not applicable. No new deserialization paths introduced. |
| Input Validation | The `valid_ids` guard in `run_s8_tick` validates both `a` and `b` before any write; the swapped second call uses the same pre-validated IDs. No external input surface. |
| Data Integrity | The migration writes data-only rows using the same UNIQUE constraint already present on `GRAPH_EDGES`. `INSERT OR IGNORE` ensures the constraint is never violated. |
| Vulnerable Components | No new dependencies introduced. Cargo.toml and Cargo.lock are unchanged in the diff. |
| Hardcoded Secrets | None found. All string literals in the diff are SQL keywords, edge type names (`'Informs'`, `'CoAccess'`), and source tags (`'S1'`, `'S2'`, `'S8'`). |

---

## Blast Radius Assessment

**Worst case if the migration has a subtle bug:**
Statement A or B inserts zero rows (e.g., wrong WHERE filter). Existing S1/S2/S8 forward-only edges remain unidirectional. `graph_expand` cannot reach lower-ID partners from higher-ID seeds. The crt-042 eval gate (`ppr_expander_enabled`) produces suboptimal P@5 scores. No data corruption, no information disclosure, no privilege escalation. Failure mode is entirely functional (degraded recall quality), not a security incident.

**Worst case if the tick second-call has a bug (e.g., args not swapped):**
Self-loop attempt (`write_graph_edge(a, a, ...)`) hits `UNIQUE(source_id, target_id, relation_type)`. Because `source_id == target_id`, the insert either succeeds (if no prior self-loop) or is ignored. Graph traversal from any seed would loop to itself and be suppressed by the visited-set in `graph_expand`. No data corruption. No privilege escalation. Failure mode: incorrect reverse edges written; graph quality degrades silently.

**Verdict:** Blast radius is bounded to graph data quality degradation. No security incident pathway exists.

---

## Regression Risk

**Existing tests updated correctly:** 12 existing tick tests were updated to double their expected edge count, consistent with the behavioral change (one pair now produces two edges). The updates are accurate and well-annotated with `// crt-044:` comments.

**New regression guards added:** Three independent per-source bidirectionality tests (`test_s1_both_directions_written`, `test_s2_both_directions_written`, `test_s8_both_directions_written`) directly guard against the highest-probability regression: removal of the second `write_graph_edge` call from any single tick function.

**Schema version conflict risk (R-02 from RISK-TEST-STRATEGY):** Verified — no crt-043 PR is open. The only open PRs are this one (#498), a draft crt-020, and the long-standing infra-001. No version conflict exists at merge time.

**Migration v18→v19 tests:** The crt-035 migration test was correctly loosened from `assert_eq!(version, 19)` to `assert!(version >= 19)`, which is forward-compatible with future version bumps. This is the correct pattern per sqlite_parity.rs comment history.

**`co_access_promotion_tick.rs` (already bidirectional):** Not touched. Correct — it was made bidirectional in crt-035 and is not part of crt-044's scope.

**NLI and cosine_supports edges:** Confirmed excluded by `source IN ('S1', 'S2')` and `source = 'S8'` filters respectively. These intentionally unidirectional edges are not affected.

---

## Input Validation Verification

**Migration SQL:** All values derived from `graph_edges` columns in a self-join. No user input, no MCP parameters, no external data.

**Tick functions (S1, S2):** IDs are `row.source_id as u64` and `row.target_id as u64` from a `HAVING count >= 3` aggregate query over internal tables. Trust level: internal DB query results.

**Tick function (S8):** IDs `*a` and `*b` are drawn from a `pairs` set that was already validated by `valid_ids.contains(a) && valid_ids.contains(b)` before the first `write_graph_edge` call. The swapped second call uses the same two IDs — both are already in `valid_ids`. No additional validation is needed.

**graph_expand inputs:** `seed_ids: &[u64]` comes from the internal query pipeline, not from raw MCP parameters. Seeds that are not present in `graph.node_index` are silently skipped — no panic path.

---

## Dependency Safety

No new crate dependencies. `Cargo.toml` and `Cargo.lock` are unchanged. `cargo audit` status is not changed by this PR.

---

## Secrets Check

No hardcoded credentials, tokens, API keys, or secrets were found in the diff. All string literals are domain-specific constants (`'Informs'`, `'CoAccess'`, `'S1'`, `'S2'`, `'S8'`, `'schema_version'`).

---

## PR Comments

- Posted 1 informational comment on PR #498
- Blocking findings: no

---

## Knowledge Stewardship

Nothing novel to store — the quarantine obligation pattern and migration source-filter approach are already documented in Unimatrix (entries #4081, #3913, #3978). The NOT EXISTS source-filter nuance is feature-specific and not a generalizable anti-pattern.
