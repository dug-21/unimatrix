# ASS-013: Initial Findings

## Dataset Overview

| Metric | Value |
|--------|-------|
| Records | 1,180 |
| Sessions | 4 |
| Time span | ~11 hours (02:05 → 13:20 UTC) |
| Feature covered | crt-006 (Adaptive Embedding Pipeline) |
| Phases captured | Research → Design → Implementation → Testing → Delivery |

## Session Breakdown

| Session | Time Range | Calls | Character |
|---------|-----------|-------|-----------|
| d1d1a6a7 | 02:05–02:29 | 73 | Human dialogue + hook setup |
| dc1e33f6 | 02:10–02:13 | 21 | Knowledge storage subagent |
| 66c19301 | 02:31–13:17 | 1,085 | Full swarm feature cycle (scrum-master) |
| 1c6597fe | 13:20–13:20 | 13 | Post-delivery cleanup |

Session `66c19301` contains the entire swarm run — this is the high-value dataset.

## Tool Distribution

| Tool | Count | % | Signal Type |
|------|-------|---|-------------|
| Read | 380 | 32% | Codebase comprehension |
| Bash | 254 | 22% | Build/test/git execution |
| TaskUpdate | 98 | 8% | Workflow state transitions |
| Edit | 96 | 8% | Iterative code refinement |
| Write | 94 | 8% | New file creation |
| Grep | 88 | 7% | Codebase exploration |
| SubagentStart/Stop | 34 | 3% | Agent lifecycle |
| TaskCreate | 32 | 3% | Work decomposition |
| Glob | 26 | 2% | File discovery |
| MCP (unimatrix) | 52 | 4% | Knowledge interaction |

### Hook Distribution

- PreToolUse: 585 (tool calls initiated)
- PostToolUse: 565 (tool calls completed — 20 fewer, likely denials/timeouts)
- SubagentStart: 7
- SubagentStop: 27 (more stops than starts — stops include subagents spawned by subagents)

## File Access Patterns

### Most-Read Files (agents need these to understand the system)

| Reads | File | Role |
|-------|------|------|
| 20x | `crates/unimatrix-server/src/tools.rs` | Integration surface |
| 15x | `crates/unimatrix-server/src/server.rs` | Core server logic |
| 6x | `product/PRODUCT-VISION.md` | Vision alignment |
| 6x | `product/features/crt-006/SCOPE.md` | Feature scope |
| 5x | `crates/unimatrix-server/src/coaccess.rs` | Co-access module |
| 5x | `crates/unimatrix-server/src/coherence.rs` | Coherence module |

### Most-Edited Files (iterative refinement targets)

| Edits | File |
|-------|------|
| 9x | `crates/unimatrix-server/src/server.rs` |
| 7x | `crates/unimatrix-server/src/tools.rs` |
| 5x | `crates/unimatrix-adapt/src/service.rs` |
| 4x | `crates/unimatrix-server/src/main.rs` |
| 4x | `crates/unimatrix-server/src/shutdown.rs` |

**Observation:** `server.rs` is both the most-read (15x) and most-edited (9x) file. This is the primary integration point — agents need to re-read it frequently because it changes frequently during the feature.

## Workflow Choreography

### Design Phase (Session 1 pattern)
8 tasks created at 02:32, executed in sequence:
1. Researcher → scope exploration
2. Human approval gate
3. Risk strategist → scope risks
4. Architect + specification (parallel)
5. Risk strategist → architecture risks
6. Vision guardian → alignment
7. Synthesizer → implementation brief
8. Artifact delivery

**Rework detected:** Task #1 was completed at 02:37, then re-opened at 02:40 with updated subject "Redo researcher scope with strategic scale analysis" — completed again at 02:44. This is a human-triggered correction loop.

### Delivery Phase (Session 2 pattern)
8 tasks created at 03:38, also sequential:
1. Branch + init
2. Pseudocode + test plans (Stage 3a)
3. Gate 3a validation
4. Implementation (Stage 3b) — **40 minutes**, longest phase
5. Gate 3b code review
6. Test execution (Stage 3c)
7. Gate 3c validation
8. Commit + PR delivery

## Temporal Density

Activity bursts correlate with swarm phases:

| Window | Calls | Phase |
|--------|-------|-------|
| 02:30 | 168 | Design kickoff — all design agents spawning |
| 03:00 | 158 | Architect + spec writer running in parallel |
| 03:50 | 93 | Pseudocode agent (Stage 3a) |
| 04:20–04:30 | 188 | Implementation peak (Stage 3b) |
| 12:30 | 68 | Late session — test execution (Stage 3c) |

## Knowledge Interaction Patterns

### Unimatrix MCP Calls (52 total)
- `context_store`: 24 calls (but only ~8 unique entries — rest are retries)
- `context_get`: 14 calls (entry #181 fetched 5 times)
- `context_search`: 12 calls
- `context_briefing`: 2 calls
- `context_lookup`: 0 calls

### Retry/Friction Signal
Two entries were retried excessively:
- "Adaptive Embedding Pipeline" pattern: **4 attempts** (02:10–02:12)
- "Session activity capture hooks": **6 attempts** (02:13–02:16)

This suggests permission denials or hook interference. Each retry is wasted tokens and latency.

### Knowledge Re-Reads
Entry #181 (the adaptive embedding research pattern) was fetched via `context_get` 5 separate times across the session. Agents don't retain cross-spawn context, so each new subagent re-fetches the same foundational knowledge.

## Preliminary Value Assessment

### High Value Signals

| Signal | What it reveals | Unimatrix application |
|--------|----------------|----------------------|
| File co-access clusters | Which files agents need together | Proactive context surfacing ("you'll also need X") |
| Retry patterns | Permission/friction points | Lessons learned, process improvement |
| Re-read frequency | Knowledge importance beyond votes | Confidence/relevance scoring input |
| Read-before-edit pairs | Understanding dependencies | Co-access graph enrichment |

### Medium Value Signals

| Signal | What it reveals | Unimatrix application |
|--------|----------------|----------------------|
| Task state transitions | Workflow phase timing | Phase duration baselines, anomaly detection |
| Subagent lifecycle | Spawn/completion patterns | Agent efficiency metrics |
| Search queries | What agents look for | Gap detection (searched but not found) |
| Temporal density | Activity bursts | Workload characterization |

### Low Value / Noise

| Signal | Why low value |
|--------|--------------|
| Raw Bash commands | Too tool-specific, captured in git history |
| Individual Read content | Already in git, too granular |
| response_snippet | Mostly success/failure boilerplate |
| Glob patterns | Useful but too ephemeral to store |

## Deep Dive: Read Classification

### Comprehension vs. Pre-Mutate Reads

Claude Code policy: always read a file before writing/editing it. This inflates read counts. Analysis classified each Read by whether a Write/Edit to the same file followed within 15 events.

**Overall: 74% comprehension, 26% pre-mutate** (across files with 2+ reads).

Key findings:
- Pure comprehension files (100% reads, 0 mutations): `coherence.rs`, `coaccess.rs`, `normalize.rs`, `shutdown.rs`, `ARCHITECTURE.md`, `RISK-TEST-STRATEGY.md`, `traits.rs`, `adapters.rs` — agents read these to understand integration surfaces
- `tools.rs`: 60% pre-mutate — primarily a mutation target, not a reference
- `server.rs`: ~50/50 — both reference and mutation target

**Implication for Unimatrix**: File co-access signals should filter out pre-mutate reads. Only comprehension reads represent genuine knowledge-seeking behavior worth tracking.

## Deep Dive: Design-to-Delivery Handoff

### Design Doc Reads During Delivery Phase

26 reads totaling ~190KB (~48K tokens) of design documents consumed during delivery.

Three distinct "context loading" bursts:

| Burst | Phase | Reads | Size | What happened |
|-------|-------|-------|------|---------------|
| 03:45 | Stage 3a kickoff | 10 | ~86KB | Pseudocode agent inhaled all design docs in 11 seconds |
| 04:32 | Gate 3b review | 4 | ~16KB | Code reviewer paged through ARCHITECTURE.md (3 reads = offset/limit paging) |
| 04:39–04:42 | Stage 3c testing | 12 | ~88KB | Tester read RISK-TEST-STRATEGY (2x), all test plans, ACCEPTANCE-MAP |

### Critical Finding: Monolithic Agent Anti-Pattern

**Protocol intent**: Scrum-master spawns per-component delivery agents for parallelization and context compartmentalization. Pseudocode/test-plans are structured per-component for this purpose.

**Actual execution**: Stage 3b used ONE agent for all 8 components:
- Sequential writes: `lib.rs` → `config.rs` → `lora.rs` → `training.rs` → `regularization.rs` → `prototypes.rs` → `episodic.rs` → `persistence.rs` → `service.rs` (03:52–04:00)
- Then re-read ALL files at 04:18 and did a second editing pass
- Single SubagentStart at 03:38, no per-component spawns

**Evidence**: All component files share the same session_id. Only 1 `Task` tool call during entire delivery phase (spawning a single delivery agent).

**Impact**: The per-component design doc structure (pseudocode/lora.md, test-plan/lora.md, etc.) was consumed monolithically instead of selectively. The 88KB tester context load could have been ~10-15KB per component agent.

### Design Doc Handoff — Value Assessment

**Key insight from discussion**: Design docs must be live file reads during the feature cycle. Unimatrix cannot substitute for them because:
1. Design agents need exact current state of integration surfaces (function signatures, struct fields)
2. Any abstraction or staleness could lead to architecture that doesn't fit the live code
3. The per-component pseudocode/test-plan structure already IS the selective delivery mechanism

**Where Unimatrix adds value**: Not intra-feature handoff, but:
- **Cross-feature learning**: "server.rs integration pattern" stored once, reused by future features
- **Retrospective analysis**: Surfacing that delivery collapsed to monolithic agents (invisible without telemetry)
- **Protocol deviation detection**: Comparing intended workflow to actual execution

## Revised Value Assessment

### Highest Value: Retrospective Function

The single most valuable application of observation data is a structured `/retrospective` capability that:
1. Transforms raw JSONL into structured analysis (rule-based, no model needed)
2. Detects known anti-patterns (monolithic agents, context overloading, retry storms)
3. Presents findings to the developer's LLM for reasoning and recommendations
4. Optionally stores lessons learned in Unimatrix

See `retrospective-design.md` for full architecture.

### Moderate Value: Cross-Feature Patterns

File access patterns across multiple features could identify stable integration surfaces and recurring navigation patterns worth storing in Unimatrix.

### Lower Than Expected: File Co-Access for Intra-Feature Use

Initial assessment rated file co-access as "HIGH value." Revised to moderate. Within a feature cycle, agents need live file reads regardless of Unimatrix knowledge. The co-access signal has more value for cross-feature learning than real-time context surfacing.

## Open Questions for Further Analysis

1. **Can we extract file co-access graphs from Read sequences?** — Which files are read within the same 5-minute window by the same session?
2. **What do the search queries that return no useful results look like?** — Gap detection signal.
3. **Is there a pattern to rework?** — When tasks get re-opened, what preceded the rework?
4. **What's the Pre→Post drop-off?** — 585 pre vs 565 post = 20 calls that didn't complete. What were they?
5. **Can temporal density predict phase transitions?** — Does a drop in activity reliably signal a phase boundary?
6. **Cross-feature comparison**: When we have telemetry from a second feature cycle, do the same files get read? Do the same anti-patterns recur?
