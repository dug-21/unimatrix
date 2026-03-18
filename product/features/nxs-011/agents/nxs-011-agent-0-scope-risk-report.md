# Agent Report: nxs-011-agent-0-scope-risk

## Output
- `/workspaces/unimatrix/product/features/nxs-011/SCOPE-RISK-ASSESSMENT.md` (38 lines)

## Risk Summary
- High severity: SR-01, SR-04, SR-07, SR-08 (4 risks)
- Medium severity: SR-02, SR-05, SR-06, SR-09, SR-10 (5 risks)
- Low severity: SR-03 promoted to Medium (stale sqlx-data.json)

## Top 3 Risks for Architect/Spec Writer Attention

1. **SR-04 (High/Med)** — migration.rs adaptation: 983 lines of rusqlite-specific migration logic adapted to sqlx without a regression harness. A schema version corruption here is silent and irreversible against a live database. Architect must sequence migration as a dedicated non-pooled connection pre-pool-construction and spec must mandate a 12-version migration regression test.

2. **SR-08 (High/Med)** — Analytics queue shed observability: the shed-under-load policy silently drops writes with only a WARN log. `co_access`, `outcome_index`, and `sessions` data can vanish without operator visibility. Spec writer should add a cumulative shed counter to `context_status` output; AC-15 (WARN log) is necessary but not sufficient for production observability.

3. **SR-09 (Med/High)** — Drain task shutdown protocol: Store-owned drain task has no defined shutdown path. In the ~1,445 tests that open/close a Store, an undefined shutdown leaves tokio tasks racing against pool teardown. Architect must define the shutdown protocol (oneshot channel + bounded await on drop) before spec — this affects both the Store API and test helper design.

## Knowledge Stewardship
- Queried: /uni-knowledge-search for "lesson-learned failures gate rejection" — returned #1105, #1045, #1775 (pass outcomes, no relevant failures for this domain)
- Queried: /uni-knowledge-search for "outcome rework storage migration" — returned nxs-006/008/009/010 outcomes, all pass; no rework events found for storage layer features
- Queried: /uni-knowledge-search for "risk pattern" category:pattern — returned #1542 (background tick error semantics), directly applicable to SR-08
- Queried: /uni-knowledge-search for "SQLite migration async spawn_blocking" — returned #771 (blocking lock_conn hang, lesson-learned), directly applicable to SR-10
- Queried: /uni-knowledge-search for "async trait RPITIT" — returned #2044 (nxs-011 pattern, already stored)
- Stored: entry #2057 "Store-owned background task requires explicit shutdown protocol before spec" via /uni-store-pattern — novel cross-feature pattern visible across crt-014, crt-018, and now nxs-011
