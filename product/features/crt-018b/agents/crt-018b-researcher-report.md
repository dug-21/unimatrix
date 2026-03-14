# crt-018b Researcher Report

## Agent ID
crt-018b-researcher

## SCOPE.md Location
`/workspaces/unimatrix/product/features/crt-018b/SCOPE.md`

## Key Findings

### crt-018 Infrastructure (Verified in Codebase)

crt-018 is fully merged and operational. The effectiveness module exists at
`crates/unimatrix-engine/src/effectiveness/mod.rs` and provides:

- `EffectivenessCategory` enum: Effective, Settled, Unmatched, Ineffective, Noisy
- `EntryEffectiveness` struct with `entry_id`, `category`, `injection_count`, `success_rate`, `helpfulness_ratio`
- `EffectivenessReport` struct (complete with `by_category`, `by_source`, `calibration`, `top_ineffective`, `noisy_entries`, `unmatched_entries`)
- `classify_entry()`, `utility_score()`, `build_report()` — pure functions
- `INEFFECTIVE_MIN_INJECTIONS = 3`, `NOISY_TRUST_SOURCES = &["auto"]`
- Store layer: `compute_effectiveness_aggregates()` and `load_entry_classification_meta()`
- StatusService Phase 8 integration in `status.rs`

**Critical gap**: Classifications are computed transiently on each `context_status` call and
stored only in `StatusReport.effectiveness`. They are never persisted, cached, or made
available to the search or briefing pipelines.

### crt-019 Infrastructure (Verified in Codebase)

crt-019 is fully merged. The confidence formula post-crt-019:
- Weights: W_BASE=0.16, W_USAGE=0.16, W_FRESH=0.18, W_HELP=0.12, W_CORR=0.14, W_TRUST=0.16
- `rerank_score(similarity, confidence, confidence_weight) -> f64` — three-parameter signature
- `ConfidenceState { alpha0, beta0, observed_spread, confidence_weight }` held as `Arc<RwLock<ConfidenceState>>`
- SearchService holds `ConfidenceStateHandle` and snapshots `confidence_weight` at the top of `search()`
- Adaptive blend: `confidence_weight = clamp(spread * 1.25, 0.15, 0.25)`

### Search Re-Ranking (Verified in Codebase)

Four `rerank_score` call sites in `search.rs`:
- Line 294: Step 7 initial sort (base_a)
- Line 295: Step 7 initial sort (base_b)
- Lines 346–347: Step 8 co-access re-sort
- Line 389: Step 11 final ScoredEntry score

Current formula: `(1 - cw) * similarity + cw * confidence` where `cw` is from `ConfidenceState`.
Existing additive adjustments: co-access boost (max 0.03), provenance boost (0.02), multiplicative
penalties for Deprecated (0.7×) and Superseded (0.5×).

No effectiveness signal exists anywhere in this pipeline today.

### Briefing Assembly (Verified in Codebase)

Two paths without effectiveness:
1. Injection history (`process_injection_history`): dedup by entry_id, **sort by confidence descending**
2. Convention lookup: query Active conventions by topic/role, **sort by confidence descending**
   (feature-tagged entries promoted first)

The semantic search path already goes through `SearchService::search()` which will benefit
automatically from the search re-ranking change.

### Quarantine Mechanism (Verified in Codebase)

`quarantine_with_audit(entry_id, reason, audit_event)` on `UnimatrixServer`:
- Sets `Status::Quarantined`, stores `pre_quarantine_status` for restore
- Writes audit event
- Calls `confidence.recompute(&[entry_id])` fire-and-forget
- No async calls; suitable for use within `spawn_blocking`

**No automated quarantine exists.** crt-018 explicitly excluded it (Non-Goal #2). crt-018b
must implement the persistence layer (in-memory consecutive cycle counter) and the trigger.

### Pattern for Runtime-Variable State Shared Across Services

The `ConfidenceStateHandle` pattern (crt-019) is the model to follow:
- New state struct held in `Arc<RwLock<_>>`
- StatusService writes at end of Phase 8 (short write lock)
- SearchService/BriefingService read at top of request handler (short read lock, clone needed value)
- Wired at server construction time

This same pattern applies to `EffectivenessState` for crt-018b.

## Proposed Scope Boundaries

### In Scope (four changes)

1. **EffectivenessState cache** — new in-memory `Arc<RwLock<HashMap<u64, EffectivenessCategory>>>`
   updated by Phase 8, read by SearchService and BriefingService
2. **Search re-ranking utility delta** — additive `±UTILITY_BOOST/PENALTY` applied at all
   four `rerank_score` call sites in search.rs
3. **Briefing sort tiebreaker** — effectiveness category as secondary sort key in injection
   history and convention paths
4. **Auto-quarantine with N-cycle guard** — in-memory consecutive bad cycle counter,
   `AUTO_QUARANTINE_CYCLES` env var (default 3, 0 = disabled), trigger in Phase 8

### Out of Scope (confirmed by issue #206 priority ordering)

- Retrospective "knowledge-that-helped-this-topic" (P2, separate feature)
- Embedding tuning from effectiveness labels (P5, requires ML infrastructure)

## Open Questions for Human

1. **Utility delta magnitude**: 0.05 (symmetric) is the proposed default for UTILITY_BOOST and
   UTILITY_PENALTY. Should these be asymmetric (e.g., 0.03 boost / 0.07 penalty) to suppress
   bad entries more aggressively than boosting good ones?

2. **Settled entries**: Should entries classified Settled receive a small positive boost
   (+0.01) in addition to Effective (+0.05)? They served their era well, but their topic is
   inactive — modest credit seems fair.

3. **Auto-quarantine default threshold**: `AUTO_QUARANTINE_CYCLES = 3` is conservative (3
   manual `context_status` calls required). Is this conservative enough, or should the default
   be higher (e.g., 5) for initial rollout?

4. **Status report visibility**: Should auto-quarantined entries be listed in the
   `context_status` effectiveness output (e.g., `auto_quarantined_this_cycle: Vec<u64>`) so
   operators can see what was actioned?

5. **Briefing semantic path**: Confirmed it automatically benefits from search re-ranking
   change (it delegates to `SearchService::search()`). No additional work needed there —
   is that the intended behavior, or should briefing semantic results apply a different
   (softer) utility delta?

## Risks and Concerns

- **Cold-start behavior is safe**: EffectivenessState starts empty; all utility deltas are
  0.0 until first `context_status` call populates it. No regression risk.
- **Auto-quarantine is irreversible without manual restore**: The `restore` action on
  `context_quarantine` can reverse it, but this requires human intervention. The N-cycle
  guard and env-var override are the primary safeguards.
- **In-memory counter resets on server restart**: This is intentional (conservative) but
  means a server restart resets the clock. In production, servers may restart during deploys —
  the operator should be aware.
- **EffectivenessState is only populated when context_status is called**: If `context_status`
  is never called, search continues with zero utility deltas indefinitely. This matches the
  current UDS usage pattern where status is called periodically by the maintenance tick
  (via background.rs). Confirm this background tick calls status regularly.

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for "effectiveness scoring retrieval re-ranking" — MCP tools
  not available in this session; query not executed
- Stored: pattern #N/A "Query-time effectiveness signal pattern for search re-ranking" via
  `/uni-store-pattern` — MCP tools not available in this session; storage attempted but
  could not complete. Pattern description recorded here for manual entry:
  > **What**: Cache effectiveness classifications in EffectivenessState (Arc<RwLock<HashMap>>),
  > snapshot at top of search(), apply additive utility delta to rerank_score calls
  > **Why**: Avoids DB round-trip per search call; cold-start safe (empty map = 0.0 deltas);
  > follows established ConfidenceStateHandle pattern from crt-019
  > **Scope**: unimatrix-server search pipeline, crt-018b
