# Risk-Based Test Strategy: crt-058 — Eager Agent-Authored Edge Cleanup at `context_deprecate`

Scope: one synchronous, non-fatal `DELETE FROM graph_edges WHERE (source_id=?1 OR target_id=?1) AND source=?2 RETURNING …` inserted at handler step 6.5, an `Option<u64>` `edges_removed` threaded through `format_status_change`, and a tuple-capturing audit event. The load-bearing safety property is **eager ⊆ tick**; the delete is irreversible.

Historical grounding: #4162 (fixing one SQL cleanup pass is insufficient — audit ALL passes on the table), #3910/#5417 (multi-pass same-table filter divergence → ghost records; the eager ⊆ tick basis), #3448 (fire-and-forget log discipline), #5427 (string/call-count tests are blind to argument threading).

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Subset test's blind spot: both ADR-003 fixtures seed a **bare-deprecated (successor-less)** entry, so the `R ⊆ T` comparison never exercises the ONE case that actually breaks the invariant — eager running on a **successor-bearing** entry Phase 1 would repoint. That case is guarded only by a separate, weaker structural assertion. | High | Med | **Critical** |
| R-02 | Eager/tick predicate drift: a future widening of the eager predicate (adds a machine source) or narrowing of the tick (gains a source filter that keeps agent edges) reintroduces divergence + ghost records (#3910/#5417). | High | Low-Med | **High** |
| R-03 | `DELETE … RETURNING` **commits**, then row-marshaling / helper returns `Err` after the commit → edges irreversibly gone but **no audit record** emitted (audit fires only on `Ok`, ADR-002) and advisory omitted (`None`). Non-fatal path assumes `Err ⇒ edges still present`; a post-commit error violates that and defeats SR-01 reconstructability. | High | Low | **High** |
| R-04 | Zero-case rendering **contradiction**: AC-05/NFR-04 require a zero-edge deprecation to be indistinguishable from pre-feature output (advisory omitted); ADR-004 mandates `Some(0)` renders `0` in **every** format. Spec Open Question 2 is unresolved. Tester cannot author AC-05 without a decision. | Med | High | **High** |
| R-05 | Per-format count drop: one of Summary/Markdown/Json fails to thread `edges_removed` and ships green under a call-count/substring test (SR-04, #5427). | Med | Med | **High** |
| R-06 | `delete_agent_edges_for_entry` is **unguarded** — the LOCKED predicate keys only on id + provenance, with no status/`superseded_by` check. Safety rests entirely on the single chokepoint call site. A future second caller (or accidental reuse) deletes **live** agent edges on an Active or successor-bearing entry. | High | Low | **High** |
| R-07 | Concurrent-tick count under-report: between step-6 flip and step-6.5 delete, a concurrent `EveryTick` compaction (same `write_pool_server()`) may already delete the entry's edges; the eager `RETURNING` then captures fewer/zero tuples. State stays consistent; the reported count + audit under-count what existed at deprecation. | Low | Low | Medium |
| R-08 | Double audit-event confusion: two records per deprecation — the flip's `"context_deprecate"` and the cleanup's `"context_deprecate.edge_cleanup"`. Tests/operators querying by entry may assert on the wrong event or double-count. | Low | Med | Medium |
| R-09 | Provenance enumeration drift (SR-03): a future `EDGE_SOURCE_*` for agent/human-authored edges is missed by the inclusive `source='agent'` filter. Subset-safe (tick sweeps), but silently under-cleaned. | Low-Med | Low | Medium |
| R-10 | Edge-case arithmetic: a self-loop (`source_id = target_id = entry`) or a high-degree entry — count/audit correctness and audit-JSON growth. | Low | Low | Low |
| R-11 | Idempotent/ordering regression: step 6.5 landing before the step-5 early-return, or before the step-6 flip, would delete on re-deprecation or match nothing (predicate keys on the now-non-Active id). | High | Low | **High** |

## Risk-to-Scenario Mapping

### R-01: Subset test cannot see the actual break condition
**Severity**: High · **Likelihood**: Med · **Impact**: The team believes eager ⊆ tick is enforced by a behavioral test, but the `R ⊆ T` assertion is computed over two successor-less fixtures where the invariant trivially holds. The only divergence that loses data — eager deleting an inbound agent edge that Phase 1 would repoint to a live successor — occurs exclusively on a successor-bearing entry, which neither fixture constructs. Enforcement of the real hazard collapses onto the "companion structural assertion" (`context_deprecate` leaves `superseded_by` NULL), which is a property of the flip path, not of the eager helper.

**Test Scenarios**:
1. **Positive subset (ADR-003 as specified):** identical seed of one edge per (direction × source); fixture A runs the real eager helper → `R`; fixture B runs the real `run_orphaned_edge_compaction` → `T`; assert `R ⊆ T` and `R` == exactly the two `agent` edges.
2. **Break-condition guard (the missing half):** construct a Deprecated entry that DOES carry a successor (`superseded_by` set), seed an inbound agent edge, and assert the **eager helper is never invoked** on it via the production `context_deprecate` path — i.e. drive `context_correct`/successor path and assert no `"context_deprecate.edge_cleanup"` audit event and the inbound edge survives (Phase 1 would repoint it).
3. **Negative mutation:** temporarily point the eager helper at a successor-bearing entry in a unit harness and assert it WOULD destroy the repointable edge — documents why the chokepoint, not the helper, is the guarantor; ensures the test author has modeled the hazard.

**Coverage Requirement**: Both the `R ⊆ T` subset assertion AND an explicit chokepoint-exclusion assertion (successor-bearing entry never reaches the eager helper through production code). The subset test alone is insufficient; the negative path must be asserted against the real handler, not asserted in prose.

### R-02: Predicate drift (eager widening / tick narrowing)
**Severity**: High · **Likelihood**: Low-Med · **Impact**: Ghost records and divergence (#3910/#5417); an agent edge the tick would keep is destroyed early, or an edge neither pass removes lingers.

**Test Scenarios**:
1. The ADR-003 test must invoke **both real functions** (never re-implement either predicate). Widening the eager SQL to a machine source breaks the exact-set assertion (`R` == two agent edges); narrowing the tick so Phase 2 keeps agent edges breaks `R ⊆ T`.
2. Fixture-identity assertion: A and B must be seeded from a **single shared helper** so the two fixtures cannot silently drift; assert the pre-deprecation edge sets are identical before divergent processing.
3. Snapshot the exact locked predicate string in a test so a casual edit to the `WHERE`/`RETURNING` clause is caught (guards against accidental relation-type blocklist creep — F2 discipline).

**Coverage Requirement**: The subset test fails on ANY divergence of either predicate; fixtures share one seeding helper; the locked predicate text is pinned.

### R-03: Post-commit error → irreversible delete with no audit trail
**Severity**: High · **Likelihood**: Low · **Impact**: The whole point of ADR-002 (tuple audit for reconstructability of an irreversible delete) is silently defeated when the delete commits but the helper returns `Err` during row marshaling — edges gone, `edges_removed = None`, no `edge_cleanup` audit event, no `warn` trail of *what* was removed.

**Test Scenarios**:
1. Fault-inject a failure in the RETURNING-row deserialization path (after the statement executes); assert whether edges are present or absent and whether an audit record exists — surface the inconsistency.
2. Verify the helper's error boundary: confirm the delete + tuple capture are one atomic statement (single `execute`/`fetch_all` on the `RETURNING`), so there is no window where rows are deleted but tuples are lost. If the implementation deletes then separately selects, that is a defect — assert single-statement capture.
3. Assert the `warn` log on the `Err` branch carries the entry id (per #3448 the warn must be visible, not downgraded).

**Coverage Requirement**: Delete + tuple capture proven atomic (single statement). On any `Err`, either the rows are still present (tick backstops) OR the audit already recorded them — never "gone with no record." A test must assert this is not possible, or the design must guarantee the audit is derivable from the same fetched tuples that would be lost.

### R-04: Zero-case rendering contradiction (AC-05 vs ADR-004)
**Severity**: Med · **Likelihood**: High · **Impact**: AC-05/NFR-04 demand the zero-edge deprecation be byte-indistinguishable from pre-feature output at the advisory slot; ADR-004 says `Some(0)` renders `0` in every format. These cannot both hold. Unresolved (spec Open Question 2). Tester will block.

**Test Scenarios**:
1. Deprecate an entry with zero agent edges (machine edges may be present and must remain); assert the resolved contract — EITHER advisory omitted in all three formats (AC-05 reading) OR literal `0` rendered (ADR-004 reading). One must be chosen before test authoring.
2. Whichever is chosen, assert the machine edges remain and deprecation success is unchanged.

**Coverage Requirement**: Human/architect resolves Open Question 2 (omit-at-zero vs render-`0`) before Stage 3a. The zero-case test asserts the resolved behavior across all three formats; NFR-04 backward-compat holds under that resolution.

### R-05: Per-format count drop
**Severity**: Med · **Likelihood**: Med · **Impact**: A format silently drops the threaded `edges_removed` and passes a naive test (#5427).

**Test Scenarios**:
1. Per-format matrix for `Some(n)`, n>0: Summary asserts the rendered count value; Markdown asserts the `**Edges removed:** {n}` line value; Json **parses the structured field** and compares the integer (not a substring).
2. Assert `None` omits the advisory in all three formats (quarantine/restore call sites pass `None` — assert their output is unchanged).
3. Cross-call-site: assert `format_quarantine_success` / `format_restore_success` output is byte-identical before and after the signature change (backward compat of the shared formatter).

**Coverage Requirement**: Behavioral, parse-based per-format assertions for `Some(n)`, `None`, and the resolved zero case — never a call-count or bare string-presence check.

### R-06: Unguarded helper — safety is call-site-only
**Severity**: High (if triggered) · **Likelihood**: Low · **Impact**: `delete_agent_edges_for_entry` deletes live agent edges for ANY id handed to it; it has no status/successor guard by design (predicate LOCKED). A future caller on an Active or successor-bearing entry destroys live, repointable relationships.

**Test Scenarios**:
1. Grep/callgraph assertion (or a documented single-caller invariant test): the helper has exactly one production caller — the `context_deprecate` handler at step 6.5.
2. The break-condition guard from R-01 scenario 2 doubles as the misuse guard.
3. Leave a code-adjacency comment (per architecture's recommendation) linking the helper to the chokepoint and the eager ⊆ tick invariant, so reuse is a conscious decision.

**Coverage Requirement**: A test or lint asserting single-caller usage, plus the R-01 chokepoint-exclusion assertion. Blast radius of any second caller must be raised as a design change, not merged silently.

### R-07: Concurrent-tick count under-report
**Severity**: Low · **Likelihood**: Low · **Impact**: Count/audit under-report the edges that existed at deprecation; state stays consistent (edges gone either way).

**Test Scenarios**:
1. Interleave a compaction run between flip and eager delete (or reason about write-pool serialization); assert the eager path tolerates zero-row RETURNING (count 0, no audit event on empty, `Some(0)` advisory) without error.

**Coverage Requirement**: Eager path is correct and non-panicking when RETURNING yields fewer rows than seeded (already-swept case). No assertion that the count equals the pre-deprecation degree under concurrency.

### R-08: Double audit-event confusion
**Severity**: Low · **Likelihood**: Med · **Impact**: Two audit records per deprecation; tests asserting "an audit record exists" may match the flip event, not the cleanup event.

**Test Scenarios**:
1. AC-03 audit assertion filters on `operation == "context_deprecate.edge_cleanup"` specifically and asserts `target_ids == [entry]`, count in `detail`, and tuple JSON in `metadata`.
2. Assert the flip audit (`"context_deprecate"`) and cleanup audit are **two distinct** records; the idempotent re-deprecation (AC-07) emits neither cleanup event.

**Coverage Requirement**: All audit assertions key on the distinct `edge_cleanup` operation string; re-deprecation emits no cleanup event.

### R-09: Provenance enumeration drift
**Severity**: Low-Med · **Likelihood**: Low · **Impact**: A new agent/human-authored `EDGE_SOURCE_*` is missed by `source='agent'`; subset-safe (tick sweeps) but under-cleaned eagerly.

**Test Scenarios**:
1. The ADR-003 per-source matrix seeds one edge of every current `source` value and asserts exactly `agent` is eagerly removed, all machine sources remain. A newly-added source surfaces as "not eagerly removed," prompting a conscious decision.

**Coverage Requirement**: Per-source removal matrix over the full current enumeration; documents completeness as enumeration-bound + subset-safe.

### R-10: Self-loop / high-degree edge cases
**Severity**: Low · **Likelihood**: Low · **Impact**: A self-loop edge matches the `OR` once (correct — counted once, not doubled); a high-degree entry inflates the RETURNING set and audit-JSON size (bounded by degree, NFR-03).

**Test Scenarios**:
1. Seed a self-referential agent edge (`source_id = target_id = entry`); deprecate; assert it is removed and counted exactly once.
2. Seed a high-degree entry (many agent edges); assert all removed, count correct, audit metadata carries all tuples.

**Coverage Requirement**: Self-loop counted once; high-degree removal complete with matching audit tuples.

### R-11: Idempotency / ordering regression
**Severity**: High · **Likelihood**: Low · **Impact**: Misplacement of step 6.5 breaks AC-07/AC-09 — re-deprecation re-deletes, or the delete keys on an Active id and matches nothing.

**Test Scenarios**:
1. Re-deprecate an already-Deprecated entry with a freshly-seeded agent edge; assert the second call returns via the step-5 early-return, the fresh edge survives, no cleanup audit event is written (AC-07).
2. Immediately on return from `context_deprecate` (no tick, no sleep), assert the agent edges are already absent (AC-09 synchronous).
3. Assert the flip precedes the delete: the delete matches because the entry is non-Active at 6.5.

**Coverage Requirement**: Idempotent path does no delete/audit; synchronous absence on return; delete runs strictly after the flip and after the step-5 guard.

## Integration Risks

- **Formatter signature ripples to 4 call sites in lockstep (ADR-004):** `format_status_change` gains `edges_removed: Option<u64>`; `format_deprecate_success` forwards it; `format_quarantine_success` / `format_restore_success` pass `None`. A missed call site fails to compile (Rust arity) — good — but a wrong-position argument (`Option` placed after `format`) or a wrong constant (`Some(0)` vs `None` at the non-delete sites) compiles and mis-renders. Assert quarantine/restore output is unchanged.
- **`rows_affected()` vs `tuples.len()`:** spec FR references `rows_affected()`; ADR-002 derives count from `tuples.len()`. For a single `DELETE … RETURNING` these agree, but the implementation must pick one source of truth (tuple length, since the tuples are needed for audit). Assert count == number of returned tuples == number actually removed.
- **Audit metadata JSON serialization:** the tuple array is serialized into `AuditEvent.metadata`; `relation_type` is an agent-authored string. Assert serialization is well-formed JSON and does not fall through to the `"{}"` sentinel on non-empty removals.
- **Write-pool contention:** the eager delete shares `write_pool_server()` with the tick compaction and all other graph writes; it adds one indexed write to the `context_deprecate` critical path (NFR-03). Assert it uses the write pool (not read pool) and is a single statement (no per-edge loop).

## Edge Cases

- Zero agent edges (machine edges present) → count 0, machine edges remain, no cleanup audit event (emitted only on non-empty), advisory per resolved R-04.
- Entry with only machine edges → nothing eagerly removed; all remain for the tick.
- Self-loop agent edge → removed and counted once.
- High-degree entry → all agent edges removed in one statement; audit metadata bounded by degree.
- Two entries sharing one edge, both deprecated in sequence → first removes it, second's RETURNING omits it (already gone); consistent, count attributed to the first.
- Concurrent tick already swept the edges before step 6.5 → zero-row RETURNING, no error (R-07).
- Re-deprecation (idempotent) → no delete, no advisory, no cleanup audit (R-11).

## Security Risks

- **Untrusted input:** the only external input reaching the delete is `entry_id` (agent-supplied via `DeprecateParams`). It is bound as `?1`; `source` is bound as `?2` to the constant `EDGE_SOURCE_AGENT` — **not** user input. No SQL injection surface; no relation-type or user string enters the predicate.
- **Blast radius / weaponized deprecation:** an agent can deprecate a high-degree hub entry to force mass deletion of agent-authored edges touching it, now accelerated to the deprecation event instead of ≤900s later. This is the *intended* semantics of retiring that entry and is bounded to edges touching the single deprecated id — the eager path adds latency reduction, not new reach, over the existing tick. Incremental risk is low; note it as a property, not a new vulnerability.
- **Audit metadata as an attack surface:** removed-tuple JSON (`relation_type` strings) is written to `AuditEvent.metadata`. Ensure it is serialized via the JSON encoder (no string interpolation) so an unusual `relation_type` cannot corrupt the audit record or a downstream metadata consumer.
- **No deserialization / path / file surface** — this feature touches only a parameterized SQL delete and in-memory formatting.

## Failure Modes

- **Eager delete fails (transient DB error):** log once at `warn` with entry id (visible, not `debug` — #3448), set `edges_removed = None` (advisory omitted), return normal deprecation success; the tick sweeps within ≤900s (AC-06). Entry stays Deprecated; state is the pre-feature status quo.
- **Post-commit marshaling failure (R-03):** the one failure mode where "non-fatal" and "consistent" can conflict — edges gone, no audit. Must be designed out (atomic single-statement capture) or explicitly accepted with a `warn` carrying enough to reconstruct.
- **Backstop dependency (SR-05, ADR-001):** the swallowed-failure safety holds ONLY while `run_orphaned_edge_compaction` keeps blanket-deleting non-Active endpoints over all sources. This is a standing coupling invariant — any future compaction change must re-verify it. The ADR-003 subset test is the re-check trigger (it fails if the tick narrows).
- **Audit fire-and-forget drop:** the audit write is detached; if it is dropped, the delete still succeeded and the caller still got its count — degraded observability, not data loss.

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (irreversible delete, no undo; audit tuples for reconstructability) | R-03, R-06 | ADR-002 captures removed tuples via `DELETE … RETURNING` into the audit `metadata`. **Residual (R-03):** a post-commit marshaling error deletes without recording — must be closed by proving single-statement atomic capture. Predicate LOCKED (R-02/R-06). |
| SR-02 (eager ⊆ tick asserted only in prose; drift breaks subset) | R-01, R-02 | ADR-003 makes it an executable test invoking both real functions. **Residual (R-01):** the fixtures are successor-less, so the test does not exercise the actual break case (eager on a successor-bearing entry) — must be closed by an explicit chokepoint-exclusion assertion against the real handler. |
| SR-03 (agent-set = `source='agent'` is a point-in-time enumeration; new sources undercount) | R-09 | ADR-003 per-source matrix seeds every current source and asserts exactly `agent` is eagerly removed; documented as enumeration-bound + subset-safe. A new source surfaces as "not eagerly removed." Covered. |
| SR-04 (response threads `edges_removed` across 3 formats + audit; call-count tests blind) | R-04, R-05 | ADR-004 behavioral per-format matrix (parse Json field, assert rendered values). **Residual (R-04):** AC-05 (omit-at-zero) vs ADR-004 (`Some(0)`→`0`) contradiction (spec Open Question 2) must be resolved before Stage 3a. |
| SR-05 (correctness depends on compaction remaining the backstop) | R-03 non-fatal path, Failure Modes | ADR-001 records it as a standing coupling invariant; #3448 log discipline (warn, not debug). AC-06 asserts the tick sweeps after an injected failure. The ADR-003 subset test is the re-check trigger if the tick ever changes. Covered. |
| SR-06 (ordering/placement: after flip, past step-5 guard; step-7 interaction) | R-11 | Architecture pins step 6.5 (after step-6 flip, past step-5 early-return; step-7 recompute is independent fire-and-forget). AC-07 (idempotent no-delete) + AC-09 (synchronous) verify. Covered. |

**All SR-01…SR-06 have a mapped architecture risk and a verifying scenario. No SR is unaddressed.** Two carry residual risk the tester must close: R-01 (subset test's successor-less blind spot → add the chokepoint-exclusion assertion) and R-04 (AC-05/ADR-004 zero-case contradiction → needs a human/architect decision on Open Question 2 before test authoring). R-03 (post-commit atomicity) is a design-closure item for delivery.

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 1 (R-01) | 3 |
| High | 6 (R-02, R-03, R-04, R-05, R-06, R-11) | 15 |
| Medium | 3 (R-07, R-08, R-09) | 4 |
| Low | 1 (R-10) | 2 |

## Knowledge Stewardship
- Queried: context_search (lesson-learned) for multi-pass same-table cleanup divergence and fire-and-forget backstop discipline — surfaced #4162 (audit ALL cleanup passes, not one), #3448 (fire-and-forget log discipline), corroborating the ADRs' #3910/#5417/#5427 citations. Findings directly on point; no new pattern beyond those.
- Stored: nothing novel — the recurring cross-feature risk (multi-pass same-table filter divergence must be enforced by a behavioral test over both real predicates, not prose) is already captured as pattern #3910 and lesson #5417; the "subset test must exercise the break case, not just the safe case" nuance (R-01) is feature-specific to this successor-less/successor-bearing split and lives in this document, not as a generalized pattern.
