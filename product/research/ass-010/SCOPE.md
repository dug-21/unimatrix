# ASS-010: Observation Pipeline — Signal Quality Validation

**Type:** Research Spike
**Date:** 2026-02-25
**Predecessor:** `product/research/claude-int/ANALYSIS.md` (raw message interception analysis)
**Purpose:** Validate whether PostToolUse hooks produce actionable signals for implicit knowledge extraction, outcome inference, and context utilization measurement.

**Note (2026-02-27):** Much of this research area was explored during ASS-011 (hook-driven orchestration spike). We determined that hook-based observation and metadata injection did not provide enough incremental value to implement at this time. The risks of adding silent interface keys and the complexity of correlating hook data with MCP calls outweighed the benefits. See `product/research/ass-011/` for findings.

---

## Research Question

Can Unimatrix passively learn from agent behavior by observing tool call inputs and outputs, and is the signal quality high enough to justify building extraction infrastructure?

Specifically:
1. What is the signal-to-noise ratio across tool types?
2. Can we detect "agent followed convention X" from Write/Edit outputs?
3. Can we infer gate pass/fail from Bash test output?
4. Can we measure whether briefing content was actually used in subsequent work?
5. What is the overhead cost (disk, latency) of continuous hook-based capture?

---

## Approach: PostToolUse Hook Data Collection

### Hook Architecture

```
Claude Code session (main or subagent)
    |
    | tool executes normally
    v
PostToolUse hook fires
    |
    | shell script receives JSON on stdin:
    |   { session_id, tool_name, tool_input, tool_response, tool_use_id, cwd, ... }
    |
    v
Filter: is tool_name in capture set?
    |
    | yes → append JSON-line to spool file
    | no  → exit 0 (no-op)
    v
Spool: ~/.unimatrix/observation/spool/{session_id}.jsonl
```

PostToolUse hooks fire for subagent tool calls (they share the parent session's hook configuration). This means a single hook definition captures the entire agent swarm's tool activity.

### Tools to Capture

| Tool Name Pattern | Signal Type | Expected Volume | Why |
|-------------------|-------------|-----------------|-----|
| `Write` | Code creation | 5-20/session | New files reveal conventions, patterns, architectural choices |
| `Edit` | Code modification | 20-80/session | Diffs reveal what changed and why — convention adherence, refactoring patterns |
| `Bash` | Command execution + output | 30-100/session | Test results (pass/fail), build output, git operations |
| `mcp__unimatrix__context_briefing` | Context delivered | 1-5/session | What Unimatrix served — baseline for utilization measurement |
| `mcp__unimatrix__context_search` | Context delivered | 2-10/session | Search results served — what was available to the agent |
| `mcp__unimatrix__context_store` | Knowledge stored | 1-5/session | What agents explicitly capture — compare against what they implicitly demonstrate |

Tools NOT captured (noise): `Read`, `Glob`, `Grep`, `Task`, `TaskOutput`, `WebFetch`, `WebSearch`. These are input-gathering tools with low knowledge signal.

### Data Retention

- **Capture window:** 2-3 feature cycles (enough for pattern detection)
- **Storage:** JSON-lines, one file per session, under `~/.unimatrix/observation/spool/`
- **Size estimate:** ~50-200KB per session (tool_response for Write/Edit includes full file content; Bash includes stdout). At 10 sessions per feature cycle, ~2-6MB total. Negligible.
- **Cleanup:** Manual deletion after analysis. No automated retention policy for the spike.

---

## Implementation

### 1. Hook Script

`.claude/hooks/observe.sh` — the data collector.

```bash
#!/usr/bin/env bash
# PostToolUse observation hook for ASS-010 research spike.
# Captures tool I/O for Write, Edit, Bash, and Unimatrix MCP tools.
# Appends a single JSON-line per captured event to a session-scoped spool file.

set -euo pipefail

# Read hook input from stdin
INPUT=$(cat)

# Extract tool name
TOOL_NAME=$(echo "$INPUT" | jq -r '.tool_name // empty')

# Filter: only capture tools in the capture set
case "$TOOL_NAME" in
  Write|Edit|Bash|mcp__unimatrix__context_briefing|mcp__unimatrix__context_search|mcp__unimatrix__context_store)
    ;;
  *)
    exit 0
    ;;
esac

# Extract session ID for file scoping
SESSION_ID=$(echo "$INPUT" | jq -r '.session_id // "unknown"')

# Ensure spool directory exists
SPOOL_DIR="${HOME}/.unimatrix/observation/spool"
mkdir -p "$SPOOL_DIR"

# Build the observation record:
# - timestamp: when the event occurred
# - tool_name: which tool
# - tool_input: what the agent asked for
# - tool_response: what came back
# - tool_use_id: unique call identifier
# - session_id: session scoping
#
# For Bash calls, tool_response can be large (test output).
# Truncate tool_response to 10KB to bound spool size.
RECORD=$(echo "$INPUT" | jq -c '{
  ts: (now | todate),
  tool_name: .tool_name,
  tool_input: .tool_input,
  tool_response: (.tool_response | tostring | if length > 10240 then .[0:10240] + "...[truncated]" else . end),
  tool_use_id: .tool_use_id,
  session_id: .session_id
}')

# Append to session-scoped spool file
echo "$RECORD" >> "${SPOOL_DIR}/${SESSION_ID}.jsonl"

exit 0
```

### 2. Hook Configuration

Add to `.claude/settings.json` (project-shared, so all sessions capture data):

```json
{
  "hooks": {
    "PostToolUse": [
      {
        "matcher": "",
        "hooks": [
          {
            "type": "command",
            "command": "\"$CLAUDE_PROJECT_DIR\"/.claude/hooks/observe.sh",
            "timeout": 5
          }
        ]
      }
    ]
  }
}
```

Empty `matcher` means the hook fires for ALL tool calls. The script itself filters to the capture set. This is simpler than maintaining a regex matcher for 6+ tool name patterns.

### 3. Directory Structure

```
product/research/ass-010/
├── SCOPE.md                    # This file
└── findings/                   # Analysis outputs (populated during analysis phase)
    ├── signal-quality.md       # Signal-to-noise assessment per tool type
    ├── convention-detection.md # Can we detect convention adherence from code diffs?
    ├── outcome-inference.md    # Can we infer gate results from test output?
    ├── utilization.md          # Context utilization measurement feasibility
    └── synthesis.md            # Go/no-go recommendation for col-001b

.claude/
├── hooks/
│   └── observe.sh              # The PostToolUse hook script
└── settings.json               # Hook configuration (NEW — does not exist yet)
```

---

## Analysis Plan

After collecting data across 2-3 feature cycles, analyze the spool files to answer:

### Q1: Signal-to-Noise Ratio

For each tool type, categorize every captured event as:
- **Signal:** Contains an extractable pattern, convention, decision, or outcome
- **Noise:** Routine operation with no knowledge value

Target: >30% signal rate for at least 2 tool types. Below that, extraction cost exceeds value.

### Q2: Convention Detection from Code

Compare Write/Edit tool_response content against stored Unimatrix conventions. For each convention in the knowledge base:
- Does the agent's code follow it? (positive signal)
- Does the agent's code contradict it? (negative signal — possible stale convention)
- Is the convention irrelevant to this code? (neutral — no signal)

Target: Can we programmatically detect alignment/contradiction for >50% of relevant conventions?

### Q3: Outcome Inference from Test Output

For Bash calls that run `cargo test` or similar:
- Can we extract pass/fail counts from stdout?
- Can we map failures to specific components or gates?
- Is the output format stable enough for reliable parsing?

Target: >90% reliable pass/fail extraction from test runner output (cargo test format is highly structured — this should be straightforward).

### Q4: Context Utilization

For sessions where `context_briefing` or `context_search` was called:
- Do subsequent Write/Edit calls reference the served content? (substring matching, concept matching)
- Which entry categories have the highest utilization rate?
- Does utilization vary by agent role?

Target: Can we assign a binary "used/not-used" label to >70% of served entries?

### Q5: Overhead

- Wall-clock latency added per tool call (measure via timestamps in spool)
- Spool file sizes per session
- Any observed impact on agent behavior (errors, timeouts)

Target: <15ms per hook invocation, <500KB per session.

---

## Success Criteria

The spike produces a **go/no-go recommendation** for col-001b (Observation Infrastructure):

**GO** if:
- At least 2 of Q1-Q4 meet their targets
- Q5 shows acceptable overhead
- The extraction rules are concrete enough to implement programmatically

**NO-GO** if:
- Signal quality is below thresholds across all tool types
- Extraction requires NLP/LLM reasoning (too expensive for inline use)
- Overhead is unacceptable

**PARTIAL** if:
- Some signals are high-quality but others aren't
- Recommendation: build col-001b with reduced scope (only the validated signal types)

---

## Constraints

- **No production code changes.** This spike adds a hook script and settings file only. No Rust code, no crate changes.
- **No interference with normal workflow.** The PostToolUse hook is observe-only — it cannot block or modify tool execution. Exit code 0 always.
- **Spike duration:** Data collection runs passively during normal feature work (col-001 implementation, any bug fixes). Analysis is a separate focused session after 2-3 cycles.
- **jq dependency:** The hook script requires `jq` for JSON processing. Available in the devcontainer.
- **Spool is ephemeral.** Not committed to git. `.gitignore` entry for `~/.unimatrix/observation/` is not needed (outside repo).

---

## Relationship to Roadmap

```
col-001 (Outcome Tracking)       ← in progress
  |
  +-- ass-010 (this spike)       ← parallel, observe-only, no code changes
  |     |
  |     v
  |   GO/NO-GO decision
  |     |
  |     +-- GO → col-001b (Observation Infra, between col-001 and col-002)
  |     +-- NO-GO → skip col-001b, col-002 uses explicit outcomes only
  |
  └→ col-002 (Retrospective Pipeline)
```

col-001's `source` tag key (recommended in `product/research/claude-int/ROADMAP-RECOMMENDATION.md`) enables col-001b to write `source:observed` outcomes into the same OUTCOME_INDEX. This tag key addition is a design influence on col-001, not a dependency on this spike.
