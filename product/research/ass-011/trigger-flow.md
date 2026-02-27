# Trigger Flow: "Start a new feature"

Traces the full activation chain from a human saying "let's build col-002" through
CLAUDE.md rules, protocol selection, agent spawning, and artifact production.

## Key Insight

There are **3 trigger layers** and **2 handoff types**:

- **Layer 1: CLAUDE.md** — Static rules loaded into every conversation. Contains the
  initial routing rule ("feature work uses swarms → spawn uni-scrum-master").
- **Layer 2: Agent def** — Loaded when Claude spawns the agent via Task tool. Contains
  role identity + workflow choreography + protocol file path.
- **Layer 3: Protocol** — Read from disk BY the agent at runtime. Contains the
  detailed phase-by-phase execution steps.

Handoff types:
- **Platform-native**: Claude Code's Task tool spawns agents from `.claude/agents/` defs
- **File-read**: Agent reads `.claude/protocols/` from disk (no platform support)

```mermaid
flowchart TD
    %% ── Human trigger ──
    H["🧑 Human: 'Let's build col-002<br/>retrospective pipeline'"]

    %% ── Layer 1: CLAUDE.md ──
    subgraph L1["Layer 1: CLAUDE.md (always loaded)"]
        R1["Rule #1: 'Feature work uses swarms —<br/>spawn uni-scrum-master'"]
    end

    %% ── Primary Agent (Claude) ──
    PA["Primary Agent (Claude)<br/>Reads CLAUDE.md rule → decides to spawn"]

    %% ── Platform handoff ──
    SPAWN1["Task(subagent_type: 'uni-scrum-master')<br/>prompt: feature-id, session type, intent"]

    %% ── Layer 2: Agent def loaded ──
    subgraph L2["Layer 2: Agent Def (.claude/agents/uni/uni-scrum-master.md)"]
        ID["Identity: coordinator, broad scope"]
        ROLE["Two Roles table:<br/>Session 1 → design protocol<br/>Session 2 → delivery protocol"]
        CHOREO["Workflow choreography:<br/>role boundaries, gate management,<br/>component routing, exit gate"]
    end

    %% ── Layer 3: Protocol read from disk ──
    subgraph L3["Layer 3: Protocol (.claude/protocols/uni/uni-design-protocol.md)"]
        PROTO_READ["Agent reads protocol file from disk<br/>(NOT platform-native — just a file read)"]
        P1["Phase 1: spawn uni-researcher<br/>→ SCOPE.md → human approval"]
        P1b["Phase 1b: spawn uni-risk-strategist<br/>(scope-risk mode) → SCOPE-RISK-ASSESSMENT.md"]
        P2a["Phase 2a: spawn uni-architect +<br/>uni-specification (parallel, ONE message)"]
        P2a_plus["Phase 2a+: spawn uni-risk-strategist<br/>(arch-risk mode) → RISK-TEST-STRATEGY.md"]
        P2b["Phase 2b: spawn uni-vision-guardian<br/>→ ALIGNMENT-REPORT.md"]
        P2c["Phase 2c: spawn uni-synthesizer<br/>(fresh context) → BRIEF + MAP + GH Issue"]
        RETURN["Return all artifacts to human"]
    end

    %% ── Each spawned agent has its own Layer 2 ──
    subgraph AGENTS["Spawned Agents (each has own Layer 2 agent def)"]
        A1["uni-researcher<br/>.claude/agents/uni/uni-researcher.md"]
        A2["uni-risk-strategist<br/>.claude/agents/uni/uni-risk-strategist.md"]
        A3["uni-architect<br/>.claude/agents/uni/uni-architect.md"]
        A4["uni-specification<br/>.claude/agents/uni/uni-specification.md"]
        A5["uni-vision-guardian<br/>.claude/agents/uni/uni-vision-guardian.md"]
        A6["uni-synthesizer<br/>.claude/agents/uni/uni-synthesizer.md"]
    end

    %% ── Connections ──
    H --> PA
    PA --> L1
    L1 --> SPAWN1
    SPAWN1 -->|"platform-native<br/>Task tool loads agent def"| L2
    L2 --> PROTO_READ
    PROTO_READ -->|"file read<br/>(not platform-native)"| P1
    P1 --> P1b
    P1b --> P2a
    P2a --> P2a_plus
    P2a_plus --> P2b
    P2b --> P2c
    P2c --> RETURN

    P1 -->|"Task(uni-researcher)"| A1
    P1b -->|"Task(uni-risk-strategist)"| A2
    P2a -->|"Task(uni-architect)"| A3
    P2a -->|"Task(uni-specification)"| A4
    P2b -->|"Task(uni-vision-guardian)"| A5
    P2c -->|"Task(uni-synthesizer)"| A6

    %% ── Styling ──
    style L1 fill:#1a1a2e,stroke:#e94560,color:#fff
    style L2 fill:#1a1a2e,stroke:#0f3460,color:#fff
    style L3 fill:#1a1a2e,stroke:#16213e,color:#fff
    style AGENTS fill:#0f3460,stroke:#533483,color:#fff
    style H fill:#e94560,color:#fff
    style PA fill:#533483,color:#fff
    style SPAWN1 fill:#0f3460,color:#fff
```

## Trigger Analysis

### What triggers what

| Step | Trigger | Mechanism | Source |
|------|---------|-----------|--------|
| 1 | Human says "build col-002" | Natural language intent | Human |
| 2 | Claude decides to spawn swarm | CLAUDE.md rule #1 match | `.claude/CLAUDE.md` line 3 |
| 3 | uni-scrum-master loads | `Task(subagent_type)` — platform-native | Claude Code Task tool |
| 4 | Scrum master reads protocol | `Read` tool on file path from agent def | Agent def line 23 → protocol file |
| 5 | Phase 1: researcher spawns | `Task(subagent_type: "uni-researcher")` | Protocol Phase 1 instructions |
| 6 | Human approves SCOPE.md | Human checkpoint (protocol-defined) | Protocol line 18 |
| 7 | Phase 1b: risk strategist spawns | `Task(subagent_type: "uni-risk-strategist")` | Protocol Phase 1b |
| 8 | Phase 2a: architect + spec spawn | Two `Task()` calls in ONE message | Protocol Phase 2a |
| 9 | Phase 2a+: risk strategist respawns | `Task()` with arch-risk mode | Protocol Phase 2a+ |
| 10 | Phase 2b: vision guardian spawns | `Task(subagent_type: "uni-vision-guardian")` | Protocol Phase 2b |
| 11 | Phase 2c: synthesizer spawns | `Task()` with fresh context | Protocol Phase 2c |
| 12 | Return to human | Scrum master returns artifacts | Protocol end |

### The two handoff types

```
Platform-native (reliable, structured):
  Human → Claude → Task(uni-scrum-master) → Task(uni-researcher) → ...
                    ↑                        ↑
                    Agent def loaded          Agent def loaded
                    automatically             automatically

File-read (fragile, convention-based):
  uni-scrum-master → Read(".claude/protocols/uni/uni-design-protocol.md")
                     ↑
                     Agent def says "read this file"
                     but nothing enforces it
```

### Where Unimatrix COULD replace file-reads

The protocol file read (Layer 3) is the non-platform-native handoff. If protocols
were stored as Unimatrix procedure entries:

```
Current:  Agent def says "read .claude/protocols/uni/uni-design-protocol.md"
          → Agent uses Read tool → gets protocol text

Future:   Agent def says "context_search(category: 'procedure', query: 'design session')"
          → Agent uses MCP tool → gets protocol text from Unimatrix
          → Protocol is versioned, tracked, confidence-scored
```

Skills and agent defs stay as files (platform-native triggers).
Protocols could move to Unimatrix (just file reads, no platform dependency).
