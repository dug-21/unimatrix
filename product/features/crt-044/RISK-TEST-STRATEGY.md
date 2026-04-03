# Risk-Based Test Strategy: crt-044

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Migration v19→v20 Statement A uses wrong `relation_type` value — copy-paste from crt-035 S8 template applies `'CoAccess'` instead of `'Informs'` for S1/S2 block, silently back-filling nothing or the wrong rows | High | Med | Critical |
| R-02 | Delivery sequencing conflict: crt-043 ships before crt-044, consuming v20 and leaving crt-044 with a duplicate version number — migration runs against already-v20 DB and the `< 20` guard skips the back-fill silently | High | Med | Critical |
| R-03 | One of the three tick functions (`run_s1_tick`, `run_s2_tick`, `run_s8_tick`) omits the second `write_graph_edge` call — graph asymmetry reappears for that source after the migration back-fill but forward writes stop repairing it | High | Med | Critical |
| R-04 | `write_graph_edge` false return on second direction call mishandled — implementation adds a warning log or error counter for the UNIQUE conflict, producing noise in steady-state post-migration operation | Med | Med | High |
| R-05 | `pairs_written` counter in `run_s8_tick` incremented once per pair instead of once per edge — counter semantics remain per-pair, understating actual DB writes and diverging from `co_access_promotion_tick` semantics | Med | Med | High |
| R-06 | Migration Statement B (`source='S8'`) back-fills edges for `source='co_access'` rows that are already bidirectional from v18→v19 — if `AND source='S8'` filter is dropped or mis-quoted, `co_access` edges get duplicate reverse attempts | Med | Low | Med |
| R-07 | `nli` or `cosine_supports` Informs edges accidentally back-filled — migration SQL uses a broader filter (e.g., `relation_type='Informs'` without source filter), converting intentionally unidirectional NLI edges to bidirectional | High | Low | High |
| R-08 | Security comment text in `graph_expand.rs` diverges from the actual `SecurityGateway::is_quarantined()` call site over time — comment becomes stale obligation marker (accepted by ADR-003, but testability gap remains) | Low | Low | Low |
| R-09 | Migration block not placed inside the outer transaction boundary — a mid-migration failure leaves schema_version unset at 20 but partial reverse edges inserted, causing inconsistent graph state on next startup | High | Low | High |
| R-10 | `CURRENT_SCHEMA_VERSION` constant not bumped in `migration.rs` — `migrate_if_needed` never executes the `< 20` block on a fresh DB open, leaving all existing edges forward-only | High | Low | High |

---

## Risk-to-Scenario Mapping

### R-01: Migration Statement A uses wrong `relation_type`
**Severity**: High
**Likelihood**: Med
**Impact**: All existing S1/S2 Informs edges remain forward-only. crt-042 eval gate fails — seeds with higher IDs cannot reach lower-ID Informs partners. Root cause invisible because no error is raised.

**Test Scenarios**:
1. Insert a forward-only S1 Informs edge `(1→2, relation_type='Informs', source='S1')` and a forward-only S8 CoAccess edge `(5→6, relation_type='CoAccess', source='S8')`. Run migration. Assert `(2→1, relation_type='Informs', source='S1')` exists AND `(6→5, relation_type='CoAccess', source='S8')` exists. Either missing means the wrong `relation_type` was used in one statement.
2. After migration, query `SELECT COUNT(*) FROM graph_edges WHERE relation_type='Informs' AND source IN ('S1','S2')` before and after — count must double (or increase by the forward-edge count if some pairs were already bidirectional). A zero increase signals Statement A ran with wrong filter.

**Coverage Requirement**: Per-source per-direction assertion in the migration test suite (AC-09). S1, S2, and S8 each verified individually, not via combined count.

---

### R-02: Delivery sequencing conflict — crt-043 ships first and consumes v20
**Severity**: High
**Likelihood**: Med
**Impact**: crt-044's `if current_version < 20` block never runs because the DB is already at v20 (set by crt-043). All S1/S2/S8 edges remain forward-only in production. This is a silent failure: no error, no warning, migration appears to succeed.

**Test Scenarios**:
1. Before crt-044 delivery merges, verify `CURRENT_SCHEMA_VERSION` in the target branch is 19 and no `current_version < 20` block exists other than crt-044's addition. If crt-043 has shipped, the implementation agent must renumber to v21 before merge.
2. Migration integration test: start from a v19 fixture (not v20). Assert after migration that `schema_version = 20` and reverse edges exist. If crt-043 has already taken v20, this test must be updated to v21 and the migration block renumbered.

**Coverage Requirement**: Pre-merge gate: reviewer must confirm `CURRENT_SCHEMA_VERSION` is 19 in the base branch before this PR is merged. Spec constraint C-08 is the acceptance gate; the crt-043 delivery sequencing note in SPECIFICATION.md §Open Questions is the coordination point.

---

### R-03: One tick function omits the second `write_graph_edge` call
**Severity**: High
**Likelihood**: Med
**Impact**: Graph asymmetry returns for that specific source going forward. Historical edges are bidirectional (back-filled by migration) but new pairs written after migration are forward-only. The regression is source-specific and invisible without per-source integration tests.

**Test Scenarios**:
1. `run_s1_tick` test: two-entry fixture with shared tags. Run tick once. Assert both `(a→b, source='S1', relation_type='Informs')` and `(b→a, source='S1', relation_type='Informs')` exist in `GRAPH_EDGES` (AC-03, AC-10).
2. `run_s2_tick` test: two-entry fixture with structural vocabulary overlap. Run tick once. Assert both directions with `source='S2'` (AC-04, AC-10).
3. `run_s8_tick` test: two entries with co-access signal. Run tick once. Assert both directions with `source='S8'` and `relation_type='CoAccess'` (AC-05, AC-10).
4. Each test must query `GRAPH_EDGES` directly — checking only the return value or counter is insufficient.

**Coverage Requirement**: Three independent per-source bidirectionality tests. These are regression guards: if any future change removes the second call in one tick, exactly one test fails without affecting the others.

---

### R-04: False return on second direction call mishandled
**Severity**: Med
**Likelihood**: Med
**Impact**: Steady-state post-migration operation produces `warn!` log entries for every tick iteration where the reverse edge already exists (most of them). Log noise obscures real errors; monitoring alert thresholds may fire.

**Test Scenarios**:
1. Simulate post-migration state: pre-insert both `(a→b)` and `(b→a)` edges in the fixture. Run tick. Assert: (a) no error or warn log entries at the warn/error level, (b) `edges_written` / `pairs_written` increments by 0 (both calls return false for already-existing edges), (c) no error counter increments (AC-13).
2. Confirm the tick completes successfully and the `false` return path does not short-circuit processing of the next pair in the loop.

**Coverage Requirement**: AC-13 test explicitly asserts absence of warn-level logging when second call returns false. Entry #4041 establishes the three-case return contract; the test must validate the UNIQUE-conflict case.

---

### R-05: `pairs_written` counter remains per-pair in `run_s8_tick`
**Severity**: Med
**Likelihood**: Med
**Impact**: Counter understates actual DB writes by 2× for new pairs. Log output misleads operators reviewing tick throughput. Monitoring or alerting keyed off this counter produces incorrect data.

**Test Scenarios**:
1. Single new pair `(a, b)` where neither direction exists. Run `run_s8_tick`. Assert `pairs_written = 2` after the tick (both directions inserted, each returning `true`).
2. Single pair `(a, b)` where reverse edge already exists (post-migration state). Run tick. Assert `pairs_written = 1` (first call `true`, second call `false` — UNIQUE conflict).
3. AC-12 verification: confirm PR description explicitly documents the semantic shift.

**Coverage Requirement**: Numeric assertion on the counter value for both the new-pair and steady-state cases. Satisfies AC-12.

---

### R-06: `co_access` edges accidentally included in Statement B back-fill
**Severity**: Med
**Likelihood**: Low
**Impact**: Already-bidirectional `co_access` CoAccess edges get `INSERT OR IGNORE` attempted for their already-existing reverse edges. No data corruption (UNIQUE constraint rejects duplicates), but the migration runs unnecessary work and the `source` field mismatch could surface unexpected edge behavior.

**Test Scenarios**:
1. Exclusion test: insert a `(source='co_access', relation_type='CoAccess')` forward edge and its existing reverse edge. Run v19→v20 migration. Assert row count is unchanged for `source='co_access'` rows — no new rows inserted.
2. Assert `NOT EXISTS` guard works correctly when the reverse edge is already present.

**Coverage Requirement**: AC-09 exclusion test (ARCHITECTURE.md §Test Requirements test case 5).

---

### R-07: `nli` or `cosine_supports` Informs edges accidentally back-filled
**Severity**: High
**Likelihood**: Low
**Impact**: NLI Informs edges are intentionally unidirectional per col-030 ADR. Adding reverse edges converts them to bidirectional, which is semantically incorrect and causes graph traversal to follow paths that were deliberately excluded.

**Test Scenarios**:
1. Insert a `(source='nli', relation_type='Informs')` edge. Run v19→v20 migration. Assert no reverse `(source='nli', relation_type='Informs')` edge was created.
2. Insert a `(source='cosine_supports', relation_type='Informs')` edge. Run migration. Assert no reverse edge with this source.
3. Both assertions are part of the exclusion test (ARCHITECTURE.md §Test Requirements test case 5).

**Coverage Requirement**: Explicit `source='nli'` and `source='cosine_supports'` exclusion assertions in migration test suite. C-04 is the constraint; these tests verify the `source IN ('S1','S2')` filter is correctly scoped.

---

### R-08: Security comment in `graph_expand.rs` becomes stale
**Severity**: Low
**Likelihood**: Low
**Impact**: The `// SECURITY:` comment text diverges from the actual `SecurityGateway::is_quarantined()` call site if the method is renamed or the calling pattern changes. The comment becomes a misleading obligation marker. Accepted per ADR-003.

**Test Scenarios**:
1. Verify the comment is present at the `pub fn graph_expand(` signature (AC-08): `grep '// SECURITY:' graph_expand.rs` is non-empty.
2. No behavioral test can verify comment accuracy — this is the accepted limitation per ADR-003. Future refactors of `SecurityGateway` should include a grep of `// SECURITY:` comments as a pre-merge checklist item.

**Coverage Requirement**: AC-08 static check only. No runtime test. Risk is accepted.

---

### R-09: Migration block outside outer transaction boundary
**Severity**: High
**Likelihood**: Low
**Impact**: If Statement A succeeds and Statement B fails, the schema_version is not bumped but Statement A's inserts are committed. On restart, `current_version < 20` runs again, re-attempts Statement A (idempotent — no harm), and re-attempts Statement B. Net effect: eventual consistency if the failure is transient. If Statement B fails permanently, S8 edges are never back-filled but S1/S2 are. Inconsistent bidirectionality by source.

**Test Scenarios**:
1. Verify in code review that both SQL statements are inside the `if current_version < 20` block, which is inside the outer transaction managed by `migrate_if_needed`. The `schema_version` bump must be the last operation in the block.
2. Migration atomicity is verified structurally (code review) and behaviorally by the idempotency test: if migration is re-run from v19, the result is the same. A partial-commit failure path is hard to test directly in SQLite — rely on code review and the outer transaction boundary established by FR-M-07.

**Coverage Requirement**: Code review confirms transaction scope. AC-07 and AC-14 idempotency tests provide indirect coverage.

---

### R-10: `CURRENT_SCHEMA_VERSION` not bumped
**Severity**: High
**Likelihood**: Low
**Impact**: `migrate_if_needed` never executes the `< 20` block against any database — fresh opens remain at v19, the block is skipped on first run, and reverse edges are never inserted. Forward-only graph state persists silently.

**Test Scenarios**:
1. AC-06: assert `grep 'CURRENT_SCHEMA_VERSION' migration.rs` returns `= 20` after the change.
2. Migration integration test: start from a v19 DB fixture. After `migrate_if_needed`, assert `schema_version = 20` via a `SELECT value FROM counters WHERE name='schema_version'` query.
3. Full fresh-DB startup test: assert that a newly opened store (starting from schema_version = 0) runs all migrations including `< 20` and ends at version 20.

**Coverage Requirement**: AC-06 + migration test that reads `schema_version` from the counters table after migration.

---

## Integration Risks

### Tick-to-GRAPH_EDGES Write Path

Each of the three tick functions calls `write_graph_edge` twice per pair. The second call targets the same `UNIQUE(source_id, target_id, relation_type)` index as the first but with swapped IDs. After migration, most second calls result in UNIQUE conflicts. The risk surface is:

- The second call's `false` return is on the Ok path (not Err) — correct per entry #4041. Implementation must distinguish Ok(false) from Err.
- If `write_graph_edge` is refactored to return `Result<bool>` or changes its inner error handling, both callers in each tick loop iteration are affected.

### Migration-to-Forward-Write Coordination

The migration back-fills historical edges; tick changes fix forward writes. Both must ship together. If only the migration ships (tick changes omitted), new post-migration pairs remain forward-only. If only tick changes ship (migration omitted), existing edges remain forward-only until a tick happens to re-encounter each pair. The `if current_version < 20` block is the deployment gate — if it runs, all three tick changes are guaranteed to be present in the same binary.

### crt-043 Baseline Dependency

crt-043 treats schema v20 as its migration input baseline (migrates v20→v21). This creates a hard ordering constraint: crt-044 must merge before crt-043 or crt-043 must renumber its migration to v21 (if crt-044 already shipped v20). A race between the two PRs produces a version conflict that silently skips one feature's migration. This is the highest-probability integration failure for this feature — the two deliveries are concurrent.

---

## Edge Cases

| Edge Case | Description | Scenario |
|-----------|-------------|----------|
| Already-bidirectional pairs in migration input | Some S1/S2/S8 pairs may have both directions before migration (e.g., if written by a one-off repair script). `NOT EXISTS` guard prevents re-insertion; `INSERT OR IGNORE` is the backstop. | AC-14 two-run idempotency test with pre-existing reverse edge |
| Empty `GRAPH_EDGES` table | Migration runs on a DB with no S1/S2/S8 edges. Both statements insert zero rows. No error. | Migration test on clean DB fixture |
| Single-entry database | No pairs can exist. Tick functions iterate over zero pairs. No `write_graph_edge` calls. | Tick test with single-entry store asserts zero edges written |
| Pair where `source_id == target_id` | Self-loops. The UNIQUE constraint and the existing tick query shapes (`t2.entry_id > t1.entry_id`) prevent self-pairs from being generated. No explicit guard needed, but migration SQL would also skip self-loops via the NOT EXISTS check. | Verify no self-loop rows in post-migration fixtures |
| Post-migration tick run on steady-state DB | All pairs already bidirectional. Every second `write_graph_edge` call returns `false`. Counter increments only on first calls. | AC-13 steady-state test (R-04 scenario 1) |
| Migration re-run (restart after crash mid-migration) | `INSERT OR IGNORE` + `NOT EXISTS` together ensure re-running from v19 produces identical row counts. | AC-07, AC-14 |

---

## Security Risks

### `graph_expand` Quarantine Obligation (documentation gap)

`graph_expand` returns a `HashSet<u64>` of entry IDs without filtering quarantined entries. The caller (`search.rs`) is currently the only call site and correctly applies `SecurityGateway::is_quarantined()`. The `// SECURITY:` comment added by this feature makes the obligation visible at every future call site.

- **Untrusted input surface**: None directly. `graph_expand` takes `seed_ids: &[u64]` from the internal query pipeline — not from raw user input. The seed IDs have already passed through the search query path.
- **Blast radius if obligation is missed by a future caller**: Quarantined entries appear in result sets. Their content is returned to agents and human queries. This is a data exposure risk, not an injection risk.
- **Mitigation**: The two-line `// SECURITY:` comment at the function signature is a call-site obligation marker. The module-level doc block (lines 12-18) contains the full obligation. Together they make accidental omission less likely. No logic change is made by this feature.

### Migration SQL — No External Input Surface

Both migration SQL statements use string literals only. No user-provided values, no parameterized inputs, no interpolation. SQL injection is not applicable to this migration block.

### Tick Functions — Internal IDs Only

`write_graph_edge` receives `source_id` and `target_id` as `u64` values derived from database query results (trusted). The swapped-argument second call uses the same trusted IDs. No external input surface.

---

## Failure Modes

| Failure | Expected Behavior | Verification |
|---------|-----------------|--------------|
| `write_graph_edge` second call returns `false` (UNIQUE conflict) | Silent ignore. No warn log. No counter increment. Tick continues. | AC-13 |
| `write_graph_edge` second call returns `false` (Err path) | `warn!` emitted inside `write_graph_edge`. Outer tick does not double-log. Counter not incremented. | Code review; existing write_graph_edge error path |
| Migration Statement B SQL syntax error | Full outer transaction rolls back. `schema_version` remains at 19. Next startup re-attempts both statements. | FR-M-07; idempotency tests |
| crt-043 ships first, consuming v20 | `< 20` block never runs. S1/S2/S8 edges remain forward-only. No error surface — silent regression. | Pre-merge gate: reviewer checks base branch version |
| Full workspace test failure after merge | `cargo test --workspace` exits non-zero. PR blocked. | AC-11 |

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 — `pairs_written` counter semantics shift doubles S8 values | R-05 | ADR-002 specifies per-edge counting. AC-12 requires PR documentation. Tests assert counter = 2 for new pair, = 1 for steady-state pair. |
| SR-02 — `write_graph_edge` false return mishandled as error | R-04 | ADR-002 cites entry #4041 three-case return contract. AC-13 tests the false-return steady-state path. FR-T-05/FR-T-06 enforce correct handling in spec. |
| SR-03 — Future source filter creep silently excludes new Informs sources | — | Accepted. Architecture scopes the migration to explicit `source IN ('S1','S2')` and `source='S8'` filters. ADR-001 notes each new source needs its own migration block (entry #4078). Low/Low severity per SCOPE-RISK-ASSESSMENT. |
| SR-04 — Security comment staleness | R-08 | Accepted per ADR-003. The obligation is carried by the module-level doc block and the `search.rs` call site. The `// SECURITY:` comment is a visibility aid only. AC-08 is a static check. |
| SR-05 — Migration idempotency on partial-bidirectionality input | R-09 | AC-14 tests migration against a DB with pre-existing reverse edges. `INSERT OR IGNORE` + `NOT EXISTS` are both required per C-05. |
| SR-06 — One tick function omits second call; asymmetry reappears per-source | R-03 | Per-source integration tests (AC-10) assert both directions after each individual tick function runs. These are independent regression guards per ARCHITECTURE.md §Test Requirements. |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 3 (R-01, R-02, R-03) | 8 scenarios — per-source migration assertions (3), delivery sequencing gate (2), per-source tick bidirectionality tests (3) |
| High | 4 (R-04, R-07, R-09, R-10) | 7 scenarios — false-return steady-state test, nli/cosine exclusion tests (2), transaction boundary code review, schema_version counter assertions (2), fresh-DB test |
| Medium | 2 (R-05, R-06) | 4 scenarios — pairs_written counter assertions (2), co_access exclusion test, NOT EXISTS guard verification |
| Low | 1 (R-08) | 1 scenario — static grep check for SECURITY comment presence |

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "lesson-learned failures gate rejection" — found entries #2758 (Gate 3c non-negotiable test names), #4076 (Gate 3b: zero mandatory tests shipped). Entry #4076 directly informs R-03 severity elevation (per-source tick tests are mandatory).
- Queried: `/uni-knowledge-search` for "risk pattern migration graph edge bidirectional" — found entries #4078 (S8 gap pattern), #3889 (back-fill filter by source), #4066 (BFS test plan pairing). All three directly inform R-01 and R-07.
- Queried: `/uni-knowledge-search` for "SQLite migration schema version delivery sequencing conflict" — found entry #3894 (schema version cascade checklist). Informs R-10 and R-02 test scenarios.
- Queried: `/uni-knowledge-search` for "write_graph_edge budget counter return value false boolean" — found entry #4041 (write_graph_edge three-case return contract). Directly informs R-04.
- Stored: nothing novel to store — R-02 (delivery sequencing version conflict) is feature-specific context. Pattern of "concurrent migrations consuming the same schema version number" is a candidate for future storage if seen again across 2+ features.

---

*Risk strategy authored by crt-044-agent-3-risk (claude-sonnet-4-6). Written 2026-04-03.*
