# Protocol vs Consolidated Agent Analysis

## The Two Eras

### Protocol Era (8a92fa7 through ~col-008)

**Architecture**: Three-layer separation.

```
CLAUDE.md                          → Routes to uni-scrum-master
uni-scrum-master.md (164 lines)    → Reads protocol file, executes it
uni-design-protocol.md (329 lines) → Step-by-step process flow
uni-synthesizer.md (119 lines)     → Knows what to produce
```

Key characteristics of the scrum master:
- **Line 1 of instructions**: "Your job is to **read the protocol and execute it** — not improvise around it."
- Contains a routing table: Session 1 → read `uni-design-protocol.md`, Session 2 → read `uni-delivery-protocol.md`
- Role boundaries table explicitly lists who does what
- The SM has NO spawn templates — those live in the protocol
- The SM is thin: identity + role boundaries + gate management + GH Issue lifecycle

Key characteristics of the protocol:
- Process flow diagram showing agent-to-agent handoffs
- Concurrency rules
- Per-phase spawn templates with `Task()` syntax
- Agent context budget rules
- Quick reference message map at the bottom

**Why this worked**: The SM's context window was *lean* when it started. It read the protocol as a separate file, which meant:
1. It treated the protocol as an *external authority* to follow
2. Its own definition had no content-generation knowledge — only coordination logic
3. The spawn templates were in the protocol, not in the SM's head — so it had to follow them

### Consolidated Era (19032cf through present — col-018, col-019)

**Architecture**: Two-layer, protocols absorbed.

```
CLAUDE.md                                     → Routes to uni-design-scrum-master
uni-design-scrum-master.md (291 lines)        → IS the protocol + coordinator + routing
uni-synthesizer.md (127 lines)                → Knows what to produce (unchanged)
uni-design-protocol.md (329 lines)            → Still exists on disk, NEVER REFERENCED
```

Key characteristics:
- SM definition now contains everything: role boundaries + spawn templates + phase flow + exit gate
- "You orchestrate — you never generate content" is present but buried at line 15
- The SM has full spawn templates for every agent including the synthesizer
- **The SM has all the information needed to write every artifact itself**

**Why this breaks**: The SM knows the synthesizer's job description (from the spawn template), knows all the input artifact paths (it managed all prior phases), and has the "produce IMPLEMENTATION-BRIEF.md" instruction right in its own definition. The leap from "spawn someone to do this" to "I can just do this" is tiny — especially for features the SM judges as simple.

## Structural Comparison

| Aspect | Protocol Era | Consolidated Era |
|--------|-------------|-----------------|
| SM definition size | 164 lines | 291 lines |
| SM knows spawn templates | No (reads from protocol) | Yes (embedded) |
| SM has content knowledge | No (only coordination) | Yes (all phase outputs described) |
| Protocol authority | External file = hard to ignore | Self = easy to rationalize |
| Dead protocol files | N/A | 4 files, 1427 lines, unreferenced |
| col-008 brief quality | Correct (matched synthesizer template) | N/A |
| col-018 brief quality | N/A | Wrong (coordinator voice, not synthesizer) |
| col-019 brief quality | N/A | Wrong (step-by-step code diffs) |

## What Changed Between Eras

The consolidation commit (`19032cf`) did three things:

1. **Split uni-scrum-master into two**: `uni-design-scrum-master` + `uni-implementation-scrum-master`
2. **Absorbed protocol content into agent defs**: Spawn templates, phase flow, and process rules moved from protocol files into agent definitions
3. **Stopped referencing protocols**: Neither new SM reads a protocol file. The protocol files became dead.

The split (item 1) was fine — separating design and delivery coordinators is sound. The absorption (item 2) is what broke things.

## Evidence

### col-008 IMPLEMENTATION-BRIEF.md (protocol era)
- Source Document Links table ✓
- Component Map with pseudocode/test-plan columns ✓
- Cross-Cutting Artifacts section ✓
- Goal statement ✓
- Resolved Decisions table with ADR references ✓
- Build Order (waves) ✓
- Risk Hotspots (top 5) ✓
- Data Structures (Rust structs) ✓
- Function Signatures ✓
- Constants ✓
- Constraints (hard + soft) ✓
- Dependencies ✓
- NOT in Scope ✓
- Alignment Status ✓
- **Verdict**: Matches uni-synthesizer template exactly

### col-018 IMPLEMENTATION-BRIEF.md (consolidated era)
- "Summary" paragraph
- "What Changes" with line-by-line code instructions
- "What Does NOT Change"
- "Existing Infrastructure to Reuse"
- "Agent Guidance" ← coordinator voice
- "Risk Items for Implementation Attention"
- **Verdict**: Coordinator wrote this, not synthesizer. Contains "This is a single-agent implementation task" — that's the SM deciding to skip decomposition.

### col-019 IMPLEMENTATION-BRIEF.md (consolidated era)
- "Summary" paragraph
- "Implementation Steps" with before/after code diffs
- "File Change Summary"
- "Dependencies"
- "Risks to Watch During Implementation"
- **Verdict**: Same pattern — coordinator voice, prescriptive code, no synthesizer structure.

## Root Cause

The SM absorbed enough context to do the synthesizer's job (badly). In the protocol era, the SM:
1. Read the protocol as an external document
2. Had no knowledge of what artifacts should contain
3. Could only follow the spawn sequence

In the consolidated era, the SM:
1. Has all spawn templates in its own definition
2. Has received results from all prior phases (full context)
3. Knows the synthesizer just needs to "Produce: IMPLEMENTATION-BRIEF.md, ACCEPTANCE-MAP.md, GH Issue"
4. Thinks: "I have all the info, the feature is simple, I'll just write this myself"

## Options

### Option A: Revert to Protocol-Primary (Recommended)

Restore the three-layer architecture:
- Slim the design SM back to ~164 lines (identity + routing + boundaries)
- Restore "read the protocol and execute it" as the primary instruction
- Protocol files become the authority again (they're already on disk, just unreferenced)
- Use the evolved protocols (329/489 lines) not the originals (232/377) — they have scope risk, branch workflow, outcome recording

### Option B: Hybrid

Keep specialized SMs but extract spawn templates back into protocol files:
- SMs keep role boundaries and gate management
- SMs read protocol files for phase sequencing
- Protocols own the "what to spawn and when"

### Option C: Strengthen Consolidated (Least Likely to Work)

Add more enforcement language to existing consolidated defs. Already tried implicitly — "You orchestrate — you never generate content" exists and is ignored.

## Files in This Comparison

```
product/workflow/base-001/
├── 001-proposal.md                              # Original workflow vision
├── analysis.md                                  # This file
├── protocol-era/                                # Git state at 8a92fa7
│   ├── CLAUDE.md                                # CLAUDE.md that referenced protocols
│   ├── uni-scrum-master.md                      # Lean SM (164 lines, reads protocols)
│   ├── uni-synthesizer.md                       # Original synthesizer
│   ├── uni-design-protocol.md                   # Original design protocol (232 lines)
│   ├── uni-delivery-protocol.md                 # Original delivery protocol (377 lines)
│   └── uni-agent-routing.md                     # Original routing (123 lines)
├── protocol-evolved/                            # Current on-disk protocols (dead files)
│   ├── uni-design-protocol.md                   # Evolved design protocol (329 lines)
│   ├── uni-delivery-protocol.md                 # Evolved delivery protocol (489 lines)
│   ├── uni-agent-routing.md                     # Evolved routing (190 lines)
│   └── uni-bugfix-protocol.md                   # Bugfix protocol (419 lines)
└── consolidated-era/                            # Current agent defs
    ├── uni-design-scrum-master.md               # Design SM (291 lines, IS the protocol)
    ├── uni-implementation-scrum-master.md        # Impl SM (501 lines, IS the protocol)
    ├── uni-synthesizer.md                        # Current synthesizer (127 lines)
    └── uni-scrum-master.md                       # Legacy SM (still exists, unused by new SMs)
```
