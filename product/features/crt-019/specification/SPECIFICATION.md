# SPECIFICATION: crt-019 — Confidence Signal Activation

## Objective

The Unimatrix confidence formula produces a 0.1471-wide spread across the active entry population,
causing confidence to contribute at most 0.022 to search re-ranking scores — weaker than the
co-access boost alone. This feature replaces three fixed-constant components with signal-bearing
alternatives, wires the dormant helpful-vote infrastructure through both the scoring formula and
query skills, and makes the search blend weight self-adjusting so that improvements in formula
quality automatically translate into stronger result differentiation.

---

## Functional Requirements

### FR-01: Bayesian Beta-Binomial Helpfulness Scoring

Replace `helpfulness_score(helpful_count, unhelpful_count)` (Wilson score with 5-vote floor) with a
Bayesian Beta-Binomial posterior mean:

```
score = (helpful_count + α₀) / (total_votes + α₀ + β₀)
```

where `total_votes = helpful_count + unhelpful_count`.

- α₀ and β₀ are estimated via method of moments from the population of entries that have
  accumulated at least one vote (`helpful_count + unhelpful_count >= 1`).
- The population threshold for activating empirical prior estimation is **≥ 10 voted entries**.
  Below this threshold, the cold-start default α₀ = β₀ = 3 applies.
- Cold-start behavior: `score(0, 0, α₀=3, β₀=3) = 3/6 = 0.5` — identical to the current neutral
  prior.
- The function signature changes to accept `alpha_prior: f64` and `beta_prior: f64` as parameters;
  callers are responsible for supplying the current population prior.
- `MINIMUM_SAMPLE_SIZE` constant is removed. `WILSON_Z` and `wilson_lower_bound` are removed.
- The result is clamped to [0.0, 1.0].

Testable property: With two unhelpful votes and cold-start prior,
`score(0, 2, 3, 3) = 3/8 = 0.375 < 0.5`. With two helpful and two unhelpful votes,
`score(2, 2, 3, 3) = 5/10 = 0.5` — equal-signal votes return neutral, demonstrating immediate
responsiveness without requiring 5 votes.

### FR-02: Weight Rebalancing

Update the six stored weight constants in `crates/unimatrix-engine/src/confidence.rs`:

| Constant  | Before | After |
|-----------|--------|-------|
| W_BASE    | 0.18   | 0.16  |
| W_USAGE   | 0.14   | 0.16  |
| W_FRESH   | 0.18   | 0.18  |
| W_HELP    | 0.14   | 0.12  |
| W_CORR    | 0.14   | 0.14  |
| W_TRUST   | 0.14   | 0.16  |
| **Sum**   | **0.92** | **0.92** |

The sum-to-0.92 invariant must hold with exact f64 equality (asserted by
`weight_sum_invariant_f64` test).

T-REG-02 in `crates/unimatrix-engine/tests/pipeline_regression.rs` hard-asserts the current weight
constants and will fail by design when these change. It must be updated to the new values as the
**first step** of Change 2 implementation (see Constraints).

### FR-03: Trust-Source Differentiated base_score

Extend `base_score` to accept the entry's trust source, differentiating auto-extracted active
entries from human/agent/system active entries:

```
base_score(status: Status, trust_source: &str) -> f64
```

Mapping for `Status::Active`:

| trust_source | base_score |
|---|---|
| `"auto"` | 0.35 |
| any other value | 0.50 |

All non-Active statuses retain existing values (Proposed = 0.50, Deprecated = 0.20,
Quarantined = 0.10) regardless of trust_source.

The differentiation applies **only to `Status::Active`**. `Status::Proposed` entries with
`trust_source = "auto"` retain base_score = 0.50 (see Constraints — SR-04 resolution).

`compute_confidence` is updated to pass `&entry.trust_source` as the second argument. All
call sites (7 total: 1 production in `confidence.rs`, 6 in tests) are updated mechanically.

### FR-04: Adaptive Search Blend Weight

Replace the compile-time constant `SEARCH_SIMILARITY_WEIGHT = 0.85` with a runtime-computed
adaptive blend:

```
confidence_weight = clamp(observed_spread * 1.25, 0.15, 0.25)
similarity_weight = 1.0 - confidence_weight
```

where `observed_spread` is the p95 – p5 confidence spread of the active entry population,
computed during the confidence refresh tick and cached as a runtime value.

- At the pre-crt-019 spread of 0.1471: `confidence_weight ≈ 0.184`
- At target spread of 0.20: `confidence_weight = 0.25` (full activation)
- Floor 0.15 ensures no regression below current baseline
- Cap 0.25 prevents over-amplification

`rerank_score(similarity, confidence)` accepts `confidence_weight: f64` as a parameter (making it
a pure function — no shared state dependency). All call sites in `search.rs` supply the cached
runtime value. The architect's ARCHITECTURE.md governs the state management mechanism for
`observed_spread` (whether parameter-passed or accessed via an Arc/atomic — SR-03).

### FR-05: Refresh Batch Size Increase with Duration Guard

In `crates/unimatrix-server/src/infra/coherence.rs`:

- Increase `MAX_CONFIDENCE_REFRESH_BATCH` from 100 to 500.
- Add a `std::time::Instant` wall-clock guard at the top of each refresh loop iteration:
  if `start.elapsed() > Duration::from_millis(200)`, break early and log the partial count.
- The guard must be checked before each `update_confidence()` call, not after.
- `spawn_blocking` cancellation is not viable (tokio cannot cancel running blocking threads);
  the Instant-inside-loop pattern is the required implementation.

### FR-06: Deliberate Retrieval Signals — context_get

In the `context_get` MCP tool handler:

- When `params.helpful.is_none()`, set `helpful = Some(true)` before constructing `UsageContext`.
- This implicit vote is injected into the **existing** `record_access` call by setting
  `UsageContext.helpful = Some(true)` — no additional `spawn_blocking` task is created (SR-07).
- UsageDedup enforces one vote per agent-per-entry in-process; repeated `context_get` calls by
  the same agent produce at most one `helpful_count` increment.
- `params.helpful = Some(false)` (explicit negative vote) is honored as-is and overrides the
  implicit injection.

### FR-07: Deliberate Retrieval Signals — context_lookup

In the `context_lookup` MCP tool handler:

- Record a doubled `access_count` increment (×2) for each returned entry, without injecting a
  helpful vote.
- Implemented by passing a weight multiplier to the access recording path (no schema change).
- UsageDedup access filter applies before the multiplier: if the same agent already recorded
  access to an entry in this session, the lookup records 0 increments (not 2) for that entry
  (see Constraints — SR-05 resolution).

### FR-08: Wire Helpful Votes in Query Skills

Update two skill definition files:

- `.claude/skills/uni-knowledge-search/SKILL.md`: add `helpful: true` to the primary example
  invocation; add guidance explaining that `helpful: false` is appropriate when the retrieved
  entry was not applicable to the task.
- `.claude/skills/uni-knowledge-lookup/SKILL.md`: add `helpful: true` to the primary example
  invocation; add equivalent `helpful: false` guidance.

This is a documentation change only. No MCP parameter schema changes are required.

### FR-09: Empirical Bayesian Prior Computation

The confidence refresh tick must compute α₀ and β₀ from the voted-entry population:

- Collect all active entries where `helpful_count + unhelpful_count >= 1`.
- If the count of such entries is **< 10**, use cold-start defaults (α₀ = 3, β₀ = 3).
- Otherwise, estimate via method of moments:
  - Population mean `p̄ = sum(helpful_count) / sum(total_votes)` across voted entries
  - Population variance `v = sample_variance(per_entry_helpfulness_rates)`
  - `α₀ = p̄ * ((p̄ * (1 - p̄) / v) - 1)`; `β₀ = (1 - p̄) * ((p̄ * (1 - p̄) / v) - 1)`
  - Clamp α₀ and β₀ to `[0.5, 50.0]` to prevent degenerate estimates.
- α₀, β₀, and `observed_spread` are computed in the same refresh tick. Their atomic
  consistency boundary is defined by ARCHITECTURE.md (SR-02).

### FR-10: New Calibration Scenario

Add a test scenario `auto_vs_agent_spread` to `crates/unimatrix-engine/tests/pipeline_calibration.rs`
that verifies: an Active entry with `trust_source = "agent"` scores strictly higher than an
identically-signaled Active entry with `trust_source = "auto"`, given the same access_count,
freshness, helpful/unhelpful counts, and correction_count.

---

## Non-Functional Requirements

### NFR-01: Confidence Spread Target

After deployment and at least one confidence refresh tick, the p95 – p5 confidence spread of the
active entry population (non-zero-confidence entries) must be ≥ 0.20. This is measured in the live
database and verified by the pipeline calibration test `T-CAL-SPREAD-01`.

### NFR-02: Calibration Stability

The six signal ablation tests T-ABL-01 through T-ABL-06 in `pipeline_calibration.rs` must pass
with the new weights. The Kendall's tau correlation test T-CAL-04 (weight sensitivity: tau > 0.6
under 10% perturbation) must pass with the new weight vector.

### NFR-03: Refresh Latency Budget

The confidence refresh batch at 500 entries must complete within 200ms wall-clock time (enforced by
FR-05 duration guard). On current hardware, 500 single-row SQLite writes in one transaction is
expected to take well under 100ms; the 200ms guard is a conservative safety ceiling.

### NFR-04: Fire-and-Forget Overhead

`record_access` must continue to return to the caller in < 50ms (existing test
`test_record_access_fire_and_forget_returns_quickly` at 50ms bound). The implicit helpful-vote
injection for `context_get` (FR-06) must not increase this latency — it modifies `UsageContext`
in-process before spawning, adding no I/O on the calling thread.

### NFR-05: No Schema Migration

No new database columns or tables are added. `helpful_count` and `unhelpful_count` already exist.
No schema version bump is required.

### NFR-06: No Blocking Pool Regression

The confidence refresh batch running in `spawn_blocking` must not introduce new direct
`lock_conn()` calls in async context. All MCP tool paths must remain mediated by `spawn_blocking`.
Consistent with entry #771 and the vnc-010 fix for blocking pool saturation (SR-08).

### NFR-07: Regression Test Ordering Preservation

T-REG-01 must continue to assert `expert > good > auto > stale > quarantined` after weight and
base_score changes. The `auto_extracted_new()` scenario profile must be verified explicitly with
the new `base_score(Active, "auto") = 0.35` value before finalizing implementation.

---

## Acceptance Criteria

### AC-01 — Confidence Spread Target

**Criterion**: After crt-019, the p95 – p5 confidence spread of the active entry population
(non-zero-confidence entries) is ≥ 0.20 when measured in the live database following at least
one completed refresh tick.

**Verification**: Manual database export + Python spread computation, and
`T-CAL-SPREAD-01` calibration test with a synthetic 50-entry population spanning the signal
space.

---

### AC-02 — Bayesian Posterior Replaces Wilson Score

**Criterion**: `MINIMUM_SAMPLE_SIZE` constant is removed from `confidence.rs`. `helpfulness_score`
is replaced by a Bayesian Beta-Binomial posterior. The following exact assertions hold:

- `bayesian_helpfulness(0, 0, 3.0, 3.0) == 0.5` — cold-start neutral
- `bayesian_helpfulness(0, 2, 3.0, 3.0) == 0.375` — two unhelpful votes lower the score
- `bayesian_helpfulness(2, 2, 3.0, 3.0) == 0.5` — balanced votes produce signal above neutral
  (equal counts return neutral, demonstrating immediate responsiveness)
- `bayesian_helpfulness(2, 0, 3.0, 3.0) > 0.5` — two helpful votes raise the score

**Verification**: Unit tests in `confidence.rs` replacing the existing T-05 helpfulness tests.
Note: the original SCOPE.md AC-02 stated `helpfulness_score(2, 2, ...) > 0.5`; the corrected
assertion is `== 0.5` because (2+3)/(4+6) = 0.5 exactly with cold-start prior. The direction
of responsiveness is confirmed by the unhelpful-vote test above 0.5.

---

### AC-03 — Weight Sum Invariant

**Criterion**: `W_BASE + W_USAGE + W_FRESH + W_HELP + W_CORR + W_TRUST == 0.92` with exact f64
equality.

**Verification**: Existing `weight_sum_invariant_f64` test passes with updated constants.

---

### AC-04 — Ablation Tests Pass with New Weights

**Criterion**: All six signal ablation tests T-ABL-01 through T-ABL-06 in
`crates/unimatrix-engine/tests/pipeline_calibration.rs` pass with the new weight vector.

**Verification**: `cargo test --test pipeline_calibration` green.

---

### AC-05 — Trust-Source Differentiated base_score

**Criterion**: `base_score(Status::Active, "auto") < 0.5`, specifically returns a value ≈ 0.35.
`base_score(Status::Active, "agent")`, `base_score(Status::Active, "human")`, and
`base_score(Status::Active, "system")` all return 0.5.
`base_score(Status::Proposed, "auto")` returns 0.5 (no differentiation for non-Active).

**Verification**: Unit tests in `confidence.rs` extending T-02 base_score tests; explicit
`auto_proposed_base_score_unchanged` test.

---

### AC-06 — Adaptive Search Blend

**Criterion**: `SEARCH_SIMILARITY_WEIGHT` compile-time constant is replaced by a
runtime-computed adaptive blend. The following assertions hold:

- `adaptive_confidence_weight(observed_spread: 0.20) == 0.25`
- `adaptive_confidence_weight(observed_spread: 0.10) == 0.15` (floor)
- `adaptive_confidence_weight(observed_spread: 0.30) == 0.25` (cap)
- `observed_spread` is computed and cached during the refresh tick (not per-query)

**Verification**: Unit tests for `adaptive_confidence_weight` formula; integration test
confirming `rerank_score` uses the cached value after a refresh tick. State management
mechanism (parameter vs. Arc) is as specified in ARCHITECTURE.md.

---

### AC-07 — Batch Size and Duration Guard

**Criterion**: `MAX_CONFIDENCE_REFRESH_BATCH == 500` in `coherence.rs`. A wall-clock guard using
`std::time::Instant` is present in the refresh loop, breaking early when
`elapsed > 200ms` and logging the partial entry count.

**Verification**: Code review of `coherence.rs`; unit test confirming early exit when the
time budget is exhausted (inject a slow mock or verify guard placement in the loop).

---

### AC-08a — context_get Implicit Helpful Vote

**Criterion**: When `context_get` is called with `params.helpful.is_none()`, the retrieved
entry's `helpful_count` is incremented by 1. When called with `params.helpful = Some(false)`,
`helpful_count` is not incremented. No additional `spawn_blocking` task is spawned.

**Verification**: Unit test in `usage.rs` or tool handler tests: call `context_get` handler
with `helpful: None`, wait for spawn_blocking, assert `entry.helpful_count == 1`.

---

### AC-08b — context_lookup Doubled access_count

**Criterion**: When `context_lookup` is called, each returned entry's `access_count` is
incremented by 2 (not 1). `helpful_count` is unchanged. If UsageDedup has already recorded
access for this agent-entry pair in the current session, the increment is 0 (not 2).

**Verification**: Unit test: call `context_lookup` handler for a new entry with a fresh
agent ID, wait for spawn_blocking, assert `access_count == 2` and `helpful_count == 0`.
Second call with same agent asserts `access_count` remains 2.

---

### AC-09 — Skill Files Updated

**Criterion**: `.claude/skills/uni-knowledge-search/SKILL.md` includes `helpful: true` in at
least one example invocation and includes guidance on when to pass `helpful: false`.
`.claude/skills/uni-knowledge-lookup/SKILL.md` includes the same.

**Verification**: Manual review of skill file diffs.

---

### AC-10 — Calibration and Regression Tests Pass

**Criterion**: All pipeline test suites pass:
- `pipeline_calibration.rs`: T-ABL-01..06 + T-CAL-04 (tau > 0.6) + T-RET-01 (verified under
  adaptive blend, not hard 0.75/0.25)
- `pipeline_regression.rs`: T-REG-01 (ordering preserved) + T-REG-02 (updated to new weight
  constants)

**Verification**: `cargo test --test pipeline_calibration --test pipeline_regression` green.

---

### AC-11 — Weight Sum Exact f64

**Criterion**: The `weight_sum_invariant_f64` test asserts `stored_sum == 0.92_f64` with exact
equality (not tolerance). The new weight vector must satisfy this.

**Verification**: `cargo test weight_sum_invariant_f64` passes.

---

### AC-12 — auto vs. Agent Spread Scenario

**Criterion**: Calibration scenario `auto_vs_agent_spread` in `pipeline_calibration.rs` confirms
that an Active entry with `trust_source = "agent"` scores strictly higher (in `compute_confidence`)
than an identically-signaled Active entry with `trust_source = "auto"`, across at least three
signal levels (zero-signal, mid-signal, high-signal).

**Verification**: `cargo test auto_vs_agent_spread` passes.

---

## Domain Models

### Entry

An `EntryRecord` in the Unimatrix knowledge store. Relevant fields for this feature:

| Field | Type | Role in crt-019 |
|---|---|---|
| `status` | `Status` | Input to `base_score` |
| `trust_source` | `String` | New second input to `base_score` |
| `helpful_count` | `u32` | Numerator in Bayesian helpfulness posterior |
| `unhelpful_count` | `u32` | Denominator contribution in Bayesian posterior |
| `access_count` | `u32` | Input to `usage_score`; doubled for lookup |
| `confidence` | `f64` | Output of `compute_confidence`; stored and used in rerank |

### Confidence Formula

A six-component additive weighted composite:

```
confidence = W_BASE * base_score(status, trust_source)
           + W_USAGE * usage_score(access_count)
           + W_FRESH * freshness_score(last_accessed_at, created_at, now)
           + W_HELP  * bayesian_helpfulness(helpful_count, unhelpful_count, α₀, β₀)
           + W_CORR  * correction_score(correction_count)
           + W_TRUST * trust_score(trust_source)
```

Result clamped to [0.0, 1.0]. Sum of weights = 0.92 (invariant). The remaining 0.08 is the
co-access affinity applied at query time (not stored).

### Bayesian Prior (α₀, β₀)

Population-level parameters for the Beta distribution prior over entry helpfulness rates.
Computed per refresh tick from the voted-entry population. Cold-start defaults: α₀ = β₀ = 3.

**Voted-entry population**: all active entries with `helpful_count + unhelpful_count >= 1`.

**Activation threshold**: ≥ 10 such entries required before empirical estimation; below threshold
the cold-start default applies.

**Staleness**: α₀/β₀ are recomputed each refresh tick. Between ticks, callers receive the cached
value from the most recent tick. Acceptable staleness window = one refresh cycle.

### observed_spread

The p95 – p5 confidence spread of the active entry population, computed during the refresh tick
alongside α₀/β₀. Used by the adaptive blend formula to determine `confidence_weight`. Cached in
memory; not persisted. Initial value before first tick: 0.1471 (pre-crt-019 measured value), so
confidence_weight starts at max(0.15, clamp(0.1471 * 1.25, 0.15, 0.25)) = 0.184.

### Adaptive Blend

```
confidence_weight = clamp(observed_spread * 1.25, 0.15, 0.25)
rerank_score = (1 - confidence_weight) * similarity + confidence_weight * confidence
```

Replaces the hard `SEARCH_SIMILARITY_WEIGHT = 0.85` constant. Self-adjusting: as the formula
improvements from FR-01 through FR-03 push spread upward over successive refresh ticks, the
confidence weight automatically increases toward 0.25.

### Deliberate Retrieval Distinction

| Tool | Signal injected | Rationale |
|---|---|---|
| `context_search` | 1× access_count | Search-hit: entry appeared in results |
| `context_lookup` | 2× access_count, no vote | Deliberate multi-entry retrieval: stronger than search-hit, weaker than targeted get |
| `context_get` | 1× access_count + implicit helpful vote | Targeted single-entry retrieval: strongest deliberate signal |

### UsageDedup

In-process deduplication struct (`UsageDedup`) enforcing:
- One `access_count` increment per agent-entry pair per process lifecycle.
- One vote per agent-entry pair (last-vote-wins correction).

The ×2 multiplier for `context_lookup` applies to DB-level increments; UsageDedup filtering
happens before multiplier application. A suppressed access (already deduped) produces 0
increments, not 2.

---

## User Workflows

### Workflow 1 — Agent Queries via context_search

1. Agent calls `context_search` with query text.
2. Server embeds query, retrieves top-k by cosine similarity.
3. Each result is re-ranked: `rerank_score = similarity_weight * similarity + confidence_weight * confidence`, where `confidence_weight` is the cached adaptive value.
4. `record_access` fires with `UsageContext { helpful: None }` unless agent passes `helpful: true`.
5. After crt-019, if the skill is updated (FR-08), agent passes `helpful: true` as standard practice.

### Workflow 2 — Agent Queries via context_get

1. Agent calls `context_get` for a specific entry ID.
2. Handler checks `params.helpful`: if `None`, sets `helpful = Some(true)` before recording.
3. `record_access` fires with `UsageContext { helpful: Some(true) }`, incrementing `helpful_count`.
4. UsageDedup enforces one vote per agent-entry pair — repeated gets by the same agent count once.

### Workflow 3 — Agent Queries via context_lookup

1. Agent calls `context_lookup` for one or more entries by ID.
2. Handler records access with ×2 weight multiplier. No helpful vote injected.
3. After UsageDedup: new agent-entry pairs receive `access_count += 2`; already-seen pairs receive
   `access_count += 0`.

### Workflow 4 — Confidence Refresh Tick

1. Coherence maintenance tick fires (triggered by `context_status maintain=true` or scheduled).
2. Stale entries sorted oldest-first, up to 500 entries selected.
3. For each entry, if `start.elapsed() > 200ms`, break and log partial count.
4. For each entry in budget: compute α₀/β₀ from voted population, compute `observed_spread`,
   call `update_confidence(entry, α₀, β₀, now)`.
5. Cache updated α₀/β₀ and `observed_spread` for use by subsequent `context_search` calls.

---

## Constraints

### C-01 — Sum-to-0.92 Invariant (Hard)

The six stored weight constants must sum to exactly 0.92 with f64 exactness. The
`weight_sum_invariant_f64` test uses `assert_eq!` (not approximate). If floating-point
representation of the new weights does not satisfy exact equality, constants must be expressed
using `f64` literals that round-trip correctly (current values 0.16, 0.12, 0.16, 0.16 are all
exact in IEEE 754 binary64).

### C-02 — T-REG-02 Update Order (Implementation Guard)

T-REG-02 deliberately hard-asserts current weight constant values and will fail on any weight
change. The implementation agent must update T-REG-02 to the new constants as the **first step**
of Change 2, not the last. Implementing weights first and updating tests last creates a window
where tests fail in non-obvious ways during a multi-change implementation cycle (SR-06).

### C-03 — base_score Differentiation: Active Only (SR-04 resolution)

`base_score(Status::Proposed, trust_source)` returns 0.50 for all trust sources including
`"auto"`. The differentiation is limited to `Status::Active` to preserve the T-REG-01 ordering
`auto > stale > quarantined`. The `auto_extracted_new()` test scenario uses `Status::Proposed`
with `trust_source: "auto"` — applying Active differentiation to Proposed would lower its score
below `stale`, inverting the expected ordering.

### C-04 — Implicit Vote Must Not Spawn New Task (SR-07)

The implicit `helpful = Some(true)` injection for `context_get` (FR-06) must modify
`UsageContext.helpful` in-process before the existing `spawn_blocking` fires. A second
`spawn_blocking` for vote injection would reintroduce the pool saturation regression fixed in
vnc-010. The implementation diff for `context_get` handler must show zero new `spawn_blocking`
or `tokio::task::spawn*` calls.

### C-05 — UsageDedup Access Order for context_lookup (SR-05 resolution)

UsageDedup access filtering applies **before** the ×2 multiplier. A repeated `context_lookup`
call by the same agent on the same entry produces 0 increments (not 2). This is consistent with
existing dedup semantics: `filter_access` returns only the IDs not previously seen; multiplier
is applied to the filtered set only.

### C-06 — No Schema Change

`helpful_count` and `unhelpful_count` are existing columns in `EntryRecord`. Adding implicit
helpful votes through the existing `UsageContext.helpful` path requires no migration. Schema
version remains at 12.

### C-07 — Adaptive Blend State Management (Deferred to Architect)

The architect's ARCHITECTURE.md must specify whether `confidence_weight` is passed as a parameter
to `rerank_score` (keeping it pure, requiring call-site updates) or accessed via a thread-safe
shared value (Arc/atomic). This specification mandates only the formula and behavior; the
implementation mechanism is an architectural decision (SR-03). The specification-level requirement
is that `rerank_score` must use the `observed_spread`-derived weight, not the former constant 0.85.

### C-08 — Voted-Entry Population Threshold

The minimum threshold of ≥ 10 voted entries for empirical prior activation (FR-09) must be
enforced. The risk (SR-01) is that method-of-moments on a sparse population produces noisy α₀/β₀
that destabilize all entry scores on every refresh tick. The threshold ensures at least minimal
statistical signal before switching from the stable cold-start default.

### C-09 — Regression Ordering Verification (T-REG-01)

Before finalizing the `base_score` differentiation, the implementation agent must verify
`T-REG-01` passes with the `auto_extracted_new()` profile under the new formula. The test asserts
`expert > good > auto > stale > quarantined`; if `base_score(Active, "auto")` in combination with
new weights places `auto` below `stale`, the approach must be reconsidered and flagged.

---

## Dependencies

### Crates Modified

| Crate | Files affected |
|---|---|
| `unimatrix-engine` | `src/confidence.rs` — all formula constants and functions |
| `unimatrix-engine` | `tests/pipeline_calibration.rs` — new scenario, spread test |
| `unimatrix-engine` | `tests/pipeline_regression.rs` — T-REG-02 constant update |
| `unimatrix-server` | `src/infra/coherence.rs` — batch size, duration guard, prior computation |
| `unimatrix-server` | `src/tools/{get,lookup}.rs` — implicit vote injection, doubled access |
| `unimatrix-server` | `src/services/usage.rs` — access multiplier support |

### Skills Modified

| File | Change |
|---|---|
| `.claude/skills/uni-knowledge-search/SKILL.md` | Add `helpful: true` to primary example |
| `.claude/skills/uni-knowledge-lookup/SKILL.md` | Add `helpful: true` to primary example |

### External Dependencies

No new crates required. All implementation uses the Rust standard library
(`std::time::Instant`, `std::time::Duration`) and existing workspace crates.

### Existing Infrastructure Relied Upon

| Infrastructure | Used by |
|---|---|
| `UsageDedup` (in-process) | FR-06 implicit vote dedup, FR-07 doubled-access dedup |
| `UsageContext.helpful: Option<bool>` | FR-06 implicit helpful injection |
| `record_usage_with_confidence` | Unchanged — receives helpful_ids unchanged |
| `spawn_blocking` single-task pattern (vnc-010) | C-04 compliance |

---

## NOT in Scope

1. **crt-018b**: Wiring effectiveness scores from crt-018 into re-ranking. Depends on crt-019
   establishing spread first.
2. **crt-020**: Implicit helpfulness from session outcomes (injection_log + session results join).
   A follow-on feature using the formula calibrated here.
3. **crt-014**: Topology-aware supersession penalties using petgraph. Independent Track B feature.
4. **Contradiction detection changes**: The `wilson_lower_bound` gaming-resistance discussion
   belongs to crt-020 implicit signals; this feature only removes it.
5. **base_score changes for non-auto trust sources in Active status**: Only `"auto"` is
   differentiated; `"agent"`, `"human"`, `"system"`, `"neural"` Active entries all remain at 0.50.
6. **base_score changes for Proposed, Deprecated, or Quarantined status**: All non-Active
   statuses retain existing values regardless of trust_source (C-03).
7. **MCP parameter schema changes**: `helpful: Option<bool>` already exists on all query tools.
   No new parameters added.
8. **Persistent storage of α₀/β₀ or observed_spread**: Both values are in-memory runtime caches
   recomputed each refresh tick. No new database columns or tables.
9. **UDS transport changes**: The deliberate retrieval signals (FR-06, FR-07) apply to MCP tool
   handlers only. UDS injection path is not modified.
10. **Scheduled refresh trigger changes**: The increased batch size (FR-05) applies to the
    existing `maintain=true`-triggered refresh path. No new scheduling mechanism is added.

---

## Open Questions

None. All design decisions were resolved during scope review:

1. **Helpfulness scoring** — Bayesian Beta-Binomial with empirical prior (α₀/β₀ from population
   ≥ 10 voted entries; cold-start default α₀=β₀=3)
2. **Deliberate retrieval** — `context_get` implicit helpful vote (existing path); `context_lookup`
   doubled access_count (new multiplier)
3. **base_score signature** — Two-parameter `base_score(status, trust_source)` applied to Active
   only
4. **Search blend gate** — Adaptive runtime blend `clamp(spread * 1.25, 0.15, 0.25)`, no binary
   gate
5. **Duration guard** — `std::time::Instant` inside batch loop
6. **State management for observed_spread/α₀/β₀** — Deferred to architect (SR-02, SR-03)

---

## Knowledge Stewardship

- Queried: /uni-query-patterns for confidence formula scoring retrieval signal — active entries
  found are ADRs about W_COAC removal (crt-013), co-access architecture, server layer patterns,
  and usage dedup conventions. No prior feature-level AC patterns or confidence formula
  procedures exist in active knowledge. The only directly relevant ADR is #705 (W_COAC deletion)
  confirming the 0.92 invariant history, and #706 (two-mechanism co-access architecture)
  confirming the query-time boost separation from stored confidence.
