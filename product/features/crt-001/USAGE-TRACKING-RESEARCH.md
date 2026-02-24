# Gaming-Resistant Usage Tracking for Self-Learning Knowledge Engine

**Date**: 2026-02-24
**Type**: Research Spike
**Status**: Complete
**Companion**: `product/features/crt-001/SCOPE.md` (current naive design under review)

---

## Executive Summary

The crt-001 SCOPE as written has a critical flaw: the usage signals that feed crt-002's confidence formula are trivially gameable. An agent calling `context_get(id=42)` in a loop inflates `access_count` linearly, and passing `helpful: true` on every retrieval inflates `helpful_count` with zero friction. Since confidence drives what gets served to agents, this creates a direct manipulation vector: a buggy or adversarial agent can boost any entry to the top of search results.

This research evaluates seven categories of approaches and recommends a layered strategy that replaces naive counters with gaming-resistant primitives. The recommended approach adds minimal complexity to crt-001 (two new fields, one deduplication check, logarithmic transform) while making the confidence inputs fundamentally harder to manipulate.

---

## 1. Approaches Evaluated

### 1.1 Naive Linear Counters (Current crt-001 Design)

**How it works**: Increment `access_count += 1` on every retrieval. Increment `helpful_count += 1` when `helpful: true` is passed. Both are unbounded, linear, per-request, and unauthenticated.

**Pros**:
- Simplest possible implementation (~20 LOC in the recording path)
- Zero additional storage beyond the two u32 fields
- Easy to reason about: more accesses = higher count

**Cons**:
- **Trivially gameable**: One loop inflates any entry to any count
- **No deduplication**: Same agent reading the same entry 100x in one session counts as 100
- **Self-reported helpfulness**: The agent that retrieves the entry decides if it was helpful, with no verification
- **Linear scaling rewards spam**: 100 reads = 100x signal, making gaming maximally efficient
- **Read path causes write side-effects**: Every retrieval modifies the entry record, meaning reads now have security implications equal to writes

**Complexity**: Very Low
**Gaming Resistance**: None

**Verdict**: Unacceptable as the sole mechanism. This is the baseline to improve upon.

---

### 1.2 Session-Based Deduplication

**How it works**: Track which (agent_id, entry_id) pairs have already been counted in the current session. An agent retrieving the same entry 50 times in one session counts as 1 access. A new session resets the deduplication window.

**Implementation options**:
- **Option A: In-memory HashSet** -- `HashSet<(String, u64)>` cleared on server restart. Zero storage cost, lost on crash.
- **Option B: Timestamp window** -- Only count an access if `now - last_accessed_at > dedup_window` (e.g., 300 seconds). No additional storage, but per-entry not per-agent-per-entry.
- **Option C: Per-agent last-access tracking** -- New field or table tracking `(agent_id, entry_id) -> last_counted_at`. Most precise but adds storage.

**Pros**:
- Eliminates the most obvious attack (loop-to-boost)
- Minimal conceptual complexity
- Option A requires zero schema changes

**Cons**:
- Option A is volatile (lost on restart, not durable)
- Option B does not distinguish between different agents accessing the same entry
- Option C adds per-agent-per-entry storage, which is a new table
- Does not address helpful-flag stuffing
- Does not address the fundamental problem that the signal source (the agent) is untrusted

**Complexity**: Low (Option A) to Medium (Option C)
**Gaming Resistance**: Moderate -- blocks loop-to-boost but not distributed attacks across sessions

**Verdict**: Necessary but not sufficient. Should be combined with other approaches.

---

### 1.3 Diminishing Returns Curves

**How it works**: Instead of feeding raw `access_count` into the confidence formula, apply a sublinear transform: `usage_signal = log(1 + access_count)` or `usage_signal = sqrt(access_count)`. The first 10 accesses contribute significantly; the next 1000 contribute very little.

**Mathematical comparison** (at various raw counts):

| Raw Count | Linear | log(1+n) | sqrt(n) |
|-----------|--------|----------|---------|
| 1 | 1.0 | 0.69 | 1.0 |
| 10 | 10.0 | 2.40 | 3.16 |
| 100 | 100.0 | 4.62 | 10.0 |
| 1000 | 1000.0 | 6.91 | 31.6 |
| 10000 | 10000.0 | 9.21 | 100.0 |

With `log(1+n)`, inflating access_count from 100 to 10000 (100x gaming effort) only increases the signal from 4.62 to 9.21 (2x gain). The return on gaming effort collapses.

**Pros**:
- Zero additional storage or schema changes -- the transform happens in crt-002 at computation time
- Dramatically reduces the payoff of count inflation
- Well-understood mathematically; no tuning parameters (logarithm is parameter-free)
- Works with the existing `access_count` field -- no recording flow changes

**Cons**:
- Does not prevent inflation, only reduces its impact
- Does not address helpful-flag gaming
- Requires choosing between log and sqrt (log is more aggressive, sqrt preserves more dynamic range)

**Complexity**: Trivial (one-line change in crt-002's formula)
**Gaming Resistance**: High against count inflation specifically

**Verdict**: Essential. Should be applied unconditionally. Near-zero cost, high value.

---

### 1.4 Agent Diversity Weighting

**How it works**: Instead of counting total accesses, count the number of *distinct agents* that accessed an entry. An entry accessed by 8 different agents is more likely genuinely useful than one accessed 800 times by one agent.

**Implementation**: New field `accessor_count: u32` on EntryRecord, incremented only when the accessing agent has not been counted before for this entry. Requires tracking which agents have accessed each entry.

**Storage options**:
- **Option A: Bloom filter per entry** -- Compact probabilistic set. False positives (undercounting) are acceptable for analytics. ~128 bytes per entry for <1% false positive rate with up to 100 agents.
- **Option B: ACCESS_AGENTS multimap table** -- `MultimapTableDefinition<u64, &str>` (entry_id -> set of agent_ids). Exact, queryable, but grows linearly with agents.
- **Option C: Counter only** -- Just store `unique_accessor_count: u32` and accept that we cannot deduplicate perfectly without per-agent tracking. Use the in-memory session dedup (1.2 Option A) as a best-effort guard.

**Pros**:
- Directly measures the thing that correlates with genuine usefulness: breadth of consumption
- Single-agent gaming is capped: one agent = +1, no matter how many times it reads
- Naturally resistant to loop-to-boost

**Cons**:
- Requires per-agent-per-entry deduplication (Options A/B) or accepts imprecision (Option C)
- Current deployment is single-agent stdio, so all accesses come from one agent -- the signal is meaningless until multi-agent deployment (every entry scores 1)
- Option B adds a potentially large table

**Complexity**: Medium (Option B/C) to High (Option A)
**Gaming Resistance**: Very High against single-agent inflation; Moderate against multi-agent collusion

**Verdict**: Highly valuable in multi-agent scenarios but premature for single-agent stdio. Design the schema to accommodate it (reserve the field), but do not make it a primary signal yet.

---

### 1.5 Implicit Outcome Correlation (Audit Log Mining)

**How it works**: Instead of trusting the agent's self-reported `helpful: true`, infer helpfulness from what happens *after* retrieval. If an agent retrieves entries and then successfully completes its task (evidenced by a subsequent `context_store` with `category: "outcome"` and `result: pass` tags), those retrieved entries were probably helpful.

**Correlation mechanism**:
1. Agent calls `context_search` or `context_briefing` -- AUDIT_LOG records `target_ids` (entries returned)
2. Agent does its work
3. Agent stores an outcome entry: `context_store(category: "outcome", tags: ["result:pass"])`
4. crt-002 (or a periodic batch job) scans AUDIT_LOG: find retrievals from the same agent_id in the same session that preceded a successful outcome store. Boost the confidence of entries in those retrievals' `target_ids`.

**This is analogous to how Netflix weights watch completion over thumbs-up**: an agent that retrieves entries and then succeeds is a stronger signal than an agent that self-reports "helpful."

**Pros**:
- **Not self-reported**: The agent does not choose to inflate this signal; it emerges from behavior
- **Resistant to gaming**: An agent would need to fake successful outcomes (which requires Write capability and passes through content scanning)
- Uses existing AUDIT_LOG data -- no new tables for the raw signal
- Aligns with the product vision's outcome tracking (col-001)

**Cons**:
- **Sparse signal**: Not every session produces an outcome entry. Many retrievals have no followup outcome.
- **Correlation is not causation**: An agent may retrieve 20 entries but only 3 were actually helpful. All 20 get boosted equally.
- **Delayed**: The signal is only available after the outcome is stored, not at retrieval time
- **Requires outcome convention adoption**: Agents must store outcome entries for this to work. Without col-001/alc-002, the signal may be too sparse to be useful.
- **Computation complexity**: Correlating audit events across sessions requires scanning and joining, which is expensive in a key-value store without SQL joins

**Complexity**: High (implementation), Medium (conceptual)
**Gaming Resistance**: High -- requires faking outcomes, which is a write operation gated by capabilities

**Verdict**: Excellent long-term signal source. Should be designed for in crt-001 (ensure AUDIT_LOG has the right data) but the actual correlation computation belongs in crt-002 or later. The raw data already exists in AUDIT_LOG `target_ids` -- no crt-001 changes needed for this signal.

---

### 1.6 Wilson Score Interval

**How it works**: Treat each retrieval as a Bernoulli trial: the entry was either helpful or not. Instead of using raw `helpful_count / access_count` as the helpfulness ratio, use the lower bound of the Wilson score confidence interval. This naturally penalizes entries with few observations (small sample sizes) and rewards entries with consistent positive signals across many observations.

**Formula** (simplified for the 95% confidence level, z = 1.96):

```
p_hat = helpful_count / access_count
wilson_lower = (p_hat + z²/2n - z * sqrt(p_hat*(1-p_hat)/n + z²/4n²)) / (1 + z²/n)
```

Where `n = access_count` and `p_hat = helpful_count / access_count`.

**Behavior at different scales**:

| access_count | helpful_count | naive ratio | wilson_lower |
|-------------|---------------|-------------|--------------|
| 1 | 1 | 1.000 | 0.207 |
| 5 | 4 | 0.800 | 0.376 |
| 10 | 8 | 0.800 | 0.494 |
| 50 | 40 | 0.800 | 0.672 |
| 100 | 80 | 0.800 | 0.714 |
| 3 | 3 | 1.000 | 0.438 |

An entry with 3 helpful marks out of 3 accesses scores *lower* than one with 80/100 -- the Wilson interval correctly handles small sample uncertainty.

**How Reddit uses this**: Reddit's comment ranking sorts by the Wilson lower bound of the upvote ratio. This ensures that a comment with 1 upvote and 0 downvotes does not outrank one with 100 upvotes and 10 downvotes, even though the naive ratio (1.0 vs 0.91) would favor the former.

**Pros**:
- **Naturally resistant to small-sample gaming**: Inflating a new entry with a few `helpful: true` calls barely moves the score
- **Converges to truth with evidence**: As real observations accumulate, the interval tightens around the true helpfulness ratio
- **Well-understood, battle-tested**: Used by Reddit, Yelp, and other systems that face adversarial voting
- **No additional storage**: Computable from `access_count` and `helpful_count` at query time
- **Handles the cold-start problem**: New entries with 0 accesses get a neutral prior, not zero

**Cons**:
- Requires both `access_count` and `helpful_count` to be meaningful (if no one ever sends `helpful`, both are 0 and the interval is undefined)
- The `helpful` signal itself is still self-reported by agents
- Does not help if the ratio of helpful/access is consistently gamed (always sending helpful: true = 100% ratio, which Wilson converges to high)
- Slightly more complex formula in crt-002

**Complexity**: Low (formula in crt-002, no storage changes)
**Gaming Resistance**: High against small-sample inflation; Moderate against sustained consistent gaming

**Verdict**: Excellent for the helpfulness factor in crt-002. Should replace naive `helpful_count / access_count` ratio. Combines naturally with logarithmic access counts.

---

### 1.7 Bayesian Confidence with Skeptical Prior

**How it works**: Model each entry's true helpfulness as a Beta distribution with a *skeptical prior* (e.g., Beta(2, 5) -- meaning "we expect most entries to be moderately unhelpful until proven otherwise"). Each `helpful: true` observation adds to the alpha parameter; each access without `helpful: true` adds to beta. The posterior mean `alpha / (alpha + beta)` is the confidence input.

**Comparison with Wilson score**:
- Wilson is frequentist: "what's the worst-case lower bound given the data?"
- Bayesian is belief-updating: "given a skeptical starting belief and the data, what do we believe now?"
- Both handle small samples well. Bayesian is more tunable (the prior encodes domain knowledge).

**Prior selection**:
- Beta(1, 1) = uniform prior (no opinion) -- equivalent to Laplace smoothing
- Beta(2, 5) = skeptical prior (most entries are not helpful until proven)
- Beta(1, 3) = mildly skeptical

With Beta(2, 5) and no observations, the posterior mean is 2/7 = 0.286. After 10 helpful observations out of 15 accesses: Beta(12, 10), mean = 12/22 = 0.545. After 50 helpful out of 60: Beta(52, 15), mean = 52/67 = 0.776.

**Pros**:
- **Principled handling of uncertainty**: Naturally skeptical of small samples
- **Tunable prior**: The skeptical prior (Beta(2,5)) means gaming requires sustained effort to overcome the initial pessimism
- **Closed-form update**: Just add to alpha and beta -- no complex computation
- **Smooth convergence**: No sharp transitions; confidence builds gradually

**Cons**:
- More conceptually complex than Wilson (beta distributions are less intuitive)
- Prior selection requires a judgment call (what counts as "skeptical enough"?)
- Still relies on the same `helpful` signal, which is self-reported
- Functionally similar to Wilson for practical purposes at Unimatrix's scale

**Complexity**: Low (two additional fields or compute from existing counts)
**Gaming Resistance**: High against small-sample gaming; Moderate against sustained consistent gaming

**Verdict**: Functionally equivalent to Wilson score for our scale and use case. Wilson is slightly preferred because it is more widely understood and does not require choosing a prior. But either works.

---

### 1.8 ELO-Like Pairwise Rating

**How it works**: When a retrieval returns multiple entries (e.g., `context_search` returns top-5), treat the entries as competing. If the agent subsequently acts on entry A's advice but not B's, A "wins" the matchup. Entries accumulate ELO ratings through pairwise comparisons.

**Pros**:
- Relative ranking is harder to game than absolute counts
- Naturally calibrated: entries compete against each other

**Cons**:
- **No pairwise signal exists**: We have no way to determine which of the returned entries the agent actually used. The agent retrieves a batch and we never see which specific entry informed its behavior.
- Would require agents to report "I used entry X specifically" -- which is an even more gameable explicit signal than `helpful: true`
- Computationally complex for marginal benefit
- Requires significant observation volume for ratings to stabilize

**Complexity**: High
**Gaming Resistance**: Moderate (if the pairwise signal were reliable, which it is not)

**Verdict**: Not applicable. The pairwise comparison signal does not exist in our system and cannot be reliably inferred. Interesting conceptually but impractical.

---

### 1.9 Exponential Time-Weighted Decay

**How it works**: Instead of treating all accesses equally, weight recent accesses more heavily than old ones. An entry accessed 50 times last month but not at all this month should score lower than one accessed 10 times this week.

**Implementation options**:
- **Option A: Exponential Moving Average (EMA)**: `ema = alpha * new_value + (1 - alpha) * old_ema`. Requires storing the current EMA value and updating on each access.
- **Option B: Time-bucketed counts**: Store access counts per time bucket (daily, weekly). Recent buckets weighted higher.
- **Option C: Compute from AUDIT_LOG at query time**: Scan recent audit events for the entry. No additional storage, but expensive at query time.
- **Option D: Use `last_accessed_at` as a proxy**: The existing field already captures recency. crt-002 can compute `freshness_factor = decay(now - last_accessed_at)` without any crt-001 changes.

**Pros**:
- Entries naturally fade without active maintenance
- Rewards sustained usage over burst gaming
- Option D requires zero schema changes -- `last_accessed_at` already exists

**Cons**:
- Option A adds an f32 field and requires careful alpha tuning
- Option B requires time-bucketed storage (additional table or complex schema)
- Option C is too expensive for real-time queries
- Option D does not capture usage *frequency*, only recency of most recent access

**Complexity**: Trivial (Option D) to High (Option B)
**Gaming Resistance**: Moderate -- an agent can still game by accessing recently, but past gaming naturally fades

**Verdict**: Option D is already available. The `last_accessed_at` field plus `created_at` give crt-002 everything it needs for a freshness factor. No crt-001 changes needed. The freshness factor in crt-002's formula already accounts for this.

---

### 1.10 Multi-Signal Fusion

**How it works**: Instead of relying on any single signal, combine multiple weak signals into a composite score. Each signal is independently hard to game, and gaming one signal does not affect the others. The composite is more robust than any individual component.

**Candidate signals for Unimatrix**:

| Signal | Source | Gameable? | Available When? |
|--------|--------|-----------|-----------------|
| Deduped access count | EntryRecord (crt-001) | Moderate (requires many sessions) | crt-001 |
| Agent diversity count | New field or derivable | Low (requires multiple agents) | crt-001 or later |
| Helpful ratio (Wilson) | EntryRecord (crt-001) | Moderate (requires sustained faking) | crt-001 |
| Recency (last_accessed_at) | EntryRecord (existing) | Low (requires recent access) | Already exists |
| Correction chain length | EntryRecord (existing) | Low (requires Write capability) | Already exists |
| Age since creation | EntryRecord (existing) | Not gameable | Already exists |
| Outcome correlation | AUDIT_LOG mining | High resistance (requires faking outcomes) | crt-002 or later |
| Content hash stability | EntryRecord (existing) | Not gameable | Already exists |
| Trust source of creator | EntryRecord (existing) | Not gameable (set at creation) | Already exists |

**The key insight from search engine anti-fraud**: Google does not rely on click count alone. It fuses click-through rate, dwell time, bounce rate, link graph, content quality signals, freshness, and dozens of other factors. Any single signal can be gamed; the fusion of signals that are independent and have different gaming costs makes manipulation impractical.

**Pros**:
- Robust: gaming one signal barely moves the composite
- Degrades gracefully: if one signal is missing or unreliable, others compensate
- Allows adding new signals over time without redesigning the formula

**Cons**:
- Requires deciding on weights for each signal
- More complex formula in crt-002
- Diminishing returns on signal count: 3-5 good signals are sufficient; 20 signals adds complexity without proportional benefit

**Complexity**: Medium (mostly in crt-002 formula design)
**Gaming Resistance**: Very High (the composite is harder to game than any component)

**Verdict**: This is the right meta-approach. The question is not "which signal do we use?" but "how do we combine several signals that are each imperfect?"

---

### 1.11 Anomaly Detection on Access Patterns

**How it works**: Monitor per-agent access patterns for statistical anomalies. An agent accessing the same entry 500 times in an hour, or systematically marking every entry helpful, triggers an anomaly flag. Anomalous access patterns are excluded from confidence computation or weighted down.

**Approaches**:
- **Z-score on per-agent access rates**: If agent X's access count for entry Y is >3 standard deviations above the mean for all agents on that entry, flag as anomalous
- **Rate limiting**: Hard cap on how many times an agent's accesses count toward confidence in a time window
- **Behavioral baseline deviation**: Establish a "normal" access pattern per agent over time; flag deviations

**Pros**:
- Catches gaming that other approaches merely tolerate
- Can retroactively discount gamed signals
- Aligns with crt-001's write rate tracking infrastructure

**Cons**:
- **Requires accumulated data to establish baselines**: Cannot detect anomalies on day 1
- **Single-agent deployment makes baselines meaningless**: With one agent, there is no "normal" to deviate from
- **False positives**: A legitimate agent doing a thorough research pass might access many entries rapidly
- **Complexity**: Requires statistical computation, thresholds, and a monitoring loop

**Complexity**: High
**Gaming Resistance**: Very High (when baselines are established)

**Verdict**: Valuable for future multi-agent deployment. Not actionable in crt-001 given single-agent stdio. The AUDIT_LOG already captures the raw data needed -- anomaly detection can be layered on in a future crt feature without schema changes.

---

### 1.12 Removing the Explicit `helpful` Parameter

**How it works**: Do not ask agents whether entries were helpful. Remove the `helpful` parameter from retrieval tools entirely. Instead, derive helpfulness exclusively from implicit signals (outcome correlation, access diversity, correction patterns).

**Rationale**: The explicit `helpful` parameter is the most directly gameable signal in the system. An agent can pass `helpful: true` on every call with zero friction. Netflix gives 10x more weight to watch-completion (implicit) than to thumbs-up ratings (explicit) because implicit signals are harder to fake.

**Pros**:
- Eliminates the most gameable signal entirely
- Simpler API (fewer parameters on retrieval tools)
- Forces the system to rely on signals that correlate with actual value

**Cons**:
- **Loses a potentially valuable signal**: Some agents may provide genuine helpfulness feedback
- **Implicit signals are sparse**: Until outcome tracking (col-001) is widely adopted, there may be no helpfulness signal at all
- **Cold-start problem**: New entries have no implicit signal

**Complexity**: Negative (removes code)
**Gaming Resistance**: Maximum (the vector does not exist)

**Verdict**: Strong argument for deferring the `helpful` parameter entirely. If we keep it, it must be heavily discounted relative to implicit signals. See Recommendation below.

---

## 2. Recommendation

### The Layered Strategy

Apply three layers of gaming resistance, each addressing a different attack vector, with increasing complexity matched to increasing maturity:

#### Layer 1: Recording-Time Defenses (crt-001)

These changes modify how crt-001 records usage data:

**A. Session-based deduplication (in-memory)**

Add an in-memory `HashSet<(String, u64)>` (agent_id, entry_id) to the server state, cleared on restart. Before incrementing `access_count`, check if this (agent, entry) pair has been counted in the current session. If yes, skip the increment. If no, insert the pair and proceed.

This blocks the most trivial attack (loop-to-boost) at near-zero cost. The set lives in server memory, requires no schema changes, and the loss of dedup state on restart is acceptable because a new session = a new legitimate access.

**Why in-memory and not persisted**: Persisting the dedup set would require a new table and add write overhead on every retrieval. The purpose is to block intra-session loops, not cross-session access. An agent that accesses an entry once per session across 100 sessions is not gaming -- it is genuinely using the entry 100 times.

**B. Replace `helpful_count` with `helpful_count` + `unhelpful_count`**

Instead of a single `helpful_count` incremented on `helpful: true`, track two counters:
- `helpful_count: u32` -- incremented when `helpful: Some(true)`
- `unhelpful_count: u32` -- incremented when `helpful: Some(false)`

When `helpful` is `None` (not provided), neither counter changes. This creates a proper Bernoulli trial: each agent actively expressing an opinion contributes one observation to the helpful/unhelpful ratio. Agents that do not express an opinion are neutral.

**Why this matters**: With only `helpful_count`, the Wilson score and Bayesian approaches cannot distinguish "no one said helpful" from "everyone said unhelpful." The two-counter design enables proper statistical treatment.

**C. Apply session deduplication to the helpful signal too**

The dedup HashSet should also track whether this (agent, entry) pair has already submitted a helpful/unhelpful vote in this session. An agent voting `helpful: true` 50 times in one session counts as 1 vote. This prevents helpful-flag stuffing.

#### Layer 2: Computation-Time Defenses (crt-002)

These changes do not affect crt-001's schema or recording flow. They modify how crt-002 computes confidence from the recorded data:

**D. Logarithmic transform on access_count**

Replace raw `access_count` with `log2(1 + access_count)` in the usage factor. This collapses the gaming payoff: inflating from 10 to 10000 accesses only doubles the signal (3.46 to 13.29).

**E. Wilson score lower bound for helpfulness factor**

Compute the helpfulness factor as `wilson_lower(helpful_count, helpful_count + unhelpful_count)` instead of a naive ratio. New entries with few votes get a conservative score. Entries with sustained positive feedback across many observations rise.

**F. Multi-signal composite confidence**

Replace the single-formula approach with a weighted composite of independent signals:

```
confidence = w_base * base_factor
           + w_usage * log2(1 + access_count) / log2(1 + MAX_MEANINGFUL_ACCESS)
           + w_fresh * freshness_decay(now - last_accessed_at)
           + w_help * wilson_lower(helpful, helpful + unhelpful)
           + w_corr * correction_factor(correction_count, version)
           + w_trust * trust_source_weight(trust_source)
```

Where weights sum to 1.0 and are tunable. The crucial property: gaming `access_count` (one signal) cannot overcome low scores on freshness, helpfulness, corrections, and trust source (four independent signals).

**Recommended initial weights**:
- `w_base = 0.20` (content quality proxy: starts at a neutral baseline)
- `w_usage = 0.15` (access count, log-transformed)
- `w_fresh = 0.20` (recency of access)
- `w_help = 0.15` (Wilson score helpfulness, when observations exist)
- `w_corr = 0.15` (correction chain: entries that have been corrected and refined score higher)
- `w_trust = 0.15` (trust_source of the creator: "human" > "agent" > "system" for user-facing relevance)

These weights make `access_count` only 15% of the composite. Even if an agent inflates access_count to the maximum, the confidence boost is capped at 0.15 (15% of the total score). Gaming one signal cannot dominate.

#### Layer 3: Future Defenses (crt-003+, post-multi-agent)

These require infrastructure that does not yet exist but should be designed for:

**G. Implicit outcome correlation** (crt-002 enhancement or separate feature)

Mine AUDIT_LOG for retrieval-then-outcome patterns. Entries that were retrieved before successful outcomes get a confidence boost that is not self-reported. This is the strongest signal but requires outcome tracking adoption.

**H. Agent diversity scoring** (when multi-agent is deployed)

Track unique accessor count per entry. Entries accessed by diverse agents score higher. This signal is meaningless in single-agent stdio but becomes powerful with multiple agents.

**I. Anomaly detection** (crt behavioral baseline feature)

Establish per-agent behavioral baselines from accumulated AUDIT_LOG data. Flag and discount anomalous access patterns. Requires weeks of data to be meaningful.

### Why This Layered Approach

The strategy follows the principle from search engine anti-fraud: **no single signal is trusted; the fusion of independent signals makes manipulation impractical.**

- Layer 1 (recording) blocks the cheapest attacks at near-zero cost
- Layer 2 (computation) makes the remaining attacks ineffective through mathematical transforms
- Layer 3 (future) adds signals that are fundamentally non-gameable because they are not self-reported

Each layer is independently valuable and incrementally deployable. crt-001 implements Layer 1. crt-002 implements Layer 2. Future crt features add Layer 3.

---

## 3. Impact on crt-001 SCOPE

### Schema Changes

The crt-001 SCOPE already adds `helpful_count: u32` to EntryRecord. The recommendation modifies this to add two fields instead of one:

**Current** (crt-001 SCOPE as written):
```
... trust_source
    helpful_count: u32    // NEW (appended)
```

**Recommended**:
```
... trust_source
    helpful_count: u32    // NEW (appended)
    unhelpful_count: u32  // NEW (appended after helpful_count)
```

Both fields have `#[serde(default)]` for zero-value initialization. The schema migration v1 -> v2 backfills both to 0. The bincode positional encoding constraint is satisfied because both are appended at the end.

**This adds one field to the migration.** The v1 -> v2 scan-and-rewrite already handles the pattern. Adding `unhelpful_count` alongside `helpful_count` is trivial -- same migration, one additional field.

### New Server State

Add to `UnimatrixServer` (or a new `UsageTracker` struct held by the server):

```rust
/// In-memory deduplication for usage tracking.
/// Cleared on server restart. Key: (agent_id, entry_id).
/// Value: bitflags indicating what has been counted this session.
struct UsageDedup {
    /// Entries where access_count has been incremented for this agent.
    access_counted: HashSet<(String, u64)>,
    /// Entries where a helpful/unhelpful vote has been recorded for this agent.
    vote_counted: HashSet<(String, u64)>,
}
```

This is in-memory only, not persisted. It is `Send + Sync` if wrapped in a `Mutex` or `RwLock` (low contention in single-agent stdio).

### Modified Recording Flow

The crt-001 SCOPE's recording flow changes from:

**Current**:
> On every successful retrieval:
> 1. Execute the query (read transaction, unchanged)
> 2. Open a write transaction for usage updates:
>    a. For each returned entry: increment `access_count`, set `last_accessed_at` to now
>    b. If `helpful == Some(true)`: also increment `helpful_count` on each entry
>    c. If `feature` is provided: insert into FEATURE_ENTRIES
> 3. Commit

**Recommended**:
> On every successful retrieval:
> 1. Execute the query (read transaction, unchanged)
> 2. Resolve agent_id from identity pipeline
> 3. Check dedup set for each returned entry_id:
>    a. If (agent_id, entry_id) is NOT in `access_counted`: mark it, include in access_count update batch
>    b. If (agent_id, entry_id) is NOT in `vote_counted` AND `helpful` is Some(_): mark it, include in vote update batch
> 4. Open a write transaction for usage updates:
>    a. For entries in the access batch: increment `access_count`, set `last_accessed_at` to now
>    b. For entries in the vote batch: increment `helpful_count` if `helpful == Some(true)`, increment `unhelpful_count` if `helpful == Some(false)`
>    c. If `feature` is provided: insert into FEATURE_ENTRIES
>    d. `last_accessed_at` is ALWAYS updated (even for deduped accesses -- recency tracking is not gaming-sensitive)
> 5. Commit

**Key difference**: Step 3 filters out repeat accesses and repeat votes within the same session. Step 4d always updates `last_accessed_at` because recency is not a gameable signal (it just records "when was this last touched" for the freshness factor).

### Modified Acceptance Criteria

The following crt-001 ACs change:

- **AC-02** (modified): Add both `helpful_count: u32` AND `unhelpful_count: u32` to EntryRecord with `serde(default)`, appended after `trust_source`.
- **AC-03** (modified): Schema migration v1 -> v2 backfills both `helpful_count = 0` and `unhelpful_count = 0`.
- **AC-04** (modified): `access_count` is incremented at most once per (agent_id, entry_id) per server session. Subsequent retrievals of the same entry by the same agent within the same session do not increment.
- **AC-05** (unchanged): `last_accessed_at` is always updated (not deduplicated).
- **AC-06** (modified): When `helpful = Some(true)` is provided AND this (agent_id, entry_id) has not voted this session, `helpful_count` is incremented.
- **AC-07** (modified): When `helpful = Some(false)` is provided AND this (agent_id, entry_id) has not voted this session, `unhelpful_count` is incremented. When `helpful` is `None`, neither counter changes.

**New AC**:
- **AC-16**: In-memory deduplication set tracks (agent_id, entry_id) for access counting and vote counting per server session. The set is cleared on server restart. Dedup state is not persisted.

### What Does NOT Change

- FEATURE_ENTRIES table design: unchanged
- Tool parameter names: `helpful: Option<bool>` stays the same (the agent already sends true/false/absent)
- Write rate tracking: unchanged
- EntryStore trait extension: unchanged (the trait method records access; dedup happens at the server layer above the trait)
- Two-transaction approach: unchanged
- context_briefing dedup behavior: unchanged (still counts per-entry in final result)

### Storage Impact

- One additional `u32` field (`unhelpful_count`) per entry: 4 bytes
- At 1000 entries: 4 KB additional storage (negligible)
- In-memory dedup set: ~64 bytes per (agent_id, entry_id) pair. At 1000 entries accessed once each: ~64 KB. At 10000: ~640 KB. Well within memory budget.

---

## 4. Impact on crt-002 SCOPE

### Confidence Formula Redesign

The crt-001 SCOPE references crt-002's formula as:
```
confidence = base * usage * freshness * correction * helpfulness
```

This multiplicative formula has a problem: any factor at 0 zeros out the entire score. And the `usage` factor (derived from raw `access_count`) is the most gameable input.

**Recommended replacement** -- additive weighted composite:

```
confidence = w_base  * base_score(entry)
           + w_usage * usage_score(access_count)
           + w_fresh * freshness_score(last_accessed_at, created_at)
           + w_help  * helpfulness_score(helpful_count, unhelpful_count)
           + w_corr  * correction_score(correction_count, version)
           + w_trust * trust_score(trust_source)
```

Where each component function maps to [0.0, 1.0] and weights sum to 1.0.

### Component Functions

**base_score(entry) -> [0.0, 1.0]**
Starting score for all entries. Could be 0.5 (neutral) or derived from content quality heuristics (title present, content length, tags present). Initial implementation: flat 0.5 for all entries.

**usage_score(access_count) -> [0.0, 1.0]**
```rust
fn usage_score(access_count: u32) -> f32 {
    let max_meaningful = 50.0_f32; // accesses beyond this add negligible signal
    (1.0 + access_count as f32).ln() / (1.0 + max_meaningful).ln()
}
// access_count=0 -> 0.0, =1 -> 0.18, =10 -> 0.61, =50 -> 1.0, =500 -> 1.59 (clamped to 1.0)
```

The `max_meaningful` constant (50) is the expected upper bound for legitimate access in a knowledge base of hundreds of entries. Accesses beyond 50 are either gaming or irrelevant -- the score is already at 1.0.

**freshness_score(last_accessed_at, created_at) -> [0.0, 1.0]**
```rust
fn freshness_score(last_accessed_at: u64, created_at: u64, now: u64) -> f32 {
    let reference = if last_accessed_at > 0 { last_accessed_at } else { created_at };
    let age_hours = (now - reference) as f32 / 3600.0;
    let half_life_hours = 168.0; // 1 week
    (-age_hours / half_life_hours).exp()
}
// Just accessed: 1.0, 1 week ago: 0.37, 2 weeks: 0.14, 1 month: 0.02
```

**helpfulness_score(helpful, unhelpful) -> [0.0, 1.0]**
```rust
fn helpfulness_score(helpful: u32, unhelpful: u32) -> f32 {
    let n = helpful + unhelpful;
    if n == 0 {
        return 0.5; // neutral prior: no votes = average
    }
    wilson_lower_bound(helpful as f64, n as f64, 1.96) as f32
}
```

When no votes exist, return 0.5 (neutral). This means the helpfulness signal does not penalize entries that are simply never voted on. Wilson score handles the rest.

**correction_score(correction_count, version) -> [0.0, 1.0]**
Entries that have been corrected (refined through the correction chain) are more trustworthy than uncorrected entries -- they have been reviewed. But entries corrected many times may be unstable.

```rust
fn correction_score(correction_count: u32, version: u32) -> f32 {
    match correction_count {
        0 => 0.5,  // uncorrected: neutral
        1..=2 => 0.8,  // corrected 1-2 times: refined, higher trust
        3..=5 => 0.6,  // corrected 3-5 times: somewhat unstable
        _ => 0.3,       // corrected 6+ times: highly unstable
    }
}
```

**trust_score(trust_source) -> [0.0, 1.0]**
```rust
fn trust_score(trust_source: &str) -> f32 {
    match trust_source {
        "human" => 1.0,
        "system" => 0.7,
        "agent" => 0.5,
        _ => 0.3,
    }
}
```

Human-authored entries have inherently higher baseline trust than agent-authored ones.

### Why Additive Instead of Multiplicative

The original multiplicative formula `base * usage * freshness * correction * helpfulness` has these problems:
1. **Zero-factor collapse**: If any factor is 0, the entire confidence is 0. An entry that has never been accessed (usage=0) has zero confidence regardless of everything else.
2. **Multiplicative amplification of gaming**: Inflating usage from 0.5 to 1.0 doubles the composite score. In the additive model with 15% weight, it changes from 0.075 to 0.15 -- a much smaller absolute impact.
3. **Non-linear interactions**: In a multiplicative formula, gaming two factors simultaneously has a superlinear payoff. In additive, the payoff is linear.

The additive model with clamped [0, 1] components and fixed weights provides **bounded, predictable, independently gameable** factors. Gaming one factor can improve confidence by at most its weight (0.15), not the entire score.

### Gaming Analysis of the Recommended Formula

**Attack: Loop-to-boost access_count**
- With session dedup: capped at 1 access per session per agent per entry
- With log transform: 1000 sessions -> usage_score = 1.0 (maximum)
- Impact: +0.15 on confidence (15% of total)
- Other signals unaffected: trust_source, correction_count, freshness, helpfulness unchanged
- Verdict: **marginal impact, high effort**

**Attack: Helpful-flag stuffing**
- With session dedup: capped at 1 vote per session per agent per entry
- With Wilson score: 10 votes of helpful:true -> wilson_lower ~0.72 (not 1.0)
- Impact: +0.11 on confidence (from neutral 0.075 to 0.108)
- Verdict: **marginal impact, requires many sessions**

**Attack: Combined access inflation + helpful stuffing**
- Maximum achievable: +0.15 (usage) + ~0.075 (helpfulness) = +0.225
- Out of a maximum total confidence of 1.0, the attacker gains 22.5%
- Compare to naive formula: +100% (loop-to-boost access_count linearly multiplies confidence)
- Verdict: **dramatically reduced gaming payoff**

**Attack: Targeted suppression (never mark good entries helpful)**
- Impact: helpfulness_score stays at 0.5 (neutral) instead of rising
- Maximum suppression: 0.075 lower than if properly marked
- Verdict: **minimal impact; neutral is the default, not zero**

### What crt-002 Needs from crt-001

| Data | Source | Used In |
|------|--------|---------|
| `access_count` | EntryRecord (deduped) | `usage_score()` |
| `last_accessed_at` | EntryRecord (always updated) | `freshness_score()` |
| `created_at` | EntryRecord (existing) | `freshness_score()` |
| `helpful_count` | EntryRecord (deduped, new) | `helpfulness_score()` |
| `unhelpful_count` | EntryRecord (deduped, new) | `helpfulness_score()` |
| `correction_count` | EntryRecord (existing) | `correction_score()` |
| `version` | EntryRecord (existing) | `correction_score()` |
| `trust_source` | EntryRecord (existing) | `trust_score()` |

All required data is either already on EntryRecord or is added by the modified crt-001 SCOPE. No additional tables, no AUDIT_LOG mining required for the base formula. Outcome correlation (Layer 3) is a future enhancement that adds a signal to the composite without changing the structure.

---

## 5. Summary of Changes to crt-001

| Aspect | Current SCOPE | Recommended Change | Rationale |
|--------|--------------|-------------------|-----------|
| `helpful_count` field | Single `u32` | Keep as-is | No change |
| `unhelpful_count` field | Not present | Add `u32` after `helpful_count` | Enables Wilson score (two-outcome trial) |
| Recording: access dedup | None (every call increments) | In-memory HashSet per session | Blocks loop-to-boost |
| Recording: vote dedup | None (every call increments) | In-memory HashSet per session | Blocks helpful-flag stuffing |
| Recording: last_accessed_at | Updated on every retrieval | Still updated on every retrieval | Recency is not gameable |
| FEATURE_ENTRIES | Unchanged | Unchanged | No gaming concern |
| AuditLog query | Unchanged | Unchanged | No gaming concern |
| Schema migration | v1 -> v2 (one field) | v1 -> v2 (two fields) | Minimal additional cost |
| Server state | No dedup state | Add `UsageDedup` struct | ~64 bytes per unique (agent, entry) pair |

**Lines of code impact**: ~50-80 additional LOC for the dedup logic and the extra field. No architectural changes. No new tables. No new crates.

---

## References

### Statistical Methods
- [How Not To Sort By Average Rating -- Evan Miller](https://www.evanmiller.org/how-not-to-sort-by-average-rating.html) (Wilson score interval for ranking)
- [How Reddit Ranking Algorithms Work -- Amir Salihefendic](https://medium.com/hacking-and-gonzo/how-reddit-ranking-algorithms-work-ef111e33d0d9)
- [Reddit's Comment Ranking Algorithm -- Possibly Wrong](https://possiblywrong.wordpress.com/2011/06/05/reddits-comment-ranking-algorithm/)

### Recommendation Systems
- [Recommending for Long-Term Member Satisfaction at Netflix](https://netflixtechblog.com/recommending-for-long-term-member-satisfaction-at-netflix-ac15cada49ef)
- [Cross the Chasm with RAG: Implicit Feedback and Click-Through Data](https://medium.com/thirdai-blog/cross-the-chasm-with-rag-implicit-feedback-and-click-through-data-a9eee6e7ec47)

### Time Decay
- [Forward Decay: A Practical Time Decay Model for Streaming Systems -- DIMACS/Rutgers](https://dimacs.rutgers.edu/~graham/pubs/papers/fwddecay.pdf)
- [Exponential Moving Averages at Scale -- ODSC](https://odsc.com/blog/exponential-moving-averages-at-scale-building-smart-time-decay-systems/)

### Adversarial RAG and Knowledge Poisoning
- [PoisonedRAG: Knowledge Corruption Attacks -- arXiv:2402.07867](https://arxiv.org/abs/2402.07867)
- [TrustRAG Defense Framework](https://medium.com/@techsachin/trustrag-rag-defense-framework-to-counter-attacks-from-malicious-injected-docs-996f5cd4af21)
- [A-MemGuard: Proactive Defense for LLM Agent Memory -- arXiv:2510.02373](https://www.arxiv.org/pdf/2510.02373)
- [MemoryGraft: Persistent Compromise of LLM Agents -- arXiv:2512.16962](https://arxiv.org/html/2512.16962v1)
- [When AI Remembers Too Much -- Palo Alto Unit 42](https://unit42.paloaltonetworks.com/indirect-prompt-injection-poisons-ai-longterm-memory/)

### Click Fraud and Anti-Gaming
- [AI-Based Techniques for Ad Click Fraud Detection -- MDPI](https://www.mdpi.com/2224-2708/12/1/4)
- [PPC Click Fraud 2024 vs 2025 -- PPC Shield](https://www.ppcshield.io/blog/ppc-click-fraud-2024-vs-2025-comparison/)

### Unimatrix Internal References
- `product/features/crt-001/SCOPE.md` -- Current naive design under review
- `product/research/ass-008/ANALYSIS.md` -- Agent authentication research (Citadel architecture)
- `product/research/ass-008/RESEARCH-adversarial-attacks.md` -- 31 attack vectors
- `product/PRODUCT-VISION.md` -- Milestone 4 definition (crt-001 through crt-004)
- `crates/unimatrix-store/src/schema.rs` -- Current EntryRecord schema
- `crates/unimatrix-server/src/audit.rs` -- AUDIT_LOG and AuditEvent
- `crates/unimatrix-server/src/registry.rs` -- Trust levels and capabilities
