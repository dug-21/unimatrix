# Unimatrix Protocol Reference

This directory contains the reference workflow protocols for Claude Code + Unimatrix
delivery. Each protocol defines how a coordinator agent orchestrates specialist agents
through a structured session (design, delivery, or bug fix) while integrating with the
Unimatrix knowledge engine via the `context_cycle` MCP tool.

---

## What These Protocols Are

The four files in this directory describe three session types:

| File | Session Type | Triggers On |
|------|-------------|-------------|
| `uni-design-protocol.md` | Design (Session 1) | specification, architecture, scope definition |
| `uni-delivery-protocol.md` | Delivery (Session 2) | implement, build, code, deliver |
| `uni-bugfix-protocol.md` | Bug Fix (single session) | bug, fix, regression, failing |
| `uni-agent-routing.md` | Routing reference | agent selection and swarm composition |

Each protocol is a coordinator playbook: it defines which specialist agents to spawn,
in what order, and what validation gates must pass before the session can advance.

---

## How context_cycle Works

`context_cycle` is the Unimatrix MCP tool that links workflow execution to
knowledge delivery. Calling it at phase boundaries does two things:

1. **Sets attribution context** — subsequent knowledge queries and stores are tagged
   against the current feature and phase, so the learning model knows what knowledge
   was used during which workflow moment.

2. **Enables phase-conditioned retrieval** — `context_briefing` uses the phase signal
   to rank knowledge entries that are historically relevant to the current phase.
   Knowledge that proved useful during design phases is surfaced to design agents;
   knowledge useful during delivery is surfaced to delivery agents.

### The Three Call Types

| Call | When to Use | Effect |
|------|------------|--------|
| `"type": "start"` | Before any agents are spawned | Opens the cycle, sets feature + phase attribution |
| `"type": "phase-end"` | At each phase transition | Records what was accomplished, advances the phase signal |
| `"type": "stop"` | After the session ends | Commits all signals to the learning model; closes the cycle |

---

## Two-Phase Example: Design to Delivery

The following shows the `context_cycle` calls across a complete two-phase workflow.
Protocol files contain agent spawn templates and gate logic; this example focuses only
on the cycle calls.

```
# --- Session 1: Design ---

# Open the cycle before spawning any agents
mcp__unimatrix__context_cycle({
  "feature": "my-feature-001",
  "type": "start",
  "next_phase": "scope"
})

# ... scope research and SCOPE.md approval ...

mcp__unimatrix__context_cycle({
  "feature": "my-feature-001",
  "type": "phase-end",
  "phase": "scope",
  "outcome": "SCOPE.md approved. Scope risk assessment complete.",
  "next_phase": "design"
})

# ... architecture, specification, risk strategy, vision alignment, synthesis ...

mcp__unimatrix__context_cycle({
  "feature": "my-feature-001",
  "type": "phase-end",
  "phase": "design",
  "outcome": "Architecture, specification, and risk strategy complete.",
  "next_phase": "design-review"
})

# Session 1 ends here. The cycle remains open for Session 2.

# --- Session 2: Delivery ---

# Re-declare the cycle at the start of Session 2
mcp__unimatrix__context_cycle({
  "feature": "my-feature-001",
  "type": "start",
  "next_phase": "spec"
})

# ... pseudocode + test plan design (Stage 3a) ...
# ... code implementation (Stage 3b) ...
# ... testing and risk validation (Stage 3c) ...

mcp__unimatrix__context_cycle({
  "feature": "my-feature-001",
  "type": "phase-end",
  "phase": "pr-review",
  "outcome": "PR review complete. No blocking findings.",
  "next_phase": "done"
})

# Close the cycle after the PR is merged and the session is complete
mcp__unimatrix__context_cycle({
  "feature": "my-feature-001",
  "type": "stop",
  "outcome": "Session 2 complete. All gates passed. PR merged."
})
```

---

## Generalizability

These protocols are Claude Code + Unimatrix reference implementations. The
`context_cycle` pattern is not Claude-specific — it applies to any agentic workflow
tool that can call MCP tools: the three call types (`start`, `phase-end`, `stop`)
model any workflow with named phases, regardless of domain or tooling. The protocols
in this directory are examples of how one team wires context_cycle into a software
delivery workflow; they are a starting point, not a requirement.
