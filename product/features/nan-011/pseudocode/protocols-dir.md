# Component 4 — protocols/ Directory

## Purpose

Create `protocols/` at the repository root containing corrected, distributable copies
of the four internal protocol files plus a new `README.md`. The `.claude/protocols/uni/`
directory is the source of truth — corrections are applied there first, then copied.

---

## Dependency

This component depends on Component 3 (Skills MCP Format Audit) only for ordering
awareness. The protocol files live in `.claude/protocols/uni/`, not `.claude/skills/`.
However, both components must complete before the npm distribution copy step (Component 5).

Source-before-copy constraint: ALL corrections to `.claude/protocols/uni/` files
must be applied and verified BEFORE running the copy commands that populate `protocols/`.

---

## Pre-Work: Read All Four Protocol Files

Before making any changes, read all four source files:
- `.claude/protocols/uni/uni-design-protocol.md`
- `.claude/protocols/uni/uni-delivery-protocol.md`
- `.claude/protocols/uni/uni-bugfix-protocol.md`
- `.claude/protocols/uni/uni-agent-routing.md`

For each file, scan for:
1. References to `unimatrix-server` — must be replaced with `unimatrix`
2. References to `NLI`, `MicroLoRA` as active pipeline requirements — must be removed
3. References to `HookType` closed enum — must be removed
4. `context_cycle` call signatures — must match current format:
   - `mcp__unimatrix__context_cycle({ "feature": "<id>", "type": "start" })`
   - `mcp__unimatrix__context_cycle({ "feature": "<id>", "type": "phase", "phase": "<name>" })`
   - `mcp__unimatrix__context_cycle({ "feature": "<id>", "type": "stop" })`

CHOREOGRAPHY CONSTRAINT: Do not change phase structure, agent spawning order,
wave definitions, gate logic, or any orchestration-level content. Only factual
inaccuracies and removed-feature references are in scope.

---

## Phase 1: Validate and Correct Source Files in .claude/protocols/uni/

### For each of the four protocol files, apply this validation sequence:

```
validate_protocol(file_path):
  read file_path

  FOR each occurrence of "unimatrix-server":
    IF in a binary name context (shell command, config example, prose about the binary):
      replace "unimatrix-server" with "unimatrix"
    ELSE:
      document the context — it may be part of a longer name

  FOR each occurrence of "NLI" or "MicroLoRA":
    IF describing NLI as an active, default, or required capability:
      remove the sentence or paragraph
      IF the section would become empty: remove the section header too
    IF describing NLI as an opt-in future capability or config option:
      retain — this is acceptable

  FOR each occurrence of "HookType":
    IF describing HookType as a closed enum of fixed event types:
      remove or replace with domain-agnostic language
    DOCUMENT the change in agent report

  FOR each context_cycle invocation:
    verify parameter names: "feature", "type", and optionally "phase"
    verify "type" values: "start", "phase", "stop"
    IF stale parameter names found (e.g., "phase_id" instead of "type"):
      replace with correct parameter names

  write corrected content back to file_path
```

Apply this to all four files. If a protocol file has none of the above issues,
it is still "validated" — no changes needed.

### Stale Reference Verification Command (run after corrections)

```bash
grep -rn "NLI\|MicroLoRA\|unimatrix-server\|HookType" .claude/protocols/uni/
```

Must return zero matches after corrections.

---

## Phase 2: Create protocols/ Directory and Copy Files

After all corrections are applied and verified in `.claude/protocols/uni/`:

```bash
# Create the directory (if it does not exist)
mkdir -p protocols

# Copy all four protocol files
cp .claude/protocols/uni/uni-design-protocol.md protocols/uni-design-protocol.md
cp .claude/protocols/uni/uni-delivery-protocol.md protocols/uni-delivery-protocol.md
cp .claude/protocols/uni/uni-bugfix-protocol.md protocols/uni-bugfix-protocol.md
cp .claude/protocols/uni/uni-agent-routing.md protocols/uni-agent-routing.md
```

### Dual-Copy Verification (run immediately after copy)

```bash
diff .claude/protocols/uni/uni-design-protocol.md protocols/uni-design-protocol.md
diff .claude/protocols/uni/uni-delivery-protocol.md protocols/uni-delivery-protocol.md
diff .claude/protocols/uni/uni-bugfix-protocol.md protocols/uni-bugfix-protocol.md
diff .claude/protocols/uni/uni-agent-routing.md protocols/uni-agent-routing.md
```

ALL four diffs must produce zero output. If any diff shows differences:
1. The copy step failed or the source was edited after copying
2. Re-apply corrections to the source file
3. Re-copy and re-verify

This check is not optional — R-04 (Critical risk) maps directly to dual-copy drift.

---

## Phase 3: Create protocols/README.md

This file is net-new — it has no source equivalent in `.claude/`. Author it directly
at `protocols/README.md`.

### Content Requirements

The README must be a single document, under 150 lines, covering:

1. **What these protocols are** — two to three sentences explaining that these are
   the reference workflow protocols for Claude Code + Unimatrix delivery, and
   how they relate to the `context_cycle` MCP tool.

2. **How context_cycle works** — explain the three call types and why cycle tracking
   enables workflow-conditioned knowledge delivery:
   - `type: "start"` — begins a feature cycle, sets attribution context
   - `type: "phase"` — marks a phase transition, updates the phase signal used by
     context_briefing to prioritize phase-relevant knowledge
   - `type: "stop"` — closes the cycle, commits all signals to the learning model

3. **Minimal two-phase example** — a worked example showing a design → delivery
   transition. The example must show all three call types:

```
mcp__unimatrix__context_cycle({ "feature": "my-feature-001", "type": "start" })

# ... design phase work ...

mcp__unimatrix__context_cycle({ "feature": "my-feature-001", "type": "phase", "phase": "delivery" })

# ... delivery phase work ...

mcp__unimatrix__context_cycle({ "feature": "my-feature-001", "type": "stop" })
```

4. **Generalizability note** — one sentence or brief paragraph stating that these
   protocols are Claude Code + Unimatrix reference implementations and that the
   `context_cycle` pattern generalizes to any workflow-centric domain. The protocols
   are examples, not requirements.

### Context_cycle Parameter Accuracy

The example MUST use the current parameter names:
- `"feature"` — the feature identifier string
- `"type"` — `"start"`, `"phase"`, or `"stop"`
- `"phase"` — the phase name string (only with `"type": "phase"`)

Do NOT use deprecated or invented parameter names (`phase_id`, `cycle_type`,
`event`, etc.). Read the IMPLEMENTATION-BRIEF.md Function Signatures section
if uncertain.

### Content to EXCLUDE from protocols/README.md

- Internal Unimatrix development conventions
- Swarm configuration details
- Agent spawning instructions
- Gate review criteria
- Anything from CLAUDE.md that is project-internal

This README is for external users who received the npm package.

---

## Verification After Phase 3

```bash
# Confirm README exists
ls protocols/README.md

# Confirm context_cycle appears in README
grep "context_cycle" protocols/README.md
# Must return at least 3 matches (start, phase, stop calls)

# Confirm all three type values appear
grep '"start"' protocols/README.md
grep '"phase"' protocols/README.md
grep '"stop"' protocols/README.md

# Confirm no stale references in the full protocols/ directory
grep -rn "NLI\|MicroLoRA\|unimatrix-server\|HookType" protocols/
# Must return zero matches
```

---

## Final Directory Structure

After this component completes:

```
protocols/
  uni-design-protocol.md       (copy of .claude/protocols/uni/uni-design-protocol.md)
  uni-delivery-protocol.md     (copy of .claude/protocols/uni/uni-delivery-protocol.md)
  uni-bugfix-protocol.md       (copy of .claude/protocols/uni/uni-bugfix-protocol.md)
  uni-agent-routing.md         (copy of .claude/protocols/uni/uni-agent-routing.md)
  README.md                    (new file, authored here)
```

---

## Error Handling

If any protocol file has choreography-level issues that would require structural
changes to fix: do NOT make those changes. File a separate GitHub issue and document
the gap in the agent report. Only factual inaccuracy corrections (feature names,
binary names, removed capabilities) are in scope.

If NLI references in a protocol appear in a section that cannot be removed without
breaking the protocol's structural coherence (e.g., an NLI step that has no replacement
step): remove only the NLI claim, retain the step structure, and note it in the agent
report.

---

## Key Test Scenarios

1. Four diffs (`diff .claude/protocols/uni/X.md protocols/X.md`) all return zero output.
2. `grep -rn "NLI\|MicroLoRA\|unimatrix-server\|HookType" protocols/` returns zero.
3. `protocols/README.md` exists and contains `context_cycle`.
4. README shows all three type values: `"start"`, `"phase"`, `"stop"`.
5. README context_cycle calls use `mcp__unimatrix__context_cycle(...)` with correct parameters.
6. README does not contain internal project conventions.
7. protocols/ directory contains exactly 5 files (4 protocols + README.md).
