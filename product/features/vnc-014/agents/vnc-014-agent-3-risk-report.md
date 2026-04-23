# Agent Report: vnc-014-agent-3-risk

## Output

- `product/features/vnc-014/RISK-TEST-STRATEGY.md` written

## Risk Summary

| Priority | Risk Count |
|----------|-----------|
| Critical | 3 |
| High | 5 |
| Med | 7 (includes 4 security risks) |
| Low | 1 |

**Total**: 16 risks identified (R-01 through R-15 + 4 security sub-risks)

## Top Risks by Severity

1. **R-01 (Critical / High)** — Append-only triggers break `gc_audit_log` and `drop_all_data` DELETE paths. ADR-005 addresses remediation; risk is that the fix is incomplete or the wrong option is chosen for `drop_all_data`. Must be verified pre-migration.

2. **R-02 (Critical / High)** — Schema version cascade: 7+ touchpoints must advance to v25. Any missed touchpoint causes CI failure or runtime panic. Existing `test_schema_version_initialized_to_current_on_fresh_db` covers part of this; the `sqlite_parity.rs` column count and migration range test renaming need explicit verification.

3. **R-03 (Critical / Med)** — Cross-session attribution bleed if `Mcp-Session-Id` header fallback to `""` misclassifies HTTP sessions as stdio. Concurrent session test (AC-07) is the primary guard; the fallback path must also be tested independently.

4. **SEC-02 (High / Med)** — JSON injection in `metadata` via format-string construction. The SCOPE.md pseudocode escapes only `"`. Backslashes, newlines, and injection sequences like `"}` are not handled. Recommend replacing format-string construction with `serde_json::json!`.

5. **R-11 (High / Med)** — `create_tables_if_needed` DDL divergence. Fresh databases must have the same schema as migrated databases; if the DDL in `db.rs` is not updated byte-identical to the migration block, fresh-DB tests pass while migrated production DBs fail.

## Key Observation for Tester

SEC-02 is the highest-surprise risk: the SCOPE.md and ARCHITECTURE.md pseudocode both use `ct.replace('"', "\\\"")` for metadata construction. This escapes double-quotes but leaves backslash, newline, tab, and JSON structural characters unescaped. Any `clientInfo.name` from a non-compliant or adversarial client containing these characters produces invalid JSON in `metadata`. The fix (use `serde_json`) should be noted as an implementation correction, not just a test scenario.

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for `lesson-learned failures gate rejection` — #2758 (non-negotiable test name validation), #1203 (cascading rework from incomplete gate validation), #4177 (tautological assertions). Informed test scenario design: R-05 and R-11 scenarios are specifically non-tautological.
- Queried: `/uni-knowledge-search` for `SQLite migration schema column audit_log` — #4092 (pragma_table_info multi-column idempotency pattern) directly informs R-04 partial-migration scenarios.
- Stored: nothing novel — SEC-02 (JSON injection via metadata format string) is feature-specific; if it recurs in a future feature, store then.
