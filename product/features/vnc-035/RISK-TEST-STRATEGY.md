# Risk-Based Test Strategy: vnc-035

> `context_correct` outgoing-edge carry-forward (step 8b′ `run_carry_forward_loop`).
> Mode: architecture-risk. Assesses the concrete designed system — ADR-001..005, the
> `query_outgoing_edges` single SQL predicate, the `CarrySummary{found,carried,failed}`
> count contract, `INSERT OR IGNORE` additive-on-triple composition, and `Contradicts`
> bidirectional/disjointness. Historical evidence: lessons #4473, #4526; patterns #4041,
> #4459, #4472, #4435. The dominant risk (R-01) carries direct precedent: vnc-017's mirrored
> warn-and-continue failure-path AC was silently omitted and **FAILed Gate 3b** (#4473).

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| **R-01** | Warn-and-continue per-edge-copy `Err`/`false` path has **no behavioral signal** — feature behaves identically whether or not the failure-path test (AC-07) exists. Highest-probability gate rejection (#4473 precedent). | High | High | **Critical** |
| **R-02** | `edges_carried` miscount — counting attempted writes instead of `true` inserts, or counting a `false` UNIQUE-conflict (re-passed `edges` from 8b) as carried, inflates the ack and breaks idempotency (AC-08/AC-11). | High | Med | **High** |
| **R-03** | Eligibility-predicate drift — the outgoing SQL predicate is a *superset* exclusion (`Supersedes`+`CoAccess`+`Informs`) vs the incoming one (`Supersedes` only). A reader "aligning" them silently carries tick-generated classes; or the exclusion list omits one and an ineligible edge carries. | High | Med | **High** |
| **R-04** | Composition order break — if 8b′ is placed *before* 8b, or if the count keys off 8b's writer, the ack reports re-passed edges as carried; if 8b′ runs *after* 8c, outgoing-carry and incoming-redirect may both touch a `Contradicts` pair. | Med | Med | **High** |
| **R-05** | `Contradicts` double-write / reverse-orphan — carry writes `B→X`+`X→B`; redirect re-homes the old `X→A` to `X→B`. The two reverse writes overlap; if `INSERT OR IGNORE` idempotency is not relied on (or a non-idempotent primitive sneaks in), the pair duplicates or one direction orphans (#4459). | Med | Med | **Medium** |
| **R-06** | Carry/redirect row-set non-disjointness assumption fails — the disjointness guarantee (ADR-005) rests on self-loops `A→A` being impossible. If self-ref rejection regresses or an edge has `source_id == target_id == A`, the same row enters both loops. | Med | Low | **Medium** |
| **R-07** | Tick-window staleness mis-filed as a carry bug — carried edges are visible to depth-1 DB reads immediately but to BFS path-mode/subgraph only after the next tick (#4526). A path-mode test that does not tick/drain first flakes and gets blamed on carry-forward. | Med | Med | **Medium** |
| **R-08** | `validate_and_write_edges` reuse trap — it **discards** the per-edge bool (`edge_write.rs:152`). If the carry loop delegates the whole batch to it (instead of owning its loop per ADR-003), no count is available and `edges_carried` is unrecoverable. | Med | Low | **Medium** |
| **R-09** | Missing `source_id` index (O-1) — `query_outgoing_edges` filters on `source_id`; only `idx_graph_edges_target_id` is confirmed. Inline on the correction path this is a latency, not correctness, risk; degrades for high-out-degree hub entries. | Low | Low | **Low** |
| **R-10** | Shed path targets Deprecated original — a test or doc that issues `context_edge remove` against A (Deprecated post-correct) hits the frozen-source rejection and falsely concludes shed is broken (SR-08). | Low | Med | **Low** |
| **R-11** | `created_at` wrongly preserved from source row — ADR-004 settles that a carried edge is written `created_at = now`, `created_by = agent`, `source = "agent"`, indistinguishable from a fresh declaration (OQ-03 / FR-11). The risk is the **inverse** of preservation: a developer carrying the original's `created_at`/`created_by`, or a test asserting preservation, would silently re-introduce a provenance marker the decision forbids. | Low | Low | **Low** |

---

## Risk-to-Scenario Mapping

### R-01: Warn-and-continue copy-failure path has no signal (Critical)
**Severity**: High · **Likelihood**: High · **Priority**: Critical
**Impact**: A single edge-copy infra failure could silently abort or roll back the correction (the exact failure mode the posture forbids), and no test would catch it because happy-path tests pass regardless. Evidence: lesson **#4473** — vnc-017's identical AC was absent, all other tests green, build clean; Gate 3b caught it only by name-checklist comparison. The implementation logic was *correct* there — the test was simply missing. This is therefore a **test-presence** risk, not a logic risk.

**Test Scenarios**:
1. **`test_carry_forward_continues_on_edge_copy_failure`** (the mandatory AC-07 test — see callout): fault-inject the store edge-write so one carry edge mid-loop returns `Err`/`false`-SQL-error. Assert: (1) `context_correct` returns **success**; (2) new entry Active, original Deprecated; (3) edges copied **before** the failing one persist on B; (4) `CarrySummary.failed` incremented and a `tracing::warn!` fired.
2. `query_outgoing_edges` itself returns `Err` → `run_carry_forward_loop` returns `CarrySummary{0,0,0}`, correction still succeeds (mirrors `run_redirect_loop` returning `None`).
3. Assert the correction transaction is committed *before* 8b′ runs (a carry failure cannot reach the correction commit).

**Coverage Requirement**: The per-edge-copy `Err` test **must exist by name** in the test plan and be verified present at Gate 3b — not inferred from passing happy-path behavior. The implementation must expose a fault-injection seam so the test can drive one edge to fail mid-loop.

### R-02: `edges_carried` miscount (High)
**Severity**: High · **Likelihood**: Med · **Priority**: High
**Impact**: The ack is the *sole* awareness channel (no DB provenance marker, OQ-03). A wrong count misleads agents about what was preserved and breaks the idempotency contract (AC-08). Evidence: pattern **#4041** — `write_graph_edge` collapses `true`=insert, `false`=UNIQUE-conflict, `false`=SQL-error; crt-040 took Gate 3a rework for getting this exact contract wrong.

**Test Scenarios**:
1. **Idempotent re-pass**: pass an `edges` triple in 8b identical to a carried triple → 8b′ insert returns `false` (UNIQUE conflict) → `carried` **not** incremented. Assert `edges_carried` equals the single-carry count, one row in `graph_edges`.
2. **Count keys off `true` only**: seed N eligible edges, none re-passed → `edges_carried == N`. Add one re-passed edge → `edges_carried` stays N (the re-passed one counted by 8b, not 8b′).
3. **Zero-carry omission**: correct an entry with no eligible outgoing edges → `edges_carried` field **absent** from the response envelope (not `0`).
4. **No edge content in ack**: assert the ack carries the integer only, no target ids / relation types.

**Coverage Requirement**: `carried` counts `write_graph_edge` `true` returns exclusively. Both `false` cases (UNIQUE conflict, SQL error) are excluded from `carried`. The 8b-before-8b′ ordering must be the mechanism under test (re-passed edges conflict in 8b′, not double-count).

### R-03: Eligibility-predicate drift (High)
**Severity**: High · **Likelihood**: Med · **Priority**: High
**Impact**: Ineligible (tick-generated) edges carry forward and re-materialize, or the predicate diverges from `query_incoming_edges` and a reader "fixes" the intentional superset difference into false symmetry — silently carrying `CoAccess`/`Informs`. This also undermines R-04 ("no ceiling") safety.

**Test Scenarios**:
1. **Exclusion set unit test** on `query_outgoing_edges`: seed A with `Supports` (eligible) + `Supersedes` + `CoAccess` + `Informs` rows → assert the query returns only `Supports`; all three derived/tick classes excluded.
2. Integration mirror (AC-04): correct A with that mix → carried edges include `Supports`, exclude all three.
3. **Single-source assertion**: a test/grep guard that the exclusion list exists in exactly one SQL clause — no parallel Rust-side filter that could drift.
4. Assert the inline rationale comment documenting the *superset* difference from the incoming predicate is present (prevents future false-symmetry "fix").

**Coverage Requirement**: The predicate is expressed once, at SQL level. The exclusion set `('Supersedes','CoAccess','Informs')` is pinned by test. The superset rationale is documented so the difference cannot be mistaken for drift.

### R-04: Composition / pipeline order break (High)
**Severity**: Med · **Likelihood**: Med · **Priority**: High
**Impact**: Wrong placement of 8b′ corrupts either the count (if before 8b) or `Contradicts` disjointness (if after 8c). The final edge *set* is order-independent (idempotent insert is commutative), so a wrong order produces a **count/`Contradicts` bug with a correct-looking edge set** — hard to catch without a targeted test.

**Test Scenarios**:
1. Assert pipeline order at the handler: 8 → 8b → 8b′ → 8c → 9 → 10 (carry between `params.edges` write and incoming redirect).
2. **Count-under-ordering**: a re-passed edge written by 8b is a UNIQUE conflict in 8b′ → not counted (validates 8b *before* 8b′).
3. `Contradicts` disjointness test (shared with R-05) validates 8b′ *before* 8c.

**Coverage Requirement**: Step order is asserted, and the count and `Contradicts` consequences of that order are each pinned by a test that would fail under reordering.

### R-05: `Contradicts` double-write / reverse-orphan (Medium)
**Severity**: Med · **Likelihood**: Med · **Priority**: Medium
**Impact**: A duplicated or orphaned `Contradicts` direction corrupts contradiction retrieval. Evidence: pattern **#4459** (vnc-017 had to decide source-validation posture for `Contradicts` in the redirect loop).

**Test Scenarios**:
1. **AC-06**: seed A with outgoing `Contradicts → X`; correct A→B; assert the bidirectional pair on B is consistent — **both** `B→X` and `X→B` exist **exactly once**, no duplicate, no orphan.
2. **Carry + redirect convergence**: seed `Contradicts(A,X)` as `A→X` (A-outgoing) and `X→A` (A-incoming); correct A→B. Assert carry re-homes `A→X`→`B→X` (+reverse `X→B`) and redirect re-homes `X→A`→`X→B`; the `X→B` written by both converges via `INSERT OR IGNORE` to one row.
3. **Count-of-logical-edge**: a carried `Contradicts` (two rows written) increments `carried` by **1**, not 2.

**Coverage Requirement**: `Contradicts` carry reuses the shared bidirectional primitive; both directions exist exactly once after carry+redirect; `edges_carried` counts the logical edge once.

### R-06: Disjointness assumption fails on self-loop (Medium)
**Severity**: Med · **Likelihood**: Low · **Priority**: Medium
**Impact**: If a self-referential edge `A→A` exists, it appears in both A's outgoing and incoming sets and is touched by both loops — the one case ADR-005's disjointness proof excludes.

**Test Scenarios**:
1. Regression guard: assert self-referential edge writes (`source_id == target_id`) are rejected at write time (the invariant ADR-005 depends on).
2. Defensive: if a self-loop somehow exists in `graph_edges`, carry/redirect do not double-process or panic.

**Coverage Requirement**: Self-ref rejection invariant is asserted; the disjointness guarantee is documented as resting on it.

### R-07: Tick-window staleness mis-attributed (Medium)
**Severity**: Med · **Likelihood**: Med · **Priority**: Medium
**Impact**: A path-mode/BFS test that asserts graph-path retrieval of a carried edge without ticking flakes and is mis-filed as a carry-forward defect. Evidence: lesson **#4526**; patterns #4517/#4114.

**Test Scenarios**:
1. Carried edges visible to **depth-1 DB read** (`query_*`/`neighbors` depth=1) **immediately** after correction (no tick).
2. Any BFS path-mode/subgraph assertion of a carried edge **forces a tick/drain first**.
3. Comment/annotate the path-mode test that pre-tick invisibility is expected (#4526), not a carry bug.

**Coverage Requirement**: DB-read tests assert immediate visibility; path-mode tests tick/drain before asserting; expected-staleness is documented in-test.

### R-08: `validate_and_write_edges` discards the bool (Medium)
**Severity**: Med · **Likelihood**: Low · **Priority**: Medium
**Impact**: Delegating the carry batch to `validate_and_write_edges` (which drops the per-edge bool, `edge_write.rs:152`) leaves no insert count — `edges_carried` becomes unrecoverable.

**Test Scenarios**:
1. Assert `run_carry_forward_loop` owns its write loop and captures each `write_graph_edge` bool (per ADR-003), delegating only `Contradicts` bidirectional structure — not calling `validate_and_write_edges` as a black box for counting.
2. Count-accuracy test (R-02 #2) doubles as the guard: an exact count is only achievable if the loop captures the bool.

**Coverage Requirement**: The carry loop captures `write_graph_edge` returns directly; the count tests fail if the bool is discarded.

### R-09: Missing `source_id` index (Low)
**Severity**: Low · **Likelihood**: Low · **Priority**: Low
**Impact**: Full-table scan per correction for high-out-degree entries. Latency only — not correctness. Open question O-1.

**Test Scenarios**:
1. Developer confirms whether `idx_graph_edges_source_id` exists; note the finding. Correctness tests pass regardless of the index.

**Coverage Requirement**: Index presence verified and noted; no functional test required.

### R-10: Shed against Deprecated original (Low)
**Severity**: Low · **Likelihood**: Med · **Priority**: Low
**Impact**: A test/doc targeting A (Deprecated) for shed hits the frozen-source rejection and falsely concludes shed is broken (SR-08).

**Test Scenarios**:
1. **AC-05**: `context_edge remove` with `source_id = B.id` (new Active) drops a carried edge → absent afterward.
2. Negative: `context_edge remove` against A.id (Deprecated) is rejected as frozen-source — asserting the shed path correctly targets B.

**Coverage Requirement**: Shed targets the new Active entry; the Deprecated-original rejection is asserted, not mistaken for a bug.

### R-11: `created_at` wrongly preserved (Low)
**Severity**: Low · **Likelihood**: Low · **Priority**: Low
**Impact**: ADR-004 / FR-11 / OQ-03 settle that a carried edge is written with `created_at = now` (the correction timestamp), `created_by = agent`, `source = "agent"` — a carried edge is **indistinguishable from a freshly-declared one**, with no provenance marker. The risk is the inverse of preservation: a developer copying the original row's `created_at`/`created_by` onto B (re-using the source row wholesale), or a test asserting preservation, would silently violate the no-marker decision and let consumers distinguish carried edges by stale timestamp.

**Test Scenarios**:
1. Assert a carried edge's `created_at` equals the correction timestamp (`now`), **not** the original source row's `created_at`; `created_by`/`source` reflect the carrying agent, not the original author. This guards against accidental preservation.

**Coverage Requirement**: A carried edge is byte-indistinguishable from a fresh agent declaration on the new entry — `created_at = now`, `created_by`/`source` = agent. No preservation, no provenance marker (ADR-004 / FR-11 / OQ-03).

---

## Integration Risks

The carry-forward sits at three live boundaries, all covered above:

- **8b ↔ 8b′ composition seam** (R-02, R-04): re-passed `edges` from 8b must conflict (not double-count) in 8b′. The `INSERT OR IGNORE` UNIQUE constraint is the load-bearing mechanism — composition correctness is structural, but the *count* is the fragile part.
- **8b′ ↔ 8c `Contradicts` seam** (R-05, R-06): carry and redirect read disjoint row sets (A-outgoing vs A-incoming); they overlap only on the converging reverse `X→B` write, which relies on idempotency. Disjointness rests on the no-self-loop invariant (R-06).
- **`query_outgoing_edges` ↔ `query_incoming_edges` predicate seam** (R-03): the two predicates legitimately differ (superset exclusion); the drift trap is a reader unifying them.

## Edge Cases

- Entry with **zero** eligible outgoing edges → `edges_carried` omitted (R-02 #3).
- Entry with **only ineligible** edges (`Supersedes`/`CoAccess`/`Informs`) → `found > 0`, `carried == 0`, ack omitted.
- Entry with **> REDIRECT_CEILING (50)** eligible outgoing edges → **all** carry, no truncation, no ceiling warn (AC-09 — the "no ceiling" assertion).
- All eligible edges already present on B because the caller re-passed the full list → `carried == 0`, idempotent, no error (back-compat, NFR-07).
- Multi-target relation: `B→X` carried, `B→Y` passed in `edges` on the same relation → **two** edges coexist (AC-08 changed-target case).
- `Contradicts` carried as one logical edge → two rows, counted once (R-05 #3).

## Security Risks

Untrusted input surface is narrow: `context_correct` accepts `original_id` and an optional `edges` array (both already validated by the vnc-015 pre-correct path and `RelationType::from_str`). Carry-forward introduces **no new external input** — it reads `source_id`-keyed rows already in `graph_edges` (written by prior validated calls) and re-writes them onto B.

- **Injection**: `query_outgoing_edges` uses a parameterized `WHERE source_id = ?1` bind (`source_id as i64`); the predicate is a static `NOT IN` literal list. No string interpolation. No injection surface.
- **Blast radius**: a compromised/corrupt `graph_edges` row carried forward lands on B with `source = "agent"`, `weight = 1.0`, `bootstrap_only = 0` — indistinguishable from a fresh agent edge. Carry does not *amplify* an existing bad edge (one row in → one row on B), and the agent-declared-only filter blocks tick-generated classes from being laundered through correction.
- **Resource exhaustion**: "no ceiling" (AC-09) means out-degree is bounded only by the eligibility filter. The filter excludes the tick-generated high-fan-out classes (`CoAccess`/`Informs`), which is the **sole** thing bounding agent-declared degree — R-03/R-04. A predicate regression that admits `CoAccess`/`Informs` turns "no ceiling" into an unbounded per-correction fan-out (the very risk vnc-017's N=50 ceiling guards on the incoming side). This makes R-03 a security-adjacent risk, not merely a correctness one.

## Failure Modes

| Failure | Designed behavior | Verified by |
|---------|-------------------|-------------|
| `query_outgoing_edges` returns `Err` | `warn!`; return `CarrySummary{0,0,0}`; correction succeeds, not rolled back | R-01 #2 |
| Single per-edge `write_graph_edge` SQL error | `warn!` (internal); `failed++`; loop continues; earlier carries persist; correction succeeds | **R-01 #1 (mandatory)** |
| `write_graph_edge` UNIQUE conflict (re-passed edge) | `false`, no warn, `carried` not incremented; idempotent | R-02 #1 |
| `Contradicts` reverse-direction write fails | accepted partial-write (ADR-003 vnc-015); warn-and-continue | R-05 (posture) |
| Carried edge invisible to BFS pre-tick | expected (#4526); not a failure; DB read is immediate | R-07 |
| Shed attempted against Deprecated original | frozen-source rejection — by design, not a carry failure | R-10 #2 |

The invariant across all: **the correction transaction is committed before 8b′ and is never rolled back by edge work.**

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|-------------------|------------|
| **SR-01** (warn-and-continue Err path has no signal; #4473) | **R-01** | ADR-002 `failed` counter + `CarrySummary` make the path observable; AC-07 mandated as a named test, verified by name at Gate 3b. Highest-priority risk. |
| **SR-02** (rows-affected/count drift; #4041) | **R-02**, **R-08** | ADR-003: `carried` keys off `write_graph_edge` `true` only; carry loop owns its write loop (cannot delegate to bool-discarding `validate_and_write_edges`). |
| **SR-03** (eligibility filter drift) | **R-03** | ADR-002: single SQL predicate `NOT IN ('Supersedes','CoAccess','Informs')`; superset rationale documented inline to block false-symmetry "fixes". |
| **SR-04** (no-ceiling safety rests on filter) | **R-03** (security-adjacent), **R-04** | ADR-002: "no ceiling" invariant valid only while eligibility = agent-declared-only; AC-09 pins all-carry-no-truncation; predicate regression escalates to fan-out (Security Risks). |
| **SR-05** (doc/ack coupling) | **R-02** (ack correctness underpins it) | AC-10 + AC-11 kept one acceptance unit; ack is the sole awareness channel — its count correctness (R-02) is what makes docs non-load-bearing. |
| **SR-06** (`Contradicts` double-touch) | **R-05**, **R-06** | ADR-005: carry (8b′) and redirect (8c) read disjoint row sets (A-outgoing vs A-incoming); convergence via `INSERT OR IGNORE`; disjointness rests on no-self-loop invariant (R-06). |
| **SR-07** (tick-window staleness; #4526) | **R-07** | NFR-04: DB reads immediate, BFS after tick; path-mode tests tick/drain first; expected-not-defect documented in-test. |
| **SR-08** (shed targets Active new entry) | **R-10** | AC-05/FR-12: shed via `context_edge` against B (Active); Deprecated-original frozen-source rejection asserted and documented. |

Every SR-XX maps to at least one R-XX. No scope risk is unaddressed by the architecture.

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|--------------------|
| Critical | 1 (R-01) | 3 — incl. the **mandatory named** `test_carry_forward_continues_on_edge_copy_failure` (AC-07) |
| High | 3 (R-02, R-03, R-04) | 11 |
| Medium | 4 (R-05, R-06, R-07, R-08) | 9 |
| Low | 3 (R-09, R-10, R-11) | 4 |
| **Total** | **11** | **27** |

### ⚠️ Mandatory test — call out by name

**`test_carry_forward_continues_on_edge_copy_failure`** (AC-07 / R-01 / SR-01).
Forces a single per-edge carry write to fail mid-loop and asserts: correction returns success,
new entry Active + original Deprecated, edges copied before the failure persist, and
`failed`-counter + `tracing::warn!` fired. This test produces **no behavioral signal if omitted**
— the feature works identically without it. vnc-017's identical AC was absent and **FAILed
Gate 3b** (#4473); there the implementation was correct, the test was simply missing. The Gate
3b validator MUST verify this test is **present by name**, not inferred from passing happy-path
behavior. The implementation MUST expose a fault-injection seam so a single mid-loop edge write
can be driven to `Err`.

## Knowledge Stewardship
- Queried: `context_search` for warn-and-continue failure-path lessons and rows-affected/count patterns — found #4473 (vnc-017 Gate 3b FAIL, directly on-point for R-01), #4041 (write_graph_edge three-case bool, root of R-02), #4459 (Contradicts source-validation, R-05), #4472/#4435 (vnc-017/vnc-015 edge patterns), #4526 (tick staleness, R-07). All applied as severity/likelihood evidence.
- Stored: nothing novel — the risk pattern "warn-and-continue side-effect failure tests are silently omittable; verify by name at the gate" is already captured as lesson #4473 and is feature-spanning (vnc-017 → vnc-035). Re-storing would duplicate. The vnc-035-specific risks live in this document, not in Unimatrix.
