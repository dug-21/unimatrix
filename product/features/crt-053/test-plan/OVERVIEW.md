# crt-053 Test Plan — OVERVIEW

**Feature**: Active-Only PPR Expansion Seeds — one surgical filter on `seed_ids`
(`crates/unimatrix-server/src/services/search.rs` Phase 0, inside `if self.ppr_expander_enabled`).
**GH Issue**: #717
**Inputs**: SPECIFICATION.md (AC-01..AC-05 + anti-AC), ARCHITECTURE.md, ADR-001 (Unimatrix #4917),
RISK-TEST-STRATEGY.md (R-01..R-12), ACCEPTANCE-MAP.md, IMPLEMENTATION-BRIEF.md.
**Binding test discipline** (carried verbatim from the brief / risk strategy):
behavior-based ID-level assertions only; never penalty constants (C-04, crt-013 #703);
NO eval-harness metric gate (P@5/MRR/soft-GT) as acceptance (SR-01, GATE-04, #500 trap);
every absence assertion carries a paired differential control arm (R-04, #4902);
NO test asserting deprecated absence in Flexible (ANTI-AC-01, C-03).

---

## 1. Overall Test Strategy

This is a **single-edit** feature in the most sensitive code in the system. The filter is trivial;
the dominant risks are *testing the wrong thing* (R-01), *over-dropping a legitimate active seed*
(R-02), *scope creep* (R-03), and *vacuous absence passes* (R-04). The strategy is therefore
weighted toward (a) positive RETENTION assertions, (b) paired differential control arms, and
(c) diff-scope / anti-AC gates — not toward proving "deprecated is gone."

| Layer | What it covers here | Surface |
|-------|---------------------|---------|
| Unit | The existing `search.rs` `mod tests` (penalty/utility/snapshot helpers) MUST pass UNCHANGED (AC-03 ranking-path proof). No new unit test is needed for the predicate itself — the filter is observable only through the full pipeline. | `cargo test -p unimatrix-server --lib` |
| Integration (Rust, full pipeline) | **AC-01, AC-02, AC-04, AC-05** + edge cases. Live `SearchService::search` via `TestHarness`, real store + embeddings + typed graph, positive edges authored explicitly, expander flag toggled per arm. | `crates/unimatrix-server/tests/pipeline_e2e.rs` |
| Integration (Python MCP harness, infra-001) | Regression baseline only — protocol + tools + lifecycle + edge_cases smoke. The expander is default-OFF over MCP and the suite cannot author positive deprecated→neighbor edges nor toggle the flag per request, so it does NOT host AC-01/AC-05 (see OQ-1). | `product/test/infra-001` |
| Diff-scope / anti-AC review gates | GATE-01..GATE-05 (grep + manual diff review). Not feature ACs; enforced at Gate 3c. | review + `Grep` |

**Why the acceptance surface is Rust integration, not the nan-018 corpus or the Python suite —
see §4 (OQ-1 resolution). This is load-bearing.**

---

## 2. Risk → Test Mapping (R-01..R-12)

| Risk | Pri | Covered by | Assertion form |
|------|-----|-----------|----------------|
| **R-01** metric-gate trap | Crit | GATE-04 + all ACs | Inspect crt-053 test files: ZERO P@5/MRR/soft-GT pass/fail gates. Every AC asserts entry-ID presence/absence/rank. (NFR-05.) |
| **R-02** over-drops a legit active seed | Crit | **AC-04**, **AC-01 positive arm**, mixed-pool edge case | POSITIVE retention: 6b terminal-active head anchors the walk and its neighbor IS injected; active-seed neighbor IS present. Prove RETAIN, not only drop. |
| **R-03** scope creep into the 5 locked exclusions | Crit | **GATE-01, GATE-02** | Diff touches only the `seed_ids` build in `search.rs`. No `find_terminal_active` on injected entries, no `penalty_map` mutation, no new flag, no edge-write change. Existing `graph_expand` write-only negative tests UNCHANGED (#4495 trip-wire). |
| **R-04** vacuous absence pass | Crit | **AC-01 control arm**, **AC-05 control arm**, fixture-precondition assert | Each absence arm is paired with a control arm: with the filter removed (expander-on, no active-only narrowing) OR the deprecated seed forced Active, the deprecated-only neighbor REAPPEARS. Plus an explicit assert that the deprecated seed's positive out-edge exists and the neighbor is reachable by no active path. |
| **R-05** off-path identity drift | High | **AC-02** | With `ppr_expander_enabled = false`, results (entries, order, scores) identical to baseline; lexical-scope confirmed by review. |
| **R-06** anti-AC violation | High | **AC-03**, ANTI-AC-01 grep gate | Positive presence-of-deprecated-in-Flexible assertion holds; grep confirms NO deprecated-absence-in-Flexible assertion added. |
| **R-07** reverse-walk direction mis-framing | High | AC-01/AC-05 fixture edge directions | Fixtures state edge direction concretely (deprecated A→X, active B→Y); verify by neighbor-ID outcome, NEVER by inspecting a `Direction::` enum (SR-06, #4077/#3744). |
| **R-08** fixture lacks positive-edge revision | High | **OQ-1 resolution (§4)** | Resolved: route AC-01/AC-05 to the Rust integration harness where positive edges are authored via `TestHarness::insert_graph_edge` + `rebuild_typed_graph`. Not silently skipped. |
| **R-09** `results_with_scores` not sole seed source | High | OQ-2 review + AC-04 (6b-injection produced) | Review confirms `:915` is the only seed collection inside the enabled branch; AC-04 exercises a query producing 6b injections and asserts they reach the seed set (no unfiltered bypass). |
| **R-10** #406 reproduces and gets "fixed" | Med | Disposition note in AC-05 fixture | If the superseded-by-active fixture reproduces #406, RAISE as fixture-divergence vs ass-073's eval graph; do NOT patch retrieval. Documented; not a test target. |
| **R-11** quarantine gate `:950` conflated/edited | Med | **GATE-03** + Quarantined-seed edge case | Diff review: `:950` unchanged. Edge case: a Quarantined entry is dropped from seeds (defense-in-depth) AND still excluded by enforcement — predicate is not a replacement for `:950`. |
| **R-12** string compare instead of typed enum | Low | **GATE-05** + Proposed/Quarantined edge case | Source review confirms `e.status == Status::Active`. Edge case proves a non-Deprecated non-Active status (Proposed/Quarantined) is excluded → predicate is `== Active`, not `!= Deprecated`. |

**Coverage tally**: 4 Critical risks → all 5 ACs + both differential control arms + diff-scope gate.
5 High → off-path equivalence, anti-AC presence check, direction-explicit fixtures, OQ-1 route,
seed-source completeness. 2 Med + 1 Low → #406 raise-not-patch, `:950` intact, `== Active` proof.

---

## 3. Acceptance Criteria → Test Surface

| AC | Surface | Test (planned name) | Control arm? |
|----|---------|---------------------|--------------|
| AC-01 | Rust integration (`pipeline_e2e.rs`) | `test_seed_filter_excludes_deprecated_only_neighbor` (+ `_control` arm) | **Yes (R-04)** |
| AC-02 | Rust integration | `test_off_path_identical_to_baseline` (expander OFF parity) | n/a |
| AC-03 | existing `search.rs` unit suite + `pipeline_e2e.rs` | existing penalty/ranking tests pass UNCHANGED; `test_active_above_deprecated` (existing) holds; deprecated-present-in-Flexible positive assert | n/a |
| AC-04 | Rust integration | `test_seed_filter_retains_terminal_active_head` | n/a (positive retention) |
| AC-05 | Rust integration | `test_supersession_false_positive_guard` (+ `_control` arm) | **Yes (R-04)** |

Per-test detail, fixture construction, edge directions, and both control arms are in
`search-seed-filter.md`.

---

## 4. OQ-1 Resolution — Chosen Acceptance Surface (BINDING)

**Question**: Does the nan-018 fixture corpus contain a deprecated entry with a positive out-edge
to a non-active-reachable neighbor (the ass-073 positive-edge revision)?

**Answer: NO.** Verified by inspection of the corpus and loader:

1. The corpus TOML fixtures (`crates/unimatrix-server/src/eval/corpus/fixtures/*.toml`) author
   relationships **only** via `superseded_by`. There is no positive-edge (`RelatedTo` / `Supports`
   / `Informs` / `Prerequisite` / `CoAccess`) authoring construct in the fixture schema.
2. `superseded_by` resolves to the **`Supersedes`** relation. `graph_expand`
   (`crates/unimatrix-engine/src/graph_expand.rs:50,68,128`) **excludes `Supersedes`** (and
   `Contradicts`) from the positive BFS set. A `Supersedes`-only graph yields zero traversable
   seed→neighbor edges, so no seed (active OR deprecated) injects any neighbor through it.
3. The corpus loader builds its graph with `build_typed_relation_graph(rows, &[])`
   (`loader.rs:408`) — the positive-edge slice is **empty** — and explicitly notes
   "no `graph_edges` rows are needed" (`loader.rs:474-475`). The corpus has no path to author the
   deprecated-A→X positive edge AC-01/AC-05 require.

**Therefore the nan-018 corpus cannot host AC-01/AC-05 without a fixture-schema extension.**

**The Python MCP suite (infra-001) is also rejected as the AC-01/AC-05 host** because: the expander
is default-OFF over MCP (`default_ppr_expander_enabled() = false`, `config.rs:1102-1104`) with no
per-request toggle; the MCP store-then-relate path cannot deterministically author a positive edge
from a *deprecated* entry to a neighbor reachable by no active path; and the differential control
arm needs to flip the filter / force the seed Active per arm, which the wire interface does not expose.

**Chosen surface: the Rust full-pipeline integration harness
`crates/unimatrix-server/tests/pipeline_e2e.rs`.** It is the only surface that simultaneously:
exercises **live `SearchService::search`** (the real edit site, not a mock); authors an explicit
positive edge from a deprecated seed via `TestHarness::insert_graph_edge(src, dst, "RelatedTo")`
followed by `TestHarness::rebuild_typed_graph()`; and supports the differential control arm by
running the same fixture with the expander's active-only narrowing absent (control) vs present
(real). This honors C-04 (ID-level behavior, no constants), NFR-05 (no eval-harness gate), and
R-04 (paired control arm).

**Required test-infrastructure extension (cumulative, not isolated scaffolding):**
`TestHarness::new()` constructs `ServiceLayer` with `InferenceConfig::default()`, whose
`ppr_expander_enabled = false`. AC-01/AC-04/AC-05 need the expander **ON**. Stage 3b/3c MUST add a
constructor variant — e.g. `TestHarness::new_with_expander(path, enabled: bool)` (or an
`InferenceConfig` parameter on the existing `new`) — that threads a non-default `InferenceConfig`
into `ServiceLayer::with_rate_config`. This is a **test-support** change only (`test_support.rs`),
NOT a production edit, and does NOT count against C-01 (the C-01 single-file boundary governs
production code; test infrastructure is cumulative per CLAUDE.md). The differential control arm for
AC-01/AC-05 is then expressed by constructing two harnesses over the identical fixture: the real arm
with the active-only filter present, the control arm with the deprecated seed forced `Active` (so the
filter cannot exclude it) — asserting the deprecated-only neighbor is absent in the real arm and
REAPPEARS in the control arm.

> If Stage 3b prefers to express the control arm without an expander toggle, the equivalent and
> acceptable form is: real arm = deprecated seed status `Deprecated`; control arm = same entry forced
> `Status::Active`; both with the expander ON. Either toggling the filter or forcing the seed active
> satisfies R-04 — the brief permits both. State which form was used in the RISK-COVERAGE-REPORT.

---

## 5. Integration Harness Plan (infra-001)

### Suites that apply (suite-selection table — feature touches server tool logic + store/retrieval)

| Suite | Run? | Rationale |
|-------|------|-----------|
| `smoke` | **YES (mandatory gate)** | Minimum Gate 3c requirement. |
| `protocol` | YES | Server search tool path is in scope. |
| `tools` | YES | `context_search` is the user-facing surface of the edited pipeline. |
| `lifecycle` | YES | Store→search + correction-chain flows exercise deprecated/superseded entries through search. |
| `edge_cases` | YES | Empty-DB / boundary search, concurrent ops — guards the empty-seed boundary. |
| `confidence` | optional | Re-ranking unaffected; run if time permits as regression. |
| `contradiction` | NO | Not touched (Contradicts edges already excluded by graph_expand; no change). |
| `security` | spot | Quarantine-from-search smoke covers R-11 at the MCP layer; full suite not required. |
| `volume` | NO | No schema/storage change. |

### Gap analysis — what no existing suite validates

The active-only seed filter's behavior is observable **only** when the PPR expander is ON and a
deprecated entry has a *positive, traversable* out-edge — a configuration the infra-001 suites do not
and cannot construct (expander default-OFF over MCP; no positive-deprecated-edge authoring; no
per-request flag toggle). **This gap is closed by the Rust integration tests above, not by new
Python tests.** Per USAGE-PROTOCOL "Adding New Tests": this is the correct call — behavior only
visible through a configuration the MCP harness cannot express belongs in the Rust pipeline tests,
and a harness infrastructure change to add expander-on MCP fixtures would be a separate infra issue,
not bundled here.

### New integration tests to add (Python)

**None.** All net-new behavioral coverage lands in `pipeline_e2e.rs` (Rust). The infra-001 run is a
**regression baseline**: confirm the filter introduces no observable change at the MCP layer (expander
is OFF there, so by C-02/AC-02 the suites must be unchanged-green). Any infra-001 failure is triaged
per USAGE-PROTOCOL: feature-caused → fix; pre-existing → GH Issue + `xfail`; bad assertion → fix test.
Given the expander is OFF at the MCP layer, a crt-053-caused infra-001 regression would itself be an
AC-02 (C-02) violation and a hard gate failure.

---

## 6. Cross-Component Test Dependencies

Single production component (`SearchService::search` seed filter). Dependencies the tests rely on,
all pre-existing and unchanged:

- `unimatrix-store::schema::Status` — the typed predicate comparator (`== Status::Active`).
- `unimatrix-engine::graph::{build_typed_relation_graph, GraphEdgeRow}` + `RelationType::RelatedTo`
  — positive-edge authoring for fixtures.
- `unimatrix-engine::graph_expand::graph_expand` — forward BFS consumer; **its existing unit tests
  must pass UNCHANGED** (GATE-02 / #4495 trip-wire; doubles as signature-stability check, R-09).
- `TestHarness` (`test_support.rs`): `insert_graph_edge`, `rebuild_typed_graph`, `search` — plus the
  one new expander-enabling constructor variant (§4).

---

## 7. Edge Cases (from RISK-TEST-STRATEGY) to cover in `pipeline_e2e.rs`

| Edge case | Expected | Risk |
|-----------|----------|------|
| All seeds deprecated → empty `seed_ids` | No panic; BFS over zero seeds; HNSW + 6b results still returned | boundary |
| No deprecated seeds (all active) | Filter is a no-op; injected set identical to unfiltered | parity |
| Superseded-but-still-Active entry (`status == Active`, `superseded_by` set) | RETAINED as a seed (discriminator is status, not `superseded_by`) | common misread |
| Proposed / Quarantined seed | Dropped by `== Active` — proves predicate is `== Active`, not `!= Deprecated` | R-12/FR-02 |
| 6b head whose neighbor is reachable only via the >50-edge redirect ceiling | **Assert NOTHING** — knowingly accepted residual (Locked Decision 4/5); NOT a test target | documented to prevent a tester writing a failing test |

---

## 8. Acceptance Is Met When

AC-01..AC-05 pass with R-04's differential control arms; the production diff touches only the
`seed_ids` build in `search.rs` (R-03/GATE-01); the off-path is bit-for-bit identical (R-05/AC-02);
no eval-harness metric gate exists (R-01/GATE-04); the anti-AC is confirmed absent (R-06/ANTI-AC-01);
existing `graph_expand` and penalty/ranking tests pass untouched (R-03/R-07/AC-03); `pytest -m smoke`
passes (infra-001 minimum gate); and `:950` quarantine enforcement is unchanged (R-11/GATE-03).

---

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` + `context_search` + `context_get` —
  found ADR-001 #4917 (the active-only seed predicate + behavior-based-only acceptance mandate),
  #724 (behavior-based ranking tests: assert order not scores → C-04), and the RISK-TEST-STRATEGY's
  cited precedents #4495 (scope-creep gate), #4902 (vacuous-pass → R-04), #4077/#3744
  (direction-semantics → R-07), #4888 (unmeasurability → R-01). Applied directly to the risk map and
  the OQ-1 surface choice.
- Stored: nothing novel at design time — the load-bearing pattern (acceptance surface forced to the
  Rust pipeline harness because the corpus/MCP surfaces cannot author a positive deprecated→neighbor
  edge nor toggle the expander) is feature-specific and is better captured at retrospective if it
  recurs. No new 2+-feature pattern emerged beyond restating existing entries for crt-053.
