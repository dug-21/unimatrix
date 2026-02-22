# Proposal A: Knowledge Oracle -- Workflow Mapping

Mapping the Spec-Driven Development with Risk-Based Testing workflow onto Proposal A's architecture: static `.claude/` control plane + Unimatrix as pure knowledge store.

---

## 1. .claude/ File Layout

### Protocol Files

The workflow's four phases and three gates require three protocol files (two existing, one new):

```
.claude/protocols/
  planning-protocol.md          # Covers Phase 1 + Phase 2 + Stage 3a
  implementation-protocol.md    # Covers Stage 3b + Stage 3c + Phase 4
  risk-testing-protocol.md      # NEW -- risk-based test strategy procedures
```

**planning-protocol.md** absorbs the entire "three source documents" lifecycle. Key sections that must exist or be added:

```markdown
## Phase 1: Research & Scope
- Human initiates feature with high-level intent
- Spawn ndp-researcher (or human + primary agent iteration)
- Output: product/features/{id}/SCOPE.md (human-approved)

## Phase 2: Design (Three Source Documents)
- Spawn ndp-architect -> ARCHITECTURE.md
- Spawn ndp-specification -> SPECIFICATION.md
- Spawn ndp-risk-strategist -> RISK-STRATEGY.md    # NEW agent
- Gate: Human must approve all three before proceeding
- Approval recorded: comment on GH Issue or marker in SCOPE.md

## Stage 3a: Component Design & Pseudocode
- Wave 1: ndp-specification (component-level), ndp-architect (component interfaces)
- Wave 2: ndp-pseudocode (per-component), ndp-tester (component test plans)
- Gate 3a: ndp-validator maps every component back to three source docs
  - Checks: component->architecture alignment, pseudocode->spec coverage, test plan->risk strategy coverage
  - Minor fail: rework loop (max 2 iterations)
  - Scope/feasibility fail: escalate to human via primary agent
```

**implementation-protocol.md** covers code delivery with backward validation:

```markdown
## Stage 3b: Code Implementation
- Agents implement from validated pseudocode
- Build test cases per component test plans
- Gate 3b: ndp-validator checks code->pseudocode, code->architecture, tests->test plans
  - Code mismatch: rework
  - Architectural deviation: escalate

## Stage 3c: Testing & Risk Validation
- Execute component, integration, and feature-level tests
- Risk coverage report: every risk from RISK-STRATEGY.md has test coverage
- Gate 3c: ndp-validator final check against all three source docs
  - Missing coverage: fill gaps
  - Unresolvable: escalate

## Phase 4: Delivery
- All three gates passed
- Code ships with full traceability
```

**risk-testing-protocol.md** (new) defines how risk strategy drives testing:

```markdown
## Risk Identification
- Risk categories: data integrity, performance, security, integration, failure modes
- Each risk: severity (critical/high/medium/low), likelihood, test scenarios

## Risk-to-Test Mapping
- Every risk in RISK-STRATEGY.md must have >= 1 test scenario
- Component test plans reference risk IDs
- Gate 3c verifies coverage completeness

## Risk Coverage Report Format
| Risk-ID | Description | Severity | Test Coverage | Status |
```

### Agent Definitions

The workflow requires 10 agents. 8 exist; 2 are new:

```
.claude/agents/ndp/
  ndp-scrum-master.md       # EXISTS -- coordinator, no changes needed
  ndp-architect.md          # EXISTS -- produces Architecture doc (source doc 1)
  ndp-specification.md      # EXISTS -- produces Specification doc (source doc 2)
  ndp-risk-strategist.md    # NEW -- produces Risk-Based Test Strategy (source doc 3)
  ndp-pseudocode.md         # EXISTS -- component-level pseudocode
  ndp-tester.md             # EXISTS -- component test plans, test execution
  ndp-rust-dev.md           # EXISTS -- code implementation
  ndp-validator.md          # EXISTS -- needs expanded gate definitions
  ndp-synthesizer.md        # EXISTS -- brief compilation
  ndp-vision-guardian.md    # EXISTS -- alignment checks
```

**ndp-risk-strategist.md** (new agent):

```markdown
---
name: ndp-risk-strategist
type: specialist
scope: planning
description: Identifies feature-level risks and maps them to test scenarios.
---

## What You Produce
- RISK-STRATEGY.md at product/features/{id}/risk-strategy/RISK-STRATEGY.md
  - Risk inventory with severity/likelihood matrix
  - Test scenario mapping per risk
  - Coverage requirements (which risks need integration tests vs unit tests)
  - Priority ordering by severity * likelihood

## What You Receive
- SCOPE.md, ARCHITECTURE.md, SPECIFICATION.md

## Self-Check
- [ ] Every acceptance criterion from SCOPE.md has at least one associated risk
- [ ] Every risk has at least one test scenario
- [ ] Severity and likelihood assigned to all risks
- [ ] No risk left without a coverage recommendation
```

**ndp-validator.md** needs expanded gate logic. Add three gate sections:

```markdown
## Gate 3a: Component Design Validation
- Component architectures align with approved ARCHITECTURE.md
- Pseudocode implements SPECIFICATION.md requirements
- Component test plans address RISK-STRATEGY.md risks
- Component interfaces match architecture contracts

## Gate 3b: Code Validation
- Code matches validated pseudocode
- Implementation aligns with approved Architecture
- Test cases match component test plans

## Gate 3c: Risk Coverage Validation
- Every risk in RISK-STRATEGY.md has passing test coverage
- Integration tests cover cross-component risks
- Feature-level tests cover feature-level risks
- Risk coverage report generated
```

### Rules

No new rule files needed. Existing `rules/rust-workspace.md` continues to trigger on `*.rs` edits. The workflow is protocol-driven, not file-pattern-triggered.

### Skills

Two existing skills suffice. No new skills needed:

```
.claude/skills/
  get-pattern/SKILL.md     # Retrieves knowledge before work
  save-pattern/SKILL.md    # Stores discoveries after work
```

Gate validation is handled by the `ndp-validator` agent definition, not a skill, because gates require contextual judgment (comparing documents), not stateless procedures.

---

## 2. Unimatrix Data Structures

### End-to-End Trace: Feature `nxs-010` (Query Caching Layer)

#### Phase 1: Research & Scope

**STORED:**
```
context_store(
  content: "Query caching evaluated: LRU with TTL preferred over write-through.
    Reason: read-heavy workload (95% reads), cache invalidation complexity
    unacceptable for v1. Benchmarks: LRU handles 10K queries/sec on Pi 5.",
  topic: "nxs-010",
  category: "research",
  tags: ["caching", "performance", "spike"],
  source: "agent:ndp-researcher"
)
```

**RETRIEVED:** Nothing -- first phase, exploring fresh ground. Unless prior features touched caching:
```
context_search(query: "caching strategies for embedded databases", topic: "performance")
--> Returns: entries from prior features about redb read patterns, memory constraints
```

#### Phase 2: Design (Three Source Documents)

**STORED (by ndp-architect):**
```
context_store(
  content: "ADR-015: LRU cache sits between MCP handler and redb. Cache key =
    hash(topic + category + tags). TTL = 300s configurable. Max entries = 1000.
    Eviction: LRU by last_accessed_at. No cache for context_store (write-through
    would add complexity without benefit for write-rare workload).",
  topic: "nxs-010",
  category: "decision",
  tags: ["architecture", "caching", "adr"],
  source: "agent:ndp-architect"
)
```

**STORED (by ndp-risk-strategist):**
```
context_store(
  content: "Risk R-003: Cache serving stale data after context_correct. Severity: HIGH.
    Mitigation: invalidate cache entries whose entry_id matches corrected original_id.
    Test scenario: store entry, cache it, correct it, verify next read returns corrected version.",
  topic: "nxs-010",
  category: "risk",
  tags: ["caching", "correctness", "testing"],
  source: "agent:ndp-risk-strategist"
)
```

**RETRIEVED (by ndp-architect before designing):**
```
context_search(query: "redb read performance patterns", k: 5)
--> Returns: redb access patterns from nxs-001, spawn_blocking convention from nxs-003
context_lookup(topic: "nxs-010", category: "research")
--> Returns: Phase 1 research findings
```

**RETRIEVED (by ndp-risk-strategist):**
```
context_search(query: "cache invalidation risks in embedded systems")
--> Returns: any prior lessons about stale data, correction chains
context_lookup(category: "lesson-learned", tags: ["testing"])
--> Returns: lessons from prior features about testing gaps
```

#### Stage 3a: Component Design & Pseudocode

**STORED (by ndp-pseudocode):**
```
context_store(
  content: "Cache key computation pattern: FxHashMap with (topic, category,
    sorted_tags) as composite key. Sorting tags ensures deterministic hashing
    regardless of caller-provided order. Use rustc-hash crate (already in workspace).",
  topic: "nxs-010",
  category: "pattern",
  tags: ["caching", "rust", "hashing"],
  source: "agent:ndp-pseudocode"
)
```

**RETRIEVED (by ndp-pseudocode):**
```
context_briefing(role: "pseudocode", task: "design query cache for MCP handler", feature: "nxs-010")
--> Returns: ADR-015, risk entries, redb patterns, Rust conventions
```

**CORRECTED (when Gate 3a fails -- validator finds component test plan missing R-003 coverage):**
No Unimatrix correction. The validator sends the tester back for rework. The test plan file is edited. Unimatrix entries remain unchanged because the *knowledge* was correct -- the *artifact* was incomplete. This is a file-level fix, not a knowledge-level fix.

However, the scrum-master may store a lesson:
```
context_store(
  content: "nxs-010 Gate 3a: Component test plan for cache module missed risk R-003
    (stale data after correction). Root cause: tester did not query risk entries before
    writing test plan. Fix: tester spawn prompt should include risk strategy path.",
  topic: "nxs-010",
  category: "lesson-learned",
  tags: ["gate-failure", "testing", "risk-strategy"],
  source: "agent:ndp-scrum-master"
)
```

#### Stage 3b: Code Implementation

**RETRIEVED (by ndp-rust-dev):**
```
context_briefing(role: "rust-dev", task: "implement LRU query cache with TTL", feature: "nxs-010")
--> Returns: ADR-015, cache key pattern, spawn_blocking convention, error handling convention
```

**STORED (by scrum-master after agent returns):**
```
context_store(
  content: "LRU cache with TTL pattern for redb: use mini-moka crate (async-compatible,
    low memory overhead). Configure: max_capacity=1000, time_to_live=Duration::from_secs(ttl).
    Invalidation: cache.invalidate(&key) on context_correct and context_deprecate calls.",
  topic: "caching",
  category: "pattern",
  tags: ["rust", "implementation", "moka"],
  source: "agent:ndp-rust-dev"
)
```

#### Stage 3c: Testing & Risk Validation

**RETRIEVED (by ndp-tester):**
```
context_lookup(topic: "nxs-010", category: "risk")
--> Returns: all risk entries including R-003 (stale data)
context_search(query: "cache invalidation test patterns")
--> Returns: any prior test patterns for similar scenarios
```

**Gate 3c pass:** No corrections needed. All risks covered.

#### Phase 4: Delivery

**STORED (by primary agent at reflexion):**
```
context_store(
  content: "nxs-010 retrospective: Risk-based test strategy caught stale-data bug
    at Gate 3a before any code was written. Time saved estimate: 1-2 implementation
    iterations. The risk strategist agent justified its existence on this feature.",
  topic: "nxs-010",
  category: "lesson-learned",
  tags: ["retrospective", "risk-strategy", "process"],
  source: "agent:ndp-scrum-master"
)
```

---

## 3. Data Structure Gaps

### What Works As-Is

- **Topic/category/tag indexing** handles the `context_lookup(topic: "nxs-010", category: "decision")` pattern well. Feature-scoped retrieval works by convention (topic = feature ID).
- **Semantic search** via hnsw_rs handles cross-feature discovery ("caching strategies for embedded databases" finds entries from unrelated features).
- **Correction chains** via `supersedes`/`superseded_by` handle knowledge evolution cleanly.
- **Confidence scoring** naturally deprioritizes stale knowledge and surfaces well-used patterns.
- **Source field** tracks provenance ("agent:ndp-architect", "agent:ndp-risk-strategist"), enabling queries like "show me all architect decisions for this feature."

### What's Missing or Awkward

**1. No feature-level grouping beyond topic convention.**
Feature scoping relies on `topic: "nxs-010"` by convention. There is no first-class `feature_id` field on `EntryRecord`. This means:
- An entry about "caching" with `topic: "caching"` rather than `topic: "nxs-010"` is invisible to `context_lookup(topic: "nxs-010")`.
- Cross-cutting entries (patterns useful to many features) cannot simultaneously belong to a feature and a general topic.
- Workaround: use tags `["nxs-010"]` for feature association while keeping topic semantic. But this splits the query model -- sometimes feature is in `topic`, sometimes in `tags`.

**2. No gate outcome tracking.**
When Gate 3a fails, the validator identifies the failure and the scrum-master stores a lesson-learned entry. But there is no structured record of "Gate 3a: FAIL, reason: missing R-003 coverage, rework iteration: 1, result: PASS." The EntryRecord schema has no fields for structured gate data. You can store it as markdown content in a lesson-learned entry, but you cannot query "show me all gate failures across all features" without semantic search against unstructured text.

**3. No relationship between entries beyond correction chains.**
The `supersedes`/`superseded_by` fields model correction chains. But there is no "related_to" or "derived_from" linkage. ADR-015 (the caching decision) and R-003 (the stale data risk) are conceptually linked, but the database has no way to express this. Finding related entries requires semantic search, which is probabilistic, not deterministic.

**4. No structured risk or acceptance criteria data.**
Risk entries are stored as markdown blobs. You cannot query "all HIGH severity risks for nxs-010" without parsing the content field. Similarly, acceptance criteria status (PENDING/PASS/FAIL) lives in ACCEPTANCE-MAP.md files, not in Unimatrix. The database cannot answer "what percentage of ACs passed across all features?"

### What Would Need to Change

For full workflow support without awkwardness, Proposal A would benefit from:

1. **A `feature` field on EntryRecord** (or a dedicated FEATURE_INDEX table). This would allow entries to belong to both a semantic topic and a feature without overloading either. Cost: one new index, one new field, minor schema change.

2. **Structured metadata via a `metadata: Option<HashMap<String, String>>` field.** This would allow gate outcomes, risk severities, AC statuses to be queryable without parsing markdown. Cost: moderate -- changes query model, adds filtering complexity.

Neither of these is fatal. The workflow *functions* without them -- it just relies more heavily on conventions and semantic search for things that would be better served by deterministic queries.

---

## 4. Continuous Tweaking

### How a Human Modifies the Workflow

Every workflow change is a file edit in `.claude/`. The touchpoints depend on what changes:

| Change Type | Files to Edit | Touchpoints | Effort |
|-------------|---------------|-------------|--------|
| Adjust a gate's strictness | `ndp-validator.md` (gate section) | 1 file, 1 section | Low (10 min) |
| Add a new phase | Protocol file + possibly new agent definition | 2-3 files | Medium (30-60 min) |
| Change agent composition for a phase | `agent-routing.md` + protocol file | 2 files | Low (15 min) |
| Evolve risk strategy approach | `risk-testing-protocol.md` + `ndp-risk-strategist.md` + `ndp-validator.md` (Gate 3c) | 3 files | Medium (30 min) |
| Add a new agent role | New agent `.md` + `agent-routing.md` + protocol file mentioning the agent | 3 files | Medium (45 min) |
| Change document structure (e.g., add a fourth source doc) | Protocol file + validator gate definitions + synthesizer deliverables | 3-4 files | High (60-90 min) |

### Specific Tweak Scenarios

**Gate too strict (Gate 3a rejects too often):**
Edit `.claude/agents/ndp/ndp-validator.md`, section `## Gate 3a`. Relax the check -- for example, change "every component test plan must address every risk" to "every HIGH/CRITICAL risk must have coverage; MEDIUM risks are WARN, not FAIL." One file, one section, 5-10 lines changed.

**Gate too lenient (bugs shipping past Gate 3c):**
Same file, section `## Gate 3c`. Add stricter checks -- for example, require integration test pass rate > 95%, or require every risk to have 2+ test scenarios. The validator reads this on every spawn; the change takes effect immediately on next feature.

**New phase added (e.g., "Phase 2.5: Threat Model"):**
1. Edit `planning-protocol.md`: insert Phase 2.5 between Phase 2 and Stage 3a, with wave ordering and spawn instructions.
2. Create `ndp-threat-modeler.md` agent definition with scope, inputs, outputs, self-check.
3. Edit `agent-routing.md`: add threat-modeler to planning swarm composition.
4. Edit `ndp-validator.md`: add Gate 2.5 checking threat model against architecture.
Total: 4 files, 60-90 minutes for a careful edit.

**Risk strategy approach evolves (e.g., adding FMEA methodology):**
1. Edit `risk-testing-protocol.md`: update risk identification methodology section.
2. Edit `ndp-risk-strategist.md`: update output format to include FMEA fields (failure mode, effect, detection method, RPN score).
3. Edit `ndp-validator.md`: update Gate 3c to check for RPN scores, validate coverage thresholds against RPN.
Total: 3 files, 30 minutes.

### The Feedback Loop

Under Proposal A, the system learns that a tweak is needed through this chain:

```
1. Feature executes -> gate fails or succeeds with issues
2. Scrum-master stores lesson-learned in Unimatrix
   context_store(category: "lesson-learned", tags: ["gate-failure", "process"])
3. Lessons accumulate across features
4. Human queries: context_lookup(category: "lesson-learned", topic: "planning")
5. Human reads lessons, identifies pattern: "Gate 3a failed 4 times in 6 features
   because tester doesn't read risk strategy"
6. Human edits protocol: add risk strategy path to tester's spawn prompt
7. Next feature benefits
```

The gap between steps 3 and 4 is entirely human-driven. Unimatrix does not alert the human that lessons have accumulated. It does not surface "you have 4 gate failures with similar root causes." The human must proactively query.

### Honest Friction Assessment

**Low friction:**
- Adjusting gate strictness: one file, one section, immediate effect.
- Adding/removing agents from a wave: protocol file edit, immediate effect.
- Storing and retrieving lessons: Unimatrix handles this naturally.

**Medium friction:**
- Adding a new phase: 3-4 file edits, requires understanding how protocols, agents, and routing interact. No single "workflow definition" file -- the workflow is *distributed* across protocol files, agent definitions, and routing tables.
- Evolving risk methodology: 3 files, requires keeping validator gate logic consistent with risk strategist output format.

**High friction:**
- Fundamental workflow restructuring (e.g., moving from 3 source docs to 4): touches protocols, agent definitions, validator gates, synthesizer output format. The workflow is encoded in prose across 5+ files. There is no schema or DSL enforcing consistency -- a human must mentally track which files reference "three source documents" and update each one.
- Detecting that a tweak is needed: the system accumulates knowledge about process problems but cannot act on it. A lesson-learned entry saying "Gate 3a fails because testers miss risk entries" will sit in Unimatrix indefinitely unless a human queries for it and decides to act. There is no notification, no dashboard, no automated retrospective trigger.

**The core tradeoff:** Proposal A makes individual tweaks cheap (edit a markdown file) but makes *discovering what to tweak* expensive (human must proactively query lessons, recognize patterns, and decide to act). The knowledge that a process is broken accumulates silently. The process stays broken until a human intervenes.
