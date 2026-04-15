# Research Spike Protocol

Triggers on: "run the spike", "execute the research", "research session", ASS spike execution.

---

## Overview

A research spike answers a bounded question to enable a human decision. It is not a feature — it produces no code, no PRs, no Unimatrix knowledge entries. It produces FINDINGS.md.

**Phase 1 (scope completion) does not happen in this protocol.** It happens interactively — in a uni-zero session, or directly with the human. By the time a research session is invoked, SCOPE.md must already be complete. If it is not, stop and complete the scope before proceeding.

### Two Execution Modes

**Single-spike**: One complete SCOPE.md → invoke `uni-spike-researcher` directly → FINDINGS.md.

**Campaign**: Multiple spikes with dependency ordering → invoke `uni-research-sm` → it dispatches researchers in order, routes findings between dependent spikes, and updates the planning document.

---

## SCOPE.md — Required Fields

Before Phase 2 can begin, SCOPE.md must contain all of the following.

| Field | Description |
|-------|-------------|
| **Goal** | The questions being answered — written as answerable questions, not topics |
| **Breadth** | Where to look: `code-only`, `code+ecosystem`, `industry`, `unknown`, or a combination |
| **Approach** | How to investigate: `investigation`, `evaluation`, `measurement`, `proof-of-concept`, `literature` |
| **Confidence required** | `directional` (recommendation without validation) / `validated` (working PoC required) / `empirical` (data from measurement required) |
| **Target outputs** | What FINDINGS.md contains: decision, ranked options, data + interpretation, go/no-go, design input |
| **Constraints** | Two types — must be explicit: **Hard** (technically fixed, changing requires rewriting shipped code) vs. **Hypothesis** (design position held by the human, subject to challenge). Researchers must treat Hypothesis constraints as challengeable. |
| **Dependencies** | *(conditional)* Inputs required before this spike starts; what this spike unblocks after finishing |
| **Prior art** | What is already known — researcher starts here, not from zero |

### Missing Field Protocol

If any required field is missing, marked TBD, or ambiguous, **stop and ask the human**. Do not assume a default. Do not infer intent from context. Do not proceed to Phase 2.

Ask specifically and concisely — name the field, explain what it determines, and offer the available options where applicable:

> "SCOPE.md for ASS-NNN is missing **[field]**. This determines **[what it controls]**. Options are: **[options if applicable]**. What should it be?"

One question per missing field. If multiple fields are missing, list them all before asking — do not ask one at a time in a chain.

**Never substitute a judgment call for a missing field.** The human wrote the scope; if a field is absent it means the decision was not made, not that the default is acceptable. A researcher executing against an incomplete scope produces findings that may not answer the right question.

---

## Which Researcher?

Three routing cases. Evaluate in order — stop at the first match.

---

### Case 1 — External only → `uni-external-researcher`

ALL of the following are true:
- Breadth is predominantly `industry`, `external`, `literature`, or `unknown`
- AND at least one of: confidence = `validated`, confidence = `empirical`, approach = `proof-of-concept`, approach = `literature`
- AND no Goal questions require reading the Unimatrix codebase or querying Unimatrix state

Single researcher. Writes `FINDINGS.md` directly.

---

### Case 2 — Internal / mixed → `uni-spike-researcher`

Use when the answer requires understanding what the project already does — mixed breadth, code-dominant, directional confidence, investigation or evaluation with internal anchoring.

Single researcher. Writes `FINDINGS.md` directly.

When in doubt between Case 1 and Case 2: if the answer requires understanding what the project already does, use `uni-spike-researcher`.

---

### Case 3 — Both tracks present → parallel dual-track

Use when the SCOPE.md has **distinct** Goal questions that cleanly map to two separate tracks:

- **Internal track**: questions requiring codebase reading, architecture analysis, or Unimatrix context (e.g., "what does the current schema look like", "how does the existing rate limiter work")
- **External track**: questions requiring ecosystem evaluation, web search, library landscape, or literature (e.g., "which MCP clients support HTTP transport", "what does OAuth 2.1 spec require")

Both tracks run in parallel. Each researcher receives the full SCOPE.md plus an explicit list of which Goal questions are theirs. Each writes a track-specific file. A synthesis step follows.

**Routing test**: Can you split the Goal questions into two non-overlapping lists — one that a researcher could answer without any external sources, and one that a researcher could answer without opening the codebase? If yes, Case 3 applies.

---

**Campaign routing example** (Wave 2 prerequisites):

| Spike | Case | Researcher(s) |
|-------|------|--------------|
| ASS-041 Transport + Auth | 2 | `uni-spike-researcher` — rmcp integration is internal |
| ASS-042 Security Architecture | 2 | `uni-spike-researcher` — project-dominant, Unimatrix-heavy |
| ASS-043 Container + Packaging | 2 | `uni-spike-researcher` — mixed but anchored in our config |
| ASS-044 Admin UI | 2 | `uni-spike-researcher` — mixed |
| ASS-045 Monetization Strategy | 1 | `uni-external-researcher` — BSL/FSL landscape, no internal source |
| ASS-046 GGUF Feasibility | 1 | `uni-external-researcher` — ecosystem evaluation + PoC |
| ASS-047 Scalability | 2 | `uni-spike-researcher` — primarily our architecture |
| ASS-049 Multi-LLM Compatibility | 3 | Both — external: client capability survey, HTTP auth; internal: tool descriptions, eval harness, injection limits |

---

## Phase 2 — Execution

**The researcher's agent message is not the findings. The written file is the findings.**

Every researcher — regardless of track count — writes their findings to a file before the primary agent reads them. The primary agent does not extract findings from the agent's response message. It reads the file. If the file does not exist when the researcher completes, the researcher has not finished.

---

### Single-track (Cases 1 and 2)

**Agent**: `uni-spike-researcher` or `uni-external-researcher` (per routing above)
**Input**: complete SCOPE.md path
**Output**: `product/research/{ass-NNN}/FINDINGS-RAW.md`

Spawn with:
```
Spike: {ass-NNN}
SCOPE.md: product/research/{ass-NNN}/SCOPE.md
Agent ID: {id}
Output file: product/research/{ass-NNN}/FINDINGS-RAW.md

Write your findings to FINDINGS-RAW.md. This is your only deliverable — do not
summarize findings in your response message. The primary agent reads the file, not
your message.
```

After the researcher completes, the primary agent reads `FINDINGS-RAW.md` and produces `FINDINGS.md` via synthesis (see Synthesis step below). The researcher's raw file is retained alongside `FINDINGS.md` as the audit trail.

---

### Dual-track (Case 3) — parallel execution + synthesis

**Step 1 — Question partitioning**

Before spawning, split the SCOPE.md Goal questions into two explicit lists:
- `INTERNAL_QUESTIONS`: questions answerable from the codebase and Unimatrix state alone
- `EXTERNAL_QUESTIONS`: questions answerable from external sources alone

Include these lists in each researcher's spawn prompt so there is no ambiguity about scope.

**Step 2 — Spawn both in parallel (single message)**

Spawn both researchers in a single message:

```
[uni-spike-researcher]
Spike: {ass-NNN}
SCOPE.md: product/research/{ass-NNN}/SCOPE.md
Output file: product/research/{ass-NNN}/FINDINGS-INTERNAL.md
Your questions: {INTERNAL_QUESTIONS — listed explicitly}
Note: Answer only your assigned questions. External questions are handled in parallel
by a separate researcher. Write findings to FINDINGS-INTERNAL.md — do not summarize
in your response message.

[uni-external-researcher]
Spike: {ass-NNN}
SCOPE.md: product/research/{ass-NNN}/SCOPE.md
Output file: product/research/{ass-NNN}/FINDINGS-EXTERNAL.md
Your questions: {EXTERNAL_QUESTIONS — listed explicitly}
Note: Answer only your assigned questions. Internal questions are handled in parallel
by a separate researcher. Write findings to FINDINGS-EXTERNAL.md — do not summarize
in your response message.
```

Neither researcher writes `FINDINGS.md`. Each writes only their track file.

---

### Synthesis — all cases

Synthesis runs after all researcher files are written. It is always a separate step — never merged into the researcher's own work.

**Single-track**: primary agent spawns `uni-spike-researcher` as synthesizer after `FINDINGS-RAW.md` exists.

**Dual-track**: primary agent spawns `uni-spike-researcher` as synthesizer after both track files exist.

Synthesizer prompt:

```
Spike: {ass-NNN} — SYNTHESIS
SCOPE.md: product/research/{ass-NNN}/SCOPE.md
Researcher file(s): {list all written findings files}
Output: product/research/{ass-NNN}/FINDINGS.md

Synthesize the researcher findings into a single coherent FINDINGS.md.
- Answer every Goal question from SCOPE.md, drawing from the input files
- Resolve any tensions between tracks explicitly — do not pick the more convenient answer silently
- Merge Unanswered Questions and Out-of-Scope Discoveries from all input files
- Write one Recommendations Summary covering all questions
Do not re-investigate. Synthesize only from the input files.
```

The synthesizer does not spawn sub-agents or do additional investigation. If a question was not answered by any researcher file, it goes to Unanswered Questions with the reason.

**Retained files**: All researcher files (`FINDINGS-RAW.md`, `FINDINGS-INTERNAL.md`, `FINDINGS-EXTERNAL.md`) are kept alongside `FINDINGS.md` — they are the audit trail showing what each researcher contributed.

The synthesizer does not spawn sub-agents or do additional investigation. It reads both track files and writes `FINDINGS.md`. If a question was not answered by either track, it goes to Unanswered Questions with the reason.

**Intermediate files**: `FINDINGS-INTERNAL.md` and `FINDINGS-EXTERNAL.md` are retained alongside `FINDINGS.md` — they are the audit trail showing what each track contributed.

---

## Phase 3 — Validation

Lightweight. Does not require a separate agent.

Check FINDINGS.md against SCOPE.md:
- Every Goal question has an explicit answer
- Every answer includes evidence and a recommendation
- Unanswered questions are listed with reason (blocked / out of scope / needs another spike)
- Confidence level is consistent with approach type

**Who validates:**
- Standalone spike → human reviews FINDINGS.md directly
- Feed-through spike → consuming spike's researcher reads FINDINGS.md as prior art context; gaps surface when they try to use it

If validation fails: return FINDINGS.md to the researcher with specific gaps. Do not proceed to Phase 4 until gaps are addressed.

---

## Phase 4 — Routing

**No Unimatrix writes. From any research session.**

Knowledge flows from research into Unimatrix only via downstream sessions (design, delivery, retro) after findings have been validated through implementation. Research is provisional; Unimatrix holds settled knowledge.

Routing actions:
1. **Update planning document** — add a one-paragraph finding summary to the relevant planning doc (e.g., `product/WAVE2-ROADMAP.md`). Findings summary, not full FINDINGS.md content.
2. **Feed-through** — for spikes that feed another spike: pass `product/research/{ass-NNN}/FINDINGS.md` path as prior art context in the consuming spike's SCOPE.md or spawn prompt.
3. **Human handoff** — for standalone spikes: present FINDINGS.md path and the recommendations summary to the human. The human decides what happens next.

---

## Campaign Execution Order

When running a campaign, the SM reads all SCOPE.md files and their dependency fields to determine execution order.

```
1. Identify independent spikes (no Dependencies field, or dependencies already satisfied)
2. Dispatch all independent spikes in parallel (one researcher per spike)
3. Wait for all independent spikes to complete and pass Phase 3
4. Dispatch dependent spikes, passing prior-spike FINDINGS.md as context
5. After all spikes complete: update planning document, present summary to human
```

Spikes within the same tier are always dispatched in a single message (parallel). Never serialize unnecessarily.

---

## Rules

- **SCOPE.md must be complete before Phase 2 begins.** No exceptions. Missing fields → ask the human. Never assume.
- **Researchers write to a file. The file is the findings.** The agent response message is not the findings. The primary agent reads the file, not the message. A researcher that returns findings only in its message has not completed its work.
- **Synthesis is always a separate step.** The researcher never produces the final `FINDINGS.md` directly. Synthesis runs after all researcher files are written.
- **Researchers are read-only in Unimatrix.** `context_search` and `context_get` are allowed (when breadth includes internal/code). `context_store`, `context_correct`, `context_deprecate`, and all write tools are prohibited.
- **FINDINGS.md is the only deliverable that gates Phase 3.** No code committed, no Unimatrix entries, no ADRs.
- **Scope guard is mandatory.** Interesting findings outside the SCOPE.md boundary are noted in FINDINGS.md under "Out-of-Scope Discoveries" — they are never pursued within the spike. Create a carry-forward issue if warranted.
- **Campaign SM does not generate findings.** It coordinates only. If it starts writing analysis, it is doing the researcher's job.
