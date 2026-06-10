# crt-053-researcher — Phase 1 Final Pass Report

## Task
Update the broadened SCOPE.md with the human's LOCKED normative decisions and add the NEW
second-surface dimension (`context_briefing`), verifying all code claims against source.

## Output
- SCOPE.md: `/workspaces/unimatrix/product/features/crt-053/SCOPE.md`

## What changed in SCOPE.md
- Reframed Problem Statement as a re-evaluation of retrieval behavior across BOTH
  `context_search` and `context_briefing`, for status-trust + relevance/cost balance.
- Added **Resolved Decisions** section (Q1, Q2, Q3, Q4, Q6, Q7 LOCKED) with accurate
  correctness notes (Q1 = `find_terminal_active`/supersession path, NOT PPR; #406 root cause =
  graph-snapshot construction; Q6 = penalty steepness, leak fixes mandatory regardless).
- Added **`context_briefing` Signal/Stage Map** (B0–B4) + how Strict mode changes the search
  stages for briefing.
- Added **Search-vs-Briefing Divergence Table** (shared status-trust requirements vs.
  usage-justified divergences).
- Extended **enforcement-sites** to ES-8 (Strict-path leak coverage) and ES-9 (briefing B3
  post-filter).
- Updated Goals (+Goal 8 briefing), ACs (+AC-12/AC-13 briefing), Approach, Non-Goal 4 caveat,
  Constraints (+C-10/11/12), Dependencies, Scope Split (briefing in-feature; #405 split-out
  locked), Open Questions (Q5/Q8/Q-B1/Q-B2/Q-B3 remain), Tracking.
- Preserved: search signal/stage map, zero-sum trade-off analysis (Policies A–D), equation
  reconciliation, ES-1…ES-7. Marked the recommended direction RATIFIED with the Q6 steepness
  nuance.

## Briefing Signal/Stage Map (summary)
`context_briefing` → `IndexBriefingService::index()` (crt-027), which **delegates to
`SearchService::search()` in `RetrievalMode::Strict`** then post-processes:
- B0 derive query (task → session current_goal → topic-ID) — agent did NOT supply query terms.
- B1 params: `retrieval_mode: Strict`, `feature_tag: None`, `current_phase: None`.
- B2 delegate to full search pipeline (Strict).
- B3 **defensive Active-only post-filter** (`status==Active`) OUTSIDE SearchService.
- B4 map→IndexEntry, re-sort, truncate k=20.

Verified consequences of Strict for briefing:
- 6a HARD-FILTERS (`retain(Active && superseded_by.is_none())`) BEFORE injection; no
  penalty_map built.
- 6b terminal redirect is a **no-op** in Strict (sources removed at 6a) → **briefing has no
  terminal-active redirect today**.
- 6d.0/6d.5 PPR/graph injection run **regardless of mode** (quarantine-filter only) → stale
  neighbors injected post-6a leave `search()`; caught ONLY by B3, which **misses
  superseded-Active**.
- ADR-007 feature boost and col-031 phase affinity are **documented but INACTIVE**
  (`feature_tag: None`, `current_phase: None`) — they described the deleted `BriefingService`.

## Search-vs-Briefing Divergence (summary)
- **SHARED status-trust requirements** (fix once inside SearchService, repairs both):
  ES-4/5/6 injection leaks; superseded-Active gap; Strict-path coverage (ES-8).
- **Justified divergences:** Strict (briefing) vs. Flexible (search); briefing drops stale
  rather than penalizes; briefing has no phase context; feature-boost relevant only to
  briefing.
- **Briefing-specific OPEN decisions:** redirect-vs-exclude for stale matches (Q-B1); revive
  feature/phase boost (Q-B2); add `superseded_by` to B3 or rely on ES-5 (Q-B3).

## Remaining Open Questions for the human
- **Q5** — exact relevance/cost metric and acceptable regression bound (does "cost" need
  explicit token/time measurement, or is P@5+MRR on active targets enough?).
- **Q8** — specific penalty-steepness target/magnitude guaranteeing connected-but-deprecated <
  weakest active; expressed as penalty floor, post-blend re-penalty, or PPR-sim clamp?
- **Q-B1** — should `context_briefing` REDIRECT stale matches to terminal active head, or
  EXCLUDE (proactive surface, no query)?
- **Q-B2** — revive ADR-007 feature boost / phase affinity as a briefing relevance lever, or
  defer as relevance-tuning out of scope?
- **Q-B3** — add `superseded_by.is_none()` to briefing's B3 as defense-in-depth, or rely on
  the SearchService-level ES-5 fix?

## Code verification (against source)
- `index_briefing.rs`: Strict mode (L182), `feature_tag: None` (L179), `current_phase: None`
  (L185), B3 post-filter (L206–210), k=20 hardcoded.
- `search.rs`: Strict retain (L712–717), 6b injection from `superseded_entries`
  (L744–803), 6d.5 PPR-only quarantine-only filter + `penalty=1.0` push (L1053–1075),
  6d.0 quarantine-only (L927), Step 7 `unwrap_or(1.0)` (L1261).
- ADRs read in full: #481 (RetrievalMode), #97 (ADR-007 feature boost), #270 (ADR-002
  delegate), #3244 (crt-027 IndexBriefingService) — confirmed ADR-007/ADR-002 describe the
  DELETED BriefingService; live path differs.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — returned briefing/retrieval ADRs (#270, #95,
  #4369, #284, #1546, #4113, #864, #97, #3244, #283, #3397, #481, …); used #97/#270/#481/#3244
  for the briefing analysis.
- Stored: nothing — attempted to store the reusable pattern "defensive post-filter outside a
  shared pipeline is not in-pipeline enforcement" via context_store, but the call returned
  `Agent 'anonymous' lacks Write capability`. Recommend the leader store this pattern
  (topic: unimatrix-server, category: pattern) post-session.
