# ASS-034: Findings — Parameterized Relationship Taxonomy and Autonomous Detection

**Date**: 2026-03-31
**Database**: `~/.unimatrix/0d62f3bf1bf46a0a/unimatrix.db`

---

## Sharpened Objective

The goal is **not** to connect entries that are already linked by other means (intra-feature
co_access, supersession chains, same-session access). The goal is to find relationships across
structurally disconnected entries — specifically: **a lesson or pattern from a past feature
that directly constrains or informs a decision in a later feature, where semantic similarity
alone was insufficient to bridge the gap at retrieval time.**

---

## Finding 1: The Gap Is Real — Zero Cross-Category Semantic Edges

Current graph edge distribution across categories (active entries only):

| From → To | Supersedes | Supports (NLI) | CoAccess | Total |
|---|---|---|---|---|
| decision → decision | 55 | 3 | (many) | – |
| lesson-learned → lesson-learned | 23 | 17 | (many) | – |
| pattern → pattern | 19 | 4 | (many) | – |
| procedure → procedure | 12 | 1 | (many) | – |
| **lesson-learned → decision** | 0 | **0** | rare | **0 semantic** |
| **pattern → decision** | 0 | **0** | rare | **0 semantic** |
| pattern → lesson-learned | 0 | 1 | rare | 1 |
| pattern → procedure | 0 | 1 | rare | 1 |
| procedure → decision | 0 | 1 | rare | 1 |

**Every single NLI Supports edge is within the same category.** The cross-category semantic
relationship space is completely unexplored. The lesson→decision bridge — the exact
relationship the user's example describes — has zero representation in the graph.

Total potential (lesson-learned, decision) pairs across different feature cycles, with no edge:
**31,680**.

---

## Finding 2: Isolated Entries Are Systematically Older

```
Category      | Connected avg age | Isolated avg age | Ratio
--------------+-------------------+------------------+------
decision      |  8.4 days         | 21.7 days        | 2.6×
pattern       | 14.5 days         | 26.9 days        | 1.9×
lesson-learned| 17.7 days         | 26.9 days        | 1.5×
procedure     | 13.7 days         | 27.0 days        | 2.0×
convention    |  9.5 days         | 33.1 days        | 3.5×
```

**The oldest institutional knowledge is the most isolated.** CoAccess accumulates within
sessions, so recent entries build edges naturally. But lessons stored during nxs-008
(March 5) never accumulated co_access with entries written during nxs-011 (March 17)
because those two features ran in separate sessions that didn't access the same entries.

The failure mode: the entries that contain the most hard-won knowledge (early-feature
lessons, foundational patterns) are the ones most likely to be invisible to later agents.

---

## Finding 3: Specific Missed Bridge Confirmed

**Lesson #376**: "DDL-before-migration ordering causes post-merge production failures"
- Written: nxs-008, March 5 (1772709449 epoch)
- Domain: `database-init`, migration sequencing in `Store::open()`
- Tags: `init-ordering`, `migration`, `nxs-008`, `post-merge-bug`
- Status: **never served to any session** (injection_log: zero rows for this entry)

**ADR #2060**: "ADR-003 nxs-011: Migration Connection Sequencing — Dedicated non-pooled
connection before pool construction"
- Written: nxs-011, March 17 (1773764946 epoch) — **12.2 days later**
- Domain: `nxs-011`, migration connection ordering in `Store::open()`
- Tags: `migration`, `connection-sequencing`, `pool-construction`, `schema-migration`
- Status: **no edge to #376**

Both entries address the same architectural concern: the ordering of migration vs. other
initialization steps in `Store::open()`. Lesson #376 says "migration MUST run before DDL."
ADR #2060 says "migration MUST run before pool construction." They're sequential layers of
the same architectural invariant — one inferred from a production failure, one designed
explicitly. An agent working on nxs-011 should have seen #376. It didn't.

Why wasn't it retrieved? Lesson #376 was never served to ANY session — its injection_log
count is zero. Even HNSW similarity wasn't bridging it. The entry exists in complete
isolation from the knowledge base's active query paths.

---

## Finding 4: NLI Explains the Gap — This Is the "Neutral Zone"

NLI cross-encoder assigns three scores: entailment, neutral, contradiction.
- **Supports edges** require `entailment > 0.6` (27 edges total, all within-category)
- **Contradicts edges** require `contradiction > 0.6` (0 active edges at present)
- **Neutral** with high cosine: **currently produces zero edges**

Lesson #376 ↔ ADR #2060:
- NLI would score: **neutral** (the lesson doesn't *logically entail* the ADR; it's a
  warning about a failure mode the ADR prevents)
- Cosine similarity: likely 0.45-0.65 (both describe migration sequencing in `Store::open()`,
  share vocabulary: migration, DDL, create_tables, ordering, init)
- HNSW pre-filter threshold is 0.5 — so this pair may not even reach NLI scoring

The neutral zone is the uncharted territory. Pairs that are:
- Semantically related (cosine 0.4-0.65)
- Not logically entailing (NLI neutral)
- Cross-category (lesson → decision)
- Temporally ordered (lesson older than decision)

**...currently produce zero edges in any graph.**

---

## Finding 5: The Signal That Discriminates Real Bridges From Noise

Three signals in combination identify genuine cross-feature semantic bridges:

**Signal A: Semantic proximity** (cosine threshold, ~0.4-0.55)
Lesson #376 and ADR #2060 share the "migration/init-ordering" semantic cluster.
But cosine alone is too broad — many lessons about "testing" or "process" will have
moderate cosine with many ADRs about testing or process.

**Signal B: Category pair direction**
lesson-learned → decision: a warning from the past informs a design choice
pattern → decision: an established technique shapes an architectural choice
lesson-learned → convention: a failure informs a rule
These pairings have directionality (older warns newer). Within-category is less meaningful
(lesson → lesson already handled by existing NLI Supports pass).

**Signal C: Cross-feature temporal separation**
`l.feature_cycle != d.feature_cycle AND l.created_at < d.created_at`
This is the key discriminator from intra-feature connections (already handled by CoAccess)
and from supersession chains (already handled by Supersedes).

Together: cosine 0.4-0.55 + category pair (lesson→decision) + cross-feature + lesson older
= candidate for a weak positive "Informs" edge.

**Noise sources to filter:**
- Retrospective lesson-learned entries (telemetry dumps) will have moderate cosine with
  many decisions. Filter: exclude entries where `content LIKE '%permission_retries%'` or
  `source = 'auto'` with `topic LIKE 'retrospective/%'`
- Very generic lessons about workflow ("token budget constraints", "worktree discipline")
  may produce false bridges with unrelated ADRs.
  Filter: cosine floor at 0.45 (stricter than the NLI pre-filter at 0.5) or category-based.

---

## Finding 6: CoAccess Promotion Doesn't Bridge Cross-Feature Either

CoAccess promotion threshold: **count ≥ 3**. Cross-feature pairs with count ≥ 3 that
haven't been promoted yet: **416**.

But even if these 416 pairs were promoted, the co_access mechanism fundamentally cannot
bridge the "never co-accessed" gap. Lesson #376 has never been co-accessed with anything
in a session working on nxs-011 — there's no co_access accumulation to promote.

The only way to bridge it is through semantic proximity, not behavioral proximity.

---

## Detection Mechanism Recommendation

### Best Case: Extend the Existing NLI Tick

The NLI detection tick (`nli_detection_tick.rs`) already:
1. Selects isolated or cross-category entry pairs where cosine > 0.5
2. Scores them with the NLI cross-encoder
3. Writes `Supports` edges for entailment > 0.6 and `Contradicts` for contradiction > 0.6
4. **Currently discards the neutral case entirely**

The minimal change: add a fourth outcome branch in the NLI tick for the neutral case:

```
if nli.neutral > 0.5
   AND category_pair in [(lesson-learned, decision), (pattern, decision),
                          (lesson-learned, convention), (pattern, convention)]
   AND source.created_at < target.created_at   // lesson older than decision
   AND source.feature_cycle != target.feature_cycle
   AND cosine ≥ 0.45
→ write "Informs" edge, weight = cosine * 0.6 (lower than Supports)
```

This requires:
1. Adding `Informs` to `RelationType` enum in `graph.rs`
2. Making `Informs` a positive edge type in PPR (alongside Supports, CoAccess, Prerequisite)
3. Adding the fourth branch in the NLI tick (≤20 lines)
4. One new config field: `nli_informs_threshold: f32` (default 0.45 cosine floor)

**No schema change.** `GRAPH_EDGES.relation_type` already stores free-text strings.
**No new ML model.** Reuses the existing NLI cross-encoder session.
**No new tick infrastructure.** Runs within the existing graph inference tick.

The key insight: the NLI session is already running on these candidate pairs — they pass
the cosine pre-filter and get scored. The score is computed but the neutral result is
currently thrown away.

### Config-Extensibility: The Right Scope Is Category Pairs, Not Relation Types

The initial SCOPE anticipated needing a full `[[inference.relation_types]]` config block
to define arbitrary new edge types. The data shows this is overkill — but partial
config-extensibility IS warranted, and at the right level.

The detection mechanism has three components. Only one is domain-specific:

| Component | Domain-specific? | Notes |
|---|---|---|
| NLI neutral zone + cosine threshold | No | NLI trained on general language (MNLI); works for any domain |
| Temporal ordering (`source.created_at < target.created_at`) | No | Universal |
| Cross-cycle separation (`feature_cycle !=`) | No | Concept generalizes to any "knowledge production context" |
| **Category pair filter** | **Yes** | `["lesson-learned", "decision"]` is Unimatrix vocabulary |

The category pair is the only domain-specific knob. The epistemic structure it captures
— **empirical knowledge informs normative decisions** — is universal:

| Domain | Empirical category | Normative category |
|---|---|---|
| Software dev (Unimatrix default) | `lesson-learned`, `pattern` | `decision`, `convention` |
| Medical | `case-report`, `adverse-event` | `treatment-protocol`, `contraindication` |
| Legal | `case-outcome`, `precedent` | `ruling`, `statute` |
| Scientific | `experiment-result`, `observation` | `hypothesis`, `theory` |
| Business | `incident-post-mortem`, `market-analysis` | `policy`, `strategic-decision` |

**Therefore**: `RelationType::Informs` stays as a fixed enum variant in code — it's a
detection outcome, not a domain concept. What varies per domain is which category pairings
trigger that detection. One `Vec<[String; 2]>` field on `InferenceConfig` with a default
covering the software development domain makes the mechanism generic:

```toml
# Default (software development domain — Unimatrix default)
[inference]
informs_category_pairs = [
  ["lesson-learned", "decision"],
  ["pattern",        "decision"],
  ["lesson-learned", "convention"],
  ["pattern",        "convention"]
]

# A medical knowledge base redeploy would configure:
# informs_category_pairs = [
#   ["case-report",   "treatment-protocol"],
#   ["adverse-event", "contraindication"]
# ]
```

**Code changes required** (revised from above):
1. `RelationType::Informs` added to enum in `graph.rs`
2. `Informs` added as positive edge in PPR (`graph_ppr.rs`)
3. Fourth branch in the NLI tick for the neutral case (≤20 lines)
4. `nli_informs_cosine_floor: f32` — cosine floor for candidate pairs (default 0.45)
5. `informs_category_pairs: Vec<[String; 2]>` — pluggable category pair list

No schema change. No new ML model. No new tick infrastructure. No `Extended(String)`
variant on `RelationType` — the full config-extensible approach remains deferred.

The distinction: config-extensible *category pairs* (2 new fields) vs. config-extensible
*relation types* (much larger architectural change). The former is what makes the platform
generic without over-engineering.

---

## Finding 7: Software Engineering Domain — Default Category Pair Definitions

The `informs_category_pairs` default for the software development domain is derived directly
from the category taxonomy and confirmed by the live data analysis. Each pair describes a
specific epistemic relationship that exists in this domain:

| Source category | Target category | Relationship meaning | Data evidence |
|---|---|---|---|
| `lesson-learned` | `decision` | A past failure or incident directly constrains a design choice | #376 (DDL ordering) → #2060 (migration sequencing ADR) — same architectural concern, 12 days apart, zero edge |
| `pattern` | `decision` | An established technique informs an architectural decision | 269 isolated patterns, 274 isolated decisions — many cross-feature domain overlaps undetected |
| `lesson-learned` | `convention` | A past failure informs a behavioral rule for future agents | Confirmed by category distribution: 233 isolated lessons, 7 isolated conventions |
| `pattern` | `convention` | An established technique becomes a codified rule | Same domain: patterns and conventions both describe "how we do things" |

**Pairs deliberately excluded from defaults:**

| Pair | Reason excluded |
|---|---|
| `lesson-learned` → `lesson-learned` | Already covered by existing NLI Supports pass (17 such edges exist) |
| `pattern` → `pattern` | Same — 4 existing Supports edges within-category |
| `decision` → `decision` | Covered by Supersedes (55 edges) and Supports (3 edges) |
| `lesson-learned` → `procedure` | Weaker signal; procedures are how-to guides, not design choices. Low priority. |
| `convention` → `decision` | Inverted temporal logic in this domain — conventions follow from decisions, not the other way |
| `procedure` → `decision` | Too rare to warrant default inclusion; explicit `context_relate` is more appropriate |

**What "Informs" means in this domain (precise definition for GRAPH_EDGES):**

> Entry A *Informs* entry B when: A is empirical knowledge from a past feature cycle
> (a lesson about what went wrong, or a pattern about what worked), B is normative
> knowledge from a later feature cycle (a decision about what to build, or a convention
> about how to behave), and their semantic content is closely related (cosine ≥ 0.45,
> NLI neutral) but no logical entailment exists between them.

This is distinct from `Supports` (logical entailment — A implies B) and from `CoAccess`
(behavioral — A and B happened to be accessed together). It captures the "institutional
memory" relationship that neither mechanism detects.

---

## Finding 8: Activity Table Retention — Edge Detection Is Isolated

Direct answer to the retention question: **edge detection reads no activity tables.**

| Table | Rows | Edge detection dependency | Retention safe? |
|---|---|---|---|
| `observations` | 165,334 | None — retrospective analysis only | **Yes. Zero impact on graph.** |
| `query_log` | 4,986 | PhaseFreqTable rebuild (PPR personalization weights, not edges) | Yes — PPR degrades gracefully to cold-start neutral weights |
| `injection_log` | 4,762 | None — audit/reporting only | **Yes. Nothing reads it for learning.** |
| `co_access` | 17,985 | **Direct feed to CoAccess graph edges** | **No — this IS the learning signal** |
| `graph_edges` | 1,443 | The graph itself | No |
| `sessions` | 292 | Session context for retrospective | Yes (sessions for active cycles should be kept) |

The full dependency chain for each edge type:

```
Supersedes:   entries.supersedes field              → graph_edges
              (no activity tables at any point)

Supports /    HNSW vector index + entries table     → NLI model → graph_edges
Contradicts:  (no activity tables at any point)

CoAccess:     context_search/get live call          → co_access table (upsert)
                                                    → co_access_promotion_tick
                                                    → graph_edges
              (activity recorded live in co_access; does NOT read injection_log
               or observations at any point)

Proposed      HNSW vector index + entries table     → NLI model (neutral branch)
Informs:      (no activity tables at any point)
```

`observations` is 165K rows and almost certainly the majority of the 197MB database.
Truncating it is safe. The one table that cannot be treated as disposable is `co_access`
— it is not an observation table; it is the accumulated co-occurrence signal that produces
CoAccess graph edges. Pruning it prunes the learning signal.

`query_log` has a secondary role: `PhaseFreqTable::rebuild()` reads it each tick to
weight PPR personalization by phase. Aggressive truncation means PPR traversal loses
phase-conditioning and falls back to neutral personalization weights — a quality
degradation, not a correctness failure. The `query_log_lookback_days` config field
(default: already defined in `InferenceConfig`) controls the window.

---

## Expected Impact

If the "Informs" detection covers cosine 0.45-0.65 + NLI neutral + cross-feature +
temporal order:

- 31,680 potential (lesson-learned, decision) cross-feature pairs
- HNSW cosine filter at 0.45 will reduce to ~500-2000 candidates (rough estimate: ~2-6%)
- NLI neutral filter and temporal/feature-cycle filter will reduce further to ~100-400 new edges
- 400 new cross-category edges → isolated entry count drops from 874 to approximately 500-600
- Isolation rate: from 79.5% → approximately 50-55%

The specifically targeted improvement: old institutional knowledge (nxs-008, vnc-008 era
lessons) becomes visible to agents working on later migration and architecture problems.

---

## What This Is Not

- **Not a taxonomy exercise**: we don't need to enumerate all possible relationship types.
  One new type ("Informs") targeting the lesson→decision cross-feature gap covers the
  primary use case.
- **Not config-extensible in v1**: hard-coding 2-3 new enum variants with configurable
  PPR weights is sufficient. Full config-extensibility deferred.
- **Not a new ML model**: reuses existing NLI infrastructure.
- **Not a schema change**: `GRAPH_EDGES.relation_type` already accepts any string.
