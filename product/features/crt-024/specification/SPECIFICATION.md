# crt-024: Ranking Signal Fusion (WA-0) — Specification

## Objective

The search ranking pipeline currently applies ranking signals as sequential sort passes, allowing
later additive passes to override earlier semantic signals (NLI entailment) without a principled
relationship between them. This feature replaces Steps 7 and 8 of the search pipeline with a single
unified linear combination where all six ranking signals are normalized to [0, 1] and weighted
proportionally via config-driven weights. The result is a formula that serves as W3-1's learnable
feature vector interface, where every signal that influences ranking is a named, weighted, tunable
dimension.

---

## Functional Requirements

### FR-01: Fused Scoring Formula

The search pipeline must compute a single fused score for each candidate using the following
formula, applied in a single pass replacing the current Steps 7 and 8:

```
fused_score = w_sim  * similarity_score
            + w_nli  * nli_entailment_score
            + w_conf * confidence_score
            + w_coac * coac_norm
            + w_util * util_norm
            + w_prov * prov_norm

final_score = fused_score * status_penalty
```

Where:
- `similarity_score` — HNSW cosine similarity (bi-encoder recall); already in [0, 1]
- `nli_entailment_score` — cross-encoder entailment (NliScores.entailment, softmax); already in [0, 1]
- `confidence_score` — EntryRecord.confidence (Wilson score composite, f64); already in [0, 1]
- `coac_norm` — co-access affinity normalized to [0, 1]: `raw_boost / MAX_CO_ACCESS_BOOST`
- `util_norm` — utility delta normalized to [0, 1]; see FR-05
- `prov_norm` — provenance boost normalized to [0, 1]; see FR-06
- `status_penalty` — topology multiplier, not a signal term; applied to the fused score

This formula is the canonical six-term implementation formula. The product vision's four-term
illustrative formula (`w_sim*sim + w_nli*nli + w_conf*conf + w_coac*coac_norm`) is descriptive,
not exhaustive. The six-term formula is the specification target; SR-02 is resolved by this
explicit canonicalization.

### FR-02: Weight Configuration Fields

`InferenceConfig` in `unimatrix-server/src/infra/config.rs` must add six f64 fields with
`#[serde(default)]` and `#[serde(rename = "...")]` as appropriate:

| Field    | TOML key   | Description                         |
|----------|------------|-------------------------------------|
| `w_sim`  | `w_sim`    | Weight for bi-encoder similarity    |
| `w_nli`  | `w_nli`    | Weight for NLI entailment           |
| `w_conf` | `w_conf`   | Weight for confidence score         |
| `w_coac` | `w_coac`   | Weight for co-access affinity       |
| `w_util` | `w_util`   | Weight for utility delta            |
| `w_prov` | `w_prov`   | Weight for provenance boost         |

All six live under the `[inference]` section of `config.toml`. No new config sections are
introduced.

### FR-03: Weight Validation at Startup

`InferenceConfig::validate()` must enforce two classes of invariant at server startup:

1. **Individual range**: each weight must be in [0.0, 1.0]. A negative weight or weight > 1.0
   triggers a structured error naming the offending field.

2. **Sum constraint**: `w_sim + w_nli + w_conf + w_coac + w_util + w_prov` must be ≤ 1.0. If the
   sum exceeds 1.0, a structured error reports the sum and names all six fields. Valid sums < 1.0
   are explicitly allowed; the remaining headroom (e.g., 0.05) is reserved for WA-2's phase boost
   term and must not be consumed by implicit re-normalization.

Validation uses the same structured error pattern as the existing NLI config validation
(`InferenceConfig::validate()` with named field errors, not panics). The error message must include
the computed sum and all six field names so operators can diagnose which weights to reduce.

### FR-04: Co-Access Affinity Normalization

The co-access raw boost value returned by `compute_search_boost` (range [0, 0.03]) must be
normalized to [0, 1] before fusion:

```
coac_norm = raw_boost / MAX_CO_ACCESS_BOOST
```

`MAX_CO_ACCESS_BOOST` must be referenced from `unimatrix_engine::coaccess` — the constant must not
be duplicated in `search.rs`. `compute_search_boost` and `MAX_CO_ACCESS_BOOST` in the engine crate
are unchanged.

### FR-05: Utility Delta Normalization

`utility_delta` (from effectiveness classification) ranges over `[-UTILITY_PENALTY, +UTILITY_BOOST]`
and must be normalized to [0, 1] before fusion using shift-and-scale:

```
util_norm = (utility_delta + UTILITY_PENALTY) / (UTILITY_BOOST + UTILITY_PENALTY)
```

This maps the full range to [0, 1]: minimum (`-UTILITY_PENALTY`) → 0.0, neutral (0.0) → midpoint,
maximum (`+UTILITY_BOOST`) → 1.0. Division by `UTILITY_BOOST` alone produces [-1, 1], which would
allow `fused_score` to go below zero (violating NFR-02). Simple clamping after division loses the
penalty signal entirely. The shift-and-scale formula preserves both the positive and negative signal
within the fused score's range guarantee.

### FR-06: Provenance Boost Normalization

The provenance boost (applied for entries matching `boosted_categories`) must be normalized to
[0, 1] before fusion:

```
prov_norm = raw_provenance_boost / PROVENANCE_BOOST
```

Where `PROVENANCE_BOOST` is the existing provenance boost scalar constant. For entries that receive
no provenance boost, `prov_norm = 0.0`.

### FR-07: NLI Absence Degradation

When NLI is absent or disabled (`nli_enabled = false` or NLI model handle not ready), the `w_nli`
term contributes 0.0 to the fused score. The remaining five signal weights must be re-normalized
before fusion so that the fused score remains in [0, 1]:

```
denominator = w_sim + w_conf + w_coac + w_util + w_prov
w_sim_eff   = w_sim  / denominator
w_conf_eff  = w_conf / denominator
w_coac_eff  = w_coac / denominator
w_util_eff  = w_util / denominator
w_prov_eff  = w_prov / denominator
```

SR-03 is resolved: the denominator covers all five non-NLI weights, not a hardcoded three-term
subset. When all five non-NLI weights are zero (degenerate configuration), behavior is
implementation-defined but must not panic; returning 0.0 with a warning log is acceptable.

### FR-08: Single Pipeline Pass

The restructured search pipeline after crt-024 must follow this step order:

```
Step 0:  Rate check
Step 1:  Query validation
Step 2-4: Embed query
Step 5:  HNSW search (k = nli_top_k when NLI enabled, else params.k)
Step 6:  Fetch entries, quarantine filter
Step 6a: Status filter / penalty marking → produces penalty_map
Step 6b: Supersession candidate injection
Step 6c: Co-access boost map prefetch (spawn_blocking, completes before Step 7)
Step 7:  [NLI scoring if enabled] → fused score computation (single pass) → sort desc → truncate to k
Step 8:  Apply floors
Step 9:  Build ScoredEntry with final_score
Step 10: Audit
```

Steps 7 and 8 from the pre-crt-024 pipeline (separate NLI re-sort and co-access re-sort) are
replaced by the unified Step 7 above. There must be no secondary sort after Step 7's fused scoring.

The co-access boost map must be fully computed (spawn_blocking completes) before the single-pass
scorer iterates over candidates. This is a correctness constraint, not an optimization: candidates
scored without their boost would be incorrectly ranked. SR-07 is resolved by making Step 6c an
explicit prefetch step.

### FR-09: WA-2 Extension Contract

The fused scoring formula must be implemented in a way that allows WA-2 to add a seventh signal
term (`w_phase * phase_boost_norm`) without changing the formula's fundamental structure. The
architect must document the extension contract — either a variable-arity signal accumulator or a
documented "add one term, re-validate sum" pattern — in the ADR. SR-04 is addressed by making this
contract explicit before implementation begins.

### FR-10: ScoredEntry.final_score Semantics

`ScoredEntry.final_score` must reflect the fused formula score. The field name and MCP response
schema are unchanged. Existing tests that assert specific `final_score` values must be updated to
use the new formula's expected values. No test is deleted.

### FR-11: apply_nli_sort Disposition

The `apply_nli_sort` function (currently `pub(crate)` in `search.rs`) must either:

- (A) Be removed, with its test coverage migrated to the new single-pass fused scorer tests, or
- (B) Be retained as an internal helper called within the single-pass scorer.

The architect decides which option to pursue; this specification leaves option open. Whichever
option is chosen, there must be no gap in test coverage for NLI entailment scoring behavior.
`apply_nli_sort`'s crt-023 unit tests must not be deleted without replacement coverage.

### FR-12: rerank_score Retention

`rerank_score` in `unimatrix-engine/src/confidence.rs` must not be removed. It remains in use by
the fallback path and existing tests. It may serve as a building block called inside the fused
formula implementation (SR-09: avoids behavioral divergence from inline duplication).

### FR-14: EvalServiceLayer Config Wiring

`EvalServiceLayer` must construct `SearchService` by passing the full `InferenceConfig`
deserialized from the eval profile TOML — including all six fusion weight fields. It must not
construct `SearchService` with a default or hardcoded `InferenceConfig`.

This is the mechanism by which the `[inference]` section in profile TOMLs takes effect during eval
runs. Without this wiring, profile-level weight overrides are silently ignored and the eval harness
compares two identical runs regardless of how profiles differ.

**Boundary**: The previous eval harness documentation stated that `[inference]` is "accepted but
has no effect." That statement is superseded by crt-024. After this feature ships, `[inference]`
fusion weights are wired and profile overrides are honored. Any eval harness documentation
containing the "no effect" statement must be updated.

### FR-13: BriefingService Untouched

The `BriefingService` co-access boost path — which uses `MAX_BRIEFING_CO_ACCESS_BOOST = 0.01` —
must not be modified by this feature. Only `SearchService` receives the fused formula. The
different normalization constant (`0.01` vs `0.03`) would require reconciliation if briefing ever
adopts the fused formula; that is out of scope for crt-024.

---

## Non-Functional Requirements

### NFR-01: No Latency Regression

The single-pass fused scorer replaces two sequential sort passes (Steps 7 and 8). Total scoring
latency must not exceed the pre-crt-024 baseline for the same candidate set size at the same
`nli_top_k` value. The single pass is expected to reduce latency by eliminating the secondary sort.

### NFR-02: Score Range Guarantee

`fused_score` must be in [0.0, 1.0] by construction when:
- All weights are in [0.0, 1.0] individually, and
- The sum of applied weights is ≤ 1.0 (after NLI re-normalization if applicable), and
- All input signals are in [0.0, 1.0].

`final_score = fused_score * status_penalty` is in [0.0, 1.0] because `status_penalty` ∈ (0, 1].
No floor clamping to [0, 1] is required for the fused formula itself, but utility delta clamping
(FR-05) prevents negative intermediate values.

### NFR-03: Determinism

For identical inputs (same candidate entries, same query embedding, same NLI scores, same boost
map, same weights), the fused scorer must produce identical `final_score` values across invocations.
No randomness or time-dependent state may enter the scoring computation.

### NFR-04: No Engine Crate Changes

All changes are confined to `unimatrix-server/src/services/search.rs` and
`unimatrix-server/src/infra/config.rs`. `unimatrix-engine` (coaccess.rs, confidence.rs) is
read-only for this feature. No schema migration is required.

### NFR-05: Config Backward Compatibility

Operators who do not add the new weight fields to `config.toml` must get valid default behavior.
All six new fields have `#[serde(default)]` with default values that sum to ≤ 1.0 and produce
rankings consistent with pre-crt-024 behavior under NLI-disabled config (sim dominant, confidence
secondary). This is verified numerically by the architect in the ADR.

---

## Acceptance Criteria

### AC-01: Weight Fields in InferenceConfig

`InferenceConfig` in `infra/config.rs` adds six f64 fields: `w_sim`, `w_nli`, `w_conf`, `w_coac`,
`w_util`, `w_prov`, all with `#[serde(default)]`. Default values sum to ≤ 1.0 with at least 0.05
headroom reserved for WA-2's phase boost term.

**Verification**: Unit test reads a `config.toml` without the new fields; all six fields parse to
their defaults. Assert `w_sim + w_nli + w_conf + w_coac + w_util + w_prov <= 0.95`.

### AC-02: Weight Sum Validation — Rejection

`InferenceConfig::validate()` rejects configurations where `w_sim + w_nli + w_conf + w_coac + w_util + w_prov > 1.0`. The error message includes the computed sum and names all six fields.

**Verification**: Unit test constructs a config with sum = 1.05; `validate()` returns `Err` with
error text containing all six field names and the computed sum.

### AC-03: Individual Weight Range Validation

`InferenceConfig::validate()` rejects any weight < 0.0 or > 1.0 with a structured error naming
the offending field.

**Verification**: Six unit tests — one per field — each set one field to -0.01 and assert `Err`;
six more tests each set one field to 1.01 and assert `Err`.

### AC-04: Single Fused Scoring Pass

The search pipeline has no secondary sort after the fused score computation. The pipeline step
sequence matches FR-08 exactly: NLI scoring (if enabled) feeds directly into fused score
computation, followed by a single sort by `final_score` descending, followed by truncation.

**Verification**: Code review — no call to a sort or re-rank function after the fused scorer
returns. Integration test: 10 candidates scored; verify no secondary sort changes the order after
Step 7.

### AC-05: Fused Formula Six-Term Correctness

The fused score formula contains all six signal terms with independent weights. A unit test
constructs a candidate with known values for all six signals and known weights, computes the
expected `fused_score` by hand, and asserts the scorer returns that exact value (within f64
epsilon).

**Verification**: Unit test with controlled inputs: sim=0.8, nli=0.7, conf=0.6, coac_raw=0.015,
util_norm=0.5, prov_norm=1.0, weights=(0.30, 0.30, 0.15, 0.10, 0.05, 0.05), penalty=1.0.
Expected: `0.30*0.8 + 0.30*0.7 + 0.15*0.6 + 0.10*0.5 + 0.05*0.5 + 0.05*1.0 = 0.24+0.21+0.09+0.05+0.025+0.05 = 0.665`.
Assert `|computed - 0.665| < 1e-9`.

### AC-06: NLI Absence Re-normalization — Five-Weight Denominator

When NLI is absent, the re-normalization denominator is `w_sim + w_conf + w_coac + w_util + w_prov`
(all five non-NLI weights). A unit test verifies the re-normalized weights sum to 1.0 and that the
resulting fused score equals the five-signal formula applied to the same inputs.

**Verification**: Set `w_nli = 0.0`, remaining five non-zero. Assert `sum(effective weights) == 1.0`
(within f64 epsilon). Assert fused score for two candidates differs in the expected direction based
on their non-NLI signals.

### AC-07: Co-Access Normalization — No Constant Duplication

The co-access normalization uses `MAX_CO_ACCESS_BOOST` from `unimatrix_engine::coaccess`.
`search.rs` must not define its own copy of this constant.

**Verification**: `grep` for `MAX_CO_ACCESS_BOOST` in `search.rs` — must appear only as an
imported reference, not a `const` definition. Unit test: raw boost = 0.03 → `coac_norm = 1.0`;
raw boost = 0.015 → `coac_norm = 0.5`; raw boost = 0.0 → `coac_norm = 0.0`.

### AC-08: Existing Tests Updated, None Deleted

All tests that previously asserted specific `final_score` values using the pre-crt-024 formula
are updated to the new formula's expected values. No test is deleted; only expected values change.
Test count after crt-024 must be ≥ test count before crt-024 (net increase due to new tests
for AC-01 through AC-13).

**Verification**: `git diff --stat` before/after shows no deleted test files. Test suite passes
with zero failures.

### AC-09: Status Penalty Preserved as Multiplier

`final_score = fused_score * status_penalty`. Penalty constants ORPHAN_PENALTY, CLEAN_REPLACEMENT_PENALTY,
DEPRECATED_PENALTY, SUPERSEDED_PENALTY are unchanged in value and application point.

**Verification**: Unit test: fused_score = 0.8, DEPRECATED_PENALTY = 0.7 → final_score = 0.56.
Assert penalty-constant values unchanged from pre-crt-024.

### AC-10: Utility Delta and Provenance in Fused Formula

`utility_delta` and provenance boost are included as `w_util * util_norm` and `w_prov * prov_norm`
terms in the fused formula. Neither is applied as an additive afterthought outside the formula.

**Verification**: Code review confirms no additive `+ utility_delta` or `+ prov_boost` outside the
fused score computation function. Unit test: entry with `util_norm=1.0` and `w_util=0.05` scores
0.05 higher than identical entry with `util_norm=0.0` (same other signals), holding all other
inputs equal.

### AC-11: NLI-High Beats Co-Access-High Regression Test

An entry with NLI entailment = 0.9 and zero co-access must rank above an entry with NLI entailment
= 0.3 and maximum co-access (raw_boost = 0.03), given equal similarity (0.8) and equal confidence
(0.65), using default weights.

- Entry A: sim=0.8, nli=0.9, conf=0.65, coac_raw=0.0,  util_norm=0.0, prov_norm=0.0
- Entry B: sim=0.8, nli=0.3, conf=0.65, coac_raw=0.03, util_norm=0.0, prov_norm=0.0

Assert `score(A) > score(B)` under default weights.

This test must pass with the crt-024 fused formula. A companion test documents the pre-crt-024
behavior (B could rank above A) via a comment explaining the formula that would have produced the
inversion; the companion test does not execute the old formula but records the defect proof.

**Verification**: Automated unit test asserts `score(A) > score(B)`. Test is marked with a comment
referencing AC-11 and GH issue for traceability.

### AC-12: All Weight Config Validation — Consistent Error Style

All weight validation errors from `InferenceConfig::validate()` use the same structured error
pattern as the existing NLI config validation — named field errors, no panics, no
`unwrap()`/`expect()` on validation results.

**Verification**: Code review confirms validation returns `Result<(), ConfigError>` (or equivalent)
rather than panicking. Integration test verifies server fails to start and logs a structured error
message when weights are invalid.

### AC-13: Valid Sum < 1.0 — No Spurious Re-normalization

When the configured weight sum is in (0.0, 1.0] (valid with headroom), the formula is applied as
configured. No re-normalization occurs for the NLI-enabled path. Re-normalization is strictly
limited to the NLI-absent path (FR-07).

**Verification**: Unit test: weights sum to 0.90 (valid). NLI enabled with a score of 0.5. Assert
`w_nli_eff = w_nli` (no modification). Assert fused score equals hand-computed value using the
configured weights directly.

### AC-14: BriefingService Unchanged

The `BriefingService` code path is not modified. `MAX_BRIEFING_CO_ACCESS_BOOST = 0.01` remains in
the briefing pipeline.

**Verification**: `git diff` for the briefing service source file shows no changes. Existing
briefing integration tests pass without modification.

### AC-15: EvalServiceLayer Passes InferenceConfig to SearchService

`EvalServiceLayer` constructs `SearchService` using the `InferenceConfig` deserialized from the
eval profile TOML. A profile with `w_sim=1.0` and all other weights set to `0.0` must produce
`final_score` values equal to `similarity_score * status_penalty` for all candidates (i.e., only
the similarity signal contributes). A profile with default weights must produce different scores
from the `w_sim=1.0` profile on any candidate with non-zero NLI, confidence, or co-access signals.

**Verification**: Unit test constructs `EvalServiceLayer` with a profile TOML containing
`w_sim=1.0, w_nli=0.0, w_conf=0.0, w_coac=0.0, w_util=0.0, w_prov=0.0`. Runs scoring on a
candidate with known signals. Asserts `final_score == similarity_score * status_penalty` within
f64 epsilon. A second test uses default weights and asserts a different (higher) score for the
same candidate when NLI or confidence signals are non-zero.

### AC-16: D1–D4 Eval Harness Run Completed Before Merge

The D1–D4 eval harness is run on the pre-crt024 snapshot (`/tmp/eval/pre-crt024-snap.db`, scenarios
at `/tmp/eval/pre-crt024-scenarios.jsonl`) comparing `old-behavior.toml` vs. `crt024-weights.toml`
profiles. Human reviews the generated report before the PR is marked ready to merge.

The review must classify any ranking changes as one of:
- **Intentional NLI-override correction**: an entry with low NLI entailment that previously ranked
  above a high-entailment entry due to co-access boost now ranks correctly below it. These are
  expected outcomes of the fix and must not be treated as regressions.
- **True regression**: a clearly correct result drops rank with no NLI-based explanation. Zero
  such regressions are acceptable.

**Verification**: Eval report file exists at `/tmp/eval/crt024-report.json`. Baseline log is
updated. PR description includes a summary of the report review outcome.

---

## Domain Models

### FusedSignals

A value object representing the six normalized input signals for a single candidate entry during
scoring. All fields are f64 in [0.0, 1.0].

```
FusedSignals {
    similarity:    f64,  // HNSW cosine (bi-encoder recall)
    nli_entailment: f64, // Cross-encoder entailment (0.0 when NLI absent)
    confidence:    f64,  // Wilson score composite
    coac_norm:     f64,  // co-access affinity / MAX_CO_ACCESS_BOOST
    util_norm:     f64,  // utility delta normalized and clamped to [0, 1]
    prov_norm:     f64,  // provenance boost normalized to [0, 1]
}
```

`FusedSignals` is the feature vector interface for W3-1. Each field is a learnable dimension.

### ScoreWeights

The config-driven weight tuple extracted from `InferenceConfig`. Used by the fused scorer.
All fields f64, individually in [0.0, 1.0], sum ≤ 1.0.

```
ScoreWeights {
    w_sim:  f64,
    w_nli:  f64,
    w_conf: f64,
    w_coac: f64,
    w_util: f64,
    w_prov: f64,
}
```

`ScoreWeights::effective(nli_available: bool)` returns an adjusted weight set: if `!nli_available`,
`w_nli` is set to 0.0 and the remaining five weights are re-normalized by dividing by their sum
(FR-07). The original configured weights are not mutated; `effective()` returns a derived value.

### BoostMap

The pre-fetched co-access boost values for a candidate set, keyed by entry UUID. Computed by
`compute_search_boost` via `spawn_blocking` before the fused scorer iterates. Consumed read-only
during the single scoring pass.

### StatusPenalty

A scalar multiplier in (0.0, 1.0] derived from an entry's lifecycle status. Not a signal term in
the fused formula; applied after fusion as `final_score = fused_score * status_penalty`.
Represents topology (deprecated/superseded) rather than relevance.

**Established constants** (unchanged):
- Active: 1.0
- Deprecated: DEPRECATED_PENALTY (≈ 0.7, from crt-010 ADR-005)
- Superseded (clean replacement): CLEAN_REPLACEMENT_PENALTY
- Superseded (orphan): ORPHAN_PENALTY

### NLI Absent vs. NLI Disabled

- **NLI absent**: model handle not ready (startup, loading). `nli_entailment = 0.0`, `w_nli` re-normalized away.
- **NLI disabled**: `nli_enabled = false` in config. Same scoring behavior as absent; the NLI scoring step is skipped entirely.
- These two conditions produce identical scoring behavior; the distinction is observable only in server logs.

---

## User Workflows

### Operator: Configuring Fusion Weights

1. Operator opens `config.toml` and locates the `[inference]` section.
2. Operator adds any subset of: `w_sim`, `w_nli`, `w_conf`, `w_coac`, `w_util`, `w_prov`.
3. Operator must ensure the sum of all six values (including any unset defaults) does not exceed 1.0.
4. Server startup calls `InferenceConfig::validate()`. If the sum exceeds 1.0, the server refuses
   to start and logs a structured error naming all six fields and the computed sum.
5. Operator adjusts values, restarts. No migration tooling provided; this is a manual process.

**Config comment guidance** (must appear in any config.toml template or documentation):

```toml
[inference]
# Ranking signal fusion weights — must sum to <= 1.0.
# Leave headroom (recommended: >= 0.05) for WA-2 phase boost term.
# w_sim + w_nli + w_conf + w_coac + w_util + w_prov <= 1.0
w_sim  = <default>  # bi-encoder recall anchor
w_nli  = <default>  # cross-encoder entailment (dominant precision signal)
w_conf = <default>  # historical reliability tiebreaker
w_coac = <default>  # usage pattern (lagging signal)
w_util = <default>  # effectiveness classification
w_prov = <default>  # category provenance hint (weakest signal)
```

### Agent: context_search with NLI Active

1. Agent calls `context_search` with a natural language query.
2. `SearchService` embeds query, runs HNSW for `nli_top_k` candidates.
3. Quarantine, status, and supersession filters applied; `penalty_map` built.
4. Co-access `BoostMap` prefetched (Step 6c) — completes before scoring.
5. NLI batch scores all candidates.
6. Single fused scoring pass: for each candidate, compute `FusedSignals`, apply `ScoreWeights`,
   multiply by `StatusPenalty`. Sort descending by `final_score`.
7. Truncate to `k`, apply floors, build `ScoredEntry` with `final_score`.
8. Return ranked results to agent.

### Agent: context_search with NLI Absent

Steps 1-4 identical. Step 5 skipped. Step 6: fused scoring uses `ScoreWeights::effective(nli_available: false)` — `w_nli = 0.0`, five remaining weights re-normalized. Semantics of result: similarity-dominant ranking consistent with pre-crt-024 fallback behavior.

---

## Constraints

1. **Implementation surface**: `unimatrix-server/src/services/search.rs` (pipeline logic) and
   `unimatrix-server/src/infra/config.rs` (weight fields + validation). No other files changed
   except test files and any config template.

2. **Engine crates read-only**: `unimatrix-engine/src/coaccess.rs` and
   `unimatrix-engine/src/confidence.rs` are not modified. `MAX_CO_ACCESS_BOOST` is imported, not
   copied. `rerank_score` is not deleted.

3. **No schema migration**: scoring formula change only; no DB tables, columns, or indexes changed.
   Schema version is not incremented.

4. **NLI absence never errors callers**: degradation path (FR-07) is silent to MCP callers. NLI
   absence is logged at debug/info level but does not surface as an MCP error response.

5. **Status penalty constants unchanged**: ORPHAN_PENALTY, CLEAN_REPLACEMENT_PENALTY,
   DEPRECATED_PENALTY, SUPERSEDED_PENALTY values and application semantics are preserved from
   crt-010/crt-013.

6. **Briefing pipeline untouched**: `BriefingService` code, `MAX_BRIEFING_CO_ACCESS_BOOST`, and
   all briefing integration tests are unchanged.

7. **Eval harness gate required before merge** (supersedes prior "no eval gate" statement): D1–D4
   eval harness run on the pre-crt024 snapshot is required before the PR is merged (AC-16).
   `EvalServiceLayer` must wire `InferenceConfig` fusion weights to `SearchService` (AC-15, FR-14)
   so that profile TOMLs control scoring behavior. Pre-implementation snapshot:
   `/tmp/eval/pre-crt024-snap.db`; scenarios: `/tmp/eval/pre-crt024-scenarios.jsonl`.

8. **config.toml migration is manual**: operators update `config.toml` by hand. No migration
   assistant or automatic config conversion tool is provided.

---

## Dependencies

### Internal Crates

| Crate | Dependency Type | Notes |
|-------|----------------|-------|
| `unimatrix-engine` | Read-only import | `MAX_CO_ACCESS_BOOST`, `compute_search_boost`, `rerank_score` |
| `unimatrix-server` | Modified | `search.rs` (pipeline), `config.rs` (InferenceConfig) |

### Existing Components Consumed

| Component | Location | Role |
|-----------|----------|------|
| `compute_search_boost` | `engine::coaccess` | Produces raw boost for co-access normalization |
| `MAX_CO_ACCESS_BOOST` | `engine::coaccess` | Normalization denominator (must not be duplicated) |
| `rerank_score` | `engine::confidence` | May be used as building block inside fused scorer |
| `InferenceConfig::validate()` | `server::infra::config` | Extended with weight validation |
| `NliServiceHandle` | `server::services::search` | Provides NLI scores; absent case drives re-normalization |
| `penalty_map` | `search.rs Step 6a` | Source of `StatusPenalty` per entry |
| `boost_map` | `search.rs Step 6c` | Pre-fetched before scoring pass |

### External / Runtime

| Dependency | Notes |
|------------|-------|
| `tokio::task::spawn_blocking` | Existing pattern for boost_map prefetch (Step 6c); no new async dependency |
| `rusqlite` | No changes; scoring is in-memory after Step 6 |

---

## NOT in Scope

The following items are explicitly excluded. Any implementation touching these areas is a scope
variance requiring design leader approval:

- **WA-1** (Phase Signal + FEATURE_ENTRIES tagging): `current_phase` in `SessionState`, FEATURE_ENTRIES schema changes.
- **WA-2** (Session Context Enrichment): `w_phase * phase_boost_norm` term, `category_counts` histogram, affinity boost formula.
- **WA-3** (MissedRetrieval Signal): post-store signal collection.
- **WA-4** (Proactive Delivery): `context_briefing` injection pipeline changes.
- **W3-1** (GNN training): training the GNN on the weight baseline established here.
- **GH #329** re-implementation: crt-024 supersedes the targeted co-access patch; do not re-apply or reference its specific implementation if it was not merged.
- **Changing NLI model or NLI post-store detection**: crt-023 pipeline is unchanged except that `apply_nli_sort`'s output is consumed by the fused scorer rather than being used as the final sort.
- **Changing GRAPH_EDGES schema**: no schema change of any kind.
- **Config migration tooling**: operators migrate manually.
- **MCP response schema changes**: `ScoredEntry` field names and shape are unchanged.
- **Eval harness structural changes**: no new eval pipeline components. However, `EvalServiceLayer`
  wiring (FR-14) and the D1–D4 gate run (AC-16) are in scope.
- **Briefing service**: any modification to `BriefingService` or `MAX_BRIEFING_CO_ACCESS_BOOST`.

---

## Open Questions for Architect

**OQ-01 (Default Weights — SR-01, SR-02)**: What are the six default weight values? The architect
must determine these from signal-role reasoning and verify numerically that:
- Under default weights with NLI enabled, a high-NLI entry beats a high-co-access entry (AC-11).
- Under default weights with NLI disabled (re-normalized), sim-dominant ranking is preserved
  (Constraint 9 from SCOPE.md: consistent with pre-crt-024 behavior).
- Defaults sum to ≤ 0.95 (leaving ≥ 0.05 for WA-2).
Document all six values with justification in the ADR. These are W3-1's cold-start initialization.

**OQ-02 (apply_nli_sort Disposition — FR-11, SR-05)**: Retain as an internal helper called within
the single-pass scorer, or remove with test coverage migrated? Must be decided before implementation
begins to avoid a coverage gap.

**OQ-03 (utility_delta Negative Range — FR-05)**: Can `utility_delta` be negative (penalty
application)? If yes, normalization by `UTILITY_BOOST` produces a negative `util_norm`. The
architect must decide: clamp to [0, 1] before fusion (loses the penalty signal but preserves score
range), or model the penalty as a separate signal with its own weight, or apply it as a multiplier
analog to `status_penalty`. This decision must be made before implementation of FR-05.

**OQ-04 (WA-2 Extension Contract — FR-09, SR-04, SR-06)**: Should the formula be implemented as
a variable-arity signal accumulator (struct with a `Vec<(f64, f64)>` of (weight, signal) pairs) or
as a fixed six-field computation with a documented "add one field, re-validate sum" extension
pattern? The variable-arity approach makes WA-2 an additive change; the fixed-field approach makes
WA-2 require adding a new field and updating validation. SR-06 (operator weight sum exceeding 1.0
after WA-2) is mitigated by either approach if weight validation is re-run at startup.

**OQ-05 (boost_map Prefetch Sequencing — FR-08, SR-07)**: Confirm the exact async sequencing:
`spawn_blocking` for boost_map must complete and its result be available in the same task context
before the scoring iterator begins. Verify there is no race condition if the NLI batch scoring and
boost_map prefetch are interleaved.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for "ranking scoring pipeline inference weights similarity confidence normalization" -- Key findings: Entry #2964 (signal fusion pattern: sequential sort passes cause NLI override) directly describes the defect crt-024 fixes; Entry #2701 (ADR-002 crt-023) establishes NLI as primary sort signal and is preserved by crt-024; Entry #2298 (config key semantic divergence) informed SR-02 resolution and the explicit canonicalization of the six-term formula; Entry #751 (updating golden regression values procedure) applies to AC-08; Entry #179 (ADR-003 lambda dimension weighting) shows the re-normalization pattern for absent dimensions, analogous to FR-07; Entry #485 (ADR-005 status penalty multipliers) confirms penalty constants remain unchanged.
