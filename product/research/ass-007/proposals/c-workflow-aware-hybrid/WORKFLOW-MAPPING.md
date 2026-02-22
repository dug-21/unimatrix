# Proposal C Workflow Mapping: Spec-Driven Development with Risk-Based Testing

Maps every element of the workflow to concrete artifacts under the split source-of-truth model. Traces feature `nxs-020` ("Add embedding cache layer") through the full lifecycle.

---

## 1. .claude/ File Layout (Thin Shells)

### Agent Definitions

Each agent file is identity only: who you are, how to orient, what to check before returning. All domain knowledge, conventions, and process guidance come from Unimatrix at runtime.

**`.claude/agents/ndp/ndp-researcher.md`** (~35 lines)

```yaml
---
name: ndp-researcher
type: researcher
scope: exploration
description: Explores problem spaces and synthesizes findings for scope definition
---
```
```markdown
# Research Agent

You explore problem spaces and synthesize findings.

## Orientation (MANDATORY FIRST STEP)
Before starting, call:
  context_briefing(role: "ndp-researcher", task: "<assigned task>", phase: "research")

## Design Principles
1. Breadth before depth -- survey the landscape before deep-diving
2. Evidence over opinion -- cite sources, quantify where possible
3. Explicit unknowns -- state what you could NOT determine

## Self-Check
- [ ] Findings are sourced and verifiable
- [ ] Unknowns and risks are explicitly stated
- [ ] Output is in product/features/{feature-id}/ only

## Outcome Reporting
context_store(category: "outcome", topic: "<feature-id>",
  tags: ["outcome:completion", "phase:research", "ndp-researcher"])
```

**`.claude/agents/ndp/ndp-designer.md`** (~40 lines)

```yaml
---
name: ndp-designer
type: designer
scope: broad
description: Creates Architecture, Specification, and Risk-Based Test Strategy documents
---
```
```markdown
# Design Agent

You create the three source-of-truth documents from approved scope.

## Orientation (MANDATORY FIRST STEP)
context_briefing(role: "ndp-designer", task: "<assigned task>", phase: "design")

## Design Principles
1. Three documents are separable -- architecture constrains, spec defines, risk strategy validates
2. Component boundaries derive from architecture, not convenience
3. Risk severity drives test investment -- not uniform coverage

## Self-Check
- [ ] Architecture has component breakdown with interfaces
- [ ] Specification has testable acceptance conditions
- [ ] Risk strategy maps every risk to test scenarios
- [ ] No cross-document contradictions

## Outcome Reporting
context_store(category: "outcome", topic: "<feature-id>",
  tags: ["outcome:completion", "phase:design", "ndp-designer"])
```

**`.claude/agents/ndp/ndp-pseudocode.md`** (thin-shell version, ~40 lines)

Identity: per-component pseudocode specialist. Design principles: read architecture first, never invent names, split by component. Self-checks: architecture read, no invented names, per-component output. Pull directive: `context_briefing(role: "ndp-pseudocode", ...)`.

**`.claude/agents/ndp/ndp-rust-dev.md`** (thin-shell version, ~45 lines)

Identity: Rust developer. Design principles: Domain Adapter, async-first, structured errors, tracing. Self-checks: cargo build, cargo test, no stubs, scope check. Pull directive: `context_briefing(role: "ndp-rust-dev", ...)`. Outcome reporting section.

**`.claude/agents/ndp/ndp-validator.md`** (thin-shell version, ~45 lines)

Identity: validation gate. Design principles: glass-box reporting, backward traceability, max 2 iterations. Self-checks: all checks run, report written, confidence computed. Pull directive: `context_briefing(role: "ndp-validator", task: "<gate-id>", phase: "<gate-phase>")`. The gate *criteria* come from Unimatrix; the gate *structure* (discovery protocol, tier system) stays in the file.

**`.claude/agents/ndp/ndp-tester.md`** (thin-shell version, ~40 lines)

Identity: testing specialist, risk-driven validation. Design principles: risk drives testing, component-level plans, Arrange/Act/Assert. Self-checks: tests pass, no flaky tests, coverage matches risk strategy. Pull directive.

### Protocol Files and the Unimatrix Split

Protocol files define the **skeleton**: phase sequence, wave structure, who spawns whom, gate positions. Process **tuning** lives in Unimatrix.

**`.claude/protocols/spec-driven-development.md`** (the workflow protocol)

```markdown
# Spec-Driven Development Protocol

## Phase Sequence
Phase 1: Research -> Phase 2: Design -> Phase 3: Delivery -> Phase 4: Ship
Gate positions: after 3a, 3b, 3c. All gates must pass.

## Phase 1: Research & Scope
Spawn: ndp-researcher
Human gate: approve scope document

## Phase 2: Design
Spawn: ndp-designer
Outputs: Architecture, Specification, Risk-Based Test Strategy
Human gate: approve all three documents

## Phase 3: Delivery
### Stage 3a: Component Design
Spawn: ndp-pseudocode (per component) + ndp-tester (component test plans)
Gate 3a: ndp-validator -- map components to three source docs
### Stage 3b: Implementation
Spawn: ndp-rust-dev (per component)
Gate 3b: ndp-validator -- map code to pseudocode + architecture + spec
### Stage 3c: Testing & Risk Validation
Spawn: ndp-tester (execution)
Gate 3c: ndp-validator -- final risk coverage check

## Escalation Rules
Minor issues: rework within stage
Scope/feasibility: escalate to human

## Process Knowledge
For gate criteria, wave sizing, agent team composition:
  context_lookup(category: "process", tags: ["workflow:spec-driven"])
```

The protocol file is ~60 lines. It defines structure. The line `context_lookup(category: "process", tags: ["workflow:spec-driven"])` is the bridge -- gate criteria specifics (what counts as "minor" vs "scope-breaking", how many rework iterations before escalation) live in Unimatrix as process entries.

**How the split works for gates**: The protocol file says "Gate 3a exists, ndp-validator runs it, it checks component-to-source-doc mapping." Unimatrix process entries say "Gate 3a should also verify that component interfaces have explicit error types" (learned from a retrospective after nxs-018 had interface ambiguity). When the validator calls `context_briefing(role: "ndp-validator", task: "gate-3a", phase: "validation")`, it gets both the base gate definition (from the protocol it reads) AND the process refinements (from Unimatrix).

---

## 2. Unimatrix Data Structures

### Feature nxs-020 Full Lifecycle Trace

#### Phase 1: Research

**STORED:**
```
context_store(
  content: "Embedding cache analysis: current pipeline re-embeds on every search.
    hnsw_rs dump persistence means vectors survive restart, but query embedding
    is recomputed. LRU cache with 1000-entry capacity would cut embedding calls 60%.",
  topic: "nxs-020", category: "research",
  tags: ["embedding", "cache", "performance"], source: "ndp-researcher")
```

**RETRIEVED:**
```
context_briefing(role: "ndp-researcher", task: "analyze embedding cache options",
  phase: "research", feature: "nxs-020")
  -> Returns: conventions for research agents, any prior embedding-related patterns,
     process knowledge for research phase
```

**OUTCOME:**
```
context_store(
  content: "Research complete. 3 findings stored. No blockers. Duration: 1 session.",
  topic: "nxs-020", category: "outcome",
  tags: ["outcome:completion", "phase:research", "ndp-researcher"])
```

**Table effects:**
- ENTRIES: 2 new entries (research finding + outcome)
- TOPIC_INDEX: `(hash("nxs-020"), entry_id)` for both
- CATEGORY_INDEX: `(hash("research"), id1)`, `(hash("outcome"), id2)`
- TAG_INDEX: "embedding" -> id1, "outcome:completion" -> id2, etc.
- FEATURE_ENTRIES: "nxs-020" -> {id1, id2}
- USAGE_LOG: entries from briefing call logged with feature_id="nxs-020", agent_role="ndp-researcher"

#### Phase 2: Design

**STORED** (3 entries, one per document approval):
```
context_store(content: "Architecture approved: EmbeddingCache trait in core/src/traits.rs,
  LruCache<String, Vec<f32>> implementation in crates/ndp-intelligence/src/cache.rs,
  cache invalidation on context_correct calls.",
  topic: "nxs-020", category: "decision",
  tags: ["architecture", "approved", "embedding-cache"])

context_store(content: "Risk strategy approved: R1 (cache staleness after correction) = HIGH,
  R2 (memory pressure from cache growth) = MEDIUM, R3 (cache miss on cold start) = LOW.",
  topic: "nxs-020", category: "decision",
  tags: ["risk-strategy", "approved"])
```

**RETRIEVED:**
```
context_briefing(role: "ndp-designer", task: "design embedding cache architecture",
  phase: "design", feature: "nxs-020")
  -> Returns: existing architecture patterns, prior ADRs about embedding pipeline,
     process knowledge for design phase ("design documents should include component
     interface error types" -- from nxs-018 retrospective)
```

#### Phase 3a: Component Design

**RETRIEVED** (by pseudocode agent and tester):
```
context_briefing(role: "ndp-pseudocode", task: "component pseudocode for embedding cache",
  phase: "implementation", feature: "nxs-020")
  -> conventions for pseudocode, relevant patterns (trait implementation, redb write pattern),
     process knowledge ("gate 3a now checks interface error types")

context_search(query: "embedding pipeline architecture", topic: "nxs-020", k: 3)
  -> returns the Phase 2 architecture decision + any prior embedding patterns
```

**STORED** (component-level knowledge discovered during pseudocode):
```
context_store(content: "EmbeddingCache must implement Send + Sync for tokio spawning.
  Use Arc<Mutex<LruCache>> for thread-safe access.",
  topic: "embedding-cache", category: "pattern",
  tags: ["rust", "async", "cache", "nxs-020"])
```

**Gate 3a -- CORRECTED** (gate finds issue):
```
# Validator discovers component interface missing error type
# Outcome stored:
context_store(content: "Gate 3a WARN: CacheError enum not defined in pseudocode.
  Component interfaces need explicit error types per process rule.
  Sent back to pseudocode agent for rework.",
  topic: "nxs-020", category: "outcome",
  tags: ["outcome:quality", "gate:3a", "rework", "ndp-validator"])
```

After rework, gate passes. The rework itself is tracked -- this data feeds the retrospective.

#### Phase 3b: Implementation

**RETRIEVED** (by coding agents):
```
context_briefing(role: "ndp-rust-dev",
  task: "implement EmbeddingCache trait for LRU cache",
  phase: "implementation", feature: "nxs-020")
  -> Returns: Rust conventions, the new thread-safety pattern from 3a,
     redb write pattern, recent corrections
```

**STORED** (new pattern discovered during implementation):
```
context_store(content: "LruCache from `lru` crate v0.12: use `lru::LruCache::new(NonZeroUsize::new(cap).unwrap())`.
  The `new` method changed signature in 0.12.",
  topic: "lru-crate", category: "pattern",
  tags: ["rust", "dependency", "cache", "nxs-020"], source: "ndp-rust-dev")
```

**Gate 3b outcome:**
```
context_store(content: "Gate 3b PASS. Code matches pseudocode. Architecture aligned.
  2 new patterns stored by coding agent.",
  topic: "nxs-020", category: "outcome",
  tags: ["outcome:completion", "gate:3b", "ndp-validator"])
```

#### Phase 3c: Testing & Risk Validation

**RETRIEVED:**
```
context_lookup(topic: "nxs-020", category: "decision", tags: ["risk-strategy"])
  -> Returns the approved risk strategy (R1, R2, R3)

context_briefing(role: "ndp-tester", task: "risk validation for embedding cache",
  phase: "testing", feature: "nxs-020")
```

**STORED:**
```
context_store(content: "Risk coverage: R1 (staleness) covered by test_cache_invalidation_on_correct,
  R2 (memory) covered by test_cache_eviction_at_capacity,
  R3 (cold start) covered by test_empty_cache_fallback. All pass.",
  topic: "nxs-020", category: "outcome",
  tags: ["outcome:quality", "gate:3c", "risk-coverage", "ndp-tester"])
```

#### Phase 4: Delivery + Retrospective

```
context_store(content: "nxs-020 complete. 3 phases, 6 agents, 1 rework cycle (gate 3a).
  Duration: 4 sessions. 8 entries retrieved, 7 helpful. 4 new patterns stored.",
  topic: "nxs-020", category: "outcome",
  tags: ["outcome:completion", "feature-summary"])
```

**Table state after nxs-020:**
- ENTRIES: ~15 new entries (research, decisions, patterns, outcomes, gate results)
- FEATURE_ENTRIES: "nxs-020" -> {15 entry IDs + all entries retrieved during the feature}
- USAGE_LOG: ~30 records (every retrieval by every agent, with helpful flags)
- OUTCOME_INDEX: `(hash("nxs-020"), outcome_entry_id)` for each outcome entry (~6)

---

## 3. Data Structure Gaps

### What Works As-Is

- **Entry storage and retrieval**: ENTRIES, TOPIC_INDEX, CATEGORY_INDEX, TAG_INDEX handle all storage and lookup patterns shown above.
- **Outcome accumulation**: category="outcome" with typed tags works for all outcome signals.
- **Usage tracking**: USAGE_LOG captures retrieval events per agent per feature. FEATURE_ENTRIES links features to entries used.
- **Process proposals**: category="process-proposal" with status=PendingReview, promoted to category="process" on approval. The approval/rejection flow through context_correct/context_deprecate works.
- **Confidence formula**: helpfulness_factor based on usage_count vs helpful_count handles entry quality decay.

### What Is Missing

**1. Gate result tracking has no dedicated structure.** Gate outcomes are stored as regular outcome entries with tags like `gate:3a`. This works for storage but makes aggregation awkward -- `context_retrospective` has to parse tags to reconstruct gate pass/fail history. A dedicated gate result schema would be cleaner:

```
Proposed: GATE_RESULTS table
  (feature_hash: u64, gate_id: &str) -> bincode<GateResult>

GateResult { passed: bool, rework_count: u32, escalated: bool,
             issues: Vec<String>, validator_entry_id: u64 }
```

Without this, the retrospective must scan all outcome entries for a feature, filter by `gate:*` tags, and reconstruct gate history from unstructured content. Feasible but brittle.

**2. Risk-to-test traceability has no index.** The workflow requires proving that every risk identified in Phase 2 has test coverage in Phase 3c. Currently this requires storing risks as entries, storing test-risk mappings as entries, and doing semantic search to correlate them. There is no structural guarantee that risk R1 maps to test T1. This is a content-level concern, not a schema-level one -- but the retrospective cannot automatically verify risk coverage without parsing entry content.

**3. Validation chain across gates is implicit.** Gate 3b validates against pseudocode (output of 3a). Gate 3c validates against risk strategy (Phase 2). The chain 3a->3b->3c is encoded in the protocol file but not in Unimatrix data. If a gate result references "validated against entry X", there is no foreign-key relationship in the schema. The OUTCOME_INDEX ties outcomes to features, not to specific validation inputs.

**4. The three source documents (Architecture, Spec, Risk Strategy) have no special status.** They are feature directory files, not Unimatrix entries. When the validator needs to "map back to the three source docs," it reads files from disk, not from Unimatrix. This is correct -- these are file-system artifacts -- but it means Unimatrix cannot track whether the source documents were actually validated against, only that the validator *reported* doing so.

### What Would Need to Change

- **Minimal change**: Add `gate_id` as an optional field to EntryMetadata. Gate outcomes stored with `gate_id: Some("3a")` become directly queryable without tag parsing.
- **Moderate change**: Add GATE_RESULTS table for structured gate tracking. Adds ~100 lines of code for table definition, insert, and query.
- **No change needed for risk traceability** if risk-test mappings are stored as structured content within entries. The retrospective parses content. This is fragile but avoids schema changes.

### Can `context_retrospective` Analyze This Workflow?

Mostly yes. It can aggregate outcomes per feature, compute retrieval efficiency from USAGE_LOG, compare gate pass rates across features, and detect patterns (e.g., "gate 3a fails 40% of the time for interface issues"). It cannot automatically verify risk coverage completeness or trace validation chains without parsing unstructured entry content. The retrospective is strong on process-level patterns (duration, rework rate, agent effectiveness) and weak on content-level patterns (was the right risk tested?).

---

## 4. Continuous Tweaking

### How a Human Modifies the Workflow

**Structural change** (add/remove phase, reorder stages): Edit `.claude/protocols/spec-driven-development.md`. Single file, single touchpoint. If the change also affects agent identity (new agent type needed), create a new thin-shell `.claude/agents/ndp/` file.

**Process tuning** (gate criteria, wave sizing, escalation thresholds): Store or correct entries in Unimatrix with `category: "process"`, `tags: ["workflow:spec-driven"]`. These are picked up by `context_briefing` on the next feature. Zero file changes.

**Agent behavior tuning** (new convention, new self-check): If it is expertise (what to do in code) -- store in Unimatrix. If it is identity (when to stop, who to ask) -- edit the `.claude/agents/ndp/` file.

### Gate Too Strict or Lenient

Scenario: Gate 3a rejects too many designs for trivial formatting issues.

**Retrospective-driven path** (system proposes):
1. After 5 features, `context_retrospective` detects: "Gate 3a rework rate: 80%. 60% of rework was formatting, not substance."
2. Generates process proposal: "PROPOSAL: Gate 3a should distinguish structural issues (interface contracts, component boundaries) from formatting issues (naming, comment style). Only structural issues block. Evidence: 3/5 features had unnecessary rework cycles."
3. Human reviews via `unimatrix proposals` or `context_lookup(category: "process-proposal")`.
4. Human approves with modification: "Gate 3a blocks on structural issues only. Formatting issues are advisory."
5. Entry stored: `category: "process"`, `tags: ["gate:3a", "workflow:spec-driven"]`, `status: Active`.
6. Next feature: validator calls `context_briefing(role: "ndp-validator", task: "gate-3a")`, gets the refined criteria.

**Human-initiated path**: Human directly stores `context_store(content: "Gate 3a: structural issues block, formatting issues are advisory.", category: "process", tags: ["gate:3a"])`. Immediate, no retrospective needed.

Both paths result in the same data. The protocol file does not change -- it still says "Gate 3a: ndp-validator maps components to source docs." The specifics of what the validator checks are now richer.

### Adding a New Phase

Scenario: Add "Phase 2.5: Security Review" between Design and Delivery.

**Touchpoints:**
1. Edit `.claude/protocols/spec-driven-development.md` -- add Phase 2.5 section, update phase sequence (~10 lines)
2. Create `.claude/agents/ndp/ndp-security-reviewer.md` -- thin shell (~35 lines)
3. Store seed expertise: `context_store(topic: "ndp-security-reviewer", category: "convention", content: "OWASP Top 10 checklist...", tags: ["security"])` (1-3 entries)
4. Optionally store process entry: `context_store(category: "process", tags: ["phase:security-review", "workflow:spec-driven"], content: "Security review runs after design approval, before component design. Blocks gate 3a.")` (1 entry)

**Total touchpoints**: 2 files created/edited + 2-4 Unimatrix entries stored. The rest of the system adapts -- existing agents continue to pull briefings as before, the new agent gets briefings from its seeded data, and process knowledge accumulates as the security reviewer is used across features.

### The Full Improvement Loop

```
Feature nxs-020 runs workflow
  |
  v
Agents store outcomes at each phase/gate:
  - "gate 3a rework: missing error types" (outcome, gate:3a)
  - "7/8 retrieved entries helpful" (outcome, efficiency)
  - "feature complete, 4 sessions, 1 rework" (outcome, completion)
  |
  v
USAGE_LOG accumulates: 30 retrieval records with helpful flags
FEATURE_ENTRIES maps nxs-020 to all entries touched
OUTCOME_INDEX maps nxs-020 to all outcome entries
  |
  v
Human calls: context_retrospective(feature: "nxs-020",
  compare_with: ["nxs-018", "nxs-019"], generate_proposals: true)
  |
  v
System aggregates:
  - nxs-018: 2 rework cycles (gate 3a), interface issues both times
  - nxs-019: 1 rework cycle (gate 3a), interface issue
  - nxs-020: 1 rework cycle (gate 3a), interface issue
  Pattern: gate 3a fails on interface errors in 3/3 recent features
  |
  v
System generates process-proposal entry:
  "PROPOSAL: Require pseudocode agents to define explicit error types
   for all component interfaces before gate 3a.
   EVIDENCE: 3/3 recent features had gate 3a rework due to missing error types.
   SUGGESTED ACTION: Add to ndp-pseudocode self-check OR store as process knowledge."
  status: PendingReview
  |
  v
Human reviews: unimatrix proposals
  Approves: "Add as process knowledge, not agent self-check."
  |
  v
New entry: category="process", tags=["gate:3a", "pseudocode", "interface-errors"]
  content: "Pseudocode must define explicit error types for component interfaces.
   Gate 3a checks for this."
  status: Active, supersedes: proposal_entry_id
  |
  v
Next feature (nxs-021):
  ndp-pseudocode calls context_briefing -> gets "define explicit error types" in process knowledge
  ndp-validator calls context_briefing for gate 3a -> gets "check interface error types" in process knowledge
  Gate 3a passes first time. No rework.
```

### What the System Learns vs. What Requires Human Initiative

**System can learn and propose:**
- Gate pass/fail rates and correlated causes (from outcome data)
- Wave sizing effectiveness (from duration + agent count data)
- Entry quality trends (from helpfulness ratios in USAGE_LOG)
- Missing knowledge areas (from searches that return no results)
- Agent effectiveness patterns (from per-agent outcome data)

**Requires human initiative:**
- Adding new phases or agents (structural workflow changes)
- Deciding whether a process proposal changes a `.claude/` file or a Unimatrix entry
- Resolving ambiguous identity/expertise boundary cases
- Setting initial risk strategies for new feature types
- Overriding the system's assessment (e.g., "the rework was actually valuable, not waste")

### Friction Points in the Split

1. **Protocol file drift**: If process knowledge in Unimatrix contradicts the protocol file, agents get conflicting signals. Example: protocol says "Gate 3a checks all three source docs" but a process entry says "Gate 3a only checks architecture alignment." The agent must reconcile. Mitigation: process entries should refine, not contradict, protocol files.

2. **Human must bridge the gap**: When a process proposal says "SUGGESTED ACTION: update protocols/planning.md", the human must actually do the file edit. Unimatrix surfaces the suggestion but cannot execute it. If the human approves the proposal but forgets to edit the file, the process knowledge and the protocol file diverge.

3. **20% boundary ambiguity**: For ~20% of process improvements, it is unclear whether the change belongs in a `.claude/` file or in Unimatrix. The rule of thumb (code knowledge = Unimatrix, coordination rules = file, process effectiveness = Unimatrix) covers 80% of cases. The remainder requires human judgment.

---

## 5. The Retrospective Pipeline After 5 Features

### Accumulated Outcome Data

After features nxs-016 through nxs-020, each using this workflow:

| Feature | Duration | Agents | Rework Cycles | Gate 3a | Gate 3b | Gate 3c | Entries Retrieved | Helpful Rate |
|---------|----------|--------|---------------|---------|---------|---------|-------------------|--------------|
| nxs-016 | 6 sessions | 5 | 2 (3a, 3b) | FAIL->PASS | FAIL->PASS | PASS | 22 | 68% |
| nxs-017 | 4 sessions | 4 | 1 (3a) | FAIL->PASS | PASS | PASS | 18 | 72% |
| nxs-018 | 5 sessions | 5 | 1 (3a) | FAIL->PASS | PASS | PASS | 25 | 76% |
| nxs-019 | 3 sessions | 4 | 0 | PASS | PASS | PASS | 20 | 85% |
| nxs-020 | 4 sessions | 6 | 1 (3a) | FAIL->PASS | PASS | PASS | 30 | 77% |

USAGE_LOG: ~600 records across 5 features. OUTCOME_INDEX: ~30 outcome entries. FEATURE_ENTRIES: ~120 entry links.

### Retrospective Call

```
context_retrospective(feature: "nxs-020",
  compare_with: ["nxs-016", "nxs-017", "nxs-018", "nxs-019"],
  generate_proposals: true)
```

### Cross-Feature Pattern Detection

**Pattern 1: Gate 3a is the bottleneck.** 4/5 features had gate 3a failures. Common cause in outcome entries: "missing interface error types" (3 features), "component boundary mismatch with architecture" (1 feature). Gate 3b and 3c are stable.

**Pattern 2: Helpful rate is improving.** 68% -> 72% -> 76% -> 85% -> 77%. The dip in nxs-020 correlates with nxs-020 having 6 agents (highest count) -- more agents means more diverse queries, some hitting knowledge gaps.

**Pattern 3: Features with fewer agents finish faster.** nxs-019 (4 agents, 0 rework, 3 sessions) vs nxs-020 (6 agents, 1 rework, 4 sessions). Consistent with existing process knowledge about wave sizing.

**Pattern 4: Embedding-related searches have low hit rate.** Across 5 features, 8 searches containing "embedding" returned useful results only 50% of the time. Other topic areas average 78%.

### Generated Process Proposals

**PP-007**: "Require ndp-pseudocode to define explicit error types for all component interfaces. Gate 3a should verify error type presence before checking architectural alignment. Evidence: 3/5 features had gate 3a failures traced to missing error types. Estimated savings: 1 rework cycle per feature."

**PP-008**: "Cap agent count at 5 per feature for this workflow. Features with 6+ agents showed lower helpful rates and more coordination overhead. Evidence: nxs-020 (6 agents) had 77% helpful rate vs nxs-019 (4 agents) at 85%."

**PP-009**: "Expand embedding pipeline knowledge base. 8 embedding-related searches across 5 features had only 50% hit rate. Agents are rebuilding embedding knowledge from scratch each time. Suggested: store canonical embedding patterns from nxs-020 as convention entries."

### Data Flow Detail

```
context_retrospective called
  |
  v
OUTCOME_INDEX scan: hash("nxs-020") -> 6 outcome entry IDs
  Read each: parse completion, quality, gate, efficiency data
  |
  v
FEATURE_ENTRIES scan: "nxs-020" -> 30 entry IDs (all entries touched)
  For each: query USAGE_LOG -> get helpful/unhelpful flags
  Compute: 30 retrieved, 23 helpful = 77%
  |
  v
Repeat for compare_with features (nxs-016 through nxs-019)
  Build comparison table
  |
  v
Gap detection:
  - TAG_INDEX scan for "gate:3a" + "rework" -> 4 entries across 5 features
  - Parse content -> "interface error types" appears in 3/4
  - TAG_INDEX scan for searches with 0 results -> 4 embedding-related
  - Agent count vs duration correlation -> 6 agents = slower
  |
  v
For each gap, create entry:
  category: "process-proposal", status: PendingReview
  content: structured proposal with evidence
  tags: ["process", "evidence:5-features", gap-type]
  Insert into ENTRIES + STATUS_INDEX(PendingReview)
  |
  v
Return retrospective report with proposal IDs
  Human reviews via: unimatrix proposals
```

After human approves PP-007 and PP-009, rejects PP-008 ("agent count depends on feature complexity, not a fixed cap"):

- PP-007 becomes active process knowledge. Next feature's pseudocode agent and validator both get it in briefings.
- PP-008 is deprecated with reason "feature complexity varies." Future retrospectives check for prior rejections on the same topic before re-proposing wave/agent caps.
- PP-009 triggers human to ask an agent to store canonical embedding patterns. Knowledge base gap closes. Future embedding searches hit rate improves.

The rejection of PP-008 is itself learning. The system records that humans rejected a fixed agent cap, with reason. If 5 more features show the same pattern, the retrospective can propose a softer version ("consider reducing agent count when helpful rate drops below 75%") -- different framing, same data, informed by the prior rejection.
