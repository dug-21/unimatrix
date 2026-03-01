# ASS-013: Observation Data Pipeline Design

## Overview

Batch pipeline for collecting, attributing, analyzing, and archiving tool-call telemetry from agent sessions. Feeds the `/retrospective` capability and future auto-knowledge extraction.

## Collection Layer

Hooks append JSONL records to per-session files. No enrichment, no feature attribution at collection time.

```
~/.unimatrix/observation/
  {session_id}.jsonl    ← one file per Claude Code session
  {session_id}.jsonl
  ...
```

### Why Per-Session Files (Not One Monolithic File)

- **Concurrent sessions never interleave** — filesystem is the partition
- **Cleanup is atomic** — archive or delete whole files, no mid-file rewriting
- **No race conditions** — completed session files are immutable (no writer contending with reader)
- **Natural lifecycle** — active session = being written, completed session = ready for retrospective

### Record Schema (unchanged from current hooks)

```json
{
  "ts": "ISO-8601",
  "hook": "PreToolUse | PostToolUse | SubagentStart | SubagentStop",
  "session_id": "uuid",
  "tool": "Read | Bash | Edit | Write | ...",
  "input": { ... },
  "response_size": 1234,
  "response_snippet": "first 500 chars"
}
```

No new fields. Feature attribution is derived at analysis time from what's already in the records.

### Hook Changes Required

Current: all records append to a single `activity.jsonl`.
New: records append to `{session_id}.jsonl` based on the `session_id` field.

One-line routing change in the hook script. No other collection changes.

## Attribution Layer

Session-to-feature mapping is derived at retrospective time by scanning record content. Not stored during collection — the data already carries the signal.

### Attribution Signals (in priority order)

1. **File paths in tool inputs**: Any tool call referencing `product/features/{id}/` definitively identifies the feature. Write to `product/features/crt-008/SCOPE.md` → crt-008.
2. **Task subjects**: TaskCreate/TaskUpdate subjects contain feature IDs (e.g., "crt-008: Research scope").
3. **Git checkout commands**: Bash commands containing `git checkout -b feature/{id}` name the feature explicitly.

### Attribution Logic

```
For each session file:
  1. Scan tool inputs for feature ID references (file paths, task subjects, branch commands)
  2. Count references per feature ID
  3. Majority feature ID wins → session attributed to that feature
  4. ALL records in that session belong to that feature (including ones without explicit feature references)
```

### Why Not Git Branch

Git branch is global to the working directory, not per-session. When two sessions share a repo:
- Session A checks out `feature/crt-007` for delivery
- Session B starts design for crt-008 — hook would see `feature/crt-007` (wrong)

Branch adds confusion in multi-session workflows. Content-based attribution is accurate regardless of branch state.

### Why Not Attribute at Collection Time

- Hooks don't know the feature context — they fire on every tool call with no workflow awareness
- Design phase happens before any branch exists — no feature signal available at the start
- The scrum-master knows the feature, but the hook runs in a different process
- Attribution from content is cheap (scan file paths in a 1MB file = milliseconds) and accurate
- Keeping collection dumb means zero risk of mis-tagging records permanently

### Edge Cases

| Scenario | Resolution |
|----------|------------|
| Session works on two features (rare) | Majority attribution; records without feature paths follow the majority. If truly split, human reviews during retrospective. |
| Session on `main` doing triage (no feature work) | No feature paths found → unattributed. Ignored by retrospective. Flagged by safety valve if stale. |
| Feature spans 3 days, 5 sessions | All sessions contain the same feature's file paths → all attributed correctly. |
| Concurrent features in separate sessions | Each session's file paths reference different features → clean separation. |
| Design session before any feature files exist | Scrum-master creates TaskCreate with feature ID in subject → attributable from task subjects. |

## Analysis Layer

Batch processing triggered by `/retrospective {feature-id}`. All analysis is on-demand — no background processing, no streaming.

### Retrospective Flow

```
/retrospective crt-007

1. SCAN: Read all session files in ~/.unimatrix/observation/
   - For each file, extract feature IDs from tool inputs
   - Identify sessions attributed to crt-007

2. LOAD: Read all records from matched session files

3. ORDER: Sort union set by timestamp
   - Records from different sessions interleave correctly by ts
   - This is the unified feature timeline

4. ANALYZE: Run hotspot rules against ordered records
   - Agent hotspots (context load, lifespan, re-reads, compile cycles)
   - Friction hotspots (permission retries, Bash misuse, sleep commands)
   - Session hotspots (cold restarts, timeouts, coordinator respawns)
   - Scope hotspots (file counts, artifact counts, phase durations)
   - Cross-session patterns (cold restart = time gap between session files)

5. REPORT: Generate hotspot report
   - Opinionated findings with supporting data
   - Feature metrics with historical comparison (if baseline exists)
   - Present to LLM + human for discussion

6. STORE: Archive metric vector in Unimatrix
   - One compact entry per feature (category: "observation", topic: feature-id)
   - Contains all numeric metrics, hotspot flags, threshold comparisons

7. ARCHIVE: Move processed session files
   - Destination: product/features/crt-007/observation/
   - Working directory cleaned of these session files
   - Metric vector in Unimatrix is the durable output; raw JSONL archived for optional re-analysis
```

### What the Analysis Engine Needs to Be

A Rust binary or library within Unimatrix that:
- Parses JSONL files
- Runs rule-based hotspot detection with configurable thresholds
- Computes metric vectors
- Produces a structured report (consumed by the `/retrospective` skill)

No LLM required for analysis. The LLM participates only in the conversation layer — interpreting hotspots, discussing with the human, and generating recommendations.

## Lifecycle & Safety Valve

### Normal Lifecycle

```
Session active    → hook appends to {session_id}.jsonl
Session ends      → file is complete, immutable
Feature completes → human runs /retrospective
Retrospective     → analyze, report, archive session files, store metric vector
```

### Safety Valve

If nobody retrospects, session files accumulate. `context_status` reports:
- Number of unprocessed session files
- Age of oldest unprocessed file
- Total observation storage size

Threshold: files older than 90 days without retrospective → flag as maintenance recommendation. Never auto-delete — human decides.

### What's Retained Long-Term

- **Metric vectors in Unimatrix** — one compact entry per feature, queryable, used for baseline convergence
- **Hotspot dismissal records** — feeds threshold convergence (which hotspots the human ignored vs. actioned)
- **Archived session files** — in feature directory, optional, for re-analysis if needed

### What Can Be Discarded

- Session files after retrospective archival (the metric vector captures the statistics)
- Unattributed sessions older than the safety valve threshold
- The archived JSONL in feature directories can be deleted after baseline convergence stabilizes (10+ features)

## Platform Constraints

These are limitations of the Claude Code hook API that the pipeline must work within.

| Constraint | Impact | Pipeline Handling |
|------------|--------|-------------------|
| All subagent tool calls share parent session_id | Cannot attribute tool calls to specific agents within a session | Hotspots operate at session/feature-cycle granularity, not per-agent |
| Nested subagent types invisible (26/31 SubagentStop have empty agent_type) | Worker agents are anonymous | Infer role from tool patterns if needed, but don't depend on it |
| No SubagentStart for nested children | Only top-level spawns emit start events | Use SubagentStop timestamps for bracketing where possible |
| Git branch is global, not per-session | Branch field would mis-attribute in multi-session workflows | Use content-based attribution instead of branch |

## Open Questions

1. Should archived session files be compressed (gzip) in the feature directory?
2. When a feature has no attributable sessions (pure human dialogue, no feature paths), how should `/retrospective` handle it — error, or analyze all unattributed sessions?
3. Should `context_status` maintenance recommendations include "run /retrospective for these features" based on detected feature paths in unprocessed files?
4. Maximum session file size before the hook rotates within a session (for extremely long sessions)?
