# nan-018 Architecture — Eval Harness Strategic Upgrade

**Feature**: nan-018 (GH #716) — the *instrument*. Tunability + trust/cost metrics + durable fixture corpus + drift guard.
**Authority**: ADR-004 (eval-in-server), ASS-037 (#3984 formula authority — not re-tuned here), crt-014 ADR-006 (the penalty consts being exposed).
**Status**: design.

This document says *what*. The ADR files in this directory say *why*. The Integration Surface section is load-bearing: downstream delivery agents MUST use the exact names/types here and not invent them.

---

## 1. System Overview

The `unimatrix eval` harness (nan-007/nan-010) lives entirely in `crates/unimatrix-server/src/eval/`. It replays scenarios through the real `SearchService.search()` against a snapshot DB, under one or more profile TOMLs (each deserialized as a `UnimatrixConfig` override), and emits per-scenario JSON + a Markdown report with a zero-regression gate. It measures **positive relevance** (P@K, MRR, Kendall-tau, CC@k, ICD, latency).

nan-018 adds four capabilities and one guard, all inside that module tree plus a minimal, additive thread into the search path:

1. **Tunability** — the crt-014 topology-penalty `const`s become `UnimatrixConfig` fields that thread through `graph_penalty`, sweepable per profile. Defaults reproduce current behavior **bit-for-bit** (AC-01, SR-08).
2. **Trust/negative metric class** — `forbidden-set absence` and `relative-rank` assertions evaluated inside the harness so they ride A/B sweeps and the regression gate (AC-02/03/04, SR-06).
3. **Token-weighted cost metric** — `cost = Σ(per-result token-proxy)`, k secondary (AC-09, SR-02).
4. **Fixture corpus + property-based ground truth** — hand-authored entry-graphs with property assertions, not literal IDs (AC-05/06, SR-07); the **two-corpus model** (AC-07).
5. **Drift guard** — a versioned, ordered **retrieval-shape hash** stamped on the fixture corpus; the harness fails-loud/warns on mismatch (AC-08, SR-01/SR-03 — the OQ-3/OQ-4/OQ-5 linchpin).

Wave-2 adds docs (Bands 1/2), a Unimatrix `convention`/`procedure`, and a **recommendation-only** Band-3 protocol doc that edits no `.claude/protocols/` file (AC-10..13, SR-04/SR-05).

```
profile TOML ──deser──► UnimatrixConfig (+ new [graph_penalty] section)
                              │
                              ├─► InferenceConfig-sibling penalty params ──► SearchService field
                              │                                                    │
fixture corpus DB ──rebuild──► TypedGraphState ──► SearchService.search() ──► graph_penalty_with(cfg)
   + shape stamp ─────────────────────────────────────────────────┐               │
                                                                   ▼               ▼
running schema shape ──hash──► compare to stamp (drift guard)   ScoredEntry list ──► metrics:
                                                                    P@K, MRR, ... (existing)
                                                                  + trust (absence, rank-below)   ← new class
                                                                  + cost (Σ token-proxy)          ← new metric
                                                                    │
                                                                    ▼
                                                          report: zero-regression gate
                                                          (now also gates trust + cost)
```

---

## 2. Component Breakdown

| Component | Location (new/extended) | Responsibility |
|-----------|------------------------|----------------|
| **Penalty config** | `infra/config.rs` (new `GraphPenaltyConfig` struct + `UnimatrixConfig` field) | Holds the 7 crt-014 levers + optional `multiplier` overlay. Serde defaults = current consts. |
| **Engine penalty entry point** | `unimatrix-engine/src/graph.rs` (new `GraphPenaltyParams` + `graph_penalty_with`) | Pure fn taking explicit penalty params; `graph_penalty` becomes a thin wrapper passing `GraphPenaltyParams::default()`. |
| **Search threading** | `services/search.rs` (`SearchService` new field; `with_rate_config` wiring) | Stores resolved penalty params; passes them to `graph_penalty_with` + applies `multiplier`-derived `FALLBACK_PENALTY`. |
| **Trust metric class** | `eval/runner/trust.rs` (new) + `scenarios/types.rs` (`ExpectedAssertions`) | Evaluates absence + rank-below per scenario against the result list. Reusable assertion enum. |
| **Cost metric** | `eval/runner/cost.rs` (new) + `metrics.rs` | Computes `Σ token-proxy` over the returned set; surfaced on `ProfileResult`. |
| **Token proxy** | `eval/runner/cost.rs` (`token_proxy`) | Deterministic per-result token estimate over the entry's scored text. |
| **Fixture corpus loader** | `eval/corpus/` (new submodule tree) | Authors/loads/searches fixture entry-graphs; builds a snapshot DB the existing `EvalServiceLayer::from_profile` consumes. |
| **Property assertions** | `eval/corpus/assertions.rs` (new) | `redirect-to-head`, `absence`, `rank-below` property types. |
| **Shape stamp / drift guard** | `eval/shape/` (new submodule) + corpus manifest | Computes the ordered retrieval-shape hash; compares running-schema shape to the corpus stamp; fails-loud/warns. |
| **Report extensions** | `eval/report/` | Surfaces trust + cost; `find_regressions` extended to count trust failures + cost growth. |
| **Docs (Wave 2)** | `docs/testing/eval-harness.md` + 4 Band-2 guides | Capability docs, authoring guide, migration runbook, two-corpus doc, config-knob reference. |
| **Band-3 recommendation (Wave 2)** | `product/features/nan-018/RECOMMENDATION-band3-protocol.md` | Recommendation-only; no protocol-file edits (AC-12a/13). |

**No new crate** (C-01). Everything is under `unimatrix-server/src/eval/` except the additive engine penalty entry point and the config struct (both pre-existing crates).

---

## 3. Component Interactions / Data Flow

### 3.1 Tunability thread (AC-01)

The penalty consts are consumed at **exactly two production penalty-application sites — both in `services/search.rs`**, inside the Flexible-mode penalty loop (`search.rs:724-733`): **`search.rs:727`** (the fallback branch, `FALLBACK_PENALTY`) and **`search.rs:729`** (`graph_penalty(...)`). These are the only two sites that thread the penalty values into a result score; both are the threading targets for AC-01/SR-08 and the enumerated R-01/SR-08 grep guard (which remains the source of truth).

**`background.rs` is NOT a threading target.** The lone reference at `background.rs:583` is a `tracing::error!` **log string** (`"TypedGraphState rebuild: cycle detected; search using FALLBACK_PENALTY"`) — it names the const in prose but applies no penalty and reads no penalty value. It is verified (grep over `background.rs` shows exactly that one log-string hit, no `graph_penalty`/`penalty_map`/numeric-const use). Do not thread config into `background.rs`; doing so would be a false positive against the R-01 enumerated-site guard.

All other references are *tests* asserting ordering invariants against the `pub const`s (`graph_tests.rs`, `pipeline_retrieval.rs`, `services/typed_graph.rs` tests, `search.rs` tests). Confirmed: `find_terminal_active` reads no penalty const; only `graph_penalty` does.

Threading design (per ADR-001 nan-018):

1. Add `GraphPenaltyConfig` to `infra/config.rs`, surfaced as `UnimatrixConfig.graph_penalty` (`#[serde(default)]`). Each of the 7 levers gets a private `default_*()` fn whose value is the *current const* (the dual-default trap, entry #4064 — the `Default` impl and the serde default fn MUST both resolve to the const). One `multiplier: Option<f64>` overlay field (OQ-2; default `None`).
2. Add `GraphPenaltyParams` (a plain `Copy` struct of 7 `f64`/`usize`) to `unimatrix-engine/src/graph.rs`, with a `Default` whose values are the existing consts. The consts **remain** as the single source of truth for the defaults (`impl Default` references them) so ordering-invariant tests stay valid.
3. Add `graph_penalty_with(node_id, graph, entries, params: &GraphPenaltyParams) -> f64`. Move the current body into it. `graph_penalty(...)` becomes `graph_penalty_with(node_id, graph, entries, &GraphPenaltyParams::default())` — preserving every existing caller and test bit-for-bit.
4. `SearchService` gains a `graph_penalty_params: GraphPenaltyParams` field, resolved once in `with_rate_config` from the config (consts → multiplier-scaled if `Some`). The Flexible loop calls `graph_penalty_with(entry.id, &typed_graph, &all_entries, &self.graph_penalty_params)`; the fallback branch uses the resolved `fallback_penalty` field.
5. **Multiplier semantics** (OQ-2): when `multiplier = Some(m)`, each *penalty value* (orphan, clean, partial, dead_end, fallback) is scaled toward harsher (lower) by `m`; `hop_decay_factor` and `max_traversal_depth` are **not** scaled (they are shape parameters, not severities). Exact transform defined in ADR-001. Individual per-field overrides take precedence over the multiplier where both are set (multiplier is a convenience overlay, never a replacement — OQ-2).

**Default-equivalence test (the earliest signal, SR-08):** a unit test asserts that for every status shape, `graph_penalty_with(.., &GraphPenaltyParams::default())` == the current `graph_penalty(..)` == the named const, AND that `UnimatrixConfig::default().graph_penalty` resolves each field to its crt-014 const, AND that a TOML omitting `[graph_penalty]` produces the same. This is the cheapest detector of a missed construction/forwarding site. See ADR-001 §Enumerated sites.

### 3.2 Trust metric class (AC-02/03/04)

`ScenarioRecord.expected: Option<Vec<u64>>` is **literal-ID only** today. nan-018 adds a parallel, additive field `assertions: Option<ExpectedAssertions>` (kept separate from `expected` for backward wire-compat; log-sourced scenarios never set it). `ExpectedAssertions` carries the property-based ground truth (§3.4) AND the trust assertions:

- `forbidden_absent: Vec<EntryRef>` — each must be **absent** from the result list.
- `rank_below: Vec<(EntryRef, EntryRef)>` — first must rank strictly below second (or be absent, which trivially satisfies "below").

`EntryRef` is a stable handle (corpus alias string, resolved to the loaded entry id at load time — §3.4) so assertions survive re-snapshot.

Evaluation happens in `eval/runner/trust.rs::evaluate_trust(entries, assertions) -> TrustOutcome`, called in `run_single_profile` right after the result list is built, *per profile*. `TrustOutcome { absence_pass: bool, rank_pass: bool, violations: Vec<String> }` is stored on `ProfileResult`. Because it is per-profile and per-scenario, the sweep reports trust **alongside** P@5/MRR for every config (AC-04), and `find_regressions` adds: a candidate that flips a trust assertion from pass→fail is a regression (OR-extended, mirroring the existing MRR/P@K OR semantics).

This is a **class**, not two hardcoded checks: `ExpectedAssertions` holds `Vec`s of assertion records, and the evaluator is a match over an `Assertion` enum. Wave-1 ships exactly the two variants needed (SR-06 — no speculative types); future variants (quarantine-absent, contradiction-suppressed) slot into the same enum without touching call sites.

### 3.3 Cost metric (AC-09)

`cost = Σ over returned results of token_proxy(result)`. Added to `ProfileResult.cost_tokens: f64` and surfaced in `report`. `k` is reported as a **secondary** axis (`ProfileResult` already implies k via `entries.len()`); cost is the primary. `token_proxy` is defined in ADR-003 (the proxy fidelity decision, SR-02) — Wave-1 builds the **explicit token-weighted metric**, not a precision+k proxy. The regression gate flags cost growth beyond a documented threshold as a reportable regression (advisory, consistent with the existing human-reviewed gate).

### 3.4 Fixture corpus + property assertions (AC-05/06)

A fixture corpus is authored as a small set of declarative entry-graph definitions (TOML/JSON in-repo under `crates/unimatrix-server/src/eval/corpus/fixtures/`). The loader (`eval/corpus/loader.rs`) materializes them into a snapshot SQLite DB + vector dir that the **existing** `EvalServiceLayer::from_profile(db_path, ..)` consumes unchanged — i.e. the corpus is just another snapshot source, so all replay/metric machinery is reused (cumulative test infra). Each fixture entry carries a stable **alias** (e.g. `"chainA.head"`); the loader returns an alias→id map. Assertions are authored against aliases and resolved at load — never literal IDs in the source (C-04, SR-07).

Property assertion types (operationally defined in ADR-004):

- **redirect-to-head** — searching for a superseded/deprecated member of a chain must surface the terminal active head (resolved via the same `find_terminal_active` semantics) at/above the queried member.
- **absence** — a forbidden alias is not in results (shared with the trust class).
- **rank-below** — alias A ranks strictly below alias B (shared with trust class).

The five status shapes (correction chain A→B→C→head, dangling deprecated, superseded-but-Active, deprecated-but-connected, plus a clean multi-correction chain) are each one fixture. **No null `expected`, no literal-ID `expected`** in the primary set (SR-07, bans the ASS-039 self-consistency trap).

**Corpus authoring depth (ADR-004 §5 — beyond the AC-14 floor).** The simplified chain (nan-018 → ass-073 → crt-053) dropped ass-073's fixture-feasibility probe, so nan-018 authors the corpus **cold**. AC-14 only proves the corpus *measures something*, not that it is a good-enough yardstick for crt-053's Q8. The Wave-1 corpus MUST therefore exceed the AC-14 minimum: enough variation — **especially in the deprecated-but-connected shape** — that the **steepness crossover is findable** (the sim/conf point where a connected-deprecated entry crosses the weakest-active threshold sits inside a bracketed range of points, not a single exemplar). This obligation flows to the Band-2 authoring-guide spec. **Named revision loop:** "ass-073 finds Wave-1 corpus insufficient → revise nan-018 corpus + re-stamp" is an **anticipated, valid loop, not a failure** — the Wave-1 corpus is not frozen; budget one revision pass.

### 3.5 Two-corpus model (AC-07)

- **Primary = fixture corpus** (durable). Property assertions; carries the shape stamp; the trust/correctness spine.
- **Realism layer = production snapshot** (ephemeral). The existing `snapshot` path; supplies realistic P@5/MRR baselines; re-snapshot when shape drifts.

Both flow through the same `EvalServiceLayer`. Docs state which is which and when to re-snapshot (Band-2). No code branch distinguishes them at replay time — the difference is the *assertion style* and the *durability contract*, documented not enforced-by-type.

### 3.6 Drift guard — retrieval-shape hash (AC-08, the linchpin)

The fixture corpus carries a **manifest** with `shape_hash` + `migration_number` (legibility) + the enumerated inputs. At eval start, the harness computes the **running schema's** retrieval shape, compares to the corpus stamp, and **fails-loud (error) on the primary corpus / warns on the snapshot** when they diverge. Full design — ordered manifest, enumerated inputs, determinism test, deliberate-mismatch test — is ADR-002. **OQ-3 branch chosen: branch (b)** — embedding model-id + dimensionality are hash inputs, so embed-at-load is safe and the durable yardstick is protected against ONNX embed-model drift without a frozen vector sidecar. Rationale in ADR-002 §OQ-3.

### 3.7 Wave independence (SR-05)

Wave-1 = §3.1–3.6 (AC-01..09) + the AC-14 proof-by-use sweep. Wave-2 = docs + Band-3 recommendation (AC-10..13). Wave-2 has **zero code dependency** on Wave-1 internals: docs reference behavior, the recommendation references the shape hash *conceptually*. AC-14 (one correlated sweep: a steepness lever moves, trust + P@5/MRR + cost all reported in one run on the fixture corpus) is the Wave-1 exit gate.

---

## 4. Technology Decisions (ADR index)

| ADR | Title | Resolves |
|-----|-------|----------|
| ADR-001 | Penalty constants exposed as `GraphPenaltyConfig` threaded through `graph_penalty_with`; defaults bit-for-bit; multiplier overlay | OQ-2, AC-01, SR-08; supersedes crt-014 ADR-006 "no runtime configurability" stance |
| ADR-002 | Retrieval-shape hash: ordered versioned manifest, enumerated inputs, OQ-3 branch (b) embed-model-in-hash | OQ-3, OQ-4, AC-08, SR-01, SR-03 |
| ADR-003 | Token-weighted cost metric `cost = Σ token-proxy`; explicit proxy definition + error bars | OQ-1, AC-09, SR-02 |
| ADR-004 | Property-based ground truth + trust assertion class; corpus-as-snapshot reuse; alias indirection | AC-02/03/04/05/06, C-04, SR-06, SR-07 |
| ADR-005 | Two-corpus model + Band-3 recommendation-only boundary + delivery waving | AC-07, AC-12/13, OQ-5, SR-04, SR-05 |
| ADR-006 | Penalty config exposure is eval-only; deployment defaults stay fixed (crt-014 consts); not license to re-tune | C-02, ASS-037 #3984 authority; guards the §7.4 deployment boundary |

---

## 5. Integration Points / Existing Components Touched

- `crates/unimatrix-engine/src/graph.rs` — add `GraphPenaltyParams`, `graph_penalty_with`; `graph_penalty` becomes a wrapper. Consts retained.
- `crates/unimatrix-server/src/infra/config.rs` — add `GraphPenaltyConfig`, `UnimatrixConfig.graph_penalty`.
- `crates/unimatrix-server/src/services/search.rs` — `SearchService` field + `with_rate_config` wiring + Flexible-loop call swap.
- `crates/unimatrix-server/src/eval/scenarios/types.rs` — additive `assertions: Option<ExpectedAssertions>` on `ScenarioRecord`.
- `crates/unimatrix-server/src/eval/runner/{replay,metrics}.rs` — call trust + cost evaluators; populate new `ProfileResult` fields.
- `crates/unimatrix-server/src/eval/runner/output.rs` — `ProfileResult` gains `cost_tokens`, `trust: TrustOutcome`.
- `crates/unimatrix-server/src/eval/report/aggregate/mod.rs` — `find_regressions` extended (trust flip, cost growth).
- New submodules: `eval/corpus/`, `eval/shape/`, `eval/runner/trust.rs`, `eval/runner/cost.rs`.
- `crates/unimatrix-embed/src/model.rs` — read-only: `EmbedModel::model_id()` (`&'static str`) and `dimension()` (`384`) feed the shape hash.
- `docs/testing/eval-harness.md` — Wave-2.

Files >500 lines are forbidden (rust-workspace rule) — corpus/shape/trust/cost each get their own submodule rather than bloating existing files.

---

## 6. Integration Surface

Exact names/types downstream agents MUST use (do not invent).

| Integration Point | Type / Signature | Source |
|-------------------|------------------|--------|
| Penalty consts (defaults) | `ORPHAN_PENALTY=0.75`, `CLEAN_REPLACEMENT_PENALTY=0.40`, `HOP_DECAY_FACTOR=0.60`, `PARTIAL_SUPERSESSION_PENALTY=0.60`, `DEAD_END_PENALTY=0.65`, `FALLBACK_PENALTY=0.70`, `MAX_TRAVERSAL_DEPTH=10`; hop-decay clamp `[0.10, CLEAN_REPLACEMENT_PENALTY]` — **ceiling is `clean_replacement` itself** (see clamp-coupling note below) | `unimatrix-engine/src/graph.rs:41-59,531` |
| Existing penalty fn | `pub fn graph_penalty(node_id: u64, graph: &TypedRelationGraph, entries: &[EntryRecord]) -> f64` | `graph.rs:478` |
| **New** penalty params | `pub struct GraphPenaltyParams { orphan: f64, clean_replacement: f64, hop_decay: f64, partial_supersession: f64, dead_end: f64, fallback: f64, max_traversal_depth: usize }` (Copy; `Default` = consts) | nan-018 ADR-001 |
| **New** penalty fn | `pub fn graph_penalty_with(node_id: u64, graph: &TypedRelationGraph, entries: &[EntryRecord], params: &GraphPenaltyParams) -> f64` | nan-018 ADR-001 |
| **New** config section | `UnimatrixConfig.graph_penalty: GraphPenaltyConfig` (`#[serde(default)]`); fields mirror params + `multiplier: Option<f64>` | nan-018 ADR-001 |
| Production penalty call site | Flexible loop, `if entry.superseded_by.is_some() || status==Deprecated { graph_penalty(...) or FALLBACK_PENALTY }` | `services/search.rs:724-733` |
| Search config carrier | `ServiceLayer::with_rate_config(.., Arc<InferenceConfig>, ..)`; `SearchService` scalar fields e.g. `ppr_alpha: f64` | `eval/profile/layer.rs:363-385`, `services/search.rs:371-379` |
| Scenario expected (literal) | `ScenarioRecord.expected: Option<Vec<u64>>` | `eval/scenarios/types.rs:57` |
| **New** assertions | `ScenarioRecord.assertions: Option<ExpectedAssertions>` where `ExpectedAssertions { redirect_to_head: Vec<EntryRef>, forbidden_absent: Vec<EntryRef>, rank_below: Vec<(EntryRef, EntryRef)> }`; `EntryRef = String` (corpus alias) | nan-018 ADR-004 |
| Ground-truth resolver | `fn determine_ground_truth(record: &ScenarioRecord) -> Vec<u64>` (expected > baseline) | `eval/runner/metrics.rs:26` |
| Result row | `ProfileResult { entries, latency_ms, p_at_k, mrr, cc_at_k, icd }` → **adds** `cost_tokens: f64`, `trust: TrustOutcome` | `eval/runner/output.rs:32` |
| **New** trust outcome | `pub struct TrustOutcome { absence_pass: bool, rank_pass: bool, violations: Vec<String> }` | nan-018 ADR-004 |
| Regression detector | `fn find_regressions(results, query_map) -> Vec<RegressionRecord>`; OR semantics on `mrr < baseline.mrr || p_at_k < baseline.p_at_k` | `eval/report/aggregate/mod.rs:129,171-174` |
| Snapshot consumer | `EvalServiceLayer::from_profile(db_path: &Path, profile: &EvalProfile, project_dir: Option<&Path>)` rebuilds `TypedGraphState` from the snapshot DB | `eval/profile/layer.rs:96` |
| Embedding identity (hash input) | `EmbedModel::model_id() -> &'static str` (`"sentence-transformers/all-MiniLM-L6-v2"`), `EmbedModel::dimension() -> usize` (`384`), `InferenceConfig.embedding_model_sha256: Option<String>` | `unimatrix-embed/src/model.rs:26,40`; `infra/config.rs:279` |
| Edge type taxonomy (hash input) | `RelationType` (16 variants) `as_str()` | `unimatrix-engine/src/graph.rs:86-122` |

**Clamp-coupling note (delivery-critical — ADR-001):** the hop-decay branch (`graph.rs:530-531`) is `raw = clean_replacement * hop_decay^(d-1)`, clamped to `[0.10, clean_replacement]`. The **upper clamp bound is `clean_replacement` itself**, by design (a depth-≥2 replacement is never penalized more harshly than a clean depth-1 one). When delivery parameterizes the body into `graph_penalty_with`, the clamp ceiling MUST become `params.clean_replacement` (NOT the `CLEAN_REPLACEMENT_PENALTY` const) so a swept value moves base and ceiling together and the depth-2 ≤ depth-1 monotonicity holds. The lower bound `0.10` stays a literal. Consequence: `clean_replacement` is an **amplified sweep knob** — moving it shifts base penalty AND clamp ceiling in the same direction; ass-073 reads its sweep as amplified, not isolated. The ceiling is intentionally **not** a separate `GraphPenaltyParams` field.

---

## 7. Locked Decisions (human-ratified) + R-04 Delivery Gate

### 7.1 Cost-growth gate: ε = 0.0, advisory (report-only, blocks nothing) — Wave-1 (LOCKED)

The cost-growth gate (ADR-003) is **advisory only**: `find_regressions` reports any candidate whose `cost_tokens` exceeds baseline by more than **ε = 0.0** (i.e. *any* growth is listed in the human-reviewed regression block) and **blocks nothing**. Rationale:

- A **blocking** ε would breach nan-018's boundary — eval is deliberately **NOT a workflow gate**; it is the instrument, not the referee.
- Choosing any non-zero ε now is **premature tuning**: the cost threshold is ASS-037's authority, and the downstream spikes (ass-073 cost-of-noise) have not yet produced a cost distribution to set a defensible number against. ε = 0.0 reports everything and pre-commits to nothing.

This promotes ADR-003's previously-"proposed" default to the **locked** decision.

### 7.2 Shape-hash mismatch severity: HARD ERROR (abort) on primary corpus / WARN on snapshot (LOCKED)

On the **primary fixture corpus**, a retrieval-shape-hash mismatch is a **HARD ERROR (abort the eval run)**. On the **production snapshot** (realism layer), the same mismatch is a **WARN**. Rationale:

- The primary corpus is the **durable yardstick** whose numbers feed ass-073's findings → crt-053's acceptance criteria → actual product ranking. Silent drift there propagates to **product behavior**, not merely a dashboard reading.
- Breaking the eval harness's exit-0 convention here is **correct**: the drift guard protects **corpus validity** — a different class of guarantee from the body-only quality verdict (which stays advisory/exit-0). Aborting is the right response to an invalid yardstick; degrading numbers are a verdict, an invalid measuring stick is a precondition failure.
- The snapshot is ephemeral by contract (re-snapshot when shape drifts), so a WARN suffices there.

This promotes ADR-002's previously-"proposed" severity (and the §3.6 parenthetical) to the **locked** decision.

### 7.3 R-04 — Retrieval-shape-hash column manifest completeness: NAMED HUMAN DELIVERY GATE (LOCKED)

Manifest **column completeness** is a **named human review obligation at delivery** — it is explicitly **NOT folded into routine code review**. A test can only prove the hash is sensitive to the columns the manifest **declares** (the deliberate-mismatch test in ADR-002 mutates a *declared* input and asserts the hash flips). No test can prove the *declared* column set is itself complete. Only a human reading the manifest's entries-column list **against the actual retrieval/ranking path** can confirm that **no retrieval-relevant column was mis-classified as display-only** — the silent-staleness path (R-04): a retrieval-affecting column omitted from the manifest yields a hash that gives false confidence while drifting.

**Delivery gate (R-04):** before nan-018 delivery is accepted, a named human reviewer MUST sign off that the shape-hash manifest's in-scope `entries` column list covers every column the live retrieval/ranking path reads. This obligation flows to the IMPLEMENTATION-BRIEF as an explicit delivery gate, distinct from automated tests and routine review.

### 7.4 Penalty deployment boundary (ADR-006)

The new `GraphPenaltyConfig` exposure is **eval/measurement-only** and is **not** license to re-tune deployed defaults. Deployed penalty values stay fixed at their crt-014 v1 const defaults; ASS-037 (#3984) remains formula authority. See **ADR-006** (`ADR-006-penalty-deployment-boundary.md`). Recorded here so a future reader cannot mis-read the new config field as a deployment tuning surface.

### 7.5 Open (non-blocking handoff)

- **Band-3 recommendation destination** (ADR-005): the recommendation doc lives in `product/features/nan-018/` and is handed to a later uni-zero session. No action needed now; flagged so the handoff isn't lost.
