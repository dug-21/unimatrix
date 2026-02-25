# crt-002: Confidence Evolution

## Problem Statement

Unimatrix has a `confidence: f32` field on every EntryRecord. It has been `0.0` since nxs-001. No code reads it for ranking, no code writes it based on evidence. It is displayed to agents in every response (`**Confidence:** 0.00`) but communicates nothing.

Meanwhile, crt-001 (merged) now populates the raw signals that confidence should derive from: `access_count` (session-deduped), `helpful_count` and `unhelpful_count` (two-counter with vote correction), and `last_accessed_at` (always-updated recency). Additional signals -- `correction_count`, `version`, `trust_source`, and `created_at` -- have existed on EntryRecord since nxs-004.

The raw data exists. The field exists. Nothing connects them.

Without computed confidence:
- Semantic search returns results ranked only by embedding similarity. An entry accessed 500 times with 95% helpful votes ranks identically to one accessed once and never voted on.
- `context_briefing` assembles orientations with no quality signal. It cannot prefer battle-tested knowledge over untested drafts.
- Agents see `confidence: 0.00` on every entry, which is actively misleading -- it suggests all knowledge is equally untrustworthy.
- The gaming resistance infrastructure built into crt-001 (session dedup, two-counter design) serves no purpose until a formula consumes its outputs.
- Downstream features (crt-003 contradiction detection, crt-004 co-access boosting) need a meaningful confidence baseline to build on.

crt-002 is the bridge from raw usage data to actionable knowledge quality signals.

## Goals

1. **Compute confidence from multiple independent signals** -- Implement an additive weighted composite formula that combines usage frequency (log-transformed), recency (exponential decay), helpfulness (Wilson score lower bound), correction history, and trust source into a single `confidence: f32` value in [0.0, 1.0].

2. **Update confidence on every retrieval** -- After crt-001's usage recording updates the raw counters, recompute confidence for each accessed entry and write the new value. Confidence evolves continuously, not in batch jobs.

3. **Recompute confidence on mutations** -- When an entry is corrected (`context_correct`) or its status changes (`context_deprecate`), recompute its confidence to reflect the mutation (correction_count changes, status changes).

4. **Seed confidence on insert** -- When a new entry is created via `context_store`, compute an initial confidence from available signals (trust_source, base score) rather than leaving it at 0.0.

5. **Boost search ranking with confidence** -- Modify semantic search to blend embedding similarity with confidence, so higher-confidence entries are preferred when similarity scores are close. Provide a confidence-weighted re-ranking that respects similarity as the primary signal while using confidence as a tiebreaker/boost.

6. **Minimum sample size guard for helpfulness** -- The Wilson score helpfulness factor must not deviate from the neutral prior (0.5) until a minimum number of votes (helpful + unhelpful) have been recorded. This defends against both boosting (a few helpful votes on a new entry) and suppression (a few unhelpful votes).

7. **Make confidence formula weights configurable** -- Store weights as constants that can be adjusted without code changes in a future configuration feature (vnc-004). The initial weights are tuned but not hardcoded in a way that prevents evolution.

## Non-Goals

- **No batch recomputation job.** Confidence is computed inline (on retrieval, on mutation, on insert). A background batch recompute of all entries is not needed. If a future feature requires retroactive recomputation (e.g., changing weights), it can trigger a one-time migration-style scan.
- **No confidence-based filtering.** crt-002 does not add a `min_confidence` parameter to retrieval tools. Filtering by confidence threshold is a future enhancement.
- **No confidence history or time series.** The current confidence value overwrites the previous one. Tracking confidence over time (for trend visualization in mtx-002) is out of scope.
- **No implicit outcome correlation.** Mining AUDIT_LOG for retrieval-then-successful-outcome patterns (Layer 3 gaming resistance) is a future enhancement. crt-002 consumes only the explicit signals on EntryRecord.
- **No agent diversity signal.** Counting unique accessor agents per entry is meaningless in the current single-agent stdio deployment. The formula is designed to accommodate this signal as an additional weighted component in the future, but does not implement it now.
- **No anomaly detection integration.** Discounting anomalous access patterns requires behavioral baselines that do not yet exist.
- **No UI for confidence tuning.** Weight adjustment via dashboard is mtx-phase work.
- **No confidence decay over time without access.** Freshness decay is computed relative to `last_accessed_at` at query time. Entries that are never accessed again will have declining freshness scores naturally. There is no background process that periodically decrements confidence.
- **No schema migration.** crt-002 does not add fields to EntryRecord. It writes to the existing `confidence: f32` field that has been 0.0 since nxs-001. No new tables are needed.

## Background Research

### Existing Confidence Field

The `confidence: f32` field exists on EntryRecord (nxs-001, line 97 of `schema.rs`). It is:
- Initialized to `0.0` on insert (`write.rs` line 47)
- Never written by any other code path
- Surfaced in JSON responses as `"confidence": entry.confidence`
- Surfaced in markdown responses as `**Confidence:** {:.2}`
- Has `#[serde(default)]` for backwards compatibility

crt-002 writes this field. No schema changes are needed.

### Recommended Formula (from crt-001 Usage Tracking Research)

The research spike (`product/features/crt-001/USAGE-TRACKING-RESEARCH.md`, Section 4) designed the formula specifically for crt-002:

```
confidence = w_base  * base_score(entry)
           + w_usage * usage_score(access_count)
           + w_fresh * freshness_score(last_accessed_at, created_at)
           + w_help  * helpfulness_score(helpful_count, unhelpful_count)
           + w_corr  * correction_score(correction_count, version)
           + w_trust * trust_score(trust_source)
```

**Recommended initial weights** (sum to 1.0):
- `w_base = 0.20` -- content quality proxy (flat 0.5 baseline initially)
- `w_usage = 0.15` -- access count, log-transformed
- `w_fresh = 0.20` -- recency of access, exponential decay
- `w_help = 0.15` -- Wilson score helpfulness (with minimum sample guard)
- `w_corr = 0.15` -- correction chain quality signal
- `w_trust = 0.15` -- trust source of creator

### Why Additive, Not Multiplicative

The product vision defined confidence as `base * usage * freshness * correction * helpfulness`. The research spike (Section 4) identified three problems:

1. **Zero-factor collapse**: Any factor at 0 zeroes the entire score. A never-accessed entry (usage=0) has zero confidence regardless of trust source or correction quality.
2. **Multiplicative gaming amplification**: Inflating one factor from 0.5 to 1.0 doubles the composite. In additive with 15% weight, it changes by at most 0.075.
3. **Superlinear interaction**: Gaming two factors simultaneously has superlinear payoff in multiplicative; linear in additive.

The additive model with clamped [0.0, 1.0] components and fixed weights provides bounded, predictable, independently-gameable factors. Gaming one factor improves confidence by at most its weight.

### Component Functions (from Research)

**usage_score(access_count) -> [0.0, 1.0]**
```
log(1 + access_count) / log(1 + MAX_MEANINGFUL_ACCESS)
```
Where `MAX_MEANINGFUL_ACCESS = 50`. Access counts beyond 50 are clamped to 1.0. The log transform collapses gaming payoff: inflating from 10 to 10000 accesses only doubles the raw signal.

**freshness_score(last_accessed_at, created_at, now) -> [0.0, 1.0]**
```
exp(-(age_hours) / half_life_hours)
```
Where `half_life_hours = 168` (1 week). Reference timestamp is `last_accessed_at` if > 0, otherwise `created_at`. Just accessed = 1.0, 1 week ago = 0.37, 2 weeks = 0.14.

**helpfulness_score(helpful, unhelpful) -> [0.0, 1.0]**
Wilson score lower bound with z = 1.96 (95% confidence). Returns 0.5 (neutral prior) when total votes < minimum sample size (5). This prevents small-sample manipulation in both directions.

**correction_score(correction_count, version) -> [0.0, 1.0]**
```
0 corrections = 0.5 (neutral)
1-2 corrections = 0.8 (refined, higher trust)
3-5 corrections = 0.6 (somewhat unstable)
6+ corrections = 0.3 (highly unstable)
```

**trust_score(trust_source) -> [0.0, 1.0]**
```
"human" = 1.0, "system" = 0.7, "agent" = 0.5, unknown = 0.3
```

**base_score(entry) -> [0.0, 1.0]**
Initial implementation: flat 0.5 for all entries. Can be enhanced later with content quality heuristics (title present, content length, tags present).

### Gaming Analysis (with Full Formula)

From the research spike's gaming analysis (Section 4):

| Attack | With crt-001+002 defenses | Impact on confidence |
|--------|--------------------------|---------------------|
| Loop-to-boost access_count | Capped at 1/session (dedup), log-transformed, 15% weight | Max +0.15 |
| Helpful-flag stuffing | Capped at 1 vote/session, Wilson-scored, 15% weight | Max ~+0.075 |
| Combined worst-case | Both attacks together | Max +0.225 (22.5%) |
| Naive (no defenses) | Linear counts, multiplicative formula | +100% |
| Targeted suppression | Helpfulness stays neutral (0.5), not zero | Max -0.075 |

### Integration Points in Existing Code

**Where confidence is read (responses)**:
- `crates/unimatrix-server/src/response.rs` -- formats confidence in JSON and markdown for all retrieval tools

**Where confidence should be written**:
- After `record_usage()` in the retrieval path (recompute and write)
- After `context_store` handler (seed initial confidence on insert)
- After `context_correct` handler (correction_count changed)
- After `context_deprecate` handler (status changed -- deprecated entries could have reduced base_score)

**Existing update path**: `Store::update(entry: EntryRecord)` takes a full EntryRecord and writes it. crt-002 can use this, but it re-serializes all fields. A more targeted approach would be to read-modify-write just the confidence field. However, since redb serializes the whole record anyway, the overhead is minimal.

### Search Re-ranking

Currently, `context_search` returns results sorted by embedding similarity only. crt-002 adds confidence as a secondary signal:

```
final_score = alpha * similarity + (1 - alpha) * confidence
```

Where `alpha = 0.85` (similarity remains dominant). This means:
- Two entries with similarity 0.92 and 0.91 -- the one with higher confidence wins
- An entry with similarity 0.70 cannot beat one with 0.95 regardless of confidence
- The blend is applied after the top-k retrieval from the vector index, as a re-ranking step

This does not change the HNSW search itself (which operates on raw embeddings). It re-ranks the returned candidates.

### Confidence Update Approach

There are two architectural choices for when confidence is computed:

**Option A: Compute on write (inline)**
After each usage recording or mutation, recompute confidence for affected entries and write the result. The `confidence` field is always up-to-date.

Pros: Reads are fast (just read the precomputed value). No stale values.
Cons: Adds computation + write overhead to every retrieval and mutation.

**Option B: Compute on read (lazy)**
Compute confidence at response-formatting time from the raw signals. Never write to the `confidence` field (or write it periodically).

Pros: No write overhead on reads. Formula changes take effect immediately.
Cons: Inconsistency -- the stored `confidence` field diverges from the computed value. Other tools that read the raw entry get stale confidence.

**Decision: Option A (compute on write).**

The write overhead is minimal (one f32 field update in an already-open write transaction). The benefit is that the stored `confidence` value is always consistent -- any code that reads EntryRecord gets the current confidence without needing to know the formula. This includes future features (crt-003, crt-004), MCP resource subscriptions, and the dashboard (mtx-002).

The freshness component is time-dependent and will naturally become stale between accesses. This is acceptable: an entry's freshness score only matters when it is being retrieved (at which point it gets recomputed). Between retrievals, the stored confidence reflects the freshness at the time of last access, which is the best available approximation.

## Proposed Approach

### Confidence Module

Create a `confidence.rs` module in `unimatrix-server` containing:

1. **Weight constants** -- Six `f32` constants summing to 1.0
2. **Component functions** -- Six pure functions, each mapping inputs to [0.0, 1.0]
3. **Wilson score implementation** -- Lower bound calculation with minimum sample guard
4. **`compute_confidence(entry: &EntryRecord, now: u64) -> f32`** -- The composite function that calls all components and returns the weighted sum
5. **`SEARCH_SIMILARITY_WEIGHT: f32 = 0.85`** -- Blend factor for search re-ranking

### Integration Points

**On retrieval (after usage recording)**:
In `record_usage_for_entries()`, after the usage write transaction commits, read back the updated entries, compute new confidence for each, and write the updated confidence values. This happens in the same fire-and-forget async block as usage recording -- confidence update failures are logged but do not affect the response.

**On insert (context_store)**:
After the entry is inserted, compute initial confidence from available signals (trust_source gives trust_score, base_score is 0.5, everything else starts at default values) and update the entry's confidence field.

**On correction (context_correct)**:
After the correction chain update, recompute confidence for the corrected entry (correction_count has changed).

**On deprecation (context_deprecate)**:
Deprecated entries could receive a reduced base_score (e.g., 0.2 instead of 0.5). Recompute after status change.

**On search (re-ranking)**:
After the vector search returns top-k candidates, apply the similarity-confidence blend and re-sort.

### Module Placement

The confidence computation belongs in `unimatrix-server` (not `unimatrix-store` or `unimatrix-core`) because:
- It depends on server-level constants (weights, thresholds)
- It is consumed by tool handlers and the re-ranking logic
- The store layer remains a pure data layer; the server layer owns business logic
- Future configuration (vnc-004) will load weights from server config, not store config

### No New Traits

crt-002 does not extend the `EntryStore` trait. The confidence update uses the existing `Store::update()` method (via the trait's `update` method) to write back entries with updated confidence. No trait changes are needed.

However, a more efficient path would be a targeted confidence-only update to avoid re-serializing and re-indexing unchanged fields. This can be implemented as a `Store::update_confidence(id: u64, confidence: f32)` method that reads, modifies only the confidence field, and writes -- avoiding the full index diff that `update()` performs.

## Acceptance Criteria

- AC-01: `compute_confidence(entry, now)` returns a value in [0.0, 1.0] for any valid EntryRecord
- AC-02: The confidence formula uses six weighted components (base, usage, freshness, helpfulness, correction, trust) with weights summing to 1.0
- AC-03: usage_score applies `log(1 + access_count) / log(1 + MAX_MEANINGFUL)` and clamps to [0.0, 1.0]
- AC-04: freshness_score applies exponential decay with configurable half-life, using `last_accessed_at` (or `created_at` as fallback) relative to current time
- AC-05: helpfulness_score returns 0.5 (neutral) when total votes (helpful + unhelpful) < MINIMUM_SAMPLE_SIZE (5), and Wilson score lower bound otherwise
- AC-06: correction_score returns higher values for entries with 1-2 corrections (refined) than uncorrected entries, and lower values for highly-corrected entries (unstable)
- AC-07: trust_score maps "human" > "system" > "agent" > unknown
- AC-08: base_score returns 0.5 for all entries (initial implementation)
- AC-09: Confidence is recomputed and written after every successful retrieval that triggers usage recording (same fire-and-forget path as crt-001 usage updates)
- AC-10: Confidence is computed and written when a new entry is created via `context_store`
- AC-11: Confidence is recomputed and written when an entry is corrected via `context_correct`
- AC-12: Confidence is recomputed and written when an entry is deprecated via `context_deprecate`
- AC-13: `context_search` re-ranks results using `alpha * similarity + (1 - alpha) * confidence` where alpha = 0.85 (similarity dominant)
- AC-14: The re-ranking step operates on the existing top-k candidates from the vector index -- it does not change the HNSW search itself
- AC-15: Wilson score lower bound uses z = 1.96 (95% confidence level)
- AC-16: All six weight constants and the search blend alpha are defined as named constants (not magic numbers inline)
- AC-17: A targeted `update_confidence(id, confidence)` method avoids full index-diff overhead on confidence-only updates
- AC-18: Deprecated entries receive a reduced base_score (0.2) compared to active entries (0.5)
- AC-19: Confidence updates on the retrieval path do not block or delay the tool response (fire-and-forget, same as usage recording)
- AC-20: All component functions are pure (deterministic given inputs, no side effects) and independently unit-testable
- AC-21: The Wilson score implementation handles edge cases: n=0 (returns 0.5), all helpful (returns lower bound, not 1.0), all unhelpful (returns lower bound, not 0.0)
- AC-22: Existing retrieval tool behavior is unchanged -- same results, same response formats, same parameters. Confidence values in responses change from 0.00 to computed values. Search result ordering may change due to re-ranking.

## Constraints

- **No schema changes.** crt-002 writes to the existing `confidence: f32` field. No new fields, no new tables, no migration.
- **bincode round-trip.** Writing confidence requires read-modify-write of the full EntryRecord (bincode is positional). The `update_confidence` method must deserialize, modify confidence, re-serialize, and write. Index tables are not modified (confidence is not indexed).
- **Fire-and-forget pattern.** Confidence updates on the retrieval path must follow the same fire-and-forget pattern as crt-001 usage recording. A confidence computation failure must not fail the retrieval.
- **Synchronous store, async server.** The confidence computation and store write are synchronous (`Store::update_confidence`). The server calls them via `spawn_blocking` in the same async block as usage recording.
- **Object-safe EntryStore trait.** If any trait methods are added, they must maintain object safety (no `&mut self`, no generics). However, crt-002 is not expected to need trait changes -- it calls the concrete `Store` type via `spawn_blocking`.
- **No background tasks.** The server has no scheduler or background thread pool. Confidence is computed inline, triggered by user-facing operations. The freshness component becoming stale between accesses is acceptable.
- **Weight sum invariant.** The six weights must sum to exactly 1.0. Changing one weight requires adjusting others. This is enforced by a test.

## Decisions

1. **Additive weighted composite, not multiplicative.** The research spike demonstrated that multiplicative has zero-factor collapse and superlinear gaming amplification. Additive with clamped [0,1] components bounds gaming impact to at most the component's weight.

2. **Compute on write (inline), not on read (lazy).** Stored confidence is always consistent. Any code reading EntryRecord gets current confidence without knowing the formula. The write overhead is minimal.

3. **Confidence module in unimatrix-server, not unimatrix-store or unimatrix-core.** The formula is business logic (weights, thresholds, statistical functions). The store is a data layer. The core defines traits. Business logic lives in the server.

4. **Targeted update_confidence method.** Avoids the full index-diff overhead of `Store::update()`. Confidence is not indexed, so only the ENTRIES table write is needed.

5. **Wilson score with minimum sample guard (n >= 5).** Prevents both boosting (a few helpful votes on a new entry inflate helpfulness) and suppression (a few unhelpful votes crater it). Below 5 votes, helpfulness is neutral (0.5).

6. **Search re-ranking at alpha=0.85.** Similarity remains the dominant signal. Confidence is a tiebreaker that nudges close results toward higher-quality entries. An entry with dramatically lower similarity cannot beat a closer match regardless of confidence.

7. **Recompute on deprecation with reduced base_score.** Deprecated entries are explicitly lower-quality. A base_score of 0.2 (vs 0.5 for active) reflects this. If a deprecated entry is later re-activated, confidence will be recomputed with the active base_score at the next retrieval.

8. **No confidence floor.** Confidence can be 0.0. This only happens for entries with no usage, no votes, and the lowest trust source -- which accurately reflects zero confidence in the entry's quality. The product vision mentioned a 0.1 floor, but this creates false confidence signals. Zero is honest.

9. **Freshness staleness is acceptable.** Between accesses, the stored freshness component reflects the time of last access, not "now." This is the correct behavior: an entry's stored confidence should reflect its state at last observation, not drift in real-time. It gets corrected at the next retrieval.

## Tracking

https://github.com/dug-21/unimatrix/issues/30
