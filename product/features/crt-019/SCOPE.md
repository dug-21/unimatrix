# crt-019: Confidence Signal Activation

## Problem Statement

The Unimatrix confidence formula produces a score that is too compressed to usefully differentiate
knowledge quality in search re-ranking. Live database analysis confirms the reported symptom:

- **192 active entries** in the live DB
- **180 non-zero confidence** range from 0.4257 to 0.5728 — a spread of **0.1471**
- **88% of entries** cluster in the 0.40–0.58 band
- Confidence contributes at most **0.022** to re-ranking scores (0.15 × 0.1471)
- The co-access boost alone can contribute up to **0.03** — confidence is meaningless as a tiebreaker

Root cause is structural, not a data problem. Three of six confidence formula components are
**fixed constants** for the vast majority of entries:

- `W_BASE × base(Active)` = 0.18 × 0.5 = **0.090** — identical for all active entries
- `W_HELP × help(no votes)` = 0.14 × 0.5 = **0.070** — fixed until 5 votes (MINIMUM_SAMPLE_SIZE)
- `W_CORR × corr(0 corrections)` = 0.14 × 0.5 = **0.070** — fixed for most entries

Together these constant components contribute **0.230** — 46.7% of the active-entry ceiling —
regardless of how useful an entry actually is. Only W_USAGE (freshness-adjacent) and W_FRESH
provide real signal, and they operate within a 0.320 range that the current weights under-amplify.

Additionally, `helpful=true` is never passed by the query skills
(`/uni-knowledge-search`, `/uni-knowledge-lookup`) — the infrastructure exists (MCP `helpful`
parameter, UsageService vote routing, wilson_lower_bound) but votes don't flow from agent usage.
The Wilson score 5-vote minimum also prevents any signal from accumulating at realistic vote volumes.

Downstream features crt-018b (effectiveness-driven retrieval) and crt-020 (implicit helpfulness)
both depend on confidence producing real spread before their signals are layered in.

## Goals

1. Reduce the dead-weight floor so that low-signal entries score lower relative to high-signal entries
2. Expand confidence spread so that the top half of the active population is meaningfully separated
   from the bottom half — targeting a spread of ≥ 0.20 after recalibration
3. Enable the Wilson score helpfulness component to activate at realistic vote volumes (≥ 2 votes)
   instead of permanently returning the 0.5 neutral prior
4. Ensure `context_get` and `context_lookup` produce a stronger confidence signal than search
   presence alone, matching the deliberate-retrieval intent documented in #199
5. Wire `helpful: true` in the `/uni-knowledge-search` and `/uni-knowledge-lookup` query skills
   so helpful votes actually flow through to `helpful_count`
6. Raise the search blend weight from 0.85/0.15 to 0.75/0.25, gated on confidence spread passing
   calibration tests, so that confidence differences actually affect result ordering
7. Increase `MAX_CONFIDENCE_REFRESH_BATCH` from 100 to 500 with a wall-clock duration guard so
   that the batch ceiling does not bottleneck at modest entry counts

## Non-Goals

1. **Not crt-018b** — Wiring effectiveness scores into re-ranking is a separate feature that
   depends on crt-019 establishing spread first
2. **Not crt-020** — Implicit helpfulness from session outcomes (joining injection_log with
   session results) is a follow-on feature; this feature establishes the formula preconditions
3. **Not petgraph / topology penalties** — crt-014 topology-aware supersession is Track B and
   independent; crt-019 does not touch the supersession DAG
4. **Not contradiction detection changes** — The Wilson score gaming-resistance discussion in
   the codebase relates to crt-020 implicit signals; this feature only changes the threshold
5. **Not a schema change** — `helpful_count` and `unhelpful_count` fields exist; no new columns
6. **Not changing correction_score semantics** — The 0/1-2/3-5/6+ step function remains; only
   weight redistribution affects its contribution
7. **Not changing base_score for agent/system/human/proposed entries** — Only the `auto`
   trust_source differentiation within base_score is in scope; Active stays 0.5 for other sources
8. **Not MCP parameter schema changes** — `helpful: Option<bool>` already exists on all query
   tools; skill changes are prose updates only

## Background Research

### Codebase State (Verified)

**`crates/unimatrix-engine/src/confidence.rs`** — the canonical source:
- `W_BASE = 0.18`, `W_USAGE = 0.14`, `W_FRESH = 0.18`, `W_HELP = 0.14`, `W_CORR = 0.14`,
  `W_TRUST = 0.14` — sum = 0.92 (invariant enforced by T-01 in unit tests and T-REG-02 in
  regression tests)
- `MINIMUM_SAMPLE_SIZE = 5 (removed; threshold for empirical prior activation set to 10 voted entries)
- `SEARCH_SIMILARITY_WEIGHT = 0.85` — confidence weight is 0.15
- `MAX_CONFIDENCE_REFRESH_BATCH` lives in `crates/unimatrix-server/src/infra/coherence.rs` at 100
- `trust_score("auto") = 0.35` already exists; `base_score(Active) = 0.5` is flat for all active
  statuses including Proposed

**Wilson score at small n** — numerical analysis reveals:
- `wilson_lb(2, 2) = 0.342`, `wilson_lb(5, 0) = 0.566` — lowering min to 2 does produce signal
- Laplace prior approach (treat 0 votes as prior 1/2 positive and 1/2 negative, alpha=1) gives
  `laplace_wilson(0,0) = 0.095` — this actively lowers the floor for unvoted entries, which is
  the intended effect but a larger behavioral change than the min-size reduction approach

**UsageService (`crates/unimatrix-server/src/services/usage.rs`)** — fully functional:
- `helpful: Option<bool>` accepted on SearchParams, LookupParams, GetParams
- When `helpful=Some(true)`: UsageDedup classifies as NewVote or CorrectedVote, increments
  `helpful_count` in `record_usage_with_confidence`
- Dedup is per-agent-per-entry (one vote per agent)
- All three query tools (`context_search`, `context_lookup`, `context_get`) call `record_access`
  with `AccessSource::McpTool` — no differentiation between deliberate and search-hit retrieval

**Skills** — `.claude/skills/uni-knowledge-search/SKILL.md` and
`.claude/skills/uni-knowledge-lookup/SKILL.md`:
- Neither passes `helpful: true` in their example invocations
- The infrastructure exists but the convention does not

**Issue #199 (deliberate retrieval signal)** — documented in
`product/workflow/base-004/mandatory-stewardship.md`:
- `context_get`/`context_lookup` are deliberate targeted retrievals; currently produce the same
  `access_count` increment as being in a search result set
- Implementation options: (a) separate `AccessSource` variant with boosted `access_count`
  increment; (b) second `access_count` increment; (c) separate counter field (schema change — out
  of scope); (d) implicit `helpful=Some(true)` injection for deliberate retrieval paths

**Issue #202** — Referenced in `product/research/ass-014/findings/impact-assessment.md` as
confirming that confidence feedback should be "conservative-but-real" rather than
"aggressive-but-noisy". The lesson from claude-flow research: fake or imprecise feedback degrades
quality. This informs the Laplace vs. min-size decision (see Open Questions).

**Regression test sensitivity** — `crates/unimatrix-engine/tests/pipeline_regression.rs` (T-REG-02)
hard-asserts exact weight constant values (`W_BASE == 0.18`, etc.). Any weight change will fail
this test by design — it is a deliberate guard that forces a developer to verify the new ordering.
Similarly, T-REG-01 checks relative ordering of expert > good > auto > stale > quarantined;
weight changes must not invert this ordering.

**Calibration tests** — `tests/pipeline_calibration.rs` contains T-ABL-01 through T-ABL-06
(signal ablation) and T-CAL-04 (weight sensitivity: tau > 0.6 under 10% perturbation). The
new weights must pass all six ablation tests and maintain tau > 0.6 under perturbation.

**`confidence_refresh` batch path** — In `status.rs`, stale entries are sorted by reference
timestamp (oldest first), truncated to `MAX_CONFIDENCE_REFRESH_BATCH`, then each entry is
individually read back and confidence updated via `update_confidence()`. There is no current
duration guard — just a count cap.

### Dead-Weight Quantification

At current weights, a new agent entry with zero access and no votes starts at:
```
0.18*0.5 + 0.14*0.0 + 0.18*0.0 + 0.14*0.5 + 0.14*0.5 + 0.14*0.5 = 0.300
```
An agent entry with 30 accesses, 1 day old, no votes:
```
0.18*0.5 + 0.14*0.873 + 0.18*0.867 + 0.14*0.5 + 0.14*0.5 + 0.14*0.5 = 0.578
```
Spread = 0.278 theoretical, but 0.230 is constant regardless of usage.

The 0.230 constant floor is 47% of the 0.460 ceiling for an agent-without-votes entry.
Weight rebalancing reduces this floor by shifting weight toward W_USAGE and W_TRUST where
actual differentiation exists.

## Proposed Approach

Seven coordinated changes, implemented together in a single feature cycle:

**Change 1 — Bayesian Beta-Binomial helpfulness scoring (replaces Wilson)**:
- Replace `wilson_lower_bound(helpful, total, MINIMUM_SAMPLE_SIZE)` with a Bayesian posterior mean
  using an empirical prior: `score = (helpful_count + α₀) / (total_votes + α₀ + β₀)`
- α₀ and β₀ are estimated from the population of entries that have accumulated ≥1 vote (method of
  moments on Beta distribution). Recomputed during each confidence refresh tick.
- Cold-start default when no population data exists: α₀=β₀=3 (6 pseudo-votes, scores new entries
  at 0.5 — same as current neutral, but immediately responsive once votes flow)
- Activates at 0 votes (prior mean), tamper-resistant (gaming requires many votes to overcome prior),
  self-calibrating (α₀/β₀ converge toward true population helpfulness rate as entries accumulate votes)
- `MINIMUM_SAMPLE_SIZE` constant is removed. Voting infrastructure unchanged.

**Change 2 — Weight rebalancing** (sum must remain 0.92):
- Proposed: `W_BASE 0.18→0.16`, `W_HELP 0.14→0.12`, `W_USAGE 0.14→0.16`, `W_TRUST 0.14→0.16`
- Rationale: W_BASE and W_HELP are dead weight for almost all entries; W_USAGE and W_TRUST have
  real signal (usage varies by entry, trust varies by trust_source)
- All six ablation tests (T-ABL-01..06) must still pass with new weights
- T-REG-02 must be updated to reflect new constants
- Net impact: agent entries gain ≤+0.035 from higher W_USAGE ceiling; human entries gain +0.020
  from higher W_TRUST ceiling; unvoted entries lose 0.007 from reduced W_HELP floor

**Change 3 — Trust-source differentiated base_score (clean signature change)**:
- Change signature: `base_score(status: Status) → base_score(status: Status, trust_source: &str) -> f64`
- Blast radius confirmed minimal: 1 production call site (`compute_confidence` line 205) + 6 test
  call sites (4 inline unit tests in confidence.rs, test_scenarios_unit.rs:68, pipeline_calibration.rs:94)
- All existing callers pass `""` or `entry.trust_source` — mechanical 1-line updates each
- `auto` trust_source Active entries → 0.35; all other trust_sources for Active → 0.5 (unchanged)
- Impact: auto-sourced active entries start at -0.027 vs agent entries, widening the trust gap

**Change 4 — Adaptive search blend (self-adjusting)**:
- Replace hard `SEARCH_SIMILARITY_WEIGHT = 0.85` constant with a runtime-computed blend:
  `confidence_weight = clamp(observed_spread * 1.25, 0.15, 0.25)`
  where `observed_spread` is the p95–p5 confidence spread of the active entry population,
  measured during the confidence refresh tick and cached as a runtime value.
- At current spread 0.1471: confidence_weight ≈ 0.184 (modest improvement over 0.15)
- At spread 0.20 (target): confidence_weight = 0.25 (full activation)
- Capped at 0.25 to prevent over-amplification beyond target; floored at 0.15 (current value)
  to never regress below the existing baseline.
- Eliminates the binary AC-06 gate; blend automatically tracks confidence quality as the formula
  improvements from changes 1–3 push spread upward over successive refresh ticks.
- `observed_spread` cached in-memory alongside the empirical Bayesian prior (same refresh cycle).

**Change 5 — Batch size increase with duration guard**:
- `MAX_CONFIDENCE_REFRESH_BATCH 100 → 500` in `crates/unimatrix-server/src/infra/coherence.rs`
- Add wall-clock duration guard using `std::time::Instant` checked at the top of each loop
  iteration: if `start.elapsed() > Duration::from_millis(200)` break early and report partial count
- `spawn_blocking` timeout is not viable (tokio cannot cancel blocking threads once started);
  `Instant` inside the loop is the correct implementation
- Each `update_confidence()` call is a single-row SQLite UPDATE — negligible per-iteration overhead

**Change 6 — Deliberate retrieval signal (#199)**:
- `context_get` (single targeted retrieval): inject `helpful: Some(true)` implicitly in the tool
  handler when `params.helpful.is_none()` — reuses existing vote infrastructure, increments
  `helpful_count`. Mitigated by UsageDedup (one vote per agent per entry).
- `context_lookup` (multi-entry retrieval): do NOT inject a helpful vote. Instead, record a
  doubled `access_count` increment (×2 instead of ×1) to signal deliberate retrieval via W_USAGE
  without claiming helpfulness. Stronger than a search-hit signal, weaker than a targeted-get vote.
  Implemented by passing a `weight: u32` multiplier to `record_access` (or equivalent in the
  access recording path — no schema change required).

**Change 7 — Wire helpful votes in query skills**:
- Update `/uni-knowledge-search/SKILL.md` and `/uni-knowledge-lookup/SKILL.md` example
  invocations to include `helpful: true` as standard practice
- Add guidance on when to pass `helpful: false` (entry was retrieved but was not applicable)
- This is a documentation change with no code impact

## Acceptance Criteria

- **AC-01**: After crt-019, the confidence spread of the active entry population (non-zero
  confidence entries) is ≥ 0.20 when measured in the live database
- **AC-02**: Wilson score replaced with Bayesian Beta-Binomial posterior. `MINIMUM_SAMPLE_SIZE`
  removed. `helpfulness_score(0, 0, α₀=3, β₀=3) == 0.5` (cold-start neutral).
  `helpfulness_score(2, 0, α₀, β₀) < 0.5` (two unhelpful votes lower the score).
  `helpfulness_score(2, 2, α₀, β₀) > 0.5` (balanced votes produce signal above neutral).
- **AC-03**: The weight sum invariant holds: `W_BASE + W_USAGE + W_FRESH + W_HELP + W_CORR + W_TRUST == 0.92`
- **AC-04**: All six signal ablation tests (T-ABL-01 through T-ABL-06) pass with the new weights
- **AC-05**: `base_score(Active)` for `trust_source = "auto"` entries returns a value < 0.5
  (specifically ≈ 0.35), while all other Active entries return 0.5
- **AC-06**: `SEARCH_SIMILARITY_WEIGHT` constant is replaced with adaptive runtime blend.
  `confidence_weight = clamp(observed_spread * 1.25, 0.15, 0.25)`. At spread=0.20,
  confidence_weight == 0.25. `observed_spread` is computed and cached during refresh tick.
- **AC-07**: `MAX_CONFIDENCE_REFRESH_BATCH` is increased to 500 and a duration guard (wall-clock
  ≤ 200ms) is added to the refresh loop in `status.rs`
- **AC-08a**: `context_get` handler injects `helpful: Some(true)` when `params.helpful.is_none()`,
  verifiable via unit test showing `helpful_count` incremented after a get call.
- **AC-08b**: `context_lookup` handler records a doubled `access_count` increment (×2) without
  modifying `helpful_count`, verifiable via unit test showing `access_count` += 2 per lookup.
- **AC-09**: `/uni-knowledge-search/SKILL.md` and `/uni-knowledge-lookup/SKILL.md` include
  `helpful: true` in their primary example invocations
- **AC-10**: All existing calibration and regression pipeline tests pass (with T-REG-02 updated
  to the new weight constants, and T-RET-01 assertions verified under 0.75/0.25 blend)
- **AC-11**: The `weight_sum_invariant_f64` test in confidence.rs unit tests passes (exact f64
  equality at 0.92)
- **AC-12**: A new calibration scenario `auto_vs_agent_spread` confirms auto-sourced active
  entries score below identically-signaled agent entries

## Constraints

- **Sum-to-0.92 invariant** — The stored weight sum must exactly equal 0.92. Co-access affinity
  (0.08) is applied at query time and is not a stored factor. Any weight redistribution must
  maintain this sum with f64 exactness (confirmed by `weight_sum_invariant_f64` test).
- **T-REG-02 is a deliberate break** — The pipeline_regression.rs test hardcodes current weight
  values and will fail when weights change. This is by design — it forces a conscious update.
  The new test must assert the new values and verify relative ordering still holds.
- **base_score signature change risk** — `base_score(status: Status) -> f64` is a pure function
  tested in isolation. Adding trust_source differentiation requires either changing the signature
  or handling it in `compute_confidence`. The signature-change path requires updating all callers
  (calibration tests use `base_score(entry.status)` directly in T-CAL-04). The `compute_confidence`
  path is lower-risk.
- **UsageDedup one-vote-per-agent protection** — The implicit helpful vote for get/lookup will not
  double-count if the same agent makes multiple calls; UsageDedup enforces one vote per
  agent-entry pair in-process.
- **SQLite batch size** — The `record_usage_with_confidence` function acquires a single connection
  lock (`BEGIN IMMEDIATE`) and iterates over all IDs. At 500 entries, this is 500 SQLite row
  reads and writes in one transaction. Duration guard prevents runaway latency on context_status.
- **Regression test ordering dependency** — T-REG-01 asserts `expert > good > auto > stale > quarantined`.
  The `auto_extracted_new` scenario uses `Status::Proposed` with `trust_source: "auto"` — if
  base_score differentiation is applied to Proposed status as well as Active, the ordering may
  shift. Verify with `auto_extracted_new()` profile before finalizing the base_score approach.
- **No schema change** — `helpful_count` and `unhelpful_count` are existing columns. Adding
  implicit helpful votes requires no migration.

## Open Questions

None — all design decisions resolved during scope review:

1. **Helpfulness scoring** → Bayesian Beta-Binomial with empirical prior (α₀/β₀ from population)
2. **Deliberate retrieval signal** → `context_get` implicit helpful vote; `context_lookup` doubled access_count
3. **base_score signature** → Clean two-parameter `base_score(status, trust_source)` (7 call sites, all mechanical)
4. **Search blend gate** → Adaptive runtime blend `clamp(spread * 1.25, 0.15, 0.25)`, no binary gate
5. **Duration guard** → `std::time::Instant` inside batch loop (spawn_blocking cancellation is not viable)

## Tracking

https://github.com/dug-21/unimatrix/issues/255
