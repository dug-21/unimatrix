# ASS-026: Power of Ten Protocol Governance Audit

## Problem Statement

Unimatrix's agent protocols and agent definitions have evolved organically across many features. As the system grows in complexity — 13 specialist agents, 3 session types, 3 validation gates, and a knowledge engine — the governance rules embedded in those protocols have not been evaluated against a principled framework for reliable software process design.

Gerard Holzmann's "The Power of Ten: Rules for Developing Safety-Critical Code" offers exactly such a framework: 10 rules distilled from decades of experience at NASA/JPL writing mission-critical software. These rules prioritize static verifiability, constraint-driven discipline, and elimination of unsafe patterns.

The problem this research addresses: **which of these rules apply analogously to protocol/agent governance, which gaps exist in our current protocols, and where have we overcorrected into rules that are too strict to be practical?**

This is an Assimilate-phase spike. The output is a findings document — no protocol edits in this session.

---

## Goals

1. Establish a canonical, complete description of the 10 Power of Ten rules in the context of Unimatrix agent governance.
2. Produce a rule-by-rule coverage analysis: already covered, gap/missing, or overkill/too strict.
3. Identify the 5–10 highest-value recommendations to adopt from the rules.
4. Identify 2–3 areas where current protocols are too strict and could be relaxed.
5. Surface open questions for human review before any protocol changes are made.

## Non-Goals

- This spike does NOT produce protocol changes or agent definition edits.
- This spike does NOT evaluate Rust source code quality (the paper is about code; we map it to process governance).
- This spike does NOT produce implementation briefs, architecture docs, or spec documents.
- This spike does NOT prioritize all possible protocol improvements — only those informed by the Power of Ten framework.
- This spike does NOT evaluate the Unimatrix knowledge engine (context_store, context_search, etc.) — only the workflow protocols and agent definitions in `.claude/`.

---

## Background Research

### The Power of Ten Rules (Gerard Holzmann, JPL/NASA, 2006)

The paper was written at NASA's Jet Propulsion Laboratory for safety-critical embedded systems in C. It establishes 10 rules specifically chosen because a static analysis tool can check all of them mechanically. The key insight: **rules are only valuable if they are verifiable**. Rules that cannot be checked create the illusion of safety without the substance.

The rules and their analogical translations to agent protocol governance:

#### Rule 1: Restrict Control Flow
**Original**: No `goto`, `setjmp`/`longjmp`, or direct recursion. These constructs make it impossible to statically verify program termination and control flow.
**Analogical translation**: Agents should follow linear, defined paths through protocols. Conditional branching (gate outcomes, rework loops) should have fixed maximum depth — no unbounded re-entry, no agent A spawning agent B which spawns agent A.

#### Rule 2: Fixed Loop Bounds (All Loops Have a Provable Upper Bound)
**Original**: Every loop must have a statically provable upper-bound on iterations. The checking tool must be able to verify the bound without running the code.
**Analogical translation**: Rework loops must have explicit iteration caps. "Try until it works" is the process equivalent of an unbounded loop.

#### Rule 3: No Dynamic Memory Allocation After Initialization
**Original**: No heap allocation after startup — all memory allocated from a fixed pool. Prevents unpredictable memory exhaustion at runtime.
**Analogical translation**: No unbounded growth of agent spawning depth, context window consumption, or protocol branching after session initialization. Resources (context window, agent count) should be budgeted at session start, not grown dynamically in response to failures.

#### Rule 4: Function Length — No More Than ~60 Lines / One Page
**Original**: Functions must fit on a single printed page (~60 lines). The constraint is cognitive, not aesthetic: if you can't hold the function in your head, you can't verify it.
**Analogical translation**: Agent definitions and protocol sections should remain concise enough to hold in a single context window read. If a protocol section requires reading 5 other files before it makes sense, its "length" in cognitive terms is too long.

#### Rule 5: Minimum Assertion Density — At Least 2 Assertions Per Function
**Original**: Every function must have at least 2 assertions that verify assumptions about inputs, invariants, and outputs. Assertions document intent and catch violations at runtime.
**Analogical translation**: Every significant protocol step should have an explicit validation check (a "gate") that verifies the output before proceeding. The gate should be specific and testable, not a general "looks good" review.

#### Rule 6: Minimal Variable Scope — Declare at Smallest Possible Scope
**Original**: Variables should be declared at the narrowest scope possible, minimizing the surface area over which state can be corrupted.
**Analogical translation**: Agent responsibilities should be scoped as narrowly as possible. Agents should not have access to or modify artifacts outside their defined output scope.

#### Rule 7: Check All Return Values and Validate All Parameters
**Original**: Non-void function return values must be checked. Function parameters must be validated inside the function (not assumed valid by the caller).
**Analogical translation**: Every agent output must be checked (via a gate or coordinator review). Agents should not assume their inputs are valid — input validation (reading the spawn prompt, verifying file paths exist) is the agent's responsibility.

#### Rule 8: Restrict Preprocessor Use — Include Files and Simple Macros Only
**Original**: No token pasting, recursive macros, conditional compilation beyond header guards. Preprocessor complexity makes code unverifiable.
**Analogical translation**: Protocols should not embed complex conditional logic that requires reading other documents to understand. Protocol branching should be explicit and self-contained, not dependent on deeply nested cross-references.

#### Rule 9: Restrict Pointer Use — At Most One Level of Dereferencing; No Function Pointers
**Original**: No `**ptr` dereferences, no function pointers. Function pointers make call graphs unverifiable; multi-level dereferencing creates aliasing that is impossible to track statically.
**Analogical translation**: Agent authority chains should be at most one level deep: coordinator → specialist. No specialist should spawn other specialists. Protocols should not rely on dynamic dispatch ("figure out the right agent at runtime").

#### Rule 10: Compile With All Warnings Enabled; Run Static Analysis Daily; Zero Warnings
**Original**: All code compiled with maximum warnings from day one. Static analysis run daily. Zero warnings required — not "fix warnings later."
**Analogical translation**: Protocol and agent definition changes should be reviewed for consistency immediately, not deferred. Validator checks at gates should catch deviations from architectural contracts immediately — not at delivery time.

---

### Existing Codebase Pattern Analysis

Reading of all files in `.claude/protocols/uni/` and `.claude/agents/uni/` revealed the following structural patterns:

**Protocols analyzed:**
- `uni-design-protocol.md` (360 lines) — Design session orchestration
- `uni-delivery-protocol.md` (573 lines) — Delivery session orchestration
- `uni-bugfix-protocol.md` (431 lines) — Bugfix session orchestration
- `uni-agent-routing.md` (188 lines) — Agent roster and swarm composition

**Agents analyzed (16 files):**
- Coordinator: `uni-scrum-master`
- Design: `uni-researcher`, `uni-architect`, `uni-specification`, `uni-risk-strategist`, `uni-vision-guardian`, `uni-synthesizer`
- Delivery: `uni-pseudocode`, `uni-tester`, `uni-rust-dev`
- Gates: `uni-validator`
- Bug: `uni-bug-investigator`
- Shared: `uni-security-reviewer`, `uni-docs`
- Meta: `AGENT-CREATION-GUIDE.md`, `README.md`

---

### Coverage Analysis

The following table maps each Power of Ten rule to its analog in Unimatrix protocols and agent definitions, with a coverage determination.

| Rule | Paper Rule | Protocol Analog | Coverage | Notes |
|------|-----------|-----------------|----------|-------|
| R-1 | Restrict control flow / no recursion | No recursive agent spawning; specialists don't spawn specialists; Scrum Master must not spawn itself ("Never spawn yourself") | **Covered** | SM cannot spawn itself. Specialists return to coordinator. Depth cap is 1 level. |
| R-2 | Fixed loop bounds | Rework loops capped at max 2 iterations per gate. "Max 2 rework iterations per gate — protects context window." | **Covered** | Both delivery and bugfix protocols specify max 2 rework iterations per gate. SCOPE FAIL escalation after cap reached. |
| R-3 | No dynamic resource growth | Context budget guidance exists but is informal. No explicit budgeting of agent count per session at start. Delivery protocol has a max 5 agents per stage rule. | **Partial Gap** | Max-5 rule exists. But context window consumption is not formally budgeted upfront. Protocol file lengths are growing (delivery protocol: 573 lines). |
| R-4 | Function length / cognitive scope | Agent definition size varies: uni-risk-strategist is 288 lines, uni-tester is 315 lines, uni-delivery-protocol is 573 lines. No explicit line limit on protocol or agent files. | **Gap** | No "protocol section size" or "agent definition size" limit. The delivery protocol is approaching cognitive overload — some agents must read 5+ files before they can begin. |
| R-5 | Minimum assertion density / every function validated | Three mandatory gates (3a, 3b, 3c) in delivery. One gate in bugfix. Knowledge stewardship checks at every gate. Self-check checklists in every agent definition. | **Well Covered** | Gate density is high — arguably higher than the rule requires. Every stage is validated. Self-checks are required before agent returns. |
| R-6 | Minimal variable scope | Agents have strictly bounded output scope: each agent writes to its own directory. Architects write to `architecture/`, testers to `test-plan/`, etc. The "docs agent" explicitly states it modifies README.md and nothing else. | **Well Covered** | Output scope is extremely well-defined in every agent definition. The "What You Do NOT Do" sections are explicit. |
| R-7 | Check all return values / validate parameters | Agents validate inputs (self-check includes "SCOPE.md has all required sections"). Coordinator checks agent return before proceeding. Gates validate outputs. Agent report format is mandatory. | **Covered** | Return format is specified. Coordinator checks gate results before proceeding. But: agents validate their *inputs* informally ("read the files in your spawn prompt") — no systematic input validation requirement. |
| R-8 | Restrict preprocessor / no complex conditional compilation | Protocol cross-references are substantial. Delivery protocol references uni-git skill, USAGE-PROTOCOL.md, integration test harness, and gate-specific procedures. Agents told to "read other documents" before acting. | **Partial Gap** | Self-contained protocol sections are preferred but not enforced. The "read 5 files before you begin" pattern is the process equivalent of deep macro nesting. Some simplification would improve verifiability. |
| R-9 | Restrict pointer depth / no function pointers | Agent spawning is one level deep (coordinator → specialist). Specialists do not spawn other specialists. Protocol branching is explicit (not dynamic). | **Covered** | Coordinator-only spawning is enforced by the SM role boundary ("MANDATORY: You MUST spawn subagents for ALL work."). |
| R-10 | Zero-warning static analysis from day one | Gate 3b requires `cargo audit` and `cargo clippy -- -D warnings` with zero warnings. Protocol compliance is checked by the validator using the knowledge stewardship block requirement. | **Well Covered** | Zero-warning policy for Rust code is explicit and enforced at Gate 3b. Stewardship compliance is checked at all three gates. |

---

### Prioritized Recommendations to Adopt

The following recommendations are ranked by expected protocol improvement value, based on the coverage gaps and partial gaps identified above.

#### Priority 1 (High Impact, Directly Actionable)

**REC-01: Establish Explicit Cognitive Size Limits for Protocol Sections and Agent Definitions**
*Maps to Rule 4 (Function Length)*

The delivery protocol is 573 lines. The uni-tester agent is 315 lines. The uni-risk-strategist is 288 lines. These are approaching the limit of what can be held in a single cognitive read. Apply a soft limit of 300 lines for agent definitions and 400 lines for protocol files. Any section exceeding this limit should be split into a dedicated sub-document (like the integration test harness already is via USAGE-PROTOCOL.md).

Specific candidates: The delivery protocol's "Integration Test Harness" section could be fully extracted (it's already mostly duplicated from USAGE-PROTOCOL.md). The risk strategist's dual-mode structure adds cognitive overhead.

**REC-02: Add a Formal Input Validation Requirement to Agent Definitions**
*Maps to Rule 7 (Check All Return Values and Parameters)*

Currently, agents are instructed to "read the files in your spawn prompt" but there is no systematic check that required inputs exist before proceeding. Agents should have an explicit "Before Starting — Validate Inputs" step in their definition that checks: required paths exist, SCOPE.md has required sections, gate results are properly formatted. A missing SCOPE.md should produce an explicit failure, not a silent deviation.

**REC-03: Formalize the Cross-Reference Budget in Protocols**
*Maps to Rule 8 (Restrict Preprocessor Complexity)*

The delivery protocol sends agents to read: IMPLEMENTATION-BRIEF.md, ARCHITECTURE.md, pseudocode/OVERVIEW.md, pseudocode/{component}.md, and test-plan/{component}.md — five documents before implementation begins. While this is necessary, the protocol does not budget these reads or specify what to do if a required document is missing or malformed. Add explicit preconditions to each stage: "Stage 3b requires documents: [list]. If any are missing, escalate to Delivery Leader before implementing."

#### Priority 2 (Medium Impact, Requires More Thought)

**REC-04: Make Context Window Budget Explicit at Session Start**
*Maps to Rule 3 (No Dynamic Memory Allocation After Initialization)*

Currently the delivery protocol estimates context usage informally. The guidance "Cargo output truncated to first error + summary line" exists to protect context, but the full budget is not stated. Consider adding a "Session Resource Budget" section to the delivery protocol: max agents per wave (5), max rework iterations (2), max file reads per agent spawn (5), expected context window budget per phase. This makes the resource contract explicit and helps coordinators identify when a session is at risk before it fails.

**REC-05: Require a Precondition Table at the Top of Each Protocol Phase**
*Maps to Rule 7 (Validate Parameters) + Rule 5 (Assertion Density)*

Each protocol phase (Phase 1, Phase 1b, Phase 2a, Stage 3a, Stage 3b, Stage 3c) should begin with an explicit precondition: "This phase requires: [artifact list]. These must exist and be non-empty before agents are spawned." This is the process equivalent of parameter validation at function entry. Currently the delivery protocol says "Prerequisite: Gate 3a PASSED" — this pattern should be generalized to every phase.

**REC-06: Establish a "Zero Drift" Rule for Agent Definitions**
*Maps to Rule 10 (Zero Warnings from Day One)*

Agent definitions should be reviewed for consistency whenever a protocol is changed. Currently there is no mechanism to catch an agent definition that references a protocol phase that no longer exists, or uses a file path convention that changed. The validator agent should include a check: "Does the agent definition's spawn template match the current protocol's phase structure?" This would be analogous to running static analysis on protocol changes.

**REC-07: Add a Recursion/Re-entry Check to the Validator's Gate 3a**
*Maps to Rule 1 (Restrict Control Flow) + Rule 9 (Restrict Pointer Depth)*

Gate 3a validates pseudocode design. It should explicitly check that no component's pseudocode requires spawning another agent or waiting on another component's output within the same wave. Currently this is an implicit convention ("agents in the same wave are independent") but it is not validated. A pseudocode design that creates hidden dependencies within a wave is the process equivalent of recursion.

#### Priority 3 (Lower Urgency, Long-Term Improvements)

**REC-08: Simplify the Risk Strategist's Dual-Mode Architecture**
*Maps to Rule 4 (Function Length) + Rule 8 (Preprocessor Complexity)*

The uni-risk-strategist has two distinct modes (scope-risk and architecture-risk) in a single 288-line agent definition. This creates cognitive overhead: readers must track which mode they are in. Consider splitting into two separate agent files: `uni-scope-risk.md` (Phase 1b only) and `uni-risk-strategist.md` (Phase 2a+ only). This would reduce each file to ~150 lines and eliminate mode-switching overhead.

**REC-09: Formalize the "Fresh Context Window" Pattern as a Protocol Constraint**
*Maps to Rule 1 (Control Flow) + Rule 3 (Resource Budgets)*

The synthesizer and security reviewer are both "fresh context window" agents. This is a documented design pattern but not a formal protocol constraint. The protocol should explicitly state which agents require fresh context windows and why — and the coordinator should verify this is respected when spawning them (i.e., no large context inherited from the parent chain).

**REC-10: Add a Minimal Scope Check to the Vision Guardian**
*Maps to Rule 6 (Minimal Variable Scope)*

The vision guardian checks scope additions (things in source docs not in SCOPE.md) but does not check whether agents in the design session read and modified files outside their declared output scope. Adding this as a check would enforce the minimal-scope rule at the process level.

---

### Areas Where Protocols May Be Too Strict

**RELAX-01: Knowledge Stewardship as a Gate-Blocking Requirement**

Currently, a missing `## Knowledge Stewardship` block in an agent report causes a REWORKABLE FAIL at every gate (3a, 3b, 3c). This is applied even when an agent has nothing novel to store — they must state "nothing novel to store -- {reason}" or the gate fails. In practice, this adds ceremony for little value when agents are performing read-only tasks (e.g., uni-synthesizer, which is explicitly exempt, but others are not). The requirement could be relaxed to a WARN rather than a FAIL for read-only-tier agents (specification writer, pseudocode specialist, vision guardian), while retaining the hard FAIL for write-tier agents (architect, rust-dev, tester).

*Risk of relaxation*: Knowledge gaps accumulate silently. Mitigation: keep the hard requirement for the write tier, and make it clear in agent definitions which tier they belong to.

**RELAX-02: The 500-Line File Size Hard Limit in Gate 3b**

Gate 3b currently fails any source file exceeding 500 lines. The rule is:
> "No source file exceeds 500 lines — flag any file over this limit as FAIL"

This is a good heuristic but it's applied as an absolute hard failure regardless of context. A 502-line file in a well-factored module with clear separation of concerns is not a safety issue; it is flagged identically to a 2000-line monolith with 50 responsibilities. The threshold could be:
- FAIL at >600 lines (with clear rationale for the exception required in the gate report)
- WARN at 500–600 lines (agent expected to address but not blocking)

*Risk of relaxation*: Gradual file size creep. Mitigation: the WARN at 500 lines keeps the pressure; only allow exceptions up to 600 with documented rationale.

**RELAX-03: Max 2 Rework Iterations Cap as Absolute SCOPE FAIL**

The rework protocol escalates to SCOPE FAIL after 2 failed iterations at any gate. In practice, some REWORKABLE FAILs are due to ambiguity in the gate check set rather than fundamental design problems. After 2 iterations without resolution, the current protocol mandates SCOPE FAIL (session stops, return to human with recommendation). This is appropriate for genuine scope problems but may be too aggressive for cases where the validator and the development agent are interpreting the architecture document differently.

A refined rule: after 2 rework iterations, the coordinator should perform a 5-minute triage before declaring SCOPE FAIL — read both the gate report and the agent's artifact and determine whether the conflict is a genuine scope problem or a gate interpretation issue. If the latter, the coordinator can resolve the interpretation and allow a 3rd iteration with clarified criteria. This adds one explicit human-triage step before the nuclear option.

*Risk of relaxation*: Coordinators may abuse the 3rd iteration escape hatch to avoid surfacing real scope failures. Mitigation: the coordinator must document the triage rationale and include it in the session return to the human.

---

## Proposed Approach

This is a research spike — no protocol changes are made in this session. The output is this SCOPE.md document, which the human reviews to determine which recommendations to pursue.

For each adopted recommendation, a follow-on feature (design → delivery session) would:
1. Scope the specific protocol/agent changes
2. Run the full design + delivery pipeline
3. Validate the change does not break existing session types

Recommendations should be implemented incrementally, one or two at a time, to avoid destabilizing working protocols.

---

## Acceptance Criteria

- AC-01: SCOPE.md contains the full list of all 10 Power of Ten rules with descriptions adapted to the Unimatrix protocol context.
- AC-02: SCOPE.md contains a coverage analysis table mapping each rule to current coverage status (covered / partial gap / gap / well covered).
- AC-03: SCOPE.md contains at least 5 prioritized recommendations with rationale and rule mapping.
- AC-04: SCOPE.md identifies at least 2 areas where current protocols are too strict, with relaxation proposals and risk analysis.
- AC-05: SCOPE.md contains open questions for human review before any protocol changes are made.
- AC-06: SCOPE.md is grounded in actual reading of all protocol and agent definition files (no assumptions about content).

---

## Constraints

- Research scope is limited to `.claude/protocols/uni/` and `.claude/agents/uni/`. No Rust source code evaluation.
- The Power of Ten rules are written for C code in safety-critical embedded systems. Direct application requires analogical reasoning, not literal translation. This introduces interpretation risk.
- The paper's core assumption — that rules must be statically verifiable — does not fully apply to process governance, where "verification" is done by humans and validators rather than static analysis tools. The analogy holds for the *principle* (rules should be checkable) but not the *mechanism*.
- No protocol edits or agent definition changes in this session.

---

## Open Questions

**OQ-01: Which tier of agents should be subject to hard stewardship requirements?**
The current system applies the same stewardship requirement to all agents. Should we formally distinguish write-tier (architect, rust-dev, tester, bug-investigator, risk-strategist) from read-tier (pseudocode, specification, vision-guardian) and apply different enforcement levels?

**OQ-02: Should the risk strategist be split into two distinct agents?**
The dual-mode design of uni-risk-strategist (scope-risk vs. architecture-risk) violates the single-responsibility principle and makes the file significantly longer than other agent definitions. Is the cognitive overhead worth the convenience of a single file?

**OQ-03: Is 500 lines the right threshold for Rust file size, and should it be a hard FAIL or a WARN?**
The current rule fails at exactly 500 lines. Is this threshold based on empirical data from the project, or was it chosen arbitrarily? Has it ever correctly blocked a problematic file vs. incorrectly blocked a well-factored file?

**OQ-04: Should the delivery protocol have an explicit "session resource budget" section?**
Adding this would make the protocol longer (worsening the Rule 4 analog) but make resource management more explicit (improving the Rule 3 analog). Is the tradeoff worth it, or should resource constraints remain implicit?

**OQ-05: How should we handle the case where a gate interpretation conflict arises (RELAX-03)?**
The proposed "coordinator triage before SCOPE FAIL" adds a step but also adds ambiguity. Should there be explicit criteria for when the coordinator is allowed to grant a 3rd iteration, to prevent abuse?

**OQ-06: Should the Power of Ten framework inform a formal protocol review cycle?**
Rather than making one-off improvements, should we establish a quarterly protocol review that uses this framework as a checklist? The analogy to Rule 10 ("run static analysis daily") would be periodic, structured protocol audits.

---

## Tracking

{Will be updated with GH Issue link after Session 1}
