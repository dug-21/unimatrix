# vnc-035 Implementation Brief — `context_correct` Outgoing-Edge Carry-Forward

> `context_correct(A → B)` currently redirects A's **incoming** edges to B (vnc-017) but
> **silently drops** A's **outgoing** edges unless the caller re-declares each in `edges`.
> vnc-035 makes `context_correct` carry the original entry's eligible outgoing graph edges
> forward to the new corrected entry **by default**. All design decisions are settled.

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/vnc-035/SCOPE.md |
| Scope Risk Assessment | product/features/vnc-035/SCOPE-RISK-ASSESSMENT.md |
| Specification | product/features/vnc-035/specification/SPECIFICATION.md |
| Architecture | product/features/vnc-035/architecture/ARCHITECTURE.md |
| ADR-001 — Carry-forward step placement | product/features/vnc-035/architecture/ADR-001-carry-forward-step-placement.md |
| ADR-002 — Outgoing query, eligibility, posture | product/features/vnc-035/architecture/ADR-002-outgoing-query-eligibility-and-posture.md |
| ADR-003 — `edges_carried` count contract | product/features/vnc-035/architecture/ADR-003-edges-carried-count-contract.md |
| ADR-004 — Additive-on-triple upsert composition | product/features/vnc-035/architecture/ADR-004-additive-on-triple-upsert-composition.md |
| ADR-005 — `Contradicts` bidirectional carry | product/features/vnc-035/architecture/ADR-005-contradicts-bidirectional-carry.md |
| Risk Strategy | product/features/vnc-035/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/vnc-035/ALIGNMENT-REPORT.md |

## Component Map

This is a **single cohesive delivery wave**: one change to `context_correct` plus a new
`unimatrix-store` query plus doc updates. Pseudocode and test-plan files are produced in
Session 2 Stage 3a; paths below are expected, filled during delivery.

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| `query_outgoing_edges` + `OutgoingEdgeRow` (store query) | pseudocode/query_outgoing_edges.md | test-plan/query_outgoing_edges.md |
| `run_carry_forward_loop` + `CarrySummary` (carry orchestrator) | pseudocode/run_carry_forward_loop.md | test-plan/run_carry_forward_loop.md |
| `context_correct` handler (step 8b′ insertion + `edges_carried` ack) | pseudocode/context_correct_handler.md | test-plan/context_correct_handler.md |
| `uni-zero` SKILL + agent docs (cleanup) | pseudocode/docs_cleanup.md | test-plan/docs_cleanup.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

## Goal

Make `context_correct` copy the original entry's eligible **outgoing** graph edges onto the
new corrected entry **by default** — no `edges` param required — closing the silent-drop
asymmetry against the already-existing incoming redirect (vnc-017). The carry composes
additively with any passed `edges`, runs under a warn-and-continue posture that never rolls
back the correction, and reports an `edges_carried` count in the response so agents are aware.

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|------------|--------|----------|
| Carry-forward step placement & composition order | New step **8b′** `run_carry_forward_loop(store, A, B)` between 8b (`params.edges` write) and 8c (incoming redirect) | SCOPE OQ-01, AC-08 | architecture/ADR-001-carry-forward-step-placement.md |
| New `query_outgoing_edges` + single-source eligibility predicate | New `unimatrix-store` query; **agent-declared only**; exclusion `NOT IN ('Supersedes','CoAccess','Informs')` expressed once at SQL level (superset of incoming predicate) | SCOPE OQ-02, AC-04, AC-09; SR-03 | architecture/ADR-002-outgoing-query-eligibility-and-posture.md |
| Warn-and-continue posture; never roll back; observable failure path | Carry never aborts the committed correction; per-edge SQL error → `failed++` + `warn!`; `query_outgoing_edges` Err → `CarrySummary{0,0,0}` | SCOPE Goal 3; SR-01; lesson #4473 | architecture/ADR-002-outgoing-query-eligibility-and-posture.md |
| `edges_carried` count contract | Carry loop **owns its write loop**; counts `write_graph_edge` `true` (actual inserts) only; UNIQUE-conflict `false` and SQL-error `false` excluded; omitted from ack when zero | SCOPE OQ-03, AC-11; SR-02; pattern #4041 | architecture/ADR-003-edges-carried-count-contract.md |
| Additive-on-triple upsert composition | Realized entirely by `graph_edges` `UNIQUE(source_id, target_id, relation_type)` + `INSERT OR IGNORE`; no diff/merge logic; back-compat automatic | SCOPE OQ-01, OQ-04, AC-08 | architecture/ADR-004-additive-on-triple-upsert-composition.md |
| Carried-edge metadata / provenance | `created_at = now` (correction timestamp), `created_by`/`source = "agent"`, `weight = 1.0`, `bootstrap_only = 0`, `metadata = ""`; **no preservation, no provenance marker**; indistinguishable from a fresh agent edge | SCOPE OQ-03; FR-11 | architecture/ADR-004-additive-on-triple-upsert-composition.md |
| `Contradicts` bidirectional carry + disjointness | Reuse `validate_and_write_edges` bidirectional structure; carry (8b′, A-outgoing) and redirect (8c, A-incoming) read disjoint row sets; one logical `Contradicts` = two rows, counted once | SCOPE AC-06; SR-06; pattern #4459 | architecture/ADR-005-contradicts-bidirectional-carry.md |

## Files to Create / Modify

| File | Change | Summary |
|------|--------|---------|
| `crates/unimatrix-store/src/read.rs` (or new `read_outgoing.rs` if 500-line rule breached — developer decides, O-2) | **New** | `query_outgoing_edges(source_id)` + `OutgoingEdgeRow` DTO; SQL eligibility predicate (single source of truth) |
| `crates/unimatrix-server/src/mcp/tools.rs` | **Modify** | Add `run_carry_forward_loop` + `CarrySummary` (sibling of `run_redirect_loop`); insert step 8b′ into `context_correct` (~:1015); thread `edges_carried` into the ack (~:1162) |
| `.claude/skills/uni-zero/` (SKILL goal-curation guidance) + any agent docs carrying the "re-declare edges on correction" warning | **Modify** | Remove manual re-declaration guidance; document carry-forward default + `context_edge remove/redirect` against the **new** entry id as shed path; note Deprecated original is frozen |
| `crates/unimatrix-server/tests/` + store tests (extend vnc-015/vnc-017 correction + edge fixtures) | **Modify** | Add carry-forward integration + unit tests, incl. the mandatory named `test_carry_forward_continues_on_edge_copy_failure` |

## Data Structures

```rust
// unimatrix-store — new
pub struct OutgoingEdgeRow {
    pub target_id: u64,
    pub relation_type: String,
    pub created_at: u64,   // read for ordering/observability only; NOT written onto B (ADR-004)
}

// unimatrix-server/src/mcp/tools.rs — new
pub(super) struct CarrySummary {
    found: usize,    // eligible outgoing rows returned by query_outgoing_edges
    carried: usize,  // write_graph_edge `true` returns → the edges_carried ack value
    failed: usize,   // distinguished SQL-error writes → the SR-01 observable signal
}
```

## Function Signatures

```rust
// unimatrix-store — new (mirrors query_incoming_edges at read.rs:1694)
pub async fn query_outgoing_edges(&self, source_id: u64) -> Result<Vec<OutgoingEdgeRow>>;

// SQL — the single source of truth for outgoing eligibility (ADR-002):
//   SELECT target_id, relation_type, created_at
//   FROM graph_edges
//   WHERE source_id = ?1
//     AND relation_type NOT IN ('Supersedes', 'CoAccess', 'Informs')
//   (bind source_id as i64; read_pool(); inline superset-vs-incoming rationale comment)

// unimatrix-server/src/mcp/tools.rs — new (sibling of run_redirect_loop at :4660)
pub(super) async fn run_carry_forward_loop(
    store: &Store,
    original_id: u64,
    new_entry_id: u64,
) -> CarrySummary;
```

Reused primitives (no signature change): `write_graph_edge` (`true`=insert, `false`=UNIQUE
conflict OR SQL error, no `Err` — pattern #4041), `validate_and_write_edges`
(`edge_write.rs:152`, bidirectional `Contradicts` structure), `EDGE_SOURCE_AGENT = "agent"`
(`edge_write.rs:28`), `run_redirect_loop` (`tools.rs:4660`, unchanged).

### Pipeline order (load-bearing — ADR-001)

```
 8.  store_ops.correct()            → commits (B created Active, A deprecated)
 8b. validate_and_write_edges(B, params.edges, now)   [EXISTING vnc-015]
 8b′ run_carry_forward_loop(store, A, B)   ◄── NEW
 8c. run_redirect_loop(store, A, B)        [EXISTING vnc-017, unchanged]
 9.  confidence.recompute
 10. format response + edges_carried ack (omit when 0)
```

## Constraints

- **Code location:** handler is `crates/unimatrix-server/src/mcp/tools.rs` — issue #730
  mis-states `unimatrix-engine`. Store queries live in `crates/unimatrix-store`.
- **Posture parity / no rollback (NFR-01):** edge-copy work runs **after** the correction
  commits and must never roll it back. Correction success depends only on `correct_entry`.
- **Eligibility predicate single-definition (NFR-02 / SR-03):** one SQL clause; outgoing is a
  **superset** exclusion vs. incoming (`Supersedes` only) — document inline so a reader does
  not "fix" it into false symmetry.
- **Count semantics (NFR-03 / SR-02 / pattern #4041):** `edges_carried` counts **actual
  inserts** (`true`); UNIQUE-conflict `false` is not counted and is not a failure. Carry loop
  **owns its write loop** — cannot delegate to `validate_and_write_edges`, which discards the
  bool (R-08).
- **`Contradicts` = one logical edge, two rows (ADR-005):** forward counted, reverse not;
  carry (8b′, A-outgoing) and redirect (8c, A-incoming) act on disjoint row sets — no source
  is invalid on the carry side (B is freshly Active), so the #4459 source-validation guard is
  not needed in the carry loop.
- **Shed targets the new entry (SR-08):** `context_edge remove/redirect` requires an Active
  source; only B qualifies. Docs must state the Deprecated original cannot be edited.
- **Graph-rebuild staleness (NFR-04 / lesson #4526):** carried edges are visible to DB-backed
  reads immediately, to BFS path-mode after the next tick — expected, not introduced here.
  Path-mode tests must tick/drain first.
- **Workspace rules (NFR-05):** no `unsafe`; no `.unwrap()`/`.expect()` in non-test code;
  ≤500 lines/file (new store query may warrant its own module — O-2, developer decides).
- **Cumulative test infra (NFR-06):** extend vnc-015/vnc-017 correction + edge fixtures; do
  not scaffold isolated harnesses.
- **AC-07 fault-injection seam:** the implementation MUST expose a seam so a single mid-loop
  edge write can be driven to `Err`/SQL-error (so the mandatory failure test has a signal).

## Dependencies

- **vnc-015** (#595) — `edges` param, `validate_and_write_edges`, edge-validation posture,
  `EDGE_SOURCE_AGENT`. Carry composes with this write path and reuses its constants /
  bidirectional handling.
- **vnc-016** (#603) — `context_edge` add/remove/redirect (the documented shed/opt-out path).
- **vnc-017** (#606) — incoming `run_redirect_loop`, `REDIRECT_CEILING`, `Supersedes`
  exclusion, terminal-active resolution, warn-and-continue. The outgoing path mirrors these.
- **unimatrix-store** — `graph_edges` schema, `query_incoming_edges` (model for the new
  query), `write_graph_edge` rows-affected `bool` contract.
- **unimatrix-engine** `graph.rs` edge-type taxonomy — defines agent-declarable vs. derived /
  tick-generated classes; the eligibility predicate filters against it.
- **uni-zero SKILL + agent docs** — doc updates land **within** this feature (AC-10).
- **Crates:** workspace-internal only (`unimatrix-store`, `unimatrix-server`,
  `unimatrix-engine`). No new external crates; uses existing `sqlx`/`tracing`.
- **Lesson #4526** (graph-rebuild staleness), **lesson #4473** (warn-and-continue failure-path
  AC silently omittable — verify by name at Gate 3b), **pattern #4041** (rows-affected
  three-case bool), **pattern #4459** (`Contradicts` source-validation). No code dependency.

## NOT in Scope

- Reopening the persist-by-default decision (settled 2026-06-10).
- Carry-forward of derived/auto-generated edge classes — `Supersedes`, tick-generated
  `CoAccess`/`Informs` are never copied.
- **Retroactive backfill / migration** of already-orphaned historical corrections. Tracked by
  **follow-up issue #745** (corpus repair sweep, OQ-05). The two known goal entries were
  restored manually 2026-06-10.
- Changes to `context_store` edge semantics (no "original" to carry from).
- Changes to the incoming-redirect path (vnc-017) beyond reusing its conventions.
- TypedRelationGraph eager rebuild — carried edges follow standard tick-window visibility.
- A new edge ceiling tuning knob exposed to agents.
- **DB provenance marker** distinguishing carried vs. freshly-declared edges (accepted
  one-way door — awareness via `edges_carried` ack only).
- Per-edge agent attribution beyond `source = "agent"`.

## Alignment Status

ALIGNMENT-REPORT.md (2026-06-12): **PASS 5, WARN 1, VARIANCE 0, FAIL 0**.

- Vision Alignment — PASS (restores typed-graph integrity, principle 4; advances
  `goal:self-learning`; AC-03 closes the confirmed-live `Advances → vision_root` regression).
- Milestone Fit — PASS (Vinculum bug fix; "no ceiling" + no tuning knob resist gold-plating).
- Scope Gaps / Additions — PASS (all 6 Goals + AC-01..AC-11 carried; ADRs are implementation
  detail, not scope expansion).
- Risk Completeness — PASS (SR-01..SR-08 → R-01..R-11; AC-07 mandated by name across docs).
- **WARN — CLOSED.** The lone WARN (cross-document `created_at` contradiction) has been
  reconciled: SPEC FR-11, ARCHITECTURE/ADR-004, and RISK R-11 now all agree — carried edges
  are written `created_at = now` (correction timestamp), `created_by`/`source = "agent"`, NOT
  preserved, no provenance marker. Treat as resolved; no open variance remains.

## Mandatory Test Callout (AC-07 / R-01 / SR-01 — lesson #4473)

**`test_carry_forward_continues_on_edge_copy_failure`** is MANDATORY and **easy to omit** —
warn-and-continue produces **no behavioral signal** if the test is absent. vnc-017's identical
AC was silently omitted and **FAILed Gate 3b** (#4473); there the implementation was correct,
the test was simply missing.

The test forces one per-edge carry write to fail mid-loop and asserts: (1) `context_correct`
returns **success**; (2) new entry Active + original Deprecated; (3) edges copied **before**
the failing one persist on B; (4) `CarrySummary.failed` incremented + a `tracing::warn!` fired.

- The test plan MUST list it **by name**.
- Gate 3b MUST verify it is **present by name** — not inferred from passing happy-path tests.
- The implementation MUST expose a fault-injection seam driving one mid-loop edge to `Err`.
