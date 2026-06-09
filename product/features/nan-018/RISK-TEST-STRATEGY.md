# Risk-Based Test Strategy: nan-018

**Feature**: nan-018 (GH #716) — Eval Harness Strategic Upgrade (the *instrument*).
**Mode**: architecture-risk. Inputs: SCOPE.md, SCOPE-RISK-ASSESSMENT.md (SR-01…SR-08), ARCHITECTURE.md, ADR-001…005, SPECIFICATION.md (AC-01…14, FR-01…31, NFR-01…08).
**Framing**: This is the measurement instrument; everything downstream (rewritten ass-073 → ass-074 → crt-053) measures *with* it. A wrong-but-confident instrument is worse than no instrument — it produces plausible numbers that mislead downstream decisions. The risk lens here is therefore weighted toward **silent-wrongness** (false confidence) over loud-failure.

Historical evidence is cited as Unimatrix entry IDs (#NNNN).

---

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Missed config construction/forwarding site silently changes default penalty behavior (AC-01 bit-for-bit violated) | High | High | **Critical** |
| R-02 | Dual-default divergence: serde default fn and `Default` impl resolve to different values, silent TOML-deser bug | High | Med | **High** |
| R-03 | Retrieval-shape hash is non-deterministic across runs (map-order / float-format) → false drift alarms or silent pass | High | High | **Critical** |
| R-04 | Hash input manifest is incomplete → a shape-affecting schema change does NOT move the hash → durable yardstick silently goes stale | High | Med | **High** |
| R-05 | Embed-model-in-hash (branch b) regresses to embed-model-dependence if model-id/dim is dropped or mis-fed from `EmbedModel` | High | Low | **High** |
| R-06 | Deliberate-mismatch path under-tested: guard "passes" but never actually fires, or message names no dimension | Med | Med | Medium |
| R-07 | Token-proxy infidelity: crude proxy mis-ranks sets → downstream cost-of-noise conclusions are unsafe | Med | Med | Medium |
| R-08 | Token-proxy non-determinism / instability across runs makes cost deltas noise, not signal | Med | Low | Low |
| R-09 | Property assertions regress to literal-ID or null-`expected` self-consistency (the ASS-037/039 rework trap) | High | Med | **High** |
| R-10 | Alias→id resolution breaks on re-snapshot / renumber → assertions silently pass-vacuously or mis-resolve | High | Med | **High** |
| R-11 | `rank-below` / `redirect-to-head` vacuous-pass: an absent anchor satisfies the assertion that should have failed | High | Med | **High** |
| R-12 | Trust outcome not OR-folded into `find_regressions` correctly → trust flip is reported but not gated, or gated but mis-counted | Med | Med | Medium |
| R-13 | Multiplier overlay vs per-field precedence wrong; shape params (hop_decay, max_depth) accidentally scaled | Med | Med | Medium |
| R-14 | Wave-1/Wave-2 entanglement: a Wave-2 doc/recommendation artifact becomes a Wave-1 code dependency, blocking the AC-14 exit | Med | Low | Low |
| R-15 | AC-14 proof-by-use passes trivially (corpus too small/degenerate to exercise a trust shape) → instrument "runs" but doesn't "measure" | High | Med | **High** |
| R-16 | Boundary breach: a `.claude/protocols/` file edited or eval wired as a gate (AC-13 hard gate) | High | Low | **High** |
| R-17 | Cost/trust added to regression gate flips `eval report` exit semantics (body-only → non-zero) breaking existing CI/scripts | Med | Low | Low |
| R-18 | Files exceed 500-line workspace rule when trust/cost/corpus/shape bolt onto existing modules | Low | Med | Low |

---

## Risk-to-Scenario Mapping

### R-01: Missed config construction/forwarding site silently changes default penalty (Critical) — SR-08, AC-01, NFR-01
**Severity**: High **Likelihood**: High
**Impact**: Default deployed retrieval behavior changes without anyone noticing; ASS-037 authority violated; every downstream baseline is computed against a silently-shifted default. This class has failed Gate 3b on ≥3 prior features (#4044, #2730, #4013, #4070).

**Test Scenarios**:
1. **Default-equivalence test (the cheapest early signal)**: for every status-shape branch and the clamp, assert `graph_penalty_with(.., &GraphPenaltyParams::default()) == graph_penalty(..) ==` the named `const`. Run per-shape across the five fixtures.
2. **Enumerated-site assertion**: enumerate every penalty/clamp consumption site — the two penalty-application sites, both in `search.rs` (:727 fallback branch, :729 `graph_penalty`), plus `with_rate_config` resolution — and assert each reads threaded config, not the module const. (`background.rs:583` is a `tracing::error!` log string, NOT a penalty site — excluded.) Add a grep-style guard test mirroring the #4070 procedure: `grep 'graph_penalty\b'` references must all route through `graph_penalty_with` / the resolved field.
3. **Empty-TOML equivalence**: a profile TOML omitting `[graph_penalty]` produces byte-identical penalties to the pre-nan-018 binary (NFR-02).
4. **Existing-profile-tests-green**: the full existing eval profile test suite passes unchanged.

**Coverage Requirement**: Bit-for-bit default equivalence proven at (a) the engine fn level, (b) the config-resolution level, (c) the empty-TOML level, AND every enumerated consumption site asserted to read config. No site may be covered by inspection alone — each gets an assertion.

### R-02: Dual-default divergence (High) — SR-08, ADR-001 §dual-default
**Severity**: High **Likelihood**: Med
**Impact**: serde default fn says 0.75, `Default` impl says 0.70 → a TOML omitting the field deserializes to the wrong value with no compile error (#3817, #4064, #4070 — recurrent, gate-failing).

**Test Scenarios**:
1. For each of the 7 levers, assert `default_orphan() == GraphPenaltyConfig::default().orphan == GraphPenaltyParams::default().orphan ==` the engine const — a single test triangulating all three sites.
2. Confirm the engine `const`s remain the single source of truth (`impl Default` references them), so ordering-invariant tests in `graph_tests.rs`/`pipeline_retrieval.rs` stay valid against the same values.

**Coverage Requirement**: A triangulation test binding serde-default fn ↔ `Default` impl ↔ engine const for every lever. This is the #4070 "five-site atomic" lesson applied to `GraphPenaltyConfig`.

### R-03: Retrieval-shape hash non-determinism (Critical) — SR-01, AC-08, NFR-03
**Severity**: High **Likelihood**: High
**Impact**: If the hash varies run-to-run, the drift guard either cries wolf (every run fails) or — worse if seeded once and compared loosely — gives false confidence. Eval already burned on HashMap iteration-order non-determinism (#2610, nan-007). float formatting and map iteration are the two classic sources (#1099 preserve_order, #3752 sorted accumulation).

**Test Scenarios**:
1. **Stability test (NFR-03)**: compute the hash N≥100 times on the same schema; assert all identical.
2. **Map-order injection**: feed the edge-type taxonomy and confidence-dimension set in shuffled order; assert the hash is unchanged (proves the manifest serializer sorts/orders before hashing).
3. **Float-format test**: assert the embedding-dimension (`384`) and any f64 manifest member serialize via a fixed, locale-independent format (no `{}` Debug float drift).
4. **Cross-process test**: compute the hash in two separate process invocations; assert equality (catches `HashMap` seed randomization, the #2610 failure mode).

**Coverage Requirement**: Hash determinism proven across (a) repeated in-process runs, (b) deliberately permuted input ordering, (c) separate processes. The manifest serialization must be the ordered, versioned form ADR-002 specifies — tested, not assumed.

### R-04: Incomplete hash manifest → silent staleness (High) — SR-01, AC-08, OQ-4
**Severity**: High **Likelihood**: Med
**Impact**: The hash is the *single* definition of "shape" for both the mechanical guard AND the OQ-5 protocol trigger (unified by design). If a retrieval-relevant column is omitted from the manifest, a schema change to it does not move the hash, the guard stays green, and the corpus silently rots — the exact failure the guard exists to prevent. This is more dangerous than R-03 because it fails *silent*, not loud.

**Test Scenarios**:
1. **Per-input sensitivity test**: for each enumerated manifest input (each entry column in the list, each edge-type-set change, each confidence dimension, embedding dim, model-id), mutate it and assert the hash *changes*. A coverage matrix: one assertion per enumerated input proves completeness against the *declared* manifest.
2. **Manifest-version test**: changing the manifest version integer changes the hash (the hash definition is itself migratable, ADR-002).
3. **Negative-completeness review gate**: a documented checklist mapping every retrieval/penalty-feeding column in the entry schema to its presence/absence in the manifest, with absences justified (e.g. display-only columns excluded by design). This is a review artifact, not a runtime test — completeness against the *real* schema cannot be proven by a test alone.

**Coverage Requirement**: Every declared manifest input has a sensitivity test proving it moves the hash. Completeness against the actual schema is covered by a documented column-mapping review (the residual that no test can close — flagged for human attention).

### R-05: Branch-(b) regression to embed-model-dependence (High) — SR-03, OQ-3, AC-08, FR-23
**Severity**: High **Likelihood**: Low
**Impact**: The binding constraint is "the durable reference must not silently become embed-model-dependent." Branch (b) holds *only if* `EmbedModel::model_id()` and `dimension()` genuinely feed the hash. If they are read but not fed, or fed as a constant literal instead of from the live model, embed-at-load fixtures drift with ONNX model swaps and produce fake MRR deltas (#4085 — exactly this class: eval ground truth drifting with the underlying state, observed -0.0143 MRR with zero scoring changes).

**Test Scenarios**:
1. **Model-id sensitivity**: mutate the value returned by the embedding-identity source (`model_id` / `dimension`) and assert the hash changes (subsumes R-04 item 1 for input 4, called out separately because it is the binding constraint).
2. **Source-of-truth test**: assert the hash reads from `EmbedModel::model_id()`/`dimension()` (or `InferenceConfig.embedding_model_sha256`) — not a hardcoded `"all-MiniLM-L6-v2"`/`384` literal in the shape module.
3. **Branch-statement assertion**: the spec/ADR states branch (b); a doc-review confirms no frozen-sidecar code path was added (and if a later reversal to branch (a) occurs, it is stated explicitly — FR durability).

**Coverage Requirement**: Embed-model identity proven to move the hash AND proven to be sourced live from the embed crate, not literal-embedded. This collapses the two staleness axes (schema + embed-model) into the one guard, as designed.

### R-06: Deliberate-mismatch path under-tested (Medium) — AC-08(b), FR-22
**Severity**: Med **Likelihood**: Med
**Impact**: The guard may compile and pass on matching hashes yet never actually fire on mismatch, or fire with an unactionable message. A guard that doesn't fire is no guard.

**Test Scenarios**:
1. **Deliberate-hash-mismatch test (AC-08b)**: stamp a corpus, then mutate one manifest input (or corrupt the stamp), run `eval run` → assert the guard triggers the chosen severity (fail-loud on primary corpus per ADR-002/OQ#2).
2. **Message-quality test**: assert the failure message names *which* shape dimension diverged (FR-22 "names what shape dimension diverged for human triage").
3. **Severity-split test**: assert primary corpus → hard error (abort), snapshot → warn-and-continue (the two-tier behavior in §3.6).

**Coverage Requirement**: The mismatch path is exercised end-to-end with severity and message assertions for both corpus tiers.

### R-07: Token-proxy infidelity (Medium) — SR-02, AC-09, NFR-08
**Severity**: Med **Likelihood**: Med
**Impact**: A crude proxy (e.g. `chars/4`) can mis-rank two result sets, so downstream cost-of-noise findings (rewritten ass-073) draw wrong conclusions. The proxy is not the real tokenizer; the danger is treating its numbers as ground truth.

**Test Scenarios**:
1. **Differential-token test (AC-09)**: two result sets with the **same k** but different per-result token loads (50-token vs 500-token snippets) yield **different** cost — proves cost is token-weighted, not k-weighted.
2. **Monotonicity test**: a strictly longer-text set costs strictly more (the proxy is order-preserving on length).
3. **Error-bar documentation gate (NFR-08)**: the proxy formula and its stated error characteristics are documented in the ADR and Band-2 config-knob reference, labeled explicitly as a proxy. Doc-review checklist item.

**Coverage Requirement**: Token-weighting (not count-weighting) proven by the same-k/different-load test; proxy error bars documented so downstream reads numbers with correct fidelity. Note: absolute fidelity to the real tokenizer is *out of scope* — what must be tested is that the proxy is token-weighted, monotonic, and honestly labeled.

### R-08: Token-proxy non-determinism (Low) — SR-02, AC-09
**Severity**: Med **Likelihood**: Low
**Impact**: If `token_proxy` is non-deterministic, cost deltas between profiles are noise.
**Test Scenarios**: 1. Compute `token_proxy(result)` repeatedly on the same entry; assert equality. 2. Cost over a fixed result set is identical across runs.
**Coverage Requirement**: Determinism of the per-result proxy and the summed cost.

### R-09: Property assertions regress to literal-ID / null-`expected` (High) — SR-07, AC-05, C-04, FR-16
**Severity**: High **Likelihood**: Med
**Impact**: The durability bet *is* property-based ground truth. If a fixture author can sneak in a literal-ID or null `expected`, the corpus reverts to the exact form that burned ASS-037/ASS-039 (#4085 snapshot-pinned drift; ASS-039 null-`expected` self-consistency rework). The corpus stops being durable.

**Test Scenarios**:
1. **Loader-rejection test (AC-05)**: a fixture with null `expected` is rejected (or unrepresentable in the format); a fixture with a literal-ID `expected` in the primary corpus is rejected.
2. **Primary-corpus audit test**: scan the shipped fixture corpus; assert *every* scenario uses only `redirect-to-head` / `absence` / `rank-below` property types — zero literal-ID, zero null.
3. **Property-resolution test**: each property type resolves its relationship anchor (chain head, weakest active) against the loaded graph and evaluates correctly.

**Coverage Requirement**: The primary corpus is statically audited to contain only property assertions, and the loader actively rejects the two forbidden forms. This is the codified form of crt-013 #703 "assert outcomes, never constants."

### R-10: Alias→id resolution breaks on re-snapshot (High) — SR-07, FR-17, §3.4
**Severity**: High **Likelihood**: Med
**Impact**: `EntryRef = String` aliases are resolved to ids at load. If resolution is order-dependent, or an alias is missing/duplicated, an assertion may resolve to the wrong entry or to nothing — and a wrong resolution can pass falsely.

**Test Scenarios**:
1. **Renumber-survival test**: load the same fixture twice with deliberately different id assignment; assert every alias-based assertion resolves to the same logical entry and yields the same pass/fail verdict (the whole point of alias indirection).
2. **Missing-alias test**: an assertion referencing an undefined alias is a hard load error, never a silent vacuous pass.
3. **Duplicate-alias test**: a fixture defining the same alias twice is rejected.

**Coverage Requirement**: Alias resolution proven stable across renumbering and loud on missing/duplicate aliases — no path where mis-resolution degrades to a silent pass.

### R-11: Vacuous-pass in rank-below / redirect-to-head (High) — SR-07, AC-03, §Domain Models
**Severity**: High **Likelihood**: Med
**Impact**: The spec's operational semantics are asymmetric and easy to invert: for `rank-below(A,B)`, **A absent ⇒ pass** (vacuous, correct) but **B absent ⇒ fail** (the entry that *should* rank higher is missing). A naive implementation that treats "either absent ⇒ pass" inverts the B-absent case into a false pass — the single most likely correctness bug in the trust class.

**Test Scenarios**:
1. **rank-below truth table (AC-03)**: explicit cases — present/present with rank(A)>rank(B) ⇒ pass; present/present with rank(A)<rank(B) ⇒ fail; A-absent ⇒ pass; **B-absent ⇒ fail**; both-absent ⇒ (per spec, A-absent dominates ⇒ pass — assert the chosen rule and document it).
2. **redirect-to-head**: head present in top-k AND every present superseded member ranks strictly below head ⇒ pass; head absent ⇒ fail; superseded member outranks head ⇒ fail.
3. **absence**: forbidden ∩ top_k == ∅ ⇒ pass; any forbidden in top_k ⇒ fail.

**Coverage Requirement**: A complete truth table per property type, with the asymmetric absent-cases explicitly asserted. No property type may be tested only on its happy path.

### R-12: Trust regression not OR-folded correctly (Medium) — FR-09, AC-02/03
**Severity**: Med **Likelihood**: Med
**Impact**: Trust flip (pass→fail vs baseline) must register in `find_regressions` with the same OR-extended semantics as MRR/P@K. If mis-wired, a trust regression is reported in the body but not counted (or vice versa), and the gate gives a wrong verdict.

**Test Scenarios**:
1. **Trust-flip regression test (AC-02/03)**: baseline satisfies a forbidden-set/rank-below assertion, candidate violates it → assert it appears in the Section 5 regression list with the existing fail-in-body-only semantics.
2. **No-flip test**: both satisfy → no regression recorded.
3. **OR-composition test**: a candidate that holds trust but regresses MRR is still flagged (trust pass does not mask a relevance regression), and vice versa.

**Coverage Requirement**: Trust regression composed with the existing relevance-regression OR logic, proven not to mask or be masked by it, with exit-code semantics unchanged (see R-17).

### R-13: Multiplier overlay precedence / shape-param scaling (Medium) — OQ-2, FR-02, §3.1.5
**Severity**: Med **Likelihood**: Med
**Impact**: `multiplier` must scale only the *severity* penalties (orphan, clean, partial, dead_end, fallback), NOT the *shape* params (`hop_decay_factor`, `max_traversal_depth`). And per-field overrides must take precedence over the multiplier. Getting either wrong silently distorts every swept result.

**Test Scenarios**:
1. **Shape-param exclusion test**: set `multiplier=Some(m)`; assert `hop_decay` and `max_traversal_depth` are unchanged while the five severities scale per the ADR-001 transform.
2. **Precedence test**: set both `multiplier` and an explicit per-field override on the same lever; assert the per-field value wins.
3. **Multiplier=None test**: `None` ⇒ exactly the per-field (or default) values — no scaling (subsumes R-01 default-equivalence when all fields default).

**Coverage Requirement**: Multiplier semantics proven on which fields scale, the override-wins precedence, and the no-op `None` case.

### R-14: Wave-1/Wave-2 entanglement (Low) — SR-05, NFR-04
**Severity**: Med **Likelihood**: Low
**Impact**: If a Wave-2 doc/recommendation artifact becomes a Wave-1 code dependency, the AC-14 exit is blocked by non-load-bearing work — defeating the deliberate waving.
**Test Scenarios**: 1. **Wave-1-alone test (NFR-04)**: Wave-1 acceptance suite (AC-01…09 + AC-14) passes with zero Wave-2 artifacts present (no Band-2 docs, no recommendation doc, no convention/procedure entries). 2. Build/test Wave-1 with the `docs/` and recommendation paths absent.
**Coverage Requirement**: Wave-1 proven independently shippable with Wave-2 artifacts removed.

### R-15: AC-14 proof-by-use passes trivially (High) — SR-06, AC-14, FR-19
**Severity**: High **Likelihood**: Med
**Impact**: AC-14 is the Wave-1 *exit*. If the corpus is too small/degenerate, the sweep "runs end-to-end" and reports the four metric families — but no profile actually exercises a trust shape, so the instrument is proven to *execute* but not to *measure*. The downstream spikes then build on a sweep that never demonstrated a moving trust signal.

**Test Scenarios**:
1. **Non-degenerate sweep test (AC-14)**: the steepness sweep across ≥2 profiles produces, on the fixture corpus, a profile where a trust assertion's outcome is *meaningfully evaluated* (at least one forbidden-set or rank-below assertion is actually checked against a non-empty result set, not vacuously satisfied) AND P@5/MRR AND cost are all reported in one correlated section.
2. **Each-shape-exercised test (FR-19)**: assert each of the four required shapes (multi-correction chain, dangling chain, superseded-Active, deprecated-connected) is loaded and produces at least one evaluated assertion.
3. **Delta-observable test (FR-04)**: the two profiles differ in a penalty lever and the report shows a non-zero penalty/ranking delta between them — proving the lever is live, not inert.

**Coverage Requirement**: AC-14 passes only when (a) the sweep reports all four metric families correlated, (b) every required shape is exercised with a non-vacuous assertion, and (c) the swept lever produces an observable delta. A sweep over an inert/degenerate corpus does NOT satisfy the exit. **This is the gating scenario for Wave-1 exit.**

### R-16: Boundary breach — protocol edit or gate wiring (High) — SR-04, AC-13
**Severity**: High **Likelihood**: Low
**Impact**: AC-13 is a hard gate. Any edit under `.claude/protocols/` or any eval-as-standing-gate wiring violates an explicit non-goal and the recommendation-only Band-3 boundary.
**Test Scenarios**: 1. **git-diff gate (AC-12a/13)**: assert zero changes under `.claude/protocols/`. 2. **No-gate-wiring review**: assert no CI/PR hook makes eval *results* a standing decision gate (the one-time migration-validation run is allowed; a standing gate is not). 3. Assert the recommendation doc exists at `product/features/nan-018/RECOMMENDATION-band3-protocol.md` and is recommendation-only.
**Coverage Requirement**: A mechanical `git diff` assertion plus a review checklist for gate-wiring absence.

### R-17: Regression-gate exit-semantics regression (Low) — FR-09, §3.3
**Severity**: Med **Likelihood**: Low
**Impact**: Adding trust + cost to `find_regressions` must preserve the existing `eval report` exit-code convention (body-only, exits 0 on gate failures — #3524, #2610 lineage). A change to non-zero exit silently breaks any script/CI relying on exit 0.
**Test Scenarios**: 1. Assert `eval report` exit code is unchanged when trust/cost regressions are present (failures reported in body). 2. Existing report tests pass unchanged.
**Coverage Requirement**: Exit-code invariance proven with trust/cost regressions present.

### R-18: 500-line file rule breach (Low) — NFR-05
**Severity**: Low **Likelihood**: Med
**Impact**: Workspace rule forbids >500-line files; bolting trust/cost/corpus/shape onto existing eval modules risks breaching it.
**Test Scenarios**: 1. Line-count check on all touched/new files ≤ 500. 2. New capabilities live in their own submodules (`eval/corpus/`, `eval/shape/`, `runner/trust.rs`, `runner/cost.rs`) per §2.
**Coverage Requirement**: All files ≤ 500 lines; new code in dedicated submodules.

---

## Integration Risks

The hardest risks live at component boundaries:

- **Engine ↔ server config thread (R-01, R-02)**: `GraphPenaltyConfig` (server `infra/config.rs`) → `GraphPenaltyParams` (engine `graph.rs`) → `SearchService` field → `graph_penalty_with` call site. Three crate/module boundaries, each a place a default can diverge or a site can be missed. The #4070 "five-site atomic" lesson is the template; the new wrinkle is it now crosses the engine/server boundary (engine const is source-of-truth; server config mirrors it).
- **Corpus loader ↔ existing replay (R-10, R-15)**: the loader materializes a snapshot DB that `EvalServiceLayer::from_profile` consumes *unchanged*. The boundary risk is the alias→id map: it is produced at load and consumed by assertion evaluation; a mis-resolution here is invisible to the replay machinery, which sees only ids.
- **Shape module ↔ embed crate (R-05)**: the hash reads `EmbedModel::model_id()`/`dimension()` (read-only, `unimatrix-embed`). A literal-embedded value instead of a live read silently severs branch (b).
- **Trust/cost evaluators ↔ report aggregation (R-12, R-17)**: `TrustOutcome` and `cost_tokens` flow from `run_single_profile` into `find_regressions`. The OR-composition with existing MRR/P@K regression logic and the exit-code convention are the fragile seams.
- **Profile-iteration baseline selection (R-03 lineage)**: #2610 — HashMap profile iteration order is non-deterministic; baseline (first profile) selection must sort keys. The sweep's correlated reporting must not depend on map order.

## Edge Cases

- Empty result set: cost = 0; absence assertion trivially passes; rank-below with both absent (assert the chosen rule).
- k larger than corpus size: every entry returned; absence assertions become strict.
- A chain whose head is itself deprecated (dead-end chain, optional 5th shape): redirect-to-head has no valid head — assert defined behavior (fail, not panic).
- Multiplier with extreme values (m near 0, m large) driving a penalty outside the `[0.10, clamp-upper]` bounds — assert clamp still applies after scaling.
- `max_traversal_depth` set below the deepest fixture chain — assert defined truncation, not a panic.
- Manifest with an unknown/future manifest-version integer — assert a clear error, not a silent mis-hash.
- Two fixtures sharing an alias across files — assert global uniqueness or scoped resolution (R-10).

## Security Risks

Eval is internal Rust tooling (C-05, NFR-06); the attack surface is narrow but the corpus/profile loaders accept developer-authored files:

- **Untrusted input**: fixture corpus files (TOML/JSON under `eval/corpus/fixtures/`) and profile TOMLs. These are in-repo and developer-authored, not network-facing — blast radius is a local eval run, not the server.
- **Path handling**: the corpus loader materializes a snapshot SQLite DB + vector dir. Assert it writes only under a controlled temp/eval path; a fixture referencing an absolute or `../` path must not escape (path-traversal check on any author-supplied file reference).
- **Deserialization**: profile TOML deserializes as `UnimatrixConfig`; a malformed/oversized TOML should error cleanly, not panic or hang. The existing `eval/profile/validation.rs` path is reused — assert new `[graph_penalty]` fields are range-validated (NFR-class), not blindly trusted (an out-of-range penalty like 99.0 or NaN must be rejected or clamped, not silently used).
- **Blast radius**: a compromised/malformed fixture corrupts only that eval run's results; it cannot affect the deployed server (config exposure is additive and the eval binary is separate from the serving path). The one durable-state concern is the corpus's own shape stamp — a malicious stamp edit is caught by the deliberate-mismatch guard (R-06) only if the running schema differs; a stamp matching a deliberately-stale corpus is a trust-of-source issue, mitigated by version-controlling the corpus (AC-06).

## Failure Modes

How the system should behave when a risk materializes:

- **Hash mismatch on primary corpus** → fail-loud (hard error, abort run) naming the divergent shape dimension (ADR-002 / OQ#2). On snapshot → warn-and-continue.
- **Missing/duplicate alias** → hard load error, never a silent vacuous pass (R-10).
- **Null or literal-ID `expected` in primary corpus** → loader rejection (R-09).
- **Out-of-range penalty config value** → validation error at config load (reuse `validation.rs`), never silently applied.
- **Trust regression** → reported in report body, counted in Section 5, exit code unchanged (R-12, R-17).
- **Cost growth** → advisory report flag (ε threshold, default 0.0 = report-don't-block, OQ#1) — consistent with the existing human-reviewed gate.
- **redirect-to-head with no valid head** → defined failure, not panic.

## Scope Risk Traceability

| Scope Risk | Architecture Risk(s) | Resolution / Architecture-Level Treatment |
|-----------|---------------------|-------------------------------------------|
| **SR-01** (retrieval-shape hash linchpin; ordering/serialization fragility) | R-03, R-04, R-06 | ADR-002: ordered, versioned manifest with explicitly enumerated inputs. Treated by NFR-03 determinism test (R-03), per-input sensitivity matrix (R-04), deliberate-mismatch test (R-06). Residual: completeness against the *real* schema is a documented review, not a test (R-04 item 3). |
| **SR-02** (token-proxy fidelity unstated) | R-07, R-08 | ADR-003: explicit token-weighted `Σ token-proxy`, proxy definition + error bars documented (NFR-08). Tested by same-k/different-load (R-07), determinism (R-08). Absolute tokenizer fidelity explicitly out of scope; honest-labeling is the control. |
| **SR-03** (embed-model-drift durability trap) | R-05 | ADR-002 branch (b): embedding model-id + dimensionality are first-class hash inputs (FR-23). Tested by model-id sensitivity + live-source assertion (R-05). Collapses the 2nd staleness axis into the one guard — no frozen sidecar. Directly mitigates the #4085 fake-MRR-drift class. |
| **SR-04** (boundary breach: proxy-as-deferral / protocol edit) | R-16, R-07(R-14 doc) | Cost: ADR-003 mandates token-weighted as primary, narrowing only as justified in-design call, never deferral (FR-14). Protocol: ADR-005 recommendation-only; AC-13 hard `git-diff` gate (R-16). |
| **SR-05** (wide feature / wave entanglement) | R-14, R-15 | §3.7 + NFR-04: Wave-2 has zero code coupling to Wave-1. Tested by Wave-1-alone suite (R-14). AC-14 is the protected Wave-1 exit (R-15). |
| **SR-06** (trust-class generality ballooning Wave-1) | R-11, R-15 | ADR-004: `Assertion` enum is extensible but Wave-1 ships exactly two variants (forbidden-absent, rank-below) — FR-11. No speculative types. Truth-table coverage (R-11) constrains scope to the two built variants. |
| **SR-07** (property-assertion durability vs literal/null regress) | R-09, R-10, R-11 | ADR-004: alias-indirection + property types resolved at load (FR-16/17); loader rejects null/literal in primary corpus. Tested by loader-rejection + corpus audit (R-09), renumber-survival (R-10), property truth tables (R-11). Codifies crt-013 #703. |
| **SR-08** (multi-site config threading; bit-for-bit trap) | R-01, R-02, R-13 | ADR-001: engine const stays source-of-truth; `graph_penalty` becomes a thin wrapper over `graph_penalty_with(..default())`. Tested by default-equivalence + enumerated-site + empty-TOML (R-01), dual-default triangulation (R-02, the #4070 lesson), multiplier semantics (R-13). |

Every SR-01…SR-08 maps to ≥1 architecture risk with a concrete test treatment. None is accepted-without-treatment.

## Coverage Summary

| Priority | Risk Count | Risks | Required Scenarios |
|----------|-----------|-------|--------------------|
| **Critical** | 2 | R-01, R-03 | Default-equivalence + enumerated-site + empty-TOML (R-01, ~4 scenarios); hash determinism in-process/permuted/cross-process (R-03, ~4 scenarios) |
| **High** | 8 | R-02, R-04, R-05, R-09, R-10, R-11, R-15, R-16 | Dual-default triangulation; per-input hash sensitivity matrix; embed-model live-source; corpus audit + loader rejection; alias renumber-survival; property truth tables; non-degenerate AC-14 sweep; git-diff boundary gate (~18 scenarios) |
| **Medium** | 5 | R-06, R-07, R-08, R-12, R-13 | Deliberate-mismatch + message + severity-split; same-k/different-load cost; proxy determinism; trust-flip OR-composition; multiplier precedence/exclusion (~12 scenarios) |
| **Low** | 3 | R-14, R-17, R-18 | Wave-1-alone; exit-code invariance; line-count + submodule placement (~5 scenarios) |

---

## Wave-1 Exit Gate (AC-14) — the scenarios that gate Wave-1

Per the SCOPE delivery-sequencing guidance ("protect AC-14 as the Wave-1 exit"), Wave-1 may not close until **all** of the following pass on the delivered fixture corpus:

1. **R-15 non-degenerate sweep**: one `eval run` of a steepness sweep across ≥2 profiles reports — for the same scenarios, in one correlated section — **trust outcomes (AC-02/03) AND P@5/MRR (AC-04) AND token-weighted cost (AC-09)**, with at least one trust assertion **non-vacuously evaluated** against a non-empty result set.
2. **R-15 each-shape-exercised**: each of the four required shapes (multi-correction chain, dangling chain, superseded-Active, deprecated-connected) loads and yields ≥1 evaluated assertion.
3. **R-15 / R-13 delta-observable**: the two swept profiles differ in a penalty lever and the report shows a non-zero penalty/ranking delta — the lever is proven live, not inert.
4. **R-01 default-equivalence** green: the swept *baseline* profile (default penalties) reproduces current behavior bit-for-bit — so the sweep delta is attributable to the lever, not to a silently-shifted default.
5. **R-03 hash determinism** + **R-06 deliberate-mismatch** green: the corpus the sweep runs against is guarded by a deterministic, actually-firing drift guard — so the sweep measures a *valid* corpus.

A sweep that runs end-to-end but fails any of 1–5 (e.g. all assertions vacuous, baseline not bit-for-bit, corpus unguarded) does **not** satisfy the Wave-1 exit. Proof-by-use means proof the instrument *measures*, not merely *executes*.

---

## Knowledge Stewardship
- Queried: context_search for config multi-site threading (#3817, #4064, #4070, #4044, #2730 — the dual-site/five-site bit-for-bit trap → R-01/R-02), eval snapshot drift (#4085 fake-MRR-drift, #4886 spike-assumption staleness → R-05/R-09), hash determinism (#2610 HashMap profile-order non-determinism in eval, #1099 preserve_order, #3752 sorted accumulation → R-03), and risk/wave patterns (#3756, #4892 ADR-004 nan-018). Each cited entry directly informed the named risk.
- Stored: nothing novel via /uni-store-pattern. The risks here are feature-specific instances of already-recorded patterns (#4070 multi-site, #4085 eval drift, #2610 eval determinism). The candidate cross-feature pattern — "instrument-vs-experiment features must gate on proof-the-instrument-measures, not proof-it-executes" (the R-15 trivial-pass risk) — is observed in only this one feature so far; per stewardship rules (2+ features) I will reassess at retro rather than store a single-instance pattern now.
