# ASS-106 — OpenCode as a Unimatrix observation harness: parity assessment + plugin-model opportunity

## Question
opencode is wired for *retrieval* (MCP `context_*` works via `opencode.json`) but emits **no
behavioral signal** — it has no hooks. What does full opencode observation support require to reach
**parity** with the Claude Code hook client, and what does opencode's plugin model let us do
*better* — expressed as improvements to **Unimatrix**, not opencode-generic features?

## Why it matters
opencode is the strategic on-ramp for **local LLMs** and the start of migrating off a Claude-only
development model. It is also the forcing function for **harness-agnostic observation** — a platform
property. Closing it proves **C18** (#5730, `delivery:missing`), the last 🔴 on personal-cloud's
floor, and de-risks every future harness after it (Codex proved the command-hook mirror; opencode
tests whether the model generalizes past that contract). The plugin model may also unlock a
platform-security capability Claude's contract cannot: a **harness-attested agent identity** the
service layer can actually trust (see section E) — the missing precondition for real capability
enforcement under Principle 3.

## Approach & bounded questions (priority order)

### A. OpenCode extension surface (do first)
- What is opencode's hook/plugin architecture — command hooks (spawn + stdin JSON + exit code, like
  Claude/Codex), a **TypeScript plugin API** (event callbacks, in-process), or both?
- What lifecycle events does it expose, and where is the wiring declared (a `hooks` block in
  `opencode.json`, a separate file, a plugin package)?

### B. Parity assessment (the priority)
- Map opencode's events to the seven canonical Unimatrix events: **SessionStart, UserPromptSubmit,
  PreToolUse, PostToolUse, SubagentStart, PreCompact, Stop** (vnc-013 canon). Which fire OOB? Which
  have no equivalent?
- Per reachable event: **payload-shape delta** vs the Claude hook schema `unimatrix hook` consumes on
  stdin. Can a `--provider opencode` path consume it directly, or is a shim/normalization branch
  required?
- Behavioral-signal legs opencode **cannot** reach = the measured **parity gap** (C18's `done_when`).
  Name them (PreCompact / SubagentStart are the usual casualties).

### C. Ingestion path (feasibility, not design)
- Given the mechanism, how does an opencode event reach the observe pipeline? Candidate paths —
  (a) command-hook mirror like `.codex/hooks.json`; (b) a TS plugin shim that invokes
  `unimatrix hook` / UDS; (c) a new `--provider opencode` normalization branch (vnc-013).
  Feasibility + `source_domain` attribution correctness for each. **Flag, don't design.**

### D. Installation changes (C17 extension)
- How does `unimatrix init` detect opencode and provision its config? `opencode.json` merge (cf.
  nan-004 prefix-match `settings.json` merge) vs separate file. MCP/STDIO is already present — must
  not regress. Scope the installer delta.

### E. Doing better — grounded in Unimatrix improvements
Identify where opencode's plugin model exceeds the command-hook contract and which **Unimatrix
capabilities** it could improve. Flag as opportunities mapped to Unimatrix caps — **do NOT design.**

- **(lead) Harness-attested agent identity → real capability enforcement.** Today `agent_id` is
  **self-asserted by the LLM** inside the tool call (it is really an agent *type*), and LLMs are not
  trustworthy — so Principle 3 ("capability checks after identity resolution") rests on an identity
  the platform cannot trust. If opencode's plugin model can attest the agent type **out-of-band**
  (harness-generated, LLM never in the loop), can Unimatrix bind capability checks to a *trusted*
  identity? Assess feasibility and the trust chain: does opencode expose the running agent/subagent
  type to a plugin reliably, can it be delivered on a channel the LLM can't forge, and what would
  Unimatrix need to consume it (vs the current self-reported `agent_id`). Target enforcement shape to
  validate against (illustrative, not a design): `uni-scrum-master` → read-only; a delivery agent →
  may write `lesson-learned` but not `decision`/ADR. Note the touch on integrity poison-resistance
  (SLN1, `asserted`) and the L2 identity seam (ass-100/101).
- Richer behavioral / transcript signal available in-process that the stdin command contract loses.
- Sharper provider / `source_domain` attribution for multi-LLM parity (C14), including **local-model**
  observation.
- Tighter session / cycle binding from typed structured events (no stdin marshalling).

## Output
`ass-106-findings.md`: opencode extension-surface map; the 7-event **parity table** + payload deltas;
a recommended ingestion path with feasibility; the C17 installation-change assessment; and a ranked
list of Unimatrix-improvement opportunities the plugin model enables (led by the trusted-identity
question) — enough for a uni-zero decision on opening a design/delivery cycle for C18.

## Constraints / prior art
- vnc-013 canonical event names (#4305); nan-004 prefix-match hook merge (#1201); col-022 split-brain
  validation; hook-client fail-open + drop-detector (#4800).
- Principle 3 (capability checks after identity resolution); ass-100/101 (edge identity +
  root-of-trust) for the trusted-identity leg; SLN1 poison-resistance (`asserted`).
- Precedent: `.codex/hooks.json` (command-hook mirror), `.claude/settings.json` +
  `.claude/hooks/*.sh`.
- Capabilities: C18 #5730, C17 #5582 (prereq), C10 #5547 (retrieval, already proven).
- Needs **external/web research** (opencode plugin docs) **+ codebase analysis**. Single spike, not a
  campaign.
- Strategic: note platform / domain-agnostic implications (harness-agnostic observation, trusted
  identity as a policy/auth seam) beyond personal-cloud — flag, don't design.
