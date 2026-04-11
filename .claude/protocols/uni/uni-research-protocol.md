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

Before Phase 2 can begin, SCOPE.md must contain all of the following. If any field is missing or marked TBD, return to Phase 1 (scope completion).

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

---

## Which Researcher?

Two researcher agents exist. The SCOPE.md fields determine which to use.

**`uni-external-researcher`** when ALL of the following are true:
- Breadth is predominantly `industry`, `external`, `literature`, or `unknown`
- AND at least one of: confidence = `validated`, confidence = `empirical`, approach = `proof-of-concept`, approach = `literature`

**`uni-spike-researcher`** for everything else — mixed breadth, code-dominant, directional confidence, investigation or evaluation approach with internal anchoring.

When in doubt: if the answer lives primarily in the external world and must be proven rather than reasoned, use `uni-external-researcher`. If the answer requires understanding what the project already does, use `uni-spike-researcher`.

**Campaign example** (Wave 2 prerequisites):

| Spike | Researcher |
|-------|------------|
| ASS-041 Transport + Auth | `uni-spike-researcher` — mixed; rmcp integration is internal |
| ASS-042 Security Architecture | `uni-spike-researcher` — project-dominant, Unimatrix-heavy |
| ASS-043 Container + Packaging | `uni-spike-researcher` — mixed |
| ASS-044 Admin UI | `uni-spike-researcher` — mixed |
| ASS-045 Licensing + Codebase | `uni-external-researcher` — BSL landscape, no internal source of truth |
| ASS-046 GGUF Feasibility | `uni-external-researcher` — ecosystem evaluation + PoC required |
| ASS-047 Scalability | `uni-spike-researcher` — primarily our architecture |

---

## Phase 2 — Execution

**Agent**: `uni-spike-researcher` or `uni-external-researcher` (see routing above)
**Input**: complete SCOPE.md path
**Output**: `product/research/{ass-NNN}/FINDINGS.md`

Spawn with:
```
Spike: {ass-NNN}
SCOPE.md: product/research/{ass-NNN}/SCOPE.md
Agent ID: {id}
```

The researcher executes independently. Campaign SM waits for completion before routing findings to dependent spikes.

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

- **SCOPE.md must be complete before Phase 2 begins.** No exceptions.
- **Researchers are read-only in Unimatrix.** `context_search` and `context_get` are allowed (when breadth includes internal/code). `context_store`, `context_correct`, `context_deprecate`, and all write tools are prohibited.
- **FINDINGS.md is the only research output.** No code committed, no Unimatrix entries, no ADRs.
- **Scope guard is mandatory.** Interesting findings outside the SCOPE.md boundary are noted in FINDINGS.md under "Out-of-Scope Discoveries" — they are never pursued within the spike. Create a carry-forward issue if warranted.
- **Campaign SM does not generate findings.** It coordinates only. If it starts writing analysis, it is doing the researcher's job.
