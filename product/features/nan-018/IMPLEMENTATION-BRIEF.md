# nan-018 — Implementation Brief: Eval Harness Strategic Upgrade

**Feature**: nan-018 (GH #716) — the *instrument*. Tunability + trust/cost metrics + durable fixture corpus + drift guard.
**Status**: Implementation-ready (Session 1 design complete; all five OQs RESOLVED; locked decisions ratified; 0 variances, 1 WARN — CORRECTED, see below).
**Authority**: ADR-004 (eval-in-server), ASS-037 (#3984 formula authority — not re-tuned), crt-014 ADR-006 (penalty consts being exposed); nan-018 ADR-006 (#4894 — penalty config is eval-only, deployment defaults stay fixed).

This brief compiles the approved design into a delivery-ready package. The Integration Surface (architecture §6) is load-bearing: delivery agents MUST use the exact names/types there and not invent them.

---

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/nan-018/SCOPE.md |
| Scope Risk Assessment | product/features/nan-018/SCOPE-RISK-ASSESSMENT.md |
| Specification | product/features/nan-018/specification/SPECIFICATION.md |
| Architecture | product/features/nan-018/architecture/ARCHITECTURE.md |
| ADR-001 — Penalty config exposure | product/features/nan-018/architecture/ADR-001-penalty-config-exposure.md |
| ADR-002 — Retrieval-shape hash | product/features/nan-018/architecture/ADR-002-retrieval-shape-hash.md |
| ADR-003 — Token-weighted cost metric | product/features/nan-018/architecture/ADR-003-token-weighted-cost-metric.md |
| ADR-004 — Property ground truth + trust class | product/features/nan-018/architecture/ADR-004-property-ground-truth-and-trust-class.md |
| ADR-005 — Two-corpus / Band-3 / waving | product/features/nan-018/architecture/ADR-005-two-corpus-band3-boundary-and-waving.md |
| ADR-006 — Penalty deployment boundary (eval-only) | product/features/nan-018/architecture/ADR-006-penalty-deployment-boundary.md |
| Risk-Test Strategy | product/features/nan-018/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/nan-018/ALIGNMENT-REPORT.md |
| Acceptance Map | product/features/nan-018/ACCEPTANCE-MAP.md |

ADRs are also in Unimatrix: ADR-001 #4897, ADR-002 #4895, ADR-003 #4896, ADR-004 #4898, ADR-005 #4893, ADR-006 #4894. (ADRs 001/002/003/004 were context_corrected from #4889/#4890/#4891/#4892 to carry the locked decisions + clamp-coupling + corpus-depth revisions; old IDs redirect via provenance chain. crt-014 ADR-006 #1606→#4899 carries the partial-supersession note.)

---

## Goal

Upgrade the in-server `unimatrix eval` harness from a positive-relevance-only instrument into one that also measures **trust** (correctness/negative properties), **cost** (token-weighted noise), and **tunability** (sweepable status-penalty levers), validated against a **durable hand-authored fixture corpus** guarded by a **retrieval-shape hash**. nan-018 is the instrument; the downstream measurement spikes (rewritten ass-073, ass-074) and crt-053 consume it. Its own success criterion is **proof-by-use** (AC-14): a single correlated steepness sweep reporting trust + P@5/MRR + cost on the fixture corpus.

---

## Delivery Waves (read this first — it drives all routing)

This is a **wide** feature delivered in **two deliberate waves**. Wave-1 is the load-bearing spine that unlocks the downstream sweep; Wave-2 is deferrable and must have **zero code coupling** to Wave-1 (NFR-04, SR-05).

| Wave | Scope | ACs | Exit |
|------|-------|-----|------|
| **Wave 1 — instrument core** | Penalty config exposure, trust metric class, token-weighted cost, fixture/primary corpus + property assertions, two-corpus plumbing, drift guard | **AC-01…AC-09** | **AC-14 proof-by-use** correlated sweep (the Wave-1 exit gate — see below) |
| **Wave 2 — docs + forward-discipline (deferrable)** | Band-1/2 docs, Unimatrix `convention`/`procedure`, Band-3 protocol **recommendation** doc | AC-10…AC-13 | Does NOT gate the downstream sweep; may land later |

**Wave independence is a hard requirement (NFR-04, R-14):** Wave-1 acceptance tests must pass with **zero Wave-2 artifacts present** (no Band-2 docs, no recommendation doc, no convention/procedure entries). A Wave-2 artifact that becomes a Wave-1 code dependency is a defect.

**AC-14 Wave-1 exit gate is non-vacuous (R-15, the gating scenario).** A sweep that merely *executes* does NOT satisfy it. The exit requires, on the delivered fixture corpus:
1. One `eval run` of a steepness sweep across ≥2 profiles reports — for the same scenarios, in one correlated section — trust outcomes (AC-02/03) AND P@5/MRR (AC-04) AND token-weighted cost (AC-09), with **at least one trust assertion non-vacuously evaluated against a non-empty result set**;
2. Each of the four required shapes (multi-correction chain, dangling chain, superseded-Active, deprecated-connected) loads and yields ≥1 evaluated assertion;
3. The two swept profiles differ in a penalty lever and the report shows a **non-zero penalty/ranking delta** (lever proven live);
4. The swept **baseline** (default penalties) reproduces current behavior **bit-for-bit** (so the delta is attributable to the lever, not a shifted default);
5. The corpus is guarded by a deterministic, actually-firing drift guard (R-03 + R-06 green).

### Three non-negotiable Wave-1 backstop tests (call out explicitly — these gate Wave-1 exit)

These three are the Wave-1 exit gates that prove the instrument **measures**, not merely **executes**. None may be deferred to Wave 2 or to a downstream spike.

1. **R-09 corpus audit** — a static audit of the shipped primary fixture corpus asserts **zero literal-ID and zero null `expected`**; every scenario uses only property assertions (redirect-to-head / absence / rank-below). Codifies crt-013 #703 "assert outcomes, never constants" and bans the ASS-037/ASS-039 self-consistency trap.
2. **R-04 hash-sensitivity matrix + the column-manifest human review** — a per-input sensitivity test asserts the hash changes iff a *declared* manifest input changes (no sensitivity to undeclared/display-only columns), **PLUS** the named human delivery gate certifying the *declared* set is complete (see "R-04 Named Human Delivery Gate" below). The test proves sensitivity to the declared set; only the human can certify the declared set is complete.
3. **R-15 non-vacuous AC-14** — the correlated sweep must non-vacuously evaluate **≥1 trust assertion against a non-empty result set** on a **bit-for-bit baseline** (R-01) guarded by a **live drift guard** (R-03 + R-06). The sweep must prove the instrument MEASURES a moving trust signal, not merely that the harness runs.

### R-04 Named Human Delivery Gate (explicit delivery obligation — §7.3 LOCKED)

The retrieval-shape-hash **column-manifest completeness review is a NAMED human review at delivery**, explicitly **NOT folded into routine code review**. A test (AC-08e) can only prove the hash is sensitive to the columns the manifest *declares*; **no test can prove the declared column set is itself complete.** Before nan-018 delivery is accepted, a named human reviewer MUST sign off that the manifest's in-scope `entries` column list covers **every** column the live retrieval/ranking path reads — confirming no retrieval-relevant column was mis-classified as display-only and silently omitted (the R-04 silent-staleness path). This is a distinct delivery gate, separate from automated tests and routine review.

---

## Component Map

| Component | Wave | Pseudocode | Test Plan |
|-----------|------|-----------|-----------|
| Penalty config (`GraphPenaltyConfig`) | 1 | pseudocode/penalty-config.md | test-plan/penalty-config.md |
| Engine penalty entry point (`graph_penalty_with`, `GraphPenaltyParams`) | 1 | pseudocode/engine-penalty.md | test-plan/engine-penalty.md |
| Search threading (`SearchService` field + `with_rate_config`) | 1 | pseudocode/search-threading.md | test-plan/search-threading.md |
| Trust metric class (`eval/runner/trust.rs`, `ExpectedAssertions`, `TrustOutcome`) | 1 | pseudocode/trust-metric.md | test-plan/trust-metric.md |
| Cost metric (`eval/runner/cost.rs`, `token_proxy`) | 1 | pseudocode/cost-metric.md | test-plan/cost-metric.md |
| Fixture corpus loader + property assertions (`eval/corpus/`) | 1 | pseudocode/corpus-loader.md | test-plan/corpus-loader.md |
| Drift guard / shape hash (`eval/shape/`) | 1 | pseudocode/shape-hash.md | test-plan/shape-hash.md |
| Report extensions (`find_regressions` trust + cost) | 1 | pseudocode/report-extensions.md | test-plan/report-extensions.md |
| Primary fixture corpus assets (`eval/corpus/fixtures/`) | 1 | pseudocode/corpus-fixtures.md | test-plan/corpus-fixtures.md |
| Band-1/2 docs | 2 | pseudocode/docs.md | test-plan/docs.md |
| Band-3 recommendation doc + Unimatrix convention/procedure | 2 | pseudocode/band3-recommendation.md | test-plan/band3-recommendation.md |

Pseudocode and test-plan files are produced in Session 2 Stage 3a. Components above are the expected set from the architecture; actual file paths are confirmed during delivery.

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| OQ-2 — Steepness exposure shape | Per-constant `UnimatrixConfig` fields (7 levers + `max_traversal_depth`) PLUS optional single-`multiplier` overlay; per-field overrides win over multiplier; multiplier scales severities only, never `hop_decay`/`max_traversal_depth` | SCOPE OQ-2, AC-01 | architecture/ADR-001-penalty-config-exposure.md (#4889) |
| SR-08 — Bit-for-bit default reproduction | Engine consts retained as single source of truth; `graph_penalty` becomes thin wrapper over `graph_penalty_with(..&Default::default())`; dual-default discipline (#4064); enumerated-site default-equivalence test | SCOPE AC-01, SR-08 | architecture/ADR-001-penalty-config-exposure.md (#4889) |
| OQ-3 / OQ-4 — Drift-guard granularity & embed dependence | **Branch (b)**: embedding `model_id` + `dimension` are first-class hash inputs → embed-at-load safe, **no frozen vector sidecar**; retrieval-shape hash over an ordered, versioned, enumerated manifest; SHA-256 hex | SCOPE OQ-3/OQ-4, AC-08 | architecture/ADR-002-retrieval-shape-hash.md (#4890) |
| OQ-1 — Cost metric | Explicit **token-weighted** `cost = Σ token_proxy(result)`; k secondary; faithful subword tier (tokenizers crate) default, word×1.3 documented fallback; char/4 rejected; precision+k NOT taken | SCOPE OQ-1, AC-09 | architecture/ADR-003-token-weighted-cost-metric.md (#4891) |
| Trust metric placement & property ground truth | Trust evaluated **in-harness** per profile/scenario; additive `assertions: Option<ExpectedAssertions>` (alias-resolved, never literal IDs); class extensible but Wave-1 ships exactly redirect-to-head / absence / rank-below; loader rejects null + literal-ID `expected` in primary corpus | SCOPE Goal 2/4, C-03/C-04, AC-02…05 | architecture/ADR-004-property-ground-truth-and-trust-class.md (#4892) |
| OQ-5 — Protocol trigger | Recommendation-only; predicate = "your change alters the retrieval-shape hash" (coupled to ADR-002 hash); **no `.claude/protocols/` edits**; two-corpus model documented-not-type-enforced; wave independence | SCOPE OQ-5, AC-07/12/13 | architecture/ADR-005-two-corpus-band3-boundary-and-waving.md (#4893) |
| **LOCKED §7.2** — Hash-mismatch severity | **HARD ERROR (abort, non-zero exit) on the PRIMARY fixture corpus / WARN (continue) on the production snapshot**; message names the diverged shape dimension. Drift guard protects corpus *validity* (a precondition failure), deliberately overriding the eval `report` exit-0 quality-verdict convention for the primary corpus only | ARCHITECTURE §7.2, FR-22, AC-08(b) | architecture/ADR-002-retrieval-shape-hash.md (#4890) |
| **LOCKED §7.1** — Cost-growth gate ε | **ε = 0.0, advisory / report-only in Wave-1**: any growth (delta > 0.0) is reported in the human-reviewed regression block but **blocks nothing**; `eval report` exit code unchanged. Non-zero ε is premature tuning (no downstream cost distribution exists yet) and would breach the eval-is-not-a-gate boundary | ARCHITECTURE §7.1, FR-12a, AC-09 | architecture/ADR-003-token-weighted-cost-metric.md (#4891) |
| **LOCKED §7.4 / ADR-006** — Penalty deployment boundary | Penalty config exposure is **eval/measurement-only**; deployed defaults stay fixed at crt-014 v1 consts; the new `[graph_penalty]` section is **NOT license to re-tune** production. Adopting a swept value as a new default is an ASS-037 (#3984) decision, out of nan-018 scope | ARCHITECTURE §7.4, C-02 | architecture/ADR-006-penalty-deployment-boundary.md (#4894) |

---

## WARN-1 — RESOLVED: penalty consumption sites (read before AC-01 work)

The Alignment Report's WARN-1 (a doc disagreement on penalty-site count) is **resolved by direct code verification**. It matters because penalty-threading completeness is the **precondition for the critical R-01/SR-08 risk** (a missed config site silently shifts default penalty behavior, corrupting every downstream baseline):

- **Resolution (verified against source)**: there are **two** penalty-application sites, **both in `crates/unimatrix-server/src/services/search.rs`** — `search.rs:727` (the fallback branch) and `search.rs:729` (`graph_penalty`). `background.rs:583` is **NOT** a penalty-application site — it is a `tracing::error!` **log string** and must **not** be threaded with config. The earlier "second site in `background.rs`" reading was a mis-identification; both source docs (ARCHITECTURE §3.1, SPECIFICATION FR-03) are now corrected to the two-site `search.rs`-only list.
- **Source of truth for delivery**: the two `search.rs` sites (:727, :729) **AND the R-01 enumerated-site grep guard**. Do **not** thread config into `background.rs`.
- **Mechanical closure**: the default-equivalence test (NFR-01) plus the enumerated-site grep guard (R-01 scenario 2 — every `graph_penalty` reference routes through `graph_penalty_with` / the resolved field) close the residual.

---

## Files to Create / Modify

**Engine (Wave 1):**
- `crates/unimatrix-engine/src/graph.rs` — *modify*: add `GraphPenaltyParams` (Copy, `Default` = consts) + `graph_penalty_with`; `graph_penalty` becomes thin wrapper. Consts retained. **Clamp coupling (load-bearing, ADR-001):** the depth-decay clamp at `graph.rs:531` becomes `raw.clamp(0.10, params.clean_replacement)` — the ceiling tracks the swept `clean_replacement`, NOT the const. Clamping to the const would break the swept-value coupling and the depth-2 ≤ depth-1 monotonicity. Lower bound `0.10` stays a literal.

**Server config (Wave 1):**
- `crates/unimatrix-server/src/infra/config.rs` — *modify*: add `GraphPenaltyConfig` + `UnimatrixConfig.graph_penalty` (`#[serde(default)]`); dual-default `default_*()` fns referencing engine consts; range validation for new fields.

**Search path (Wave 1):**
- `crates/unimatrix-server/src/services/search.rs` — *modify*: `SearchService.graph_penalty_params` field; resolve in `with_rate_config`; **both** penalty sites live here — `search.rs:729` `graph_penalty` call swaps to `graph_penalty_with`, and `search.rs:727` fallback branch uses the resolved `fallback`. These are the ONLY two penalty-application sites; `background.rs` is NOT touched (its `FALLBACK_PENALTY` reference is a log string).

**Eval harness — trust + cost (Wave 1):**
- `crates/unimatrix-server/src/eval/scenarios/types.rs` — *modify*: additive `assertions: Option<ExpectedAssertions>` on `ScenarioRecord`.
- `crates/unimatrix-server/src/eval/runner/trust.rs` — *new*: `evaluate_trust(...) -> TrustOutcome`; assertion-enum evaluator.
- `crates/unimatrix-server/src/eval/runner/cost.rs` — *new*: `token_proxy` + per-profile cost sum.
- `crates/unimatrix-server/src/eval/runner/{replay,metrics}.rs` — *modify*: call trust + cost evaluators; populate new `ProfileResult` fields.
- `crates/unimatrix-server/src/eval/runner/output.rs` — *modify*: `ProfileResult` gains `cost_tokens: f64`, `trust: TrustOutcome`.
- `crates/unimatrix-server/src/eval/report/aggregate/mod.rs` — *modify*: `find_regressions` extended (trust flip, cost growth); exit-code semantics unchanged.

**Eval harness — corpus + shape (Wave 1):**
- `crates/unimatrix-server/src/eval/corpus/` — *new submodule*: `loader.rs`, `assertions.rs`, `fixtures/` (in-repo TOML/JSON fixture entry-graphs + manifest stamp). **Authoring depth (ADR-004 §5 — beyond the AC-14 floor):** author enough variation — especially in the deprecated-but-connected shape — that the steepness crossover (where connected-deprecated crosses the weakest-active threshold) sits inside a *bracketed range* of points, so ass-073's sweep rests on real evidence, not a single exemplar. AC-14 only proves the corpus *measures*; it does not prove it is a good-enough yardstick. The Wave-1 corpus is **NOT frozen**: "ass-073 finds it insufficient → revise + re-stamp" is an anticipated, valid loop — budget one revision pass.
- `crates/unimatrix-server/src/eval/shape/` — *new submodule*: ordered manifest, hash computation, drift-guard compare.

**Embed (Wave 1, read-only):**
- `crates/unimatrix-embed/src/model.rs` — *read-only*: `EmbedModel::model_id()`, `dimension()` feed the shape hash (no edit).

**Docs + Band-3 (Wave 2):**
- `docs/testing/eval-harness.md` — *modify*: new capabilities.
- Band-2 guides (authoring guide, migration runbook, two-corpus model, config-knob reference) — *new* under `docs/testing/`.
- `product/features/nan-018/RECOMMENDATION-band3-protocol.md` — *new*: recommendation-only, **no `.claude/protocols/` edits**.

---

## Data Structures (Integration Surface — use exactly; do not invent)

```rust
// unimatrix-engine/src/graph.rs (NEW)
pub struct GraphPenaltyParams {           // Copy; Default = engine consts
    pub orphan: f64, pub clean_replacement: f64, pub hop_decay: f64,
    pub partial_supersession: f64, pub dead_end: f64, pub fallback: f64,
    pub max_traversal_depth: usize,
}

// infra/config.rs (NEW)  #[serde(default)]
pub struct GraphPenaltyConfig {           // mirrors params + overlay
    pub orphan: f64, pub clean_replacement: f64, pub hop_decay: f64,
    pub partial_supersession: f64, pub dead_end: f64, pub fallback: f64,
    pub max_traversal_depth: usize, pub multiplier: Option<f64>,
}
// UnimatrixConfig gains: #[serde(default)] pub graph_penalty: GraphPenaltyConfig

// eval/scenarios/types.rs (NEW field on ScenarioRecord)
pub type EntryRef = String;               // corpus alias e.g. "chainA.head"
pub struct ExpectedAssertions {
    pub redirect_to_head: Vec<EntryRef>,
    pub forbidden_absent: Vec<EntryRef>,
    pub rank_below: Vec<(EntryRef, EntryRef)>,
}
// ScenarioRecord gains: pub assertions: Option<ExpectedAssertions>
// existing: pub expected: Option<Vec<u64>>  (literal-ID; log-sourced only)

// eval/runner/trust.rs (NEW)
pub struct TrustOutcome { pub absence_pass: bool, pub rank_pass: bool, pub violations: Vec<String> }

// eval/runner/output.rs  ProfileResult ADDS: cost_tokens: f64, trust: TrustOutcome

// corpus manifest stamp (TOML alongside fixtures)
//   manifest_version = 1   migration_number = 47 (legibility only, NOT hashed)   shape_hash = "<64-hex>"
```

**Penalty const defaults** (`graph.rs:41-59,531`): `ORPHAN_PENALTY=0.75`, `CLEAN_REPLACEMENT_PENALTY=0.40`, `HOP_DECAY_FACTOR=0.60`, `PARTIAL_SUPERSESSION_PENALTY=0.60`, `DEAD_END_PENALTY=0.65`, `FALLBACK_PENALTY=0.70`, `MAX_TRAVERSAL_DEPTH=10`; clamp `[0.10, clean_replacement]` — **ceiling = the config `clean_replacement` field, not the const** (see clamp coupling under Files to Modify). Sweeping `clean_replacement` is an **amplified knob**: it moves the base penalty AND the clamp ceiling together, same direction — intended; ass-073 reads it as amplified, not isolated.

---

## Function Signatures (use exactly)

```rust
// existing — preserved
pub fn graph_penalty(node_id: u64, graph: &TypedRelationGraph, entries: &[EntryRecord]) -> f64;
// NEW — explicit-params entry point
pub fn graph_penalty_with(node_id: u64, graph: &TypedRelationGraph,
    entries: &[EntryRecord], params: &GraphPenaltyParams) -> f64;
// trust evaluator
pub fn evaluate_trust(entries: &[ScoredEntry], assertions: &ExpectedAssertions,
    alias_map: &AliasMap) -> TrustOutcome;
// existing seams touched
fn determine_ground_truth(record: &ScenarioRecord) -> Vec<u64>;            // expected > baseline
fn find_regressions(results, query_map) -> Vec<RegressionRecord>;          // + trust flip + cost growth
EvalServiceLayer::from_profile(db_path: &Path, profile: &EvalProfile, project_dir: Option<&Path>); // corpus reuses unchanged
EmbedModel::model_id() -> &'static str;  EmbedModel::dimension() -> usize; // hash inputs (read-only)
```

### Property assertion operational semantics (do not soften — R-11 vacuous-pass trap)
- **redirect-to-head**: chain terminal-active head present in top-k at rank ≤ the queried member; head absent ⇒ fail; superseded member outranking head ⇒ fail. Head resolved via existing `find_terminal_active` (`graph.rs:547`).
- **absence**: `forbidden ∩ top_k == ∅` ⇒ pass; any forbidden present ⇒ fail.
- **rank-below `(A,B)`**: A present & B present ⇒ `rank(A) > rank(B)`; **A absent ⇒ pass**; **B absent (A present) ⇒ FAIL**; both absent ⇒ A-absent dominates ⇒ pass. The asymmetric B-absent case is the most likely correctness bug — assert it explicitly.

---

## Constraints

- **C-01** Eval lives in `unimatrix-server` (ADR-004); extend the module tree, **no new crate**.
- **C-02** Config exposure is **additive** — deployed defaults and behavior unchanged at default values (ASS-037 authority; no re-tuning).
- **C-03** Trust metrics live **in the harness**, evaluated in the same `eval run` pass as P@5/MRR/cost (so the sweep correlates trust + relevance in one result).
- **C-04** Property-based ground truth **only** for the fixture corpus; **no literal-ID `expected`** and **no null `expected`** in the primary set (crt-013 #703; loader-rejected).
- **C-05** Single edge language JS/TS — eval CLI/harness is internal Rust tooling, not a client surface; Python integration suite boundary unchanged.
- **C-06** Ships via `cargo install` from main — no npm release, no packaging churn.
- **Band-3 boundary (AC-12a/AC-13, hard gate)**: **NO edits to any `.claude/protocols/` file**; the protocol layer is a recommendation handed off for separate uni-zero ratification; recommended trigger is asset-maintenance only, NOT execution-gating.
- **NFR**: hash deterministic across runs/processes (NFR-03); Wave independence (NFR-04); no new crate / files ≤500 lines (NFR-05); `eval report` exit-code semantics unchanged when trust/cost regressions present (R-17).

---

## Dependencies

- **No upstream feature/spike gate** — premises verified against current main; design proceeds immediately.
- **nan-007 / nan-010** — existing eval harness this extends (`eval/{scenarios,runner,report,profile}`; `docs/testing/eval-harness.md`).
- **crt-014** — `graph.rs` topology penalties (the constants being exposed; `graph_penalty` at `graph.rs:478`, consts at `graph.rs:41-59`).
- **ASS-037 (#3984)** — fixed-formula authority; do not re-tune defaults.
- **Crates / libs**: `tokenizers` (already in embed dependency tree — faithful token tier, ADR-003); SHA-256 (shape hash); `serde`/`toml` (existing). No new external service.
- **Downstream consumers (NOT inputs)**: rewritten ass-073 (measurement), ass-074, crt-053 (HOLD).

---

## NOT in Scope

1. **Eval-execution-as-workflow-gate** (CI-on-every-PR, automated regression policy). Separate future design. nan-018 may run the corpus *once* to validate a migration; it does not make eval *results* a standing decision gate.
2. **Answering crt-053's Q5/Q8** — downstream spikes take the measurements.
3. **Building crt-053's retrieval behavior** (leak fixes, redirect policy, #406, #585).
4. **Re-tuning fusion weights / confidence formula** (ASS-037 authority).
5. **Changing the PPR algorithm / `personalized_pagerank` / positive-edge set** — config exposure is additive.
6. **Reviving NLI scoring** (`w_nli=0.00`, `nli_enabled=false` stand).
7. **A new crate.**
8. **An exhaustive production-scale scenario suite** — the primary fixture corpus is curated and small.
9. **Any `.claude/protocols/` edit** — Band-3 protocol layer is recommendation-only.

---

## Alignment Status

**0 variances requiring approval. 1 WARN.** (PASS 5, WARN 1, VARIANCE 0, FAIL 0 — see ALIGNMENT-REPORT.md.)

- **Vision Alignment — PASS**: directly advances goal:self-learning ("eval harness confirms MRR improvement"); "verify, don't hope" is the goal's own logic. The retrieval-shape hash mirrors the vision's checksum/input-hash integrity posture.
- **Milestone Fit — PASS**: correct Nanoprobes (test-infra) home; no premature future-milestone capability (does not build crt-053 behavior, does not answer measurement questions, does not wire eval-as-gate).
- **Scope Gaps — PASS**: all 14 ACs + 8 goals map 1:1 into spec FR/NFR and architecture components.
- **Scope Additions — WARN→justified**: one bounded addition within the authorized envelope — an optional 5th "dead-end chain" fixture shape (SCOPE AC-06 says "at minimum"; needed to keep the DEAD_END_PENALTY sweep non-degenerate). (The earlier "`background.rs` second penalty site" addition is **withdrawn** — see WARN-1 resolution; it was a mis-identified log line, not a penalty site.)
- **WARN-1 (RESOLVED)**: the architecture/spec penalty-site disagreement is resolved by code verification — two sites, both in `services/search.rs` (:727, :729); `background.rs:583` is a log line, not a penalty site. Both source docs corrected. Delivery uses the two `search.rs` sites + the R-01 grep guard for AC-01 bit-for-bit. Detail in the WARN-1 section above.

---

## Open Questions for the Human

All design-time open questions are **RESOLVED and human-ratified** (see Resolved Decisions §7.1/§7.2/§7.4):
1. **Cost-growth gate ε** — RESOLVED: ε = 0.0, advisory/report-only in Wave-1 (§7.1).
2. **Shape-hash failure severity** — RESOLVED: hard error (abort) on the primary fixture corpus, warn on the snapshot (§7.2).
3. **Band-3 recommendation destination** — confirmed: `product/features/nan-018/RECOMMENDATION-band3-protocol.md`, handed to a later uni-zero session.

The single remaining human obligation is the **R-04 named column-manifest review** at delivery (see "R-04 Named Human Delivery Gate" above) — a delivery gate, not a design open question.
