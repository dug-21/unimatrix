# ASS-014: claude-flow Feedback, Confidence, and Retrospective Analysis

Research date: 2026-03-01
Source: github.com/dug-21/claude-flow (research fork of ruvnet/claude-flow, Ruflo v3.5)

---

## Executive Summary

claude-flow has **two separate learning systems** at different maturity levels:

1. **intelligence.cjs** (stub) -- A minimal trigram/word-matching context engine with no-op feedback. This is the system actually wired into the hook lifecycle.
2. **learning-service.mjs** (aspirational) -- A SQLite + HNSW + ONNX embedding learning system with short-term/long-term pattern promotion. This is NOT wired into any hooks and requires `better-sqlite3` which is not a declared dependency.

The feedback loop described in marketing materials does not exist in the shipped code. The confidence bumps (+0.05/-0.02) described in the README are not implemented. The system that IS wired in calls `feedback(true)` as a no-op.

---

## 1. Confidence Feedback Loop

### What the code says

**intelligence.cjs** (the module actually loaded by hook-handler.cjs):

```js
feedback: function(success) {
    // Stub: no-op in minimal version
},
```

This is the only feedback function reachable from the hook lifecycle. The file header says "Intelligence Layer Stub (ADR-050) -- Minimal fallback, full version is copied from package source."

### What triggers feedback

In **hook-handler.cjs**, feedback is called in exactly ONE place:

```js
'post-task': () => {
    if (intelligence && intelligence.feedback) {
      try {
        intelligence.feedback(true);
      } catch (e) { /* non-fatal */ }
    }
    console.log('[OK] Task completed');
},
```

The `post-task` handler fires on `TaskCompleted` and `TeammateIdle` events (per settings.json). It **always passes `true`** -- there is no failure path. No hook ever calls `feedback(false)`.

### The "full version" learning-service.mjs

The learning-service.mjs has a more sophisticated system that would, if wired in:

- Track `usage_count` and `success_count` per pattern
- Update quality via running average: `quality = (quality * usage_count + success_score) / (usage_count + 1)`
- Promote short-term patterns to long-term when: `usage_count >= 3 AND quality >= 0.6`

But this service is never instantiated by hook-handler.cjs. It requires `better-sqlite3` (native addon) and the hooks only load `intelligence.cjs` (which uses JSON files).

### context-persistence-hook.mjs confidence

The transcript archival system has its own parallel confidence mechanism:

- **Access boost**: `confidence = MIN(1.0, confidence + 0.03)` when transcript entry is restored
- **Decay**: `0.5% per hour` applied on session boundaries
- **Semantic search boost**: cosine similarity score multiplied by confidence: `score = dot_product * confidence`

This is for transcript entries only, not for the learning/pattern system. It runs independently of intelligence.cjs.

### Verdict: Confidence feedback is theater

The actual wired system calls `feedback(true)` into a no-op. The sophisticated system (learning-service.mjs) exists as code but has no integration path. The transcript system has working confidence arithmetic but only for context restoration ordering, not for learning.

---

## 2. Session Summarization

### Does claude-flow produce session summaries?

**No, not in the wired code.**

**session.cjs** tracks:
- `id`, `startedAt`, `endedAt`, `duration`
- `platform`, `cwd`
- `context` (arbitrary key-value bag)
- `metrics`: `{ edits: 0, commands: 0, tasks: 0, errors: 0 }`

The session-end command in the CLI documentation describes summary generation:
```
"summaryPath": "/sessions/dev-session-2024-summary.md"
```

But this is documentation for the `npx claude-flow hook session-end` CLI command, which would invoke the V3 TypeScript package (`@claude-flow/cli`). That package is not built or available in the repo's `.claude/helpers/` hooks path.

The actual session.cjs `end()` function:
1. Reads current session from JSON
2. Adds `endedAt` and `duration`
3. Archives to `sessions/{session-id}.json`
4. Deletes `current.json`
5. Logs duration in minutes

No summary text generation. No data analysis. No "work accomplished" or "key decisions" extraction.

### context-persistence-hook.mjs

The transcript archival system does produce **summaries of transcript chunks** (stored as `summary` column in SQLite), but these are metadata about archived conversation turns, not session-level analysis.

---

## 3. Cross-Session Learning

### How PageRank + trigram matching evolves

**It doesn't.**

**intelligence.cjs** `getContext(prompt)`:
1. Tokenizes the prompt into words (lowercased, >2 chars)
2. Computes Jaccard similarity (word overlap / union) against each entry's words
3. Returns top 5 entries above 0.05 similarity threshold
4. Formats as `[INTELLIGENCE] Relevant patterns for this task:`

There is no PageRank computation anywhere in intelligence.cjs. The word matching is stateless -- computed fresh each time from the entry store. No edges, no graph.

The `init()` function writes a `ranked-context.json` file but just copies entries as-is with `edges: 0`.

### Persistence: auto-memory-store.json

**intelligence.cjs** data flow:
1. On init: reads `auto-memory-store.json` OR bootstraps from MEMORY.md files
2. Entries are simple objects: `{id, content, summary, category, confidence: 0.5, sourceFile, words}`
3. `getContext()` reads from `ranked-context.json` (snapshot from init)
4. `recordEdit(file)` appends to `pending-insights.jsonl` (not used for matching)
5. `consolidate()` counts lines in pending-insights.jsonl, truncates it, returns count

No entries are ever added, modified, or removed from the store by the intelligence module itself. Bootstrap from MEMORY.md is one-way. The confidence field is set to 0.5 on load and never changed.

### auto-memory-hook.mjs (if @claude-flow/memory package exists)

This hooks into SessionStart/Stop to import/export between MEMORY.md files and a JSON backend. It delegates to `AutoMemoryBridge`, `LearningBridge`, and `MemoryGraph` from the `@claude-flow/memory` npm package. The package is not published or built in the repo. The hook gracefully degrades to no-op when the package is missing.

Config options reference a `LearningBridge` with:
- `sonaMode: 'balanced'`
- `confidenceDecayRate: 0.005`
- `accessBoostAmount: 0.03`
- `consolidationThreshold: 10`

And a `MemoryGraph` with:
- `pageRankDamping: 0.85`
- `maxNodes: 5000`
- `similarityThreshold: 0.8`

These would be the "real" PageRank and confidence evolution -- but the package that implements them does not exist.

---

## 4. Consolidation Process

### intelligence.cjs consolidation (actual)

```js
consolidate: function() {
    var count = 0;
    if (fs.existsSync(PENDING_PATH)) {
      try {
        var content = fs.readFileSync(PENDING_PATH, "utf-8").trim();
        count = content ? content.split("\n").length : 0;
        fs.writeFileSync(PENDING_PATH, "", "utf-8");
      } catch (e) { /* skip */ }
    }
    return { entries: count, edges: 0, newEntries: 0 };
},
```

This counts lines in pending-insights.jsonl, truncates the file, and returns stats. **Nothing is promoted, merged, or learned from.** The pending insights are thrown away.

The hook-handler.cjs session-end handler prints: `'[INTELLIGENCE] Consolidated: N entries, 0 edges, PageRank recomputed'` -- but PageRank was never computed.

### learning-service.mjs consolidation (unwired)

The full LearningService has a substantive consolidation:
1. Delete short-term patterns older than 24 hours with usage < 3
2. Rebuild HNSW indexes
3. Remove long-term duplicates (>0.95 cosine similarity, keep higher quality)
4. Prune long-term patterns not accessed in 30 days with usage < 2
5. Rebuild indexes again

### pattern-consolidator.sh (unwired)

A shell wrapper that runs on a 15-minute timer:
1. Remove exact duplicate strategies (keep highest quality)
2. Prune patterns with quality < 0.3 and older than 7 days
3. Promote quality > 0.8 to long-term
4. Decay unused patterns by 5% quality

This also requires sqlite3 CLI and is not wired into any hooks.

---

## 5. Outcome Attribution

### Does claude-flow attribute failures to specific context entries?

**No.** There is no attribution mechanism in any of the analyzed files.

- `feedback(true)` is always true; there is no error/failure path
- Session metrics track aggregate counts (edits, commands, tasks, errors) but not which entries were involved
- `lastMatchedPatterns` is stored in session context but never used for attribution
- The transcript system tracks `access_count` but not "was this helpful"
- No entry-level success/failure tagging exists

The closest thing to attribution is `intelligence.getContext()` storing `lastMatchedPatterns` in the session, but nothing reads this value back to attribute outcomes.

### learning-service.mjs recordPatternUsage (unwired)

The full learning service has `recordPatternUsage(patternId, success)` which:
- Increments usage_count and optionally success_count
- Adjusts quality via running average
- Checks for promotion to long-term

But this requires explicit calls from hooks with pattern IDs, and no hook passes pattern IDs.

---

## 6. Agent Tracking

### swarm-hooks.sh

Agents are tracked via a file-based registry:
- `AGENTS_FILE = .claude-flow/swarm/agents.json`
- Each agent gets: `{id, name, status, lastSeen}`
- Agent IDs come from env vars `AGENTIC_FLOW_AGENT_ID` or are auto-generated
- Registration happens on first message send/receive

### swarm-comms.sh

A message queue system using files:
- Priority-based queue: `$QUEUE_DIR/{priority}_{msg_id}.json`
- Agent mailboxes: `$SWARM_DIR/mailbox/{agent_id}/`
- Connection pooling (file-based simulation)
- Pattern broadcasting between agents

### Session-level agent tracking

session.cjs tracks no agent identity. There is no concept of "which agents participated in this session." The swarm system and session system are completely independent.

The settings.json wires `SubagentStart` to a simple status check, and `TeammateIdle`/`TaskCompleted` to `post-task` (which calls the no-op feedback). No agent identification is captured at these hook points.

---

## 7. Feature-Level Grouping

### Does claude-flow group data by feature/task?

**No.** All grouping is session-based.

- intelligence.cjs: no grouping at all (flat entry list)
- session.cjs: groups by session ID only
- learning-service.mjs: groups by `session_id` and `domain` (e.g., "code", "general"), not by feature
- transcript system: groups by session ID, not by feature
- swarm system: no feature concept

There is no equivalent to Unimatrix's `feature_cycle` field. The closest concept is `domain` in the learning service (e.g., "code", "test", "security") but these are task-type categories, not project features.

---

## Architecture Summary

### What's Actually Wired (via settings.json hooks)

```
SessionStart  -> hook-handler.cjs session-restore
                   -> session.cjs start/restore
                   -> intelligence.cjs init (load entries from JSON/MEMORY.md)
              -> auto-memory-hook.mjs import (no-ops without @claude-flow/memory)

UserPromptSubmit -> hook-handler.cjs route
                   -> intelligence.cjs getContext (trigram match, inject context)
                   -> router.cjs routeTask (regex-based agent recommendation)

PostToolUse (Write/Edit) -> hook-handler.cjs post-edit
                           -> session.cjs metric('edits')
                           -> intelligence.cjs recordEdit (append to JSONL)

TaskCompleted -> hook-handler.cjs post-task
               -> intelligence.cjs feedback(true) [NO-OP]

SessionEnd -> hook-handler.cjs session-end
             -> intelligence.cjs consolidate (truncate JSONL, return count)
             -> session.cjs end (archive session JSON)

Stop -> auto-memory-hook.mjs sync (no-ops without @claude-flow/memory)
```

### What Exists But Is Not Wired

| Component | Status | Why Not Wired |
|-----------|--------|---------------|
| learning-service.mjs | Code exists, never called | Requires better-sqlite3 native addon |
| learning-hooks.sh | Code exists, never called | Not in settings.json hooks |
| learning-optimizer.sh | Code exists, never called | Not in settings.json hooks |
| pattern-consolidator.sh | Code exists, never called | Not in settings.json hooks |
| @claude-flow/memory package | Referenced, not built | npm package not published |
| LearningBridge, MemoryGraph | Config exists, classes don't | Part of unbuilt package |
| context-persistence-hook.mjs | Code exists, not in settings.json | Would need PreCompact/SessionStart hooks |
| SONA micro-LoRA optimizer | Shell wrapper, no implementation | npx agentic-flow call, no agentic-flow installed |

---

## Lessons for Unimatrix

### 1. Unimatrix's confidence system is real; claude-flow's is not

Unimatrix has: Wilson score + 6-factor composite + co-access affinity + f64 pipeline.
claude-flow has: `confidence: 0.5` hardcoded, never updated.

### 2. Feedback requires deliberate signal design

claude-flow's `feedback(true)` always-true pattern is worse than no feedback at all -- it creates the illusion of learning. Unimatrix's `helpful`/`unhelpful` parameters on search/lookup/get are explicit and user-driven, which is the correct approach. The challenge remains getting agents to actually call them.

### 3. Consolidation needs a real pipeline

claude-flow's consolidation truncates a file and claims "PageRank recomputed." Unimatrix's `maintain=true` on context_status actually does work: confidence refresh, graph compaction, co-access cleanup. The lesson is that consolidation must produce visible, measurable results.

### 4. Feature grouping is a real differentiator

claude-flow groups nothing by feature. Unimatrix's `feature_cycle` on entries + `FEATURE_ENTRIES` index + `OUTCOME_INDEX` keyed by feature provides genuine per-feature learning. This is a significant architectural advantage.

### 5. Session summarization remains an open problem

Neither system produces meaningful session summaries. This is a real gap for retrospective analysis. Unimatrix's `context_retrospective` tool (col-002) is attempting this via observation telemetry, which is the right approach.

### 6. Agent identity matters

claude-flow's swarm system has agent IDs via env vars but the learning system doesn't use them. Unimatrix's `agent_id` parameter on every tool call, plus the `AGENT_REGISTRY` table, enables real per-agent tracking. The enrollment system (alc-002) makes this explicit and persistent.

### 7. Two-tier storage is sound in theory

The short-term/long-term pattern promotion in learning-service.mjs (promote after 3 uses and quality >= 0.6) is a good design. Unimatrix's approach of single-tier storage with confidence evolution and decay may benefit from a similar explicit promotion/demotion boundary in the future.

---

## Key Takeaway

claude-flow's learning infrastructure is aspirational code, not operational code. The pieces that are wired into the actual hook lifecycle (intelligence.cjs, session.cjs, memory.cjs) are minimal stubs that track basic counters and perform word matching. The sophisticated pieces (learning-service.mjs, @claude-flow/memory package) are unwired prototypes that require missing dependencies.

For Unimatrix's purposes, the interesting **design patterns** to note are:
- Short-term -> long-term promotion threshold (3 uses + 0.6 quality)
- Context autopilot (proactive pruning before compaction)
- File-based agent messaging with priority queues
- The aspiration for HNSW + ONNX embeddings in the learning layer (which Unimatrix already ships)

The interesting **anti-patterns** to avoid are:
- No-op feedback functions that claim to learn
- Consolidation that discards data and prints "PageRank recomputed"
- Always-true feedback calls with no failure path
- Multiple parallel confidence systems that don't interact
