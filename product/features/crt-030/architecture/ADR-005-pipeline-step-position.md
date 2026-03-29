## ADR-005: PPR Pipeline Position — Step 6d Between 6b and 6c

### Context

The search pipeline in `search.rs` has the following step structure before crt-030:

```
Step 3:  Embed query
Step 5:  HNSW search
Step 6:  Fetch entries, quarantine filter
Step 6a: Status penalty marking
Step 6b: Supersession candidate injection
Step 6c: Co-access boost prefetch
Step 7:  NLI scoring + fused score computation + sort + truncate
Step 9:  Truncate to k (safety)
Step 10: Floor scoring
Step 10b: Contradicts collision suppression (col-030)
```

PPR must insert after HNSW (which provides the seed set) and before NLI scoring (Step 7,
which computes the final ranking). The question is whether PPR runs before or after
Step 6c (co-access boost prefetch).

Two options:
1. **PPR after Step 6c (6c → PPR as new 6d)**: Co-access prefetch runs over the HNSW
   pool only. PPR-surfaced entries do not receive a co-access boost.
2. **PPR before Step 6c (PPR as new 6d, then 6c)**: Co-access prefetch runs over the
   full expanded pool (HNSW + PPR entrants). PPR-surfaced entries with co-access history
   receive their boost.

SCOPE.md Background Research contains an inconsistency: the step numbering table lists
"Step 6c: co-access boost prefetch" without mentioning PPR, and the Background Research
text says PPR runs "after co-access boost prefetch (Step 6c), before NLI (Step 7)" —
contradicting Goals item 2 and the Proposed Approach section, which both specify
"BEFORE co-access prefetch (Step 6c)".

The SR-03 risk item in SCOPE-RISK-ASSESSMENT.md identifies this contradiction as High/High.

### Decision

PPR is inserted as **Step 6d** between Step 6b and Step 6c. The authoritative pipeline
order is: `6b → 6d (PPR) → 6c (co-access prefetch) → 7 (NLI)`.

This is the order specified in SCOPE.md Goals item 2 and the Proposed Approach section.
The Background Research section's alternative phrasing is stale and incorrect.

Rationale for 6d-before-6c:
- Co-access prefetch builds a `boost_map` over `result_ids` — the IDs of all candidates
  currently in `results_with_scores`. If PPR runs before co-access prefetch, PPR-surfaced
  entries are included in `result_ids` and participate in the boost map.
- PPR activates `GRAPH_EDGES.CoAccess` edges as a relevance channel. Placing PPR before
  Step 6c means CoAccess-connected entries surfaced by PPR also receive their co-access
  boost — creating a consistent signal path.
- Placing PPR after Step 6c would silently zero-out `coac_norm` for all PPR-surfaced
  entries, defeating the stated goal of activating CoAccess as a positive channel.

Step 6b (supersession injection) runs before PPR because injected terminal entries may
themselves be graph neighbors. Ordering 6b → 6d ensures the full supersession-expanded
pool is the PPR seed base.

### Consequences

- PPR-surfaced entries receive co-access boosts from Step 6c, providing the full signal
  treatment for every candidate in the pool.
- Step 6c's anchor-based co-access computation (`anchor_ids = top 3 results`) runs over
  the expanded pool — the anchors remain the top HNSW/PPR-blended entries.
- The step numbering in code comments in `search.rs` must be updated to reflect the
  insertion of Step 6d. Step 6c does not change its number — it is still the co-access
  prefetch; its position shifts relative to PPR but its label is retained.
- All future features inserting steps between 6b and 7 must consider this ordering
  carefully: anything before Step 6c runs over the PPR-expanded pool.
