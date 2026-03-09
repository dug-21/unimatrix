# ASS-018: Use Cases — What Connected Data Enables

## Tier 1: Restore & Improve Retrospective

### UC-1: Multi-Session Feature Retrospective

**Before** (JSONL era): Single-pass content scan, all data in one place, worked.
**After migration**: Broken — feature_cycle always NULL.
**With connected data**: Better than before.

A connected retrospective can answer:
- How many sessions did this feature take? (sessions grouped by feature_cycle)
- What was the design-to-delivery ratio? (phase metrics across sessions)
- Did knowledge persist across sessions? (injection_log: same entries served in session N and N+1?)
- Where did rework happen? (session outcomes: which sessions ended in "rework" vs "success"?)
- Was there context loss between sessions? (Read patterns in session N+1 repeating session N)

**New metrics enabled**:
- `context_reload_pct`: % of Read calls in session N+1 that target files already read in session N
- `knowledge_reuse_rate`: % of injected entries in session N that appear again in N+1
- `session_efficiency_trend`: tool_calls/duration improving or degrading across sessions
- `rework_session_count`: sessions with outcome="rework" per feature

### UC-2: Baseline-Aware Cross-Feature Comparison

Already partially implemented (baseline.rs computes mean/stddev from historical MetricVectors).

With feature_deliveries, baselines become richer:
- Compare by feature phase (e.g., all nexus features vs all collective features)
- Compare by complexity tier (small: <3 sessions, medium: 3-6, large: 7+)
- Outlier detection at feature level, not just metric level

---

## Tier 2: Knowledge System Intelligence

### UC-3: Search Quality Evaluation

**Requires**: query_log table (query text + results)

With (query, results, outcome) triples:
- **Precision**: Of entries returned, how many were actually used? (cross-ref: injection_log entry_ids vs subsequent tool calls referencing those entries' content)
- **Recall proxy**: Sessions where agents re-search with different queries → first query missed
- **Query reformulation patterns**: Same session, multiple searches → refinement behavior
- **Zero-result analysis**: Queries that return nothing → knowledge gap candidates
- **Similarity score distribution**: Are our thresholds (SIMILARITY_FLOOR, CONFIDENCE_FLOOR) well-calibrated?

### UC-4: Knowledge Gap Detection

**Requires**: query_log + observation data

Signals that indicate missing knowledge:
1. Query returns 0 results → topic not covered
2. Query returns results but agent ignores them (no subsequent use of returned entry content) → entries exist but aren't useful
3. Agent creates new knowledge entry (context_store) on a topic that has existing entries → existing entries inadequate
4. Repeated queries on same topic across features → persistent gap
5. Agents falling back to Bash/grep/Read instead of using injected knowledge → knowledge format wrong

### UC-5: Entry Lifecycle Analysis

**Requires**: injection_log + signal_queue + session outcomes

Track an entry from creation to utility:
```
Created (context_store) → First injection → Peak usage period → Decay → Deprecation
```

Metrics per entry:
- Time-to-first-use: days between creation and first injection
- Active lifespan: period where injection_count > 0
- Utility score: helpful_signals / total_injections
- Audience: which agent types consume this entry?
- Feature coverage: how many distinct features use this entry?

### UC-6: Confidence Calibration

**Requires**: injection_log (has confidence at injection time) + session outcomes

The confidence system (6-factor composite) should predict utility:
- Bin entries by confidence score (0.0-0.2, 0.2-0.4, ..., 0.8-1.0)
- For each bin: what % of injections led to helpful signals?
- If high-confidence entries aren't more useful than low-confidence ones → calibration is off
- Can identify which confidence factors (freshness, helpfulness, etc.) are most predictive

---

## Tier 3: Agent & Workflow Intelligence

### UC-7: Agent Performance Profiling

**Requires**: Connected session → feature → observations + SubagentStart data

Per agent type:
- Average tool calls per spawn
- Average duration per spawn
- Success rate (did the spawning session complete successfully?)
- Common tool patterns (e.g., uni-rust-dev: Read→Edit→Bash cycle)
- Respawn rate (same agent type spawned multiple times in one session)

Cross-feature comparison:
- Is `uni-tester` more efficient on certain feature types?
- Which agents get spawned but don't contribute to outcome?
- Agent spawn tree depth: how deep does delegation go?

### UC-8: Workflow Phase Optimization

**Requires**: feature_deliveries + observation_phase_metrics

Per workflow phase (design, pseudocode, implementation, testing, PR):
- Duration benchmarks with statistical bounds
- Tool call distribution (what's the right Read/Write/Edit mix?)
- Phase transition patterns (what signals a phase is done?)
- Phase skip detection (did testing get skipped? Did design get skipped?)

### UC-9: Context Window Efficiency

**Requires**: observations (response_size) + compaction events

Per session:
- Total context loaded (sum of PostToolUse response_size)
- Context loaded before first mutation (Read-heavy startup)
- Compaction events (how often did the context window fill up?)
- Context-per-useful-action ratio (total context / productive tool calls)

---

## Tier 4: Export & Tuning

### UC-10: Embedding Model Tuning Data

**Requires**: query_log + injection_log + signal_queue

Export format for search quality tuning:
```json
{
  "query": "how does confidence scoring work",
  "positive_entries": [42, 67],      // entries injected + got helpful signal
  "negative_entries": [15, 23],      // entries injected + NOT helpful (or flagged)
  "hard_negatives": [88, 91],        // high similarity but not injected (below floor)
  "similarity_scores": {"42": 0.87, "67": 0.82, "15": 0.79}
}
```

This is direct training data for:
- Fine-tuning embedding models for domain-specific retrieval
- Learning-to-rank on top of raw similarity
- Threshold optimization (similarity floor, confidence floor)

### UC-11: Detection Rule Calibration

**Requires**: shadow_evaluations + retrospective outcomes

We already have shadow_evaluations (rule name, neural confidence, convention score, accepted flag). Connect this to feature outcomes:
- Rules that fire but don't correlate with actual problems → false positive candidates
- Problems that occur but no rule fires → gap in detection coverage
- Rule severity calibration: are "Critical" findings actually more impactful than "Warning"?

### UC-12: Behavioral Pattern Export

**Requires**: observations (full tool trace) + session outcomes

Export tool call sequences as training data for:
- Predicting next tool call (workflow modeling)
- Identifying successful vs unsuccessful patterns
- Agent behavior cloning (what does a successful implementation session look like?)

Format:
```json
{
  "session_id": "...",
  "feature": "col-015",
  "outcome": "success",
  "tool_sequence": [
    {"tool": "Read", "target": "SCOPE.md", "ts": 1700000000},
    {"tool": "Grep", "target": "src/", "ts": 1700000002},
    {"tool": "Edit", "target": "lib.rs", "ts": 1700000010}
  ]
}
```

### UC-13: Cross-Feature Knowledge Graph

**Requires**: feature_deliveries + co_access + feature_entries

Build a graph where:
- Nodes = features + entries
- Edges = feature used entry, entry co-accessed with entry
- Can identify: knowledge clusters, bridge entries (used across many features), orphan entries

Applications:
- "Before starting feature X, you should review entries A, B, C" (predictive briefing)
- "Features X and Y share knowledge dependencies — coordinate changes"
- Knowledge pruning: entries with no feature associations and no recent access → deprecation candidates

---

## Tier 5: Real-Time / Proactive Intelligence

### UC-14: In-Session Feature Detection

**Requires**: UserPromptSubmit stored + attribution at write time

Instead of waiting for retrospective, detect the feature during the session:
- Parse first UserPromptSubmit for feature references
- Auto-populate sessions.feature_cycle
- Enable real-time feature delivery tracking

### UC-15: Proactive Knowledge Serving

**Requires**: query_log + feature context

When a new session starts for a known feature:
- Look at what previous sessions for this feature searched for
- Pre-compute likely queries based on current phase
- Surface relevant entries proactively (before the agent searches)

### UC-16: Rework Early Warning

**Requires**: Connected session data + historical rework patterns

During an active session, detect signals that correlate with eventual rework:
- High Read/Write ratio without Edit → exploration without progress
- Multiple compile failures → stuck on implementation
- Repeated searches on same topic → can't find what they need
- Session duration exceeding phase baseline by 2x → potential scope issue

---

## Data Volume Considerations

Rough estimates per feature delivery (5-8 sessions):
- observations: 500-2000 rows (50-400 tool calls per session)
- injection_log: 50-200 rows (5-25 injections per session)
- query_log (new): 20-80 rows (matching injection_log cardinality roughly)
- feature_deliveries: 1 row

At current pace (~2 features/week):
- ~100 features/year
- ~150K observations/year
- ~15K injection_log records/year
- ~6K query_log records/year

SQLite handles this trivially. No scaling concerns.

---

## What We Have Today for Test/Query Data

### Test Data (Partial)
Test execution is captured indirectly through Bash observations:
- `cargo test` commands appear as Bash PreToolUse/PostToolUse
- response_snippet (500 chars) may capture pass/fail counts
- response_size gives rough output volume

**What's missing**: Structured test results (pass count, fail count, test names, durations). Would require parsing Bash output or a dedicated test result capture hook.

**Practical value**: Even without structured parsing, the raw Bash observations let us count test runs per session, detect test-fix-test cycles (compile_cycles detection rule already does this), and estimate time spent in testing.

### Query Data (Mostly Missing)
MCP tool calls (context_search, context_lookup) appear as observations when called as Claude Code tools. But:
- UDS ContextSearch (from UserPromptSubmit hook) is NOT stored as observation
- Audit log records operation + result count but NOT query text
- Injection log records results but NOT query text

**Bottom line**: We're capturing the answers but not the questions. The query_log table closes this gap.
