# nan-018 Test Plan — OVERVIEW

**Feature**: nan-018 (GH #716) — Eval Harness Strategic Upgrade (the *instrument*).
**Mode**: risk-driven. Inputs: RISK-TEST-STRATEGY.md (R-01…R-18), ACCEPTANCE-MAP.md (AC-01…AC-14), ARCHITECTURE.md, SPECIFICATION.md, ADR-001…006.

This is the **measurement instrument**; everything downstream (ass-073 → ass-074 → crt-053) measures *with* it. The test lens is therefore weighted toward **silent-wrongness** (false confidence) over loud-failure: a wrong-but-confident instrument is worse than no instrument. Tests must prove the harness **measures**, not merely **executes**.

---

## 1. Test Strategy

| Layer | Where | What it proves |
|-------|-------|----------------|
| **Unit (engine)** | `unimatrix-engine/src/graph.rs` tests | Default-equivalence of `graph_penalty_with` vs `graph_penalty` vs const; clamp coupling; multiplier semantics. |
| **Unit (config)** | `infra/config.rs` tests | Dual-default triangulation (serde fn ↔ `Default` ↔ engine const); range validation. |
| **Unit (eval submodules)** | `eval/runner/trust.rs`, `eval/runner/cost.rs`, `eval/shape/`, `eval/corpus/` tests | Trust truth tables, token-proxy weighting/determinism, hash determinism/sensitivity, loader rejection, alias resolution. |
| **Integration (in-crate)** | `eval/runner/tests.rs`, `eval/report/tests*.rs`, new `eval/corpus` + `eval/shape` test mods | Trust+cost flow into `ProfileResult` → `find_regressions`; drift-guard fires on `eval run`; corpus loads through `EvalServiceLayer::from_profile`. |
| **Feature-level (proof-by-use)** | new `eval/corpus/tests_ac14.rs` (or equivalent) | AC-14 non-vacuous correlated sweep — the Wave-1 exit gate. |
| **Static audit** | new `eval/corpus/tests_audit.rs` | R-09 corpus audit; R-16 git-diff boundary gate; R-18 line-count/submodule guard. |
| **MCP integration harness** | `product/test/infra-001/suites/` | Server still boots, tools unchanged at default config — see §4. |

**Test conventions** (codified prior lessons):
- Arrange/Act/Assert; names `test_{fn}_{scenario}_{expected}`.
- **Assert the value, not just the path** (#3548): every assertion the risk register names must appear literally in the test body, not be implied by exercising the path. Gate 3c cross-checks assertion text against this plan.
- **Non-trivial round-trip values** (#3557): any new serde field is round-tripped with a non-null, non-default value; both serde directions (producer `output.rs`, consumer `types.rs`) tested independently.
- **No literal IDs in fixture assertions** (crt-013 #703): property assertions only — this is itself a test target (R-09).
- `#[tokio::test]` for async replay paths; deterministic only (no time/random dependence).

---

## 2. Risk → Test Mapping (master table)

| Risk | Pri | Component test plan | Headline test(s) | AC |
|------|-----|---------------------|------------------|----|
| **R-01** missed config site → default shift | Crit | engine-penalty, search-threading | default-equivalence (per-shape+clamp); enumerated-site grep guard; empty-TOML equivalence | AC-01 |
| **R-02** dual-default divergence | High | penalty-config | triangulation: `default_x()` == `Default::default().x` == engine const, 7 levers | AC-01 |
| **R-03** hash non-determinism | Crit | shape-hash | N≥100 in-process; permuted-input; cross-process; float-format | AC-08c, NFR-03 |
| **R-04** incomplete manifest → silent staleness | High | shape-hash | per-input sensitivity matrix (hash changes iff *declared* input changes); manifest-version bump; **+ named human gate (not a test)** | AC-08e/f |
| **R-05** embed-model-dependence regression | High | shape-hash | model-id/dimension sensitivity; live-source assertion (reads `EmbedModel`, not literal) | AC-08d |
| **R-06** mismatch path under-tested | Med | shape-hash | deliberate-mismatch fires; message names dimension; severity split primary=abort/snapshot=warn | AC-08b |
| **R-07** token-proxy infidelity | Med | cost-metric | same-k/different-load ⇒ different cost; monotonicity | AC-09 |
| **R-08** token-proxy non-determinism | Low | cost-metric | repeated `token_proxy` equal; summed cost stable | AC-09 |
| **R-09** assertions regress to literal/null | High | corpus-loader, corpus-fixtures | loader rejection (null + literal-ID); **static corpus audit: zero literal-ID, zero null** | AC-05 |
| **R-10** alias resolution breaks | High | corpus-loader | renumber-survival; missing-alias hard error; duplicate-alias reject | AC-05 |
| **R-11** vacuous-pass rank-below/redirect | High | trust-metric | full truth tables; **asymmetric B-absent ⇒ FAIL** explicitly | AC-02/03 |
| **R-12** trust not OR-folded | Med | report-extensions | trust-flip regression listed; no-flip→none; OR-composition with MRR | AC-02/03 |
| **R-13** multiplier precedence/shape-scaling | Med | engine-penalty, search-threading | shape-param exclusion; per-field override wins; `None`=no-op | AC-01 |
| **R-14** Wave-1/Wave-2 entanglement | Low | corpus-loader (OVERVIEW gate) | Wave-1 suite green with zero Wave-2 artifacts present | NFR-04 |
| **R-15** AC-14 passes trivially | High | corpus-fixtures (AC-14 plan §3) | non-vacuous sweep; each-shape-exercised; observable delta | **AC-14** |
| **R-16** boundary breach (protocol/gate) | High | corpus-loader (audit) | `git diff` zero `.claude/protocols/`; no gate-wiring | AC-13 |
| **R-17** exit-semantics regression | Low | report-extensions | exit code unchanged with trust/cost regressions present | AC-09 |
| **R-18** 500-line breach | Low | (audit) | line-count ≤500 on all touched/new files; submodule placement | NFR-05 |

Every SR-01…SR-08 (RISK-TEST-STRATEGY §Scope Risk Traceability) maps through these risks; none accepted without treatment.

---

## 3. AC-14 Proof-by-Use Plan (Wave-1 EXIT GATE — non-vacuous, R-15)

AC-14 is **not** satisfied by a sweep that merely runs. The feature-level test (planned in `corpus-fixtures.md` §AC-14) must assert **all five** conditions on the delivered fixture corpus. A sweep that executes but fails any one does NOT pass.

| # | Condition | Concrete assertion |
|---|-----------|--------------------|
| 1 | **Correlated four-family report** | One `eval run` over ≥2 steepness profiles produces a report section where, for the **same scenarios**, trust outcomes (AC-02/03) AND P@5/MRR (AC-04) AND token-weighted cost (AC-09) all appear. Assert all four families present in one correlated section. |
| 2 | **Non-vacuous trust** | Assert ≥1 trust assertion (forbidden-absent or rank-below) is **evaluated against a non-empty result set** — i.e. the anchor entries are present so the assertion is *meaningfully* checked, not vacuously satisfied (e.g. a rank-below where both A and B are in top-k). Inspect `TrustOutcome.violations`/evaluation count, not just `pass==true`. |
| 3 | **Each-shape-exercised** | Assert each of the 4 required shapes (multi-correction chain, dangling chain, superseded-Active, deprecated-connected) loads and yields ≥1 **evaluated** assertion. |
| 4 | **Observable lever delta** | The two profiles differ in one penalty lever; assert a **non-zero** penalty/ranking delta between them in the report (lever proven live, not inert). |
| 5 | **Bit-for-bit baseline on a guarded corpus** | The swept **baseline** profile (default penalties) reproduces current behavior **bit-for-bit** (R-01 default-equivalence green) so the delta is attributable to the lever; the corpus is guarded by a **deterministic, actually-firing** drift guard (R-03 + R-06 green). |

**Proof-by-use is proof the instrument MEASURES a moving trust signal, not that the harness runs.** Condition 2 is the load-bearing assertion — it is where the R-15 trivial-pass trap is closed.

### The three non-negotiable Wave-1 backstop tests (gate Wave-1 exit; may NOT be deferred)

1. **R-09 corpus audit** (`corpus-fixtures.md`) — static audit of the shipped primary corpus asserts **zero literal-ID and zero null `expected`**; every scenario uses only redirect-to-head / absence / rank-below property assertions.
2. **R-04 hash-sensitivity matrix** (`shape-hash.md`) — per-input test asserts the hash changes **iff a declared manifest input changes**; no sensitivity to display-only columns. (The named human column-manifest completeness review is a separate **delivery gate**, NOT a test — flagged in §5.)
3. **R-15 non-vacuous AC-14** (`corpus-fixtures.md` §AC-14) — the five conditions above, on a bit-for-bit baseline guarded by a live drift guard.

---

## 4. Integration Harness Plan (infra-001)

The infra-001 MCP harness exercises the compiled `unimatrix-server` binary through JSON-RPC. nan-018 is **internal eval tooling** (C-05, NFR-06): it adds **no MCP tool, no tool parameter, no client surface**. The additive `[graph_penalty]` config section and the eval submodules are not reachable through the MCP interface. Therefore:

### Suites to run (Stage 3c)

| Suite | Why | Expectation |
|-------|-----|-------------|
| **smoke** (`-m smoke`) | MANDATORY minimum gate — any change at all. | Green. Proves the binary still boots and the critical path per capability is intact after the config struct + search-path edits. |
| **protocol** | nan-018 touches `infra/config.rs` (`UnimatrixConfig`) and `services/search.rs` — server-tool-adjacent. | Green, unchanged. Handshake/discovery must not shift. |
| **tools** | `search.rs` penalty threading is on the live retrieval path; `with_rate_config` resolves the new field. Default-config behavior MUST be identical. | Green, unchanged — this is the **MCP-level proof of AC-01 bit-for-bit** at default config: search results through the tool surface are unchanged. |
| **lifecycle** | store→search persistence touches the same retrieval path; confirms no default-shift across a realistic flow. | Green, unchanged. |

### Gaps — new integration tests needed

**None in the infra-001 (MCP) harness.** All nan-018 behavior (penalty sweep, trust/cost metrics, corpus, drift guard) is visible **only through the `eval` CLI/in-crate path**, not the MCP interface. Per the "When NOT to plan integration tests" guidance, this is pure internal tooling with no MCP-visible effect — unit + in-crate integration tests suffice, and the MCP suites serve as a **regression backstop proving default config did not perturb the serving path**.

The eval-level "integration" (corpus → `EvalServiceLayer::from_profile` → replay → report) is covered by in-crate Rust integration tests, planned per-component (corpus-loader, report-extensions, corpus-fixtures/AC-14). These are cumulative extensions of the existing `eval/runner/tests.rs` and `eval/report/tests*.rs`, not isolated scaffolding.

### Failure triage (Stage 3c)

Any infra-001 failure is triaged per USAGE-PROTOCOL.md: (1) caused by nan-018 (a default-config search-path shift) ⇒ **fix the code** — this is the R-01 failure mode surfacing at MCP level, treat as a defect; (2) pre-existing/unrelated ⇒ GH Issue + `xfail`, do not fix in this PR; (3) bad assertion ⇒ fix the test. A default-config MCP search-result change is **category 1** and blocks.

---

## 5. Cross-Component Test Dependencies & Boundaries

The hardest tests live at boundaries (RISK-TEST-STRATEGY §Integration Risks):

- **engine ↔ config ↔ search (R-01/R-02)**: const (source of truth) → `GraphPenaltyParams::default()` → `GraphPenaltyConfig` serde defaults → `SearchService.graph_penalty_params` → `graph_penalty_with` call site. Triangulation + enumerated-site guard span `penalty-config.md`, `engine-penalty.md`, `search-threading.md` — they must agree on the same const values.
- **corpus loader ↔ replay (R-10/R-15)**: alias→id map produced at load, consumed by trust evaluation; invisible to replay (sees only ids). Renumber-survival test is the seam guard.
- **shape module ↔ embed crate (R-05)**: hash reads `EmbedModel::model_id()`/`dimension()` live; live-source assertion is the seam guard.
- **trust/cost ↔ report aggregation (R-12/R-17)**: `TrustOutcome` + `cost_tokens` flow into `find_regressions`; OR-composition + exit-code invariance are the fragile seams.
- **profile-iteration baseline (R-03 lineage, #2610)**: baseline (first profile) selection must sort keys — the sweep's correlated reporting must not depend on HashMap order.

### Out-of-band (NOT tests — flagged for the leader)

- **R-04 named human delivery gate** (ARCHITECTURE §7.3, LOCKED): a **named human reviewer** must certify the manifest's `entries` column list covers every retrieval/ranking-path column (no retrieval-relevant column mis-classified as display-only). No test can prove the *declared set is complete*; the R-04 sensitivity matrix only proves sensitivity to the declared set. This is a distinct **delivery gate**, separate from automated tests and routine code review.
- **NFR-08 cost-proxy error-bar doc** (R-07 item 3): the proxy formula + stated error bars documented in ADR-003 and the Band-2 config-knob reference — a doc-review checklist item (Wave-2 for the Band-2 doc; the ADR statement is Wave-1).

---

## 6. Wave Independence (NFR-04, R-14)

Wave-1 acceptance tests (AC-01…AC-09 + AC-14) MUST pass with **zero Wave-2 artifacts present** — no Band-2 docs, no recommendation doc, no `convention`/`procedure` entries. A Wave-2 artifact that becomes a Wave-1 code dependency is a defect. Tested by the Wave-1-alone scenario (corpus-loader plan): the Wave-1 suite is run with `docs/` Band-2 guides and `RECOMMENDATION-band3-protocol.md` absent.

---

## Knowledge Stewardship

**Queried** (consulted before authoring these Stage-3a test plans; each shaped the risk→test mapping):
- **#4895 / #4898** (nan-018 ADRs) — anchored the test plans to the locked design decisions: ADR-001's penalty-config threading (R-01/R-02/R-13 triangulation), ADR-002's embed-in-hash + ordered manifest (R-03/R-04/R-05 shape-hash plan), ADR-003's token-weighted cost (R-07/R-08 cost-metric plan), ADR-004's three `Assertion` variants (R-11 trust truth tables), ADR-005/006's recommendation-only / eval-only boundary (R-14/R-16 wave-independence + boundary audit).
- **#3557** (eval-harness dual-direction serde pattern) — drove the "non-trivial round-trip, both serde directions tested independently" convention in §1 (producer `output.rs` ↔ consumer `types.rs`), applied to every new `ProfileResult` field (`cost_tokens`, `trust`).
- **#3548** ("test exists but omits the plan's assertion" coverage-gap lesson) — drove the §1 **assert-the-value-not-the-path** convention and the Gate-3c cross-check that each risk-named assertion appears literally in the test body; directly closes the R-11 vacuous-pass and R-15 trivial-AC-14 traps.
- **#3526** (dual-type-copy JSON boundary) — informed the R-02 dual-default triangulation (serde `default_x()` ↔ `Default` ↔ engine const must agree across the type-copy boundary) and the empty-TOML byte-identity check.
- **#4070** (multi-site bit-for-bit) — informed the enumerated-site grep guard and per-shape default-equivalence in R-01/R-13 (two threading sites, background.rs excluded), and the AC-14 condition-5 bit-for-bit baseline.
- **#2610** (eval hash determinism) — informed the R-03 shape-hash determinism battery (N≥100 in-process, permuted-input, cross-process, float-format) and the profile-iteration baseline key-sort dependency in §5 (no HashMap-order reliance).

**Stored**: nothing novel to store — every pattern applied here is a single-feature instance of an already-recorded entry (#3557 serde round-trip, #3548 assert-the-value, #4070 multi-site bit-for-bit, #2610 hash determinism). The one candidate cross-feature abstraction — an **"instrument-measures-not-executes"** test lens (weighting silent-wrongness over loud-failure for measurement tooling; §intro + AC-14 condition 2) — is currently a one-instance observation. Per the dedup rule, hold and reassess at retro: if ass-073/crt-053 reuse the same lens, promote it to a stored pattern then.
