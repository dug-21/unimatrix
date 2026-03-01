# ASS-013: Tool Call Observation Analysis

## Purpose

Analyze raw LLM tool call telemetry captured during a complete feature cycle (crt-006: Adaptive Embedding Pipeline) to determine what signals have value for Unimatrix's self-learning capabilities.

## Conclusions

### 1. Retrospective Function Is the Highest-Value Application

Raw telemetry → rule-based hotspot detection → LLM conversation. Unimatrix identifies anomalies ("this looks wrong"), the human decides what to do about it. Most findings (6/8 categories) need zero model intelligence — pure rules with evolving thresholds. See `retrospective-design.md`.

### 2. Auto-Knowledge Extraction Is Feasible With Safeguards

Three tiers of extractable knowledge:
- **Structural conventions** (high confidence): file naming, directory structure, crate naming — verifiable against existing project structure
- **Procedural knowledge** (medium confidence): server integration sequence, crate bootstrapping, gate validation — need 3+ feature cycles to confirm
- **Dependency graphs** (medium-high confidence): read-before-edit chains reveal which files an agent needs to understand before modifying a target

Noise prevention requires: PostToolUse confirmation, cross-feature validation (3+ features), excluding fix cycles, and human confirmation gate. See `auto-knowledge.md`.

### 3. Compound Signal Correlation Requires LLM + Accumulated Data

Cannot hard-code compound signal thresholds from 1 data point. Design: per-feature metric table → LLM reasons about correlations → human confirms → promoted to tracked compound signal. Honest about limitations. See `compound-signals.md`.

### 4. Key Quantitative Findings (crt-006 Baseline)

| Metric | Value |
|--------|-------|
| Total tool calls | 1,180 across 4 sessions, ~11 hours |
| Thinking vs execution | 97% reasoning (128 min), 3% tool execution (4 min) |
| Edit response bloat | 44% of all response data (1,793 KB from 17 edits to 2 large files) |
| Search miss rate | 32% (18/57 Grep/Glob returned empty) |
| Parallel call rate | 25% (149/600 calls in parallel groups) |
| Design phase | ~60 min (human-in-loop) |
| Delivery phase | ~70 min (excluding timeout) |
| Knowledge interaction during delivery | 0% — agents don't consult Unimatrix during implementation |

### 5. Telemetry Gaps Identified

- Nested subagent types invisible (26/31 SubagentStop have empty `agent_type`)
- No agent-to-task correlation (can't link agent to specific task)
- No context window size signal (can't measure actual context pressure)
- Edit response sizes inflate metrics (44% of response data is echo-back)

## Raw Data

- `crt-006-raw-activity.jsonl` — 1,180 records, 1.4MB, captured via Claude Code hooks
  - Source: `~/.unimatrix/observation/activity.jsonl`
  - Time range: 2026-02-28T02:05 → 2026-02-28T13:20 (~11 hours)
  - 4 sessions covering human dialogue + full swarm feature cycle
  - **Frozen snapshot** — no further writes to this file

## Data Schema

Each JSONL record:
```json
{
  "ts": "ISO-8601",
  "hook": "PreToolUse | PostToolUse | SubagentStart | SubagentStop",
  "session_id": "uuid",
  "tool": "Read | Bash | Edit | Write | Grep | Glob | TaskCreate | TaskUpdate | mcp__unimatrix__* | ...",
  "input": { ... },
  "response_size": 1234,
  "response_snippet": "first 500 chars of response"
}
```

## Analysis Artifacts

### Telemetry Analysis
- `initial-findings.md` — First-pass sampling, distribution analysis, read classification (74% comprehension / 26% pre-mutate), design-to-delivery handoff (190KB, 48K tokens), monolithic agent anti-pattern discovery, revised value assessment
- `deep-findings.md` — Pass 2+ findings: edit bloat (44% of context load), permission friction (10 retries), Bash compliance issues, cold restart cost (123KB/31K tokens), thinking time analysis (97% reasoning), 32% search miss rate, activity profile shifts by phase, agent warmup patterns, phase duration baseline

### Design Documents
- `retrospective-design.md` — Hotspot-driven `/retrospective` function architecture: 4 layers (telemetry → hotspot detection → LLM conversation → optional small model), 4 hotspot categories (agent/friction/session/scope), threshold convergence model (3 stages), data lifecycle, detection tier summary
- `compound-signals.md` — Compound signal correlation assessment: metric table + LLM reasoning + human confirmation design, per-feature metrics to collect, promoted compound signal lifecycle
- `data-pipeline.md` — Observation data pipeline: per-session JSONL files, content-based feature attribution (not git branch), batch retrospective flow, lifecycle management, platform constraints

### Auto-Knowledge Extraction
- `auto-knowledge.md` — Feasibility analysis: 3 extraction tiers (structural/procedural/dependency), noise prevention rules (5 specific filters), gap analysis (what agents store vs. what they do), proposed extraction strategy with confidence levels

## Related

- #52 — Bug: MCP server connection drops and fails to recover (discovered during this research)
- crt-006 feature cycle — Source telemetry dataset
- ASS-011 — Hook-driven orchestration spike (observation hooks used here)
