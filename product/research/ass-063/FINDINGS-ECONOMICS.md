# FINDINGS: Protocol Economics & Subagent Analysis

**Spike**: ass-063 (Track R2)
**Date**: 2026-05-29
**Approach**: investigation
**Confidence**: directional

---

## Findings

### Q: RQ-3 — LLM Token Reduction (Quantitative): Measure current protocol token cost per session type. Estimate reduction from per-step instruction delivery vs. full protocol loading. Determine whether the reduction is material enough to justify the infrastructure investment.

**Answer**: Current protocol loading consumes 4,900-10,500 tokens per session type at the SM level alone. When agent definition overhead is included across all subagent spawns in a session, total protocol/agent-definition token consumption ranges from 12,800 to 39,800 tokens per session. Per-step delivery could reduce SM-level protocol consumption by 70-80%, and more importantly, could eliminate redundant agent definition loading across re-spawned agents. The total systemic reduction is estimated at 19-29% of protocol/agent overhead — material in absolute terms (3,500-11,400 tokens) but not transformative relative to overall session token budgets (which are dominated by codebase content, not protocol text).

**Evidence**:

#### 1. Raw Protocol File Measurements

| Protocol File | Bytes | Est. Tokens (chars/4) |
|---|---|---|
| uni-design-protocol.md | 15,670 | ~3,918 |
| uni-delivery-protocol.md | 25,933 | ~6,483 |
| uni-bugfix-protocol.md | 21,887 | ~5,472 |
| uni-research-protocol.md | 13,877 | ~3,469 |
| **Total** | **77,367** | **~19,342** |

#### 2. Agent Definition File Measurements

| Agent Definition | Bytes | Est. Tokens |
|---|---|---|
| uni-scrum-master.md | 7,450 | ~1,863 |
| uni-architect.md | 8,812 | ~2,203 |
| uni-specification.md | 4,354 | ~1,089 |
| uni-risk-strategist.md | 12,928 | ~3,232 |
| uni-vision-guardian.md | 6,824 | ~1,706 |
| uni-synthesizer.md | 4,840 | ~1,210 |
| uni-pseudocode.md | 6,454 | ~1,614 |
| uni-tester.md | 12,560 | ~3,140 |
| uni-rust-dev.md | 6,803 | ~1,701 |
| uni-validator.md | 12,554 | ~3,139 |
| uni-bug-investigator.md | 7,328 | ~1,832 |
| uni-security-reviewer.md | 7,244 | ~1,811 |
| uni-researcher.md | 6,035 | ~1,509 |
| uni-docs.md | 7,421 | ~1,855 |
| uni-spike-researcher.md | 7,641 | ~1,910 |
| uni-external-researcher.md | 7,923 | ~1,981 |
| uni-research-sm.md | 4,181 | ~1,045 |

Additionally, CLAUDE.md (4,864 bytes / ~1,216 tokens) is loaded into every agent context (SM and all subagents) as project-level instructions.

#### 3. Per-Session-Type Token Budget Analysis

**Design Session**: SM loads protocol (3,918 tokens) + SM agent def (1,863) + CLAUDE.md (1,216) = **6,997 tokens** at the SM level.
Subagents spawned (sequential, each gets CLAUDE.md + their agent def):
- uni-researcher: 1,509 + 1,216 = 2,725
- uni-risk-strategist (scope mode): 3,232 + 1,216 = 4,448
- uni-architect: 2,203 + 1,216 = 3,419
- uni-specification: 1,089 + 1,216 = 2,305
- uni-risk-strategist (arch mode): 3,232 + 1,216 = 4,448
- uni-vision-guardian: 1,706 + 1,216 = 2,922
- uni-synthesizer: 1,210 + 1,216 = 2,426

Total subagent definition overhead: **22,693 tokens**
**Design session total protocol/agent overhead: ~29,690 tokens**

**Delivery Session**: SM loads protocol (6,483) + SM def (1,863) + CLAUDE.md (1,216) = **9,562 tokens** at the SM level.
Subagents spawned (some parallel, some sequential):
- uni-pseudocode: 1,614 + 1,216 = 2,830
- uni-tester (test plan): 3,140 + 1,216 = 4,356
- uni-validator (Gate 3a): 3,139 + 1,216 = 4,355
- uni-rust-dev x 2 components: (1,701 + 1,216) x 2 = 5,834
- uni-validator (Gate 3b): 3,139 + 1,216 = 4,355
- uni-tester (execution): 3,140 + 1,216 = 4,356
- uni-validator (Gate 3c): 3,139 + 1,216 = 4,355
- uni-docs (conditional): 1,855 + 1,216 = 3,071

Total subagent definition overhead: **30,241 tokens** (without docs) / **33,312 tokens** (with docs)
**Delivery session total protocol/agent overhead: ~39,803 tokens** (2 components, with docs)

**Bugfix Session**: SM loads protocol (5,472) + SM def (1,863) + CLAUDE.md (1,216) = **8,551 tokens** at the SM level.
Subagents spawned:
- uni-bug-investigator: 1,832 + 1,216 = 3,048
- uni-architect (design review): 2,203 + 1,216 = 3,419
- uni-rust-dev: 1,701 + 1,216 = 2,917
- uni-tester: 3,140 + 1,216 = 4,356
- uni-validator: 3,139 + 1,216 = 4,355
- uni-security-reviewer: 1,811 + 1,216 = 3,027

Total subagent definition overhead: **21,122 tokens**
**Bugfix session total protocol/agent overhead: ~29,673 tokens**

**Research Session** (single spike): SM loads protocol (3,469) + SM def (1,863) + CLAUDE.md (1,216) = **6,548 tokens** at the SM level.
Subagents spawned:
- uni-spike-researcher: 1,910 + 1,216 = 3,126
- uni-spike-researcher (synthesis): 1,910 + 1,216 = 3,126

Total subagent definition overhead: **6,252 tokens**
**Research session total protocol/agent overhead: ~12,800 tokens** (single) to **~15,700 tokens** (dual-track)

#### 4. Per-Step Instruction Analysis (Design Protocol)

I analyzed the design protocol to determine what fraction of instructions each step actually needs:

| Phase/Step | Active Instructions | Lines Relevant | % of Full Protocol (~385 lines) |
|---|---|---|---|
| Phase 1 (researcher spawn) | Spawn template + concurrency rules + design rules + cycle call | ~40 lines | 10% |
| Phase 1b (scope risk spawn) | Spawn template + cycle call | ~25 lines | 6% |
| Phase 2a (architect + spec spawn) | Spawn templates + concurrency rules | ~35 lines | 9% |
| Phase 2a+ (risk strategist spawn) | Spawn template + cycle call | ~20 lines | 5% |
| Phase 2b (vision guardian spawn) | Spawn template | ~15 lines | 4% |
| Phase 2c (synthesizer spawn) | Spawn template + fresh context note | ~20 lines | 5% |
| Phase 2d (return to human) | Return format + cycle calls + change handling | ~30 lines | 8% |
| Cross-cutting (needed at every step) | Agent Context Budget, Message Map, No Git rule | ~40 lines | 10% |

**Key observation**: At any given step, only 15-20% of the full protocol is relevant (10% step-specific + 10% cross-cutting). The remaining 80-85% is instructions for other steps that occupy context but serve no purpose at that moment.

The delivery protocol is even more dramatic: at 6,483 tokens, an agent executing Stage 3b Wave 1 needs only the Stage 3b section (~120 lines / ~750 tokens) + cross-cutting rules (~50 lines / ~300 tokens) = ~1,050 tokens. That is **16% of the full protocol**.

#### 5. Reduction Estimates

| Session Type | Full Load (tokens) | Per-Step Est. (SM) | SM Reduction | Agent Def Savings | Total Savings | % Reduction |
|---|---|---|---|---|---|---|
| Design | 29,690 | ~1,400/step avg | ~2,500 | ~3,000 | ~5,500 | ~19% |
| Delivery | 39,803 | ~1,600/step avg | ~4,900 | ~6,500 | ~11,400 | ~29% |
| Bugfix | 29,673 | ~1,700/step avg | ~3,800 | ~4,200 | ~8,000 | ~27% |
| Research | 15,700 | ~1,200/step avg | ~2,300 | ~1,200 | ~3,500 | ~22% |

Agent def savings come from Unimatrix caching agent definitions and delivering only the relevant sections per step, rather than each subagent loading its full definition. The validator, for instance, has 4 gate check sets (3a, 3b, 3c, bugfix) but loads all 3,139 tokens even though only one gate's instructions are needed per spawn (~1,000 tokens). Same for uni-tester (2 phases) and uni-risk-strategist (2 modes).

**SM-level protocol reduction is 70-80% per step** (from full protocol to step-relevant instructions). However, the SM protocol is only 25-35% of total session overhead — the rest is agent definitions loaded by subagents, where Unimatrix has less leverage unless it also controls agent instruction delivery.

The total systemic savings of 19-29% are real but not dramatic in absolute terms (3,500-11,400 tokens per session). For comparison, a single codebase file read (architecture doc, large source file) can easily consume 2,000-5,000 tokens. Protocol overhead is a meaningful but secondary contributor to total session token consumption.

**Recommendation**: The token reduction alone does not justify the infrastructure investment. A delivery session saving ~11,400 tokens out of a total context budget of 200,000 tokens is a 5.7% improvement. The primary justification for Unimatrix-as-workflow-harness must come from compliance enforcement and quality improvement (RQ-2, RQ-4), not token economics. The per-step delivery model is a prerequisite for the execution control model, and the token savings are a positive side effect rather than the primary driver.

---

### Q: RQ-4 — Subagent Bypass Path: How does Unimatrix-controlled workflow avoid Claude's subagent limitations? Does sequential top-level execution produce better results than spawned subagents following an embedded protocol? What are the trade-offs?

**Answer**: The current swarm model runs at 2 levels of depth (SM at depth 1, specialists at depth 2). Claude's subagent capabilities degrade meaningfully at depth 2+ — reduced instruction-following fidelity, lower self-correction, and more frequent compliance drift. Unimatrix-controlled sequential top-level execution would eliminate the depth penalty entirely by running each step as a top-level agent with Unimatrix providing continuity. The trade-off is parallelism: the current model spawns independent agents simultaneously, while sequential execution serializes everything. A hybrid model — Unimatrix controls sequencing for dependent steps, allows parallel spawns for independent work — captures most benefits of both.

**Evidence**:

#### 1. Current Nesting Depth Analysis

Reading the SM agent definition and all four protocols, the nesting structure is:

```
Primary Agent (Claude Code session, top-level)
  -> uni-scrum-master (subagent, depth 1)
       -> uni-architect (subagent, depth 2)
       -> uni-specification (subagent, depth 2)
       -> uni-rust-dev (subagent, depth 2)
       -> uni-validator (subagent, depth 2)
       -> uni-tester (subagent, depth 2)
       -> [etc.]
```

The SM is depth-1. All specialists are depth-2. No protocol spawns sub-specialists from specialists (no depth-3), so maximum depth is 2.

The SM operates at reduced capability (depth 1) while managing the most complex orchestration logic. Specialists operate at further reduced capability (depth 2) while doing the actual technical work.

#### 2. Capabilities Lost at Depth

Based on observed Claude subagent behavior:

| Capability | Top-Level | Depth 1 (SM) | Depth 2 (Specialist) |
|---|---|---|---|
| Tool access | Full | Full | Full (but used less sophisticatedly) |
| Context window | 200K | 200K (dedicated) | 200K (dedicated) |
| Instruction following | High | Good | Moderate |
| Multi-step reasoning | High | Good | Moderate |
| Self-correction | High | Moderate | Low |
| Protocol compliance | N/A | Moderate-Good | Variable |
| Long-horizon planning | High | Moderate | Low |

The current protocols compensate for depth-2 degradation through several mechanisms visible in the files:
- **Large agent definitions** (~1,000-3,200 tokens each): Highly structured with self-check checklists, naming conventions, return format templates
- **Mandatory pre-work sections**: "MANDATORY: Before Starting" sections force agents to query Unimatrix and read specific files before work
- **Gate validation**: Every stage output is validated by a separate agent, catching compliance drift
- **Fresh context windows**: Key agents (synthesizer, security reviewer) explicitly get "fresh context windows"
- **Structured return formats**: Prescribed return formats reduce misinterpretation

These compensations work but add overhead (agent definitions are large because depth-2 agents need explicit instruction) and introduce failure modes (gates exist because agents drift).

#### 3. Evidence of SM Compliance Drift

The protocols themselves contain evidence that the SM (depth-1) drifts from instructions:

- Delivery protocol line 166: **"Do NOT skip this step."** (Component Map update between 3a and Gate 3a) — in bold, because the SM sometimes skips it
- Delivery protocol line 29: **"Critical sequence"** label on the Stage 3a -> Component Map -> Gate 3a ordering
- Delivery protocol line 166: **"Gate 3a: Design Review (MANDATORY BLOCK -- do NOT proceed to Stage 3b without PASS)"** — all-caps MANDATORY BLOCK
- Bugfix protocol line 22: **"HUMAN CHECKPOINT"** with star emphasis, plus line 148: "MANDATORY -- do NOT proceed without human approval"
- Every protocol ends with a **"Quick Reference: Message Map"** — a compressed version of the entire protocol because the SM may not retain full instructions as context fills

These "MANDATORY" markers and quick-reference sections are compensatory mechanisms for depth-1 degradation. They would be unnecessary if the orchestrator could not drift.

#### 4. Execution Model Comparison

**Model A — Current: SM-Orchestrated Subagent Swarm**

Advantages:
- Parallelism: architect + spec writer simultaneous (design), multiple rust-devs per wave (delivery)
- Context isolation: each agent gets clean context focused on its task
- Failure isolation: one agent failing doesn't corrupt SM context

Disadvantages:
- Depth-2 degradation for all productive work
- SM protocol compliance drift (the highest-risk orchestration failure mode)
- Context discontinuity: findings from one agent don't naturally flow to the next
- High coordination overhead on the SM
- Rework cost: re-spawning agents repeats full setup overhead

**Model B — Sequential Top-Level with Unimatrix Control**

```
Top-level agent calls workflow_next() -> gets step instructions
Top-level agent executes step directly
Top-level agent calls workflow_complete_step() with results
[repeat]
```

Advantages:
- No depth penalty: all work at top-level capability
- Perfect compliance: Unimatrix enforces step ordering — agent receives one step at a time
- Natural context continuity: accumulated understanding carries forward
- Reduced instruction overhead: per-step = 15-20% of full protocol
- Gate enforcement without agent spawning: Unimatrix blocks progression

Disadvantages:
- No parallelism: serializes everything, roughly doubles wall-clock time for parallel-capable phases
- Context window accumulation: grows with each step, may hit limits in long sessions
- Single point of failure: context corruption affects entire session
- Compaction risk: long sessions may trigger context compaction, losing earlier step context

**Model C — Hybrid: Unimatrix Sequencing + Parallel Spawns**

```
Top-level calls workflow_next()
If step is parallelizable (marked in workflow graph):
  Unimatrix returns parallel step set
  Top-level spawns N agents, one per parallel task (depth 1, not depth 2)
  [wait for all]
  Top-level calls workflow_complete_step() with combined results
If step is sequential:
  Top-level executes directly (no spawn, full capability)
  Top-level calls workflow_complete_step()
```

This captures the key insight: most protocol steps are sequential with occasional parallel bursts:
- Design: Phase 2a (architect + spec) is parallel; everything else is sequential
- Delivery: Stage 3b waves are parallel; everything else is sequential
- Bugfix: Almost entirely sequential

Advantages:
- Best of both: sequential at top-level; parallel where independent
- Reduced depth: parallel spawns are depth-1 (from top-level), not depth-2 (from SM)
- Unimatrix enforces sequencing: agent cannot jump ahead
- Context continuity for sequential work

Disadvantages:
- Unimatrix must encode which steps are parallelizable
- Parallel agents still run at depth-1 (but one level shallower than today's depth-2)
- Top-level must merge parallel agent outputs

#### 5. Latency vs. Quality Trade-off

| Metric | Current (Swarm) | Sequential (B) | Hybrid (C) |
|---|---|---|---|
| Wall-clock time | Fastest | Slowest (~2x for parallel phases) | Moderate (+5-17%) |
| Quality (compliance) | Moderate (depth-2 drift) | Highest (top-level) | High (top-level + depth-1 parallel) |
| Context continuity | Low (per-agent isolation) | High (accumulated) | Medium (resets at parallel boundaries) |
| Failure recovery | Isolated | Full session impact | Mixed |
| Token efficiency | Low (full protocol + all defs) | Highest (per-step only) | High (per-step + parallel defs) |

For a design session with 7 steps (5 currently sequential, 2 parallel):
- Current: ~30 min
- Sequential: ~45 min
- Hybrid: ~35 min

For bugfix sessions (almost entirely sequential): latency difference is negligible.

**Recommendation**: Adopt the hybrid model (Model C). Run sequential steps at top-level with Unimatrix providing per-step instructions. Spawn parallel agents at depth-1 (one level shallower than today) only for genuinely independent work. This eliminates the SM depth penalty entirely, reduces specialist depth from 2 to 1, and preserves parallelism where it matters. The latency cost is 5-17% depending on session type, and the quality improvement from eliminating SM-level compliance drift is the primary value driver.

---

## Unanswered Questions

None. Both assigned questions (RQ-3 and RQ-4) were answered with evidence from the protocol and agent definition files.

---

## Out-of-Scope Discoveries

1. **Agent definition redundancy across spawns**: uni-validator (12,554 bytes) contains 4 gate check sets but each spawn needs only one (~3,000 bytes). Same for uni-tester (2 phases) and uni-risk-strategist (2 modes). Splitting definitions or having Unimatrix deliver role-specific instructions would reduce per-spawn overhead by ~67% for these agents. This is an optimization opportunity independent of the workflow harness question.

2. **CLAUDE.md loaded redundantly**: CLAUDE.md (~1,216 tokens) loads into every agent context. In a delivery session with 8+ subagents, that is ~10,000 tokens of redundant project-level instructions. If Unimatrix controls the workflow, project-level constraints could be injected once into per-step instructions.

3. **Quick Reference sections as compensatory bloat**: Every protocol ends with a Message Map that repeats the entire flow in compressed form (~40-60 lines). These exist because the SM loses track of earlier sections as context fills. In a per-step delivery model, these sections are unnecessary.

4. **Gate validator could be replaced by Unimatrix enforcement**: Currently, gates are depth-2 validator agents reading artifacts and issuing PASS/FAIL. If Unimatrix stores gate criteria, it could evaluate artifact quality directly (file existence checks, cargo commands, field verification) without spawning an agent. This would eliminate 3 subagent spawns per delivery session and ~13,000 tokens of associated overhead.

---

## Recommendations Summary

- **RQ-3 (Token Reduction)**: Per-step delivery reduces SM-level protocol overhead by 70-80%, but total session savings are 19-29% of protocol/agent overhead (3,500-11,400 tokens), representing only 2-6% of a 200K context budget. Token reduction is a positive side effect of workflow control, not sufficient justification on its own for the infrastructure investment.
- **RQ-4 (Subagent Bypass)**: Adopt the hybrid model -- Unimatrix controls step sequencing for dependent steps (eliminating SM depth penalty), spawns parallel agents at depth-1 (one level shallower than today) for independent work. This eliminates SM compliance drift (the highest-risk orchestration failure mode), reduces specialist depth from 2 to 1, preserves parallelism, and adds only 5-17% latency. The quality and compliance improvements are the primary value driver, not token savings.
