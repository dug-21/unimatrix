# ASS-013: Retrospective Function Design

## Core Concept

Transform raw tool-call telemetry from agent sessions into **hotspot-driven retrospective reports** that give the LLM (and human) specific, opinionated findings to investigate — not raw data to sift through.

Unimatrix has a point of view. It says "these things look wrong." The LLM and human discuss whether they are. Unimatrix learns from the feedback.

## Architecture

### Layer 1: Telemetry Collection (Proven)

Raw observation via Claude Code hooks. JSONL stream of every tool call, subagent spawn, and task state transition.

- Already implemented and validated with crt-006 feature cycle
- Passive observation — no workflow modification
- Hooks: PreToolUse, PostToolUse, SubagentStart, SubagentStop
- Output: `~/.unimatrix/observation/activity.jsonl`

### Layer 2: Hotspot Detection (Rule-Based, Evolving Thresholds)

Deterministic analysis that transforms raw JSONL into **hotspots** — specific, scoped findings with supporting data attached. Each hotspot is a claim: "this looks anomalous."

Hotspots are not raw metrics. They are Unimatrix's opinion, based on thresholds that improve with iterations.

### Layer 3: LLM Conversation

The `/retrospective` skill presents hotspots to the LLM in conversation. The LLM + human discuss: is this real? what caused it? what to change? Findings feed back into Unimatrix.

### Layer 4: Optional Small LLM (Future)

Rust-native model for environments without frontier LLM access, or for novel pattern detection beyond rules. Deferred until 2+ use cases justify integration cost. Exposed as a general Unimatrix inference capability, not embedded in retrospective logic.

## Hotspot Categories

### Agent Hotspots (per agent lifecycle)

Identify agents that exhibit characteristics correlated with context engineering problems.

| Signal | Metric | Starting Threshold | Rationale |
|--------|--------|--------------------|-----------|
| Context load | Total KB read before first Write/Edit | >100 KB | crt-006 tester loaded 88KB (borderline) |
| Lifespan | Duration SubagentStart → SubagentStop | >45 min | crt-006 Stage 3b was 40 min (known hotspot) |
| File breadth | Distinct files touched | >20 files | crt-006 monolithic agent touched ~35 |
| Re-read rate | Files read 2+ times within agent lifetime | >3 re-reads | crt-006 ARCH.md read 3x in 7 sec |
| Mutation spread | Distinct files written/edited | >10 files | crt-006 monolithic agent edited 15 |
| Compile cycles | cargo check/test invocations | >6 per phase | crt-006 had 8 in Stage 3b |
| Edit bloat | Edit responses >50KB (large file iteration) | >50 KB avg | crt-006 tools.rs averaged 91KB |

### Friction Hotspots (per session)

| Signal | Detection | Starting Threshold |
|--------|-----------|-------------------|
| Permission retries | PreToolUse count - PostToolUse count per tool | >2 retries same tool |
| Search-via-Bash | Regex match Bash commands for find/grep/rg | >5% of Bash calls |
| Sleep workarounds | Any `sleep` command in Bash | any occurrence |
| Output parsing struggle | Same cargo command with different output filters within 3 min | >2 filter variations |

### Session Hotspots (per feature cycle)

| Signal | Detection | Starting Threshold |
|--------|-----------|-------------------|
| Cold restart | Gap >30 min followed by burst of reads to already-read files | any occurrence |
| Session timeout | Gap >2 hours in a single scrum-master run | any occurrence |
| Coordinator respawns | SubagentStart count for coordinator agent type | >3 per feature |
| Post-completion work | Tool calls after final task marked completed / total calls | >8% |
| Rework events | Task status completed → in_progress | any occurrence |

### Scope Hotspots (per feature, accumulated metric comparison)

| Signal | Detection | Starting Threshold |
|--------|-----------|-------------------|
| Source file count | New *.rs files created via Write | >6 files |
| Design artifact count | Files in feature directory | >25 files |
| ADR count | ADR-* files created | >3 ADRs |
| Post-delivery issues | GH issues created after final task completion | >0 |
| Phase duration outlier | Any phase >2x its evolving baseline duration | 2x baseline |

**Scope hotspots are the weakest category initially** — they depend on accumulated data to establish baselines. Early iterations use conservative absolute thresholds. After 5+ features, relative thresholds (vs. historical mean) replace absolutes.

## Hotspot Report Format

Each hotspot includes: the claim, the evidence, and the supporting data for deeper investigation.

```
# Retrospective: crt-006 (Adaptive Embedding Pipeline)
## Feature Metrics
  Source files: 9 | Artifacts: 35 | ADRs: 4
  Design: 60 min | Delivery: 70 min (excl. timeout)
  Coordinator respawns: 5 | Cold restarts: 1

## Hotspots (4 flagged)

### 🔴 Agent Hotspot: Stage 3b Implementation
  Lifespan: 40 min | Files: 35 | Mutations: 15 | Re-reads: 8
  Compile cycles: 8 (3 with output filter variation)
  Context load: 190KB before first write

  Top files by access:
    tools.rs      — 7 reads, 3 edits (91KB per edit response)
    server.rs     — 5 reads, 4 edits (69KB per edit response)
    service.rs    — 2 reads, 5 edits

### 🟡 Friction Hotspot: context_store Permission
  10 retries across 3 distinct entries
  Worst: "Session activity capture hooks" — 6 attempts before success

### 🟡 Session Hotspot: Timeout + Cold Restart
  3-hour gap during Phase 4 (PR delivery)
  Cold restart loaded 123KB across 17 files
  Post-restart: full code review + test rerun

### 🟡 Scope Hotspot: Post-Delivery Effort
  12% of tool calls occurred after final task completion
  2 GH issues created post-delivery (tech debt surfaced)
  4+ investigation subagents spawned after "done"

## Baseline Comparison
  (available after 5+ features — first iteration, no history)

## Raw Metrics
  [attached: full metric vector for archival]
```

## Threshold Convergence

### Iteration 1-3: Bootstrapped Priors

Hard-coded defaults from context engineering principles + crt-006 baseline. Conservative — flag obvious problems, accept some false positives. Every hotspot is reviewed.

### Iteration 4-10: Empirical Adjustment

Per-feature metric vectors stored in Unimatrix. Thresholds shift toward `mean + 1.5σ` as data accumulates. Human-dismissed hotspots push thresholds higher (reduce false positives).

### Iteration 10+: Project-Normalized

Thresholds reflect this project's norms. "Stage 3b usually takes 35-45 min" isn't a guess — it's computed from 10 data points. Scope hotspot comparison becomes meaningful.

### Dismissed Hotspot Feedback

When a human says "that's fine" to a hotspot:
- Unimatrix records the dismissal with the metric values
- Patterns consistently dismissed get suppressed (threshold raised)
- Patterns sometimes dismissed, sometimes actioned stay active
- Novel patterns (never seen before) always flagged

## Data Lifecycle

### The Problem

Raw JSONL is large (1.4MB for one feature). Unimatrix is a knowledge engine, not an audit trail. We need the statistics for baseline convergence, but not the raw records forever.

### Proposed Lifecycle

```
Phase 1: Collection
  └─ Raw JSONL accumulates during sessions
       ~/.unimatrix/observation/activity.jsonl

Phase 2: Analysis (on /retrospective)
  ├─ Rule engine processes JSONL → hotspots + metric vector
  ├─ Hotspot report presented to LLM + human
  └─ Human feedback recorded (dismiss/acknowledge/action)

Phase 3: Archival
  ├─ Metric vector stored in Unimatrix (compact — one entry per feature)
  │   category: "observation", topic: feature-id
  │   Contains: all numeric metrics, hotspot flags, dismissal annotations
  ├─ Raw JSONL optionally archived to feature directory
  │   product/features/{id}/observation/activity.jsonl
  └─ Working JSONL rotated (cleared for next feature)

Phase 4: Baseline computation (ongoing)
  └─ On each /retrospective, Unimatrix queries all "observation" entries
     to compute current baselines, stddev, threshold adjustments
```

### What's Retained Long-Term

- **Metric vectors** — one compact entry per feature, queryable, used for baselines
- **Hotspot dismissal/action records** — feeds threshold convergence
- **Promoted compound signals** — human-confirmed correlations between metrics

### What's Discarded (or archived to files)

- Raw JSONL — optionally kept in feature directory, not in Unimatrix
- Individual tool call records — summarized into metrics, then discarded
- Response snippets — no long-term value

### Multi-Session / Multi-Workstream

- Session boundaries detected by session_id changes and time gaps
- Feature association: JSONL records tagged with feature ID (from task subjects or git branch)
- Multiple concurrent features: partition JSONL by session_id → feature mapping
- Cross-feature sessions (human dialogue spanning features): attribute to most-recent feature or mark as "unattributed"

## Telemetry Gaps and Platform Constraints

### Platform Constraints (out of our control)

These are limitations of the Claude Code hook API that Unimatrix cannot change. Analysis must work within these boundaries.

| Constraint | Impact | Workaround |
|------------|--------|------------|
| All subagent tool calls share parent `session_id` | Cannot directly attribute tool calls to specific agents within a session | Timestamp bracketing (sequential agents) or tool-pattern inference (fragile) |
| Nested subagent types invisible (26/31 SubagentStop have empty `agent_type`) | Worker agents (researcher, architect, tester, coder) spawned by scrum-master are anonymous | Infer role from tool call patterns (writes pseudocode/ = pseudocode agent) |
| No SubagentStart for nested children | Only top-level spawns emit start events; children spawned by coordinators don't | Use SubagentStop timestamps to bracket end-of-agent windows |
| No context window size signal | Can't measure actual context pressure directly | Proxy via total KB read (imperfect — doesn't account for compression) |
| Edit response sizes inflate metrics | 44% of all response data is platform echo-back of edited files | Filter or discount edit responses in context load calculations |

**Design implication**: Retrospective hotspots should operate at **session/feature-cycle granularity**, not per-agent. Per-agent attribution is possible via heuristics but should not be a hard requirement. Session-level detection ("this session had unusually high context load") delivers most of the retrospective value without requiring agent-level precision.

**Session isolation is clean**: `session_id` fully separates concurrent sessions. Two humans running two features simultaneously produce two distinct session_ids. Multi-session partitioning is a solved problem.

### Telemetry Gaps (potentially addressable)

| Gap | Impact | Potential Fix |
|-----|--------|---------------|
| No agent-to-task correlation | Can't link "this agent worked on task #12" | Need task ID in tool call context or agent prompt |

## Detection Tier Summary

| Detection Method | Findings Covered | Model Required |
|-----------------|-----------------|----------------|
| Pure rules | Permission friction, Bash misuse, cold restart, compile loops, context overload, rework, sleep, edit bloat | None |
| Rules + threshold convergence | Agent hotspots, scope hotspots, post-completion effort | None (thresholds from data) |
| Rules + protocol comparison | Monolithic agent, parallelization failure | None (if protocol declares expectations) |
| LLM required | Grep intent clustering, compound signal interpretation, novel pattern discovery, actionable recommendations | Frontier LLM in conversation |

## Open Questions

1. Should the analysis engine be a Rust binary (part of Unimatrix server) or a Python/script tool?
2. How to partition JSONL across concurrent features in the same session?
3. Minimum feature count before scope hotspot baselines are meaningful?
4. How to handle features of very different natures (research spike vs. full implementation) in baseline computation?
5. Should dismissed hotspot feedback be per-metric or per-hotspot-instance?
6. How to present baseline comparison when history exists — inline in hotspot, or separate section?
