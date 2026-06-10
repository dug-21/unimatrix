# nan-018 — Specification: Eval Harness Strategic Upgrade

**Status**: SPECIFICATION (post-scope, pre-architecture)
**Feature**: nan-018 (GH #716)
**Source scope**: `product/features/nan-018/SCOPE.md` (AC-01…AC-14; all five OQs RESOLVED)
**Risk inputs**: `product/features/nan-018/SCOPE-RISK-ASSESSMENT.md` (SR-01…SR-08)
**Architecture authority**: ADR-004 (eval lives in `unimatrix-server`). Formula authority: ASS-037 (#3984).

---

## Objective

Upgrade the in-server `unimatrix eval` harness from a positive-relevance-only instrument into one that can also measure **trust** (correctness/negative properties), **cost** (token-weighted noise), and **tunability** (sweepable status-penalty levers), validated against a **durable hand-authored fixture corpus** guarded by a **retrieval-shape hash**. nan-018 is the instrument; the downstream measurement spikes (rewritten ass-073, ass-074) and crt-053 consume it. Its own success criterion is proof-by-use: a single correlated steepness sweep that reports trust + P@5/MRR + cost on the fixture corpus.

---

## Domain Models / Ubiquitous Language

### Corpora (two-corpus model)

- **Fixture corpus (primary / durable)** — a small, hand-authored set of entry-graphs with property-based `expected` assertions. The stable spine for trust/correctness measurement. Version-controlled in-repo. Carries the retrieval-shape hash stamp.
- **Production snapshot corpus (realism / ephemeral)** — the existing `unimatrix snapshot` → `eval scenarios` path. Supplies realistic P@5/MRR baselines from real `query_log` traffic. Re-snapshotted when retrieval shape drifts; never the trust/correctness authority.

### Fixture entry-graph shapes (the five status topologies)

Each shape is a hand-authored cluster of entries plus Supersedes edges and statuses that exercises a distinct branch of `graph_penalty` (`crates/unimatrix-engine/src/graph.rs`):

| Shape | Definition | Exercises |
|-------|-----------|-----------|
| **Correction chain** | A→B→C→head: deprecated entries superseded transitively to a single Active terminal (depth > 1). | `CLEAN_REPLACEMENT_PENALTY` × `HOP_DECAY_FACTOR^(depth-1)`; redirect-to-head |
| **Dangling deprecated** | Deprecated entry with no successors. | `ORPHAN_PENALTY` |
| **Superseded-but-Active** | Entry marked superseded yet still `Active` status. | status/topology conflict path |
| **Deprecated-but-connected** | Deprecated entry that is still reachable via non-Supersedes (positive) edges. | absence / rank-below under connectivity |
| **Dead-end chain** (optional 5th) | Superseded entries whose chain reaches no Active terminal. | `DEAD_END_PENALTY` |

AC-06 requires at minimum: multi-correction chain, dangling chain, superseded-Active, deprecated-connected.

### Property assertion types (property-based `expected`)

`expected` for fixture scenarios is expressed as **relationships/properties**, never literal ID lists (C-04). Three operationally-defined types:

- **redirect-to-head** — for a query whose best answer is a corrected chain, the **head (terminal Active)** entry of the chain must appear in results; superseded predecessors must not outrank it. Operational test: the chain-head ID is present in top-k AND every superseded chain member, if present, ranks strictly below the head.
- **absence** (forbidden-set) — a specified set of IDs (e.g. the stale source) must be **absent** from the top-k result set. Operational test: `forbidden_set ∩ top_k_ids == ∅`.
- **rank-below** (relative-rank) — ID-A must rank strictly below ID-B (deprecated below weakest active). Operational test: both present ⇒ `rank(A) > rank(B)`; A absent ⇒ vacuously satisfied; B absent ⇒ violated (the entry that should rank higher is missing).

The forbidden-set and rank-below assertions are the **trust/negative metric class** (Goal 2). They are a new metric *class* alongside positive relevance — reusable for future correctness properties (quarantine, contradiction suppression) but in Wave 1 only these two are built (SR-06: no speculative assertion types).

### Retrieval-shape hash

A deterministic hash stamped on the fixture corpus capturing the schema's retrieval-relevant shape. **Enumerated, ordered, versioned input manifest** (SR-01):

1. **Entry columns** that feed retrieval/penalty (the retrieval-relevant subset, e.g. `status`, `supersedes`, `category`, confidence-bearing columns — the architect enumerates the exact list).
2. **Edge types** (the `RelationType` taxonomy in `graph.rs`).
3. **Confidence dimensions** (the `ConfidenceWeights` field set).
4. **Embedding dimensionality / model-id** — first-class input (settles OQ-3; see Durability below).

The hash is computed over an explicit, ordered serialization of this manifest (no map-iteration-order or float-formatting nondeterminism). It carries a **manifest version** integer so the hash definition itself is migratable. The **migration number** is stamped alongside the hash for human legibility (the hash is the machine guard; the migration number is the human label).

### Durability branch (OQ-3, binding)

The fixture corpus is a durable yardstick and **must not silently become embed-model-dependent** (binding constraint). Because embedding dimensionality / model-id is a hash input (item 4 above), **OQ-3 branch (b) holds**: embed-at-load is safe — an embed-model change moves the hash and trips the drift guard, so a frozen vector sidecar is **not** required. The spec takes branch (b). (If the architect later removes embed-model-id from the hash inputs, branch (a) — a frozen vector sidecar — becomes mandatory; that reversal must be stated explicitly in design.)

### Cost (token-weighted)

**cost = Σ(per-result token-proxy)** over the returned set — the tokens an agent pays to read the result set. **Set-size (k) is a SECONDARY axis, not the primary**: the same k carries different cost when token loads differ. The token-proxy is a documented approximation (not a real tokenizer); its definition and known error bars are stated in design (SR-02), and documented as a *proxy* in the Band-2 config-knob reference so downstream reads numbers correctly.

---

## Functional Requirements

Each FR is testable. Wave assignment in brackets.

### Tunability (Goal 1, AC-01) [Wave 1]

- **FR-01** Each crt-014 status-penalty constant — `ORPHAN_PENALTY`, `CLEAN_REPLACEMENT_PENALTY`, `PARTIAL_SUPERSESSION_PENALTY`, `DEAD_END_PENALTY`, `FALLBACK_PENALTY`, `HOP_DECAY_FACTOR`, and the penalty clamp bounds `[0.10, 0.40]` — is exposed as an individually-overridable `UnimatrixConfig` field (per-constant exposure, OQ-2). `MAX_TRAVERSAL_DEPTH` is exposed as a lever as well.
- **FR-02** An **optional single-multiplier overlay** field scales the per-constant penalty values uniformly. The overlay is a convenience layer over FR-01, **not** a replacement: per-constant fields remain independently settable. (OQ-2)
- **FR-03** `graph_penalty` and **both penalty-application sites — which are BOTH in `unimatrix-server/src/services/search.rs`: `:727` (the `FALLBACK_PENALTY` fallback branch) and `:729` (the `graph_penalty(...)` call) — read these values from threaded config rather than module `const`s. These two `search.rs` sites are the only penalty-application sites (LOCKED).** `background.rs:583` is **NOT** a threading target: it is a `tracing::error!` log string mentioning `FALLBACK_PENALTY` for human-readable diagnostics, not a penalty-*application* site, and must not be threaded. Every construction/forwarding site is enumerated and updated; the R-01 enumerated-site grep guard remains the source of truth for AC-01 bit-for-bit equivalence (SR-08).
- **FR-04** A profile TOML can set any subset of these fields; unset fields fall back to the current `const` values. A sweep of a steepness value across two profiles produces the expected A/B penalty delta, observable in the report.
- **FR-05** The exposed levers appear in the Band-2 config-knob reference with meaning, valid range, default, and effect (see FR-25).

### Trust / Negative Metric Class (Goal 2, AC-02…AC-04) [Wave 1]

- **FR-06** A scenario can carry a **forbidden-set** assertion: a set of entry IDs asserted absent from the top-k results. The harness evaluates `forbidden ∩ top_k == ∅` per scenario.
- **FR-07** A scenario can carry a **rank-below** assertion: `(A, B)` asserting `rank(A) > rank(B)` in results, with the operational semantics defined in Domain Models (A absent ⇒ pass; B absent ⇒ fail).
- **FR-08** Trust assertion outcomes (pass/fail per assertion, aggregate pass rate per profile) are surfaced in the `eval report` output as a distinct section.
- **FR-09** Trust assertion failures are **counted in the regression check**: a candidate profile that newly violates a forbidden-set or rank-below assertion the baseline satisfied is flagged as a regression in Section 5, with the same fail-in-body-only semantics as the existing zero-regression check (`eval report` exit code unchanged).
- **FR-10** Trust assertions live **in the harness** (C-03), evaluated in the same `eval run` pass as P@5/MRR/cost — not routed to a separate suite — so a single run correlates "steepness X → trust holds AND relevance did not regress" (AC-04).
- **FR-11** The trust assertion representation is designed to admit future assertion types (the metric *class*) but Wave 1 implements exactly forbidden-set and rank-below (SR-06).

### Token-Weighted Cost Metric (Goal 3, AC-09) [Wave 1]

- **FR-12** The harness computes a per-profile **token-weighted cost** = Σ(per-result token-proxy) over the returned set, and surfaces it per profile in `eval report` with a cost delta column relative to baseline.
- **FR-12a** The **cost-growth gate is advisory at ε = 0.0 (report-only) in Wave 1 (LOCKED)**: any cost growth relative to baseline (delta > 0.0) is *reported* in `eval report`, but it **blocks nothing** — consistent with eval not being a workflow gate (NOT-in-scope #1). The ε = 0.0 threshold means "report any growth"; it does not introduce a passing tolerance band and does not affect the `eval report` exit code.
- **FR-13** The token-proxy definition is fixed and documented (its formula and error characteristics), labeled as a proxy (not a real tokenizer) (SR-02). Result-set-size (k) is reported as a **secondary** axis alongside cost, never as the primary cost definition.
- **FR-14** If the architect elects a precision+k proxy, that is an **explicit, justified in-design architecture call** recorded in the ADR — never a deferral to a downstream spike, and it does not displace the token-weighted definition as primary. The default and lean is the explicit token-weighted metric. (OQ-1)

### Fixture Corpus + Property-Based Ground Truth (Goal 4, AC-05) [Wave 1]

- **FR-15** A fixture entry-graph (entries + Supersedes/positive edges + statuses for the five status shapes) can be authored in a version-controlled in-repo format, loaded into a `TypedGraphState`/replay-compatible state, and searched via the existing `SearchService.search()` replay path.
- **FR-16** Fixture scenarios express `expected` as the property types redirect-to-head, absence, and rank-below (Domain Models) — **never literal ID lists** (C-04). The loader rejects (or the format cannot represent) null `expected` and literal-ID `expected` in the primary corpus (SR-07; bans the ASS-037/ASS-039 null-`expected` self-consistency trap).
- **FR-17** Property assertions resolve relationship anchors (e.g. "the chain head", "the weakest active") to concrete IDs at load time against the loaded fixture graph, so assertions survive ID renumbering and shape changes that literal-ID assertions would not.

### Primary Corpus + Two-Corpus Model (Goal 5, AC-06/AC-07) [Wave 1]

- **FR-18** The canonical fixture corpus ships in-repo, version-controlled, covering at minimum: multi-correction chain, dangling chain, superseded-Active, deprecated-connected.
- **FR-19** The corpus is small and curated by design (Non-Goal 8); it is the durable primary, not an exhaustive production-scale suite. It must be large enough to exercise each trust shape end-to-end (SR-06/AC-14).
- **FR-20** The production-snapshot path (`snapshot` → `eval scenarios` → `eval run`) continues to produce realistic P@5/MRR baselines unchanged; the two-corpus roles (fixture = primary/durable, snapshot = realism/ephemeral) are documented (FR-24).

### Drift Guard — Retrieval-Shape Hash (Goal 6, AC-08) [Wave 1]

- **FR-21** The fixture corpus carries a stamped retrieval-shape hash (plus migration number) computed over the enumerated, ordered, versioned manifest (Domain Models). The hash function is deterministic across runs (stable ordering and serialization).
- **FR-22** On `eval run`, the harness recomputes the running schema's retrieval-shape hash and compares it to the stamped hash. **The divergence behavior is corpus-dependent (LOCKED):** on the **primary fixture corpus**, a mismatch is a **HARD ERROR (abort, non-zero exit)**; on the **production snapshot corpus**, a mismatch is a **WARN (continue)**. In both cases the message names what shape dimension diverged for human triage. Rationale: the durable yardstick's numbers propagate to product ranking policy, so the drift guard must protect corpus validity by aborting — this is distinct from, and deliberately overrides for the primary corpus, the eval `report` exit-0 quality-verdict convention (which governs *quality* gate failures, not corpus *validity*). The snapshot corpus is realism/ephemeral and re-snapshotted on drift, so warn-and-continue is correct there.
- **FR-23** The hash inputs explicitly include embedding dimensionality / model-id (FR-21 item 4), so an embed-model change trips the guard (branch (b), FR durability) — the durable reference is protected against ONNX embed-model drift without a frozen sidecar.

### Documentation (Goal 7, AC-10/AC-11) [Wave 2]

- **FR-24** `docs/testing/eval-harness.md` is updated with the new capabilities: tunability levers, trust metric class, token-weighted cost, fixture corpus, two-corpus model, drift guard.
- **FR-25** Band-2 docs exist and are sufficient for a dev (human or agent) to author/migrate/sweep without reverse-engineering code: (a) fixture-corpus authoring guide; (b) schema-migration runbook for the corpus + its assertions; (c) two-corpus model (when to use which, how to re-snapshot); (d) config-knob reference for the newly-exposed levers (meaning, range, default, effect) including the cost-metric proxy caveat.
- **FR-26** ADRs are stored in Unimatrix for the architecture decisions (trust-metric placement, two-corpus model, penalty-config exposure shape, fixture-as-primary, cost-metric definition, hash manifest, durability branch). No ADR files (Unimatrix-only).

### Band-3 Forward-Discipline (Goal 8, AC-12) [Wave 2]

- **FR-27** nan-018 delivers **one document** recommending a conditional eval-corpus-migration protocol trigger — patterned on the `[CONDITIONAL] uni-docs` step, firing when **"your change alters the retrieval-shape hash"** (coupled to the Goal-6 hash, deterministic, not an enumerated list). The document describes how the design and delivery/bugfix protocols *would* carry the conditional step.
- **FR-28** A Unimatrix `convention` entry couples schema/shape change ⇒ corpus migration and is surfacable in briefing.
- **FR-29** `procedure` entries document how to migrate the corpus and how to author a scenario.
- **FR-30** **nan-018 makes NO edits to any `.claude/protocols/` file.** FR-27 is a recommendation handed off for separate uni-zero ratification. FRs-28/29 (knowledge) and the version-stamp guard (FR-21..23, code) ship inside nan-018; FR-27 (protocol recommendation) is a handoff.

### Proof by Use (AC-14) [Wave 1 — the Wave-1 exit]

- **FR-31** A single `eval run` performs a steepness sweep of an exposed lever (FR-01/02) across ≥2 profiles on the fixture corpus and the report shows, in one correlated result: trust outcomes (FR-06..09), P@5/MRR (existing), and the token-weighted cost metric (FR-12). This demonstrates the instrument runs end-to-end and the downstream spikes have something to measure with.

---

## Non-Functional Requirements

- **NFR-01 (Bit-for-bit default equivalence)** With all penalty fields unset (or at default values), `graph_penalty` and the clamp produce **bit-for-bit identical** output to the current `const`-based implementation, across every consumption site. Exposure is purely additive; deployed defaults and behavior are unchanged (C-02, AC-01, SR-08). Measurable: a default-equivalence test asserts penalties at default config equal the current `const`s for every shape branch.
- **NFR-02 (Additive-only config)** Adding the new config fields changes no behavior at default values and breaks no existing profile TOML (omitted sections fall back to defaults). Measurable: existing eval profile tests pass unchanged; an empty/baseline TOML reproduces current results.
- **NFR-03 (Hash determinism)** The retrieval-shape hash is identical across repeated runs on the same schema (stable ordering, deterministic float/string serialization, no map-iteration nondeterminism). Measurable: a stability unit test computes the hash N times and asserts equality (SR-01).
- **NFR-04 (Wave independence)** Wave 1 (AC-01…AC-09 + AC-14) is independently shippable and is the proof-by-use exit; Wave 2 (AC-10…AC-13: docs, knowledge, protocol recommendation) has **zero code coupling** to the instrument core and can land later without blocking the downstream sweep (SR-05). Measurable: Wave-1 acceptance tests pass without any Wave-2 artifact present.
- **NFR-05 (No new crate; module-tree extension)** All code lives under `crates/unimatrix-server/src/eval/` and `unimatrix-engine`/`unimatrix-core` config, extending the existing module tree (C-01, ADR-004). Files stay ≤ 500 lines (workspace rule).
- **NFR-06 (No client-surface change)** Eval CLI/harness remains internal Rust tooling; no JS/TS client surface is added or changed (C-05). The Python integration suite boundary is unchanged.
- **NFR-07 (Ships via `cargo install`)** Enhancement rides the existing dev-binary path; no npm release, no packaging churn (C-06).
- **NFR-08 (Cost-proxy fidelity stated)** The token-proxy's definition and known error bars are documented, so downstream cost-of-noise findings are read with correct fidelity (SR-02).

---

## Acceptance Criteria — Verification Methods (1:1 with SCOPE)

| AC | Criterion (abbrev.) | Verification method |
|----|---------------------|---------------------|
| **AC-01** | Penalty levers are config fields threaded through `graph_penalty`; TOML steepness sweep yields expected A/B delta; defaults reproduce current behavior **bit-for-bit**. | (a) **Default-equivalence test**: with default config, `graph_penalty` output == current `const`s for every shape branch and clamp (NFR-01). (b) A two-profile TOML sweep of one steepness lever produces the predicted penalty delta in the report. (c) Enumerate every consumption/construction site and assert each reads config; the **two penalty-application sites are BOTH in `services/search.rs` (`:727` fallback branch, `:729` graph_penalty)** — `background.rs:583` is a `tracing::error!` log string, NOT a site. The R-01 enumerated-site grep guard is the source of truth for bit-for-bit equivalence (SR-08). |
| **AC-02** | Forbidden-ID-set absence assertion, surfaced in `report`, counted in regression check. | Fixture scenario with a forbidden set: assert `forbidden ∩ top_k == ∅` evaluates correctly (pass when absent, fail when present); assert the outcome appears in report and a newly-introduced violation registers in Section 5 regression list. |
| **AC-03** | Relative-rank (A below B) assertion, surfaced and gated likewise. | Fixture scenario asserting `rank(A) > rank(B)`: unit + end-to-end test covering present/present (compare), A-absent (pass), B-absent (fail); assert surfaced and regression-counted. |
| **AC-04** | Steepness sweep reports trust **alongside** P@5/MRR in one run. | One `eval run` over ≥2 steepness profiles on the fixture corpus; assert the report contains trust outcomes and P@5/MRR for the same scenarios in a single correlated table/section. |
| **AC-05** | Fixture entry-graph authored/loaded/searched; `expected` property-based. | Author a fixture graph of the five shapes; load and run search replay; assert property assertions (redirect-to-head/absence/rank-below) resolve and evaluate; assert loader rejects null/literal-ID `expected` (SR-07). |
| **AC-06** | Canonical fixture corpus ships in-repo covering ≥ the four shapes. | In-repo version-controlled corpus file(s); test asserts presence of multi-correction chain, dangling chain, superseded-Active, deprecated-connected shapes and that each loads and searches. |
| **AC-07** | Two-corpus model: snapshot still yields realistic P@5/MRR; docs state primary/durable vs realism/ephemeral. | Run the snapshot path → assert P@5/MRR still produced unchanged (existing tests green); doc-review checklist confirms the two roles are stated (FR-24/25). |
| **AC-08** | Retrieval-shape hash stamp over enumerated inputs; **hard-error on primary-corpus mismatch / warn on snapshot-corpus mismatch** (tested by simulating a mismatch on each); spec states durability branch; manifest completeness is a named human delivery gate. | (a) Hash computed over the enumerated manifest. (b) **Deliberate-hash-mismatch test (per corpus)**: stamp a corpus, mutate one manifest input (or the stamp), run → (i) on the **primary fixture corpus** assert the guard **aborts with a non-zero exit** and a dimension-naming message; (ii) on the **production snapshot corpus** assert the guard **warns and continues** with the same dimension-naming message. (c) NFR-03 determinism test. (d) Spec states **branch (b)** (embed-model-id/dim in hash; no sidecar). (e) **Manifest-completeness sensitivity test**: assert the hash changes iff a *declared* manifest entry changes (no sensitivity to undeclared/display-only columns). (f) **R-04 named human delivery gate**: column-manifest completeness is confirmed by a **named human review at delivery** (not routine code review) — the human confirms no retrieval-relevant entry column was mis-classified as display-only and thus omitted from the manifest. The (e) test proves sensitivity only to the *declared* set; the human review certifies the declared set is *complete*. |
| **AC-09** | Token-weighted cost = Σ per-result token-proxy, k secondary; surfaced in `report`; proxy-narrowing only as explicit in-design call, never a deferral. | Unit test of cost = Σ token-proxy on a known result set; assert two sets with same k but different token loads yield different cost; assert cost + k both surfaced in report; assert the **cost-growth gate is advisory at ε=0.0** — any growth (delta > 0.0) is reported but the `eval report` exit code is unchanged (blocks nothing, FR-12a); ADR records the chosen definition (token-weighted default, NFR-08). |
| **AC-10** | `docs/testing/eval-harness.md` updated; ADRs stored. | Doc diff present and covers all new capabilities; Unimatrix ADR entries exist for each architecture decision (FR-26). |
| **AC-11** | Band-2 docs sufficient for author/migrate/sweep. | Authoring guide, migration runbook, two-corpus doc, config-knob reference exist; doc-review confirms a dev could author a scenario / migrate the corpus / run a sweep from the docs alone. |
| **AC-12** | (a) Protocol recommendation document for later uni-zero, **no `.claude/protocols/` edits**; (b) `convention` couples shape-change⇒migration; (c) `procedure` entries. | (a) Recommendation doc exists; `git diff` shows **zero** changes under `.claude/protocols/`. (b) Unimatrix `convention` entry exists and is surfacable in briefing. (c) `procedure` entries exist. |
| **AC-13** | No protocol/workflow changes at all; deferred-design boundary documented; recommended trigger is asset-maintenance only, not execution-gating. | **Hard gate**: `git diff` confirms no `.claude/protocols/` file edited and no eval-execution-as-gate wiring added; docs state the deferred-separate-design boundary (Goal 7 assumptions) and that the trigger is asset-maintenance-only. |
| **AC-14** | Proof-by-use: a correlated steepness sweep runs end-to-end on the fixture corpus reporting trust + P@5/MRR + cost in one run. | **End-to-end sweep test (Wave-1 exit)**: one `eval run` of a steepness sweep on the fixture corpus; assert the report contains, for the same scenarios, trust outcomes (AC-02/03) **and** P@5/MRR (AC-04) **and** the cost metric (AC-09). nan-018 only demonstrates the sweep executes; it does not answer crt-053's Q5/Q8. |

---

## User / Agent Workflows

1. **Sweep a steepness lever (the core use, AC-14).** Author two profile TOMLs differing in a penalty lever (e.g. `clean_replacement_penalty` or the single-multiplier overlay) → `eval run --configs base.toml,steep.toml` against the fixture corpus → `eval report`. Read the correlated section: trust pass-rate, P@5/MRR, and cost per profile. Decide whether the steepness held trust without regressing relevance or inflating cost.
2. **Author a fixture scenario.** Following the Band-2 authoring guide, define an entry-graph of one of the five shapes and a property-based `expected` (redirect-to-head / absence / rank-below). Load → search → confirm the assertion resolves.
3. **Migrate the corpus after a schema change.** A schema change moves the retrieval-shape hash; the drift guard trips on the next run. Following the migration runbook, re-stamp the corpus and update assertions, bumping the migration number.
4. **Run the realism baseline.** Snapshot → `eval scenarios` → `eval run` on the production snapshot for realistic P@5/MRR (unchanged path, AC-07).

---

## Constraints

- **C-01** Eval lives in `unimatrix-server` (ADR-004); extend the module tree, no new crate.
- **C-02** Config exposure is **additive** — deployed defaults and behavior unchanged at default values (ASS-037 authority; no re-tuning).
- **C-03** Trust metrics live **in the harness**, evaluated in the same run as P@5/MRR (so the sweep correlates trust + relevance in one result).
- **C-04** Property-based ground truth only for the fixture corpus; **no literal-ID `expected`** in the primary set (crt-013 #703).
- **C-05** Single edge language JS/TS — eval CLI/harness is internal Rust tooling, not a client surface; Python integration suite boundary unchanged.
- **C-06** Ships via `cargo install` from main — no npm release, no packaging churn.
- **C-07 — DISSOLVED.** No protocol-vs-corpus sequencing constraint: nan-018 makes no protocol edits.
- **Band-3 boundary (AC-12a/AC-13)**: recommendation-only. **NO edits to any `.claude/protocols/` file**; the protocol layer is a handoff for separate uni-zero ratification. The recommended trigger is asset-maintenance only, explicitly NOT execution-gating.

---

## Dependencies

- **nan-007 / nan-010** — the existing eval harness this extends (`eval/{scenarios,runner,report,profile}`); `docs/testing/eval-harness.md`.
- **ADR-004** — eval-in-server architecture.
- **crt-014** — `graph.rs` topology penalties (the constants being exposed; `graph_penalty` at `graph.rs:478`, constants at `graph.rs:41–59`).

### Integration Surface (penalty-application sites — LOCKED)

The penalty/clamp values are *applied* at exactly **two sites, both in `crates/unimatrix-server/src/services/search.rs`**:

| Site | Branch | What it applies |
|------|--------|-----------------|
| `services/search.rs:727` | `use_fallback` fallback branch | `FALLBACK_PENALTY` |
| `services/search.rs:729` | non-fallback branch | `graph_penalty(...)` (which internally applies the crt-014 constants + clamp) |

`background.rs:583` is **explicitly NOT** a penalty-application site — it is a `tracing::error!` diagnostic string that names `FALLBACK_PENALTY` in human-readable log text only, and is not a config-threading target. The **R-01 enumerated-site grep guard** is the authoritative source of truth for AC-01 bit-for-bit equivalence across these sites.
- **ASS-037 (#3984)** — fixed-formula authority; do not re-tune defaults.
- **No upstream feature/spike gate.** Premises verified against current main (penalty `const`s present; no penalty/orphan/hop_decay fields in config; metric set lacks absence/rank-below; cost is latency-only). Design proceeds immediately.
- **Downstream consumers (NOT inputs)**: rewritten ass-073 (measurement), ass-074, crt-053 (HOLD).

---

## NOT in Scope (explicit exclusions)

1. **Eval-execution-as-workflow-gate** (CI-on-every-PR, automated regression policy, blocking-vs-advisory). Separate future design. nan-018 may run the corpus *once* to validate a migration; it does not make eval *results* a standing decision gate.
2. **Answering crt-053's Q5/Q8** — the downstream spikes take the measurements.
3. **Building crt-053's retrieval behavior** (leak fixes, redirect policy, #406, #585).
4. **Re-tuning fusion weights / confidence formula** (ASS-037 authority).
5. **Changing the PPR algorithm / `personalized_pagerank` / positive-edge set** — config exposure is additive; algorithms unchanged.
6. **Reviving NLI scoring** (`w_nli=0.00`, `nli_enabled=false` stand).
7. **A new crate.**
8. **An exhaustive production-scale scenario suite** — the primary fixture corpus is curated and small.
9. **Any `.claude/protocols/` edit** — Band-3 protocol layer is recommendation-only.

---

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` — surfaced the eval-harness positive-relevance baseline (#4888), the nan-010 "7-component fixed layer order" pattern for extending the harness (#3610), the A/B profile-TOML pattern (#2806), and the config-field-mismatch lesson where a spec listed a field type that differed from the actual struct (#4148, #4333) — informs FR-03/AC-01 site enumeration. No prior trust/cost/fixture-corpus pattern exists; nan-018 establishes it.
- Queried (R2 lock-in): `mcp__unimatrix__context_briefing` — re-surfaced the nan-018 ADRs (#4889 penalty config, #4890 hash manifest, #4892/#4893 corpus model) and the config-field-mismatch lessons (#4148/#4333), which corroborate the penalty-site correction: FR-03's prior `background.rs` claim was a doc-vs-code mismatch of the same class. Verified the real penalty-application sites directly against `services/search.rs:727/:729` and confirmed `background.rs:583` is a `tracing::error!` log line. No new patterns stored (spec decisions are feature-specific; read-only tier).

---

## Open Questions for Architect / Human

These do not block writing pseudocode but need an explicit architecture call:

1. **OQ-1/AC-09 cost-proxy formula (architect).** Token-weighted is mandated; the *exact* per-result token-proxy formula (e.g. chars/4 vs title+snippet length model) and its stated error bars are an in-design call (SR-02). The default and lean is the explicit token-weighted metric; precision+k is allowed only as a justified, non-deferral call.
2. **OQ-4 hash manifest column list (architect).** The exact set of entry columns / confidence dimensions that feed the retrieval-shape hash must be enumerated precisely (SR-01). Including too few gives false confidence; too many makes the guard noisy. The spec fixes the four input *categories* and branch (b); the architect fixes the precise field list and manifest version.
3. **FR-22 fail-vs-warn on hash mismatch — RESOLVED (human-ratified, LOCKED).** Corpus-dependent: **HARD ERROR (abort, non-zero exit) on the primary fixture corpus; WARN (continue) on the production snapshot corpus.** The durable yardstick's numbers propagate to product ranking policy, so the drift guard must protect primary-corpus validity by aborting — deliberately overriding the eval `report` exit-0 quality-verdict convention for the primary corpus (the convention governs *quality* verdicts, not corpus *validity*). The snapshot is realism/ephemeral and re-snapshotted on drift, so warn-and-continue is correct there. See FR-22 and AC-08(b). No longer an open question.

   **R-04 — column-manifest completeness is a named human delivery gate (LOCKED).** The retrieval-shape-hash manifest's *completeness* (that no retrieval-relevant entry column was mis-classified as display-only and omitted) is certified by a **named human review at delivery**, NOT routine code review. A test (AC-08e) proves the hash is sensitive only to the *declared* manifest; a human must separately confirm the *declared* set is complete. The architect still fixes the precise column list (open question #2 below).
4. **Trust-assertion JSONL schema (architect).** Whether forbidden-set/rank-below assertions extend the existing `expected` field shape or add a new sibling field on `ScenarioRecord`. Property-based `expected` (FR-16) and the trust class (FR-06/07) must share a coherent on-disk representation. Architect's call; flagged so pseudocode/tester align.
