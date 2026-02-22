# ASS-007: Design Discussion Log

**Date**: 2026-02-21 — 2026-02-22
**Purpose**: Capture the full design discussion that led to strategic decisions for D7

---

## Part 1: Concept Mining from ruvector/claude-flow

### From ruvector/claude-flow — Genuinely Useful Patterns

**1. Memory Type Classification**
claude-flow classifies entries as `episodic | semantic | procedural | working | cache`. This maps to cognitive architecture — episodic memories are events ("we tried X, it failed"), semantic memories are facts ("the convention is Y"), procedural memories are how-to ("to deploy, do Z"). Different types have different retrieval patterns.

Our current model doesn't distinguish. `{ topic, category, tags }` treats everything as flat entries. A `type` field on entries would let us optimize: procedural knowledge retrieval is by task-match, semantic is by topic-match, episodic is by temporal/situational similarity.

**2. Access Level Model**
`private | team | swarm | public | system` — scoping who can see what. In the control plane, the scrum-master curates what each agent sees. An access level on stored entries would let the system enforce this rather than relying on orchestrator discipline.

**3. Co-Access Edge Tracking**
When entries are frequently retrieved together, they should boost each other's ranking. claude-flow's MemoryGraph tracks `co-accessed` edges and blends PageRank with vector similarity (70/30 ratio). This is cheap to implement and directly useful — if "error handling convention" and "CoreError enum pattern" are always retrieved together, surfacing one should boost the other.

**4. The ReasoningBank Pipeline (Retrieve → Judge → Distill → Consolidate)**
The consolidation step is the real value: dedup at 0.95 similarity, contradiction detection at 0.85, pruning by age/confidence floor. We have dedup planned — but contradiction detection (similar embeddings, different outcomes) is a concept we hadn't considered.

**5. EWC Importance (Elastic Weight Consolidation)**
Patterns have an `ewc_importance` float — higher values resist being overwritten by new learning. Constitutional knowledge (non-negotiable rules) should have high importance. Freshly-observed patterns should have low importance until validated. This maps directly to our lifecycle state machine but adds a continuous dimension.

**6. Confidence Boost + Time Decay**
+0.03 per access, -0.005/hour decay, floor at 0.1. Simple, effective. Entries that keep getting retrieved stay alive; forgotten entries fade. We have this concept in D3's confidence formula — but the specific mechanism of tracking `access_count` and `last_accessed_at` as first-class fields is worth adopting.

### From ruvector — What's Fluff

- **Hyperbolic embeddings**: Interesting math, no clear benefit for flat code knowledge. Skip.
- **GNN message passing**: The simpler PageRank approach gives 80% of the value at 20% complexity.
- **LoRA/SONA RL**: No LLM to adapt. The "trajectory" concept is heavyweight for a context engine. Skip the RL, keep the access tracking.
- **"150x-12,500x faster"**: Marketing. That's just HNSW vs brute force. Every HNSW implementation gets this.
- **42+ capabilities**: Most are crate stubs. The real core is HNSW + redb + embeddings — same stack we chose independently.

---

## Part 2: Control Plane Taxonomy

### What the .claude/ Directory Actually Encodes

Seven categories of control:

| Category | Where | What It Encodes |
|----------|-------|-----------------|
| **Constitutional Rules** | CLAUDE.md | Absolute constraints (3-5 rules, never overridden) |
| **Routing Rules** | agent-routing.md | Task-type → team-shape dispatch table (8 templates) |
| **Orchestration Protocols** | protocols/*.md | Wave sequences, gate conditions, iteration caps |
| **Role Definitions** | agents/ndp/*.md | Scope boundaries, cognitive priming, domain expertise, self-checks, collaboration |
| **Contextual Rules** | rules/*.md | File-pattern-triggered constraints |
| **Skills** | skills/*/SKILL.md | Stateless procedures (agent knows WHEN, skill knows HOW) |
| **Meta-Rules** | AGENT-CREATION-GUIDE.md | Rules about how to create rules |

Six types of knowledge encoded across these:

1. **Process knowledge** — how work flows (wave ordering, gate conditions)
2. **Domain expertise** — subject-matter facts (EPA breakpoints, NWS API quirks)
3. **Architectural conventions** — how to build (Domain Adapter Pattern, channel-based flow)
4. **Quality criteria** — what "good" looks like (self-check gates, validation tiers)
5. **Organizational knowledge** — who does what (routing tables, collaboration graphs)
6. **Learning/meta knowledge** — knowledge about knowledge (pattern categories, reflexion scoring)

**The fundamental insight**: the control plane is a workflow engine implemented as static documents. Every construct — routing, sequencing, gating, feedback — has an analog in traditional orchestration engines, but expressed as prose consumed by LLM agents instead of code consumed by a runtime.

**What a context engine would need to make this dynamic**: role resolution, contextual knowledge assembly, protocol parameterization, drift-aware constraint injection, pattern lifecycle management, and collaboration graph queries.

---

## Part 3: Control Plane as Data — The Reframe

The reframe: **the control plane is data, not infrastructure**. Protocols, routing rules, self-checks — they're entries in the same knowledge store as conventions and patterns. They just have a different `category` and a different consumption pattern (agents follow them as instructions, not just reference them as facts).

That means the correction chain applies to PROCESS, not just KNOWLEDGE:

```
Protocol v1: "Run unit tests, then integration tests, then deploy"
  → Used across 5 features
  → Feature X: 12 releases to get right
  → Retrospective: "Gap identified — no smoke test between staging and prod"
  → context_correct(protocol_v1_id, protocol_v2, reason: "Retro from Feature X")

Protocol v2: "Run unit tests, integration tests, smoke test staging, THEN deploy"
  → supersedes v1
  → v1 preserved (audit trail)
  → Next feature automatically gets v2
```

The "self-learning" is a **structured feedback loop on the control plane itself**:

1. **Execute** — agent retrieves protocol, follows it
2. **Observe** — outcome signal (releases-to-stable, drift corrections, validation failures)
3. **Attribute** — which process step failed? which convention was missing?
4. **Correct** — retrospective produces improved process
5. **Propagate** — next retrieval serves the corrected version

Three consumption patterns for entries:

| Pattern | Category examples | How agent consumes it |
|---------|------------------|-----------------------|
| **Reference** | convention, decision, pattern | Reads it, applies judgment |
| **Instruction** | protocol, self-check, routing-rule | Reads it, follows it as steps |
| **Signal** | lesson-learned, retrospective, outcome | Reads it, adjusts behavior/confidence |

The layered model for balancing file complexity vs database opacity:

```
Layer 1: Starter Kit (repo template, generic agents, standard protocols)
Layer 2: Unimatrix as knowledge backend (evolving, learning)
Layer 3: .claude/ files as rendering layer (thin shells that pull from Unimatrix)
Layer 4: UI for visualization (Matrix phase)
```

What goes WHERE:

| Stays in `.claude/` files | Goes in Unimatrix |
|--------------------------|-------------------|
| Agent identity (who am I, core principles) | Agent expertise (what do I know) |
| Protocol skeleton (phases, waves, gates) | Protocol details (specific steps, evolved versions) |
| Routing structure (agent types exist) | Routing rules (which agent for which task) |
| Constitutional rules (never change) | Conventions (change frequently) |
| Skill definitions (tool integration) | Patterns (learned from experience) |

---

## Part 4: Three Competing Proposals

### Proposal A: Knowledge Oracle (Conservative)

**Core assumption**: Unimatrix is purely a knowledge store. `.claude/` files are human-maintained source of truth for all process/structure. Unimatrix enhances what agents KNOW, not how they WORK.

- Process improvement happens through manual file edits informed by retrospectives
- .claude/ files are never generated by Unimatrix
- Learning loop operates ONLY on knowledge (conventions, patterns, decisions, lessons-learned)
- 8 redb tables (baseline)
- v0.1: context_search, context_lookup, context_store, context_get
- v0.2: context_correct, context_deprecate, context_status, context_briefing

**Strengths**: Minimal blast radius. Clear debugging. Human retains full control.
**Weaknesses**: Process improvement is slow — if human doesn't act, nothing changes. Knowledge about process sits in database but cannot self-actuate.

### Proposal B: Dynamic Control Plane (Ambitious) — REJECTED

**Core assumption**: Unimatrix owns the full control plane. `.claude/` files are generated artifacts. Everything lives as Unimatrix entries. Learning loop operates on both knowledge AND process.

**Rejected because**: Making `.claude/` files generated output is too aggressive. CLI becomes critical-path dependency. Blast radius of bad export corrupting protocols is too high. Too similar to claude-flow/ruvector complexity trap.

### Proposal C: Workflow-Aware Hybrid (Balanced)

**Core assumption**: Split source of truth. Agent IDENTITY stays in `.claude/` files (stable). Agent EXPERTISE, PROCESS KNOWLEDGE, and WORKFLOW OUTCOMES go in Unimatrix (dynamic). Retrospective loop is first-class. System PROPOSES improvements, humans APPROVE.

- .claude/ files contain thin shells: core identity + "pull from Unimatrix" directive
- Unimatrix stores dynamic parts: expertise, conventions, patterns, AND process knowledge
- Process improvements are PROPOSED by system, APPROVED by humans
- 8 + 3 tables (USAGE_LOG, FEATURE_ENTRIES, OUTCOME_INDEX)
- Additional tool: context_retrospective
- Confidence formula includes helpfulness_factor

**Strengths**: Learning on both knowledge AND process. Evidence-based proposals. Helpfulness decay.
**Weaknesses**: Identity/expertise boundary is fuzzy (~20% of cases). More moving parts. Outcome tracking requires agent discipline.

---

## Part 5: Head-to-Head Comparison (A vs C)

### Source of Truth

| | A: Knowledge Oracle | C: Workflow-Aware Hybrid |
|---|---|---|
| `.claude/` files | Source of truth for process | Source of truth for identity only (thin shells) |
| Unimatrix DB | Source of truth for knowledge only | Source of truth for expertise + process knowledge + outcomes |
| Who writes `.claude/`? | Human, always | Human writes thin shells once, rarely touches again |

### What the Learning Loop Operates On

| | A | C |
|---|---|---|
| Knowledge (conventions, patterns) | Yes | Yes |
| Process (protocols, routing) | No — human reads lessons, manually edits files | Yes, but human-gated — system proposes, human approves |
| Workflow outcomes | Not tracked | First-class — outcome entries, usage logs, helpfulness tracking |

### Database Differences

| | A | C |
|---|---|---|
| Core tables | 8 (baseline) | 8 + 3 (USAGE_LOG, FEATURE_ENTRIES, OUTCOME_INDEX) |
| Entry types | Freeform category string | Freeform category with conventions ("outcome", "process-proposal", "process") |
| Extra metadata | None | feature_id, usage_count, helpful_count, last_used_at |
| Confidence formula | base * usage * freshness * correction | A's formula + helpfulness_factor |

### The 12-Release Retrospective

**A**: Scrum-master stores lessons. Human queries. Human reads. Human edits 3 files. If human doesn't act, nothing changes.

**C**: Agents store outcomes throughout. System aggregates. System generates proposals with evidence. Human approves with one keystroke. Process knowledge updates. Next feature benefits. Rejection teaches the system what humans don't want.

### Process Tuning Effort

| Change | A | C |
|--------|---|---|
| Gate criteria refinement | Edit ndp-validator.md (1 file) | Store process entry (0 files) |
| Discover what needs tuning | Human proactively queries lessons | System proposes after retrospective |

---

## Part 6: Strategic Decision — Start A, Evolve to C

### The Decision

Start with Proposal A's simplicity. Evolve toward Proposal C's process-awareness incrementally. A's schema is a strict subset of C's — the evolution is purely additive.

### Why This Works

C's additions over A are exactly 3 tables and 4 fields:

| C Addition | Additive? | Can add later? |
|---|---|---|
| USAGE_LOG table | Yes | Start logging whenever ready. Lose history before that point. |
| FEATURE_ENTRIES table | Yes | Start linking features to entries at any point. |
| OUTCOME_INDEX table | Yes | Just indexes category="outcome" entries. |
| feature_id on EntryMetadata | Yes | Option<String>, defaults to None via serde. |
| usage_count, helpful_count, last_used_at | Yes | Default to 0/None. Inert before tracking exists. |
| PendingReview status variant | Yes | New enum variant, backward-compatible. |
| helpfulness_factor in confidence | Yes | Before usage data, factor = 1.0 (neutral). |

### The Evolution Path

```
v0.1 — Ship A (Knowledge Oracle)
  Schema: 8 tables, EntryRecord with #[serde(default)] on all fields
  Tools: context_search, context_lookup, context_store, context_get
  Agent files: full (existing style)
  Learning: knowledge only

  KEY DESIGN DECISION: use #[serde(default)] on EntryRecord.
  Costs nothing. Makes every future field addition backward-compatible.

v0.2 — Add lifecycle tools + outcome convention
  Tools: + context_correct, context_deprecate, context_status, context_briefing
  Convention: agents start using context_store(category: "outcome")
  Still no tracking infrastructure — outcomes are just regular entries
  Still A. But outcome CONVENTION is established.

v0.3 — Turn on usage tracking (the bridge to C)
  Schema: + USAGE_LOG, + FEATURE_ENTRIES
  Fields: start populating usage_count, helpful_count, last_used_at
  Confidence: add helpfulness_factor
  Data accumulates. Tracking validated manually.

v0.4 — Add retrospective pipeline (now C)
  Schema: + OUTCOME_INDEX
  Tools: + context_retrospective
  Status: + PendingReview variant
  CLI: + unimatrix proposals, approve, reject
  System proposes process improvements from accumulated data.

v0.5 — Thin-shell migration (optional, gradual)
  Slim agent files one at a time as expertise moves to Unimatrix.
  No big bang. Some agents may never thin out.
```

### The Key Schema Decision for v0.1

```rust
#[derive(Serialize, Deserialize)]
struct EntryRecord {
    id: u64,
    title: String,
    content: String,
    topic: String,
    category: String,
    tags: Vec<String>,
    source: String,
    status: Status,
    confidence: f32,
    created_at: u64,
    updated_at: u64,
    last_accessed_at: u64,
    access_count: u32,
    supersedes: Option<u64>,
    superseded_by: Option<u64>,
    correction_count: u32,
    embedding_dim: u16,
    // Future C fields — present from day 1, unpopulated
    #[serde(default)]
    feature_id: Option<String>,
    #[serde(default)]
    usage_count: u32,
    #[serde(default)]
    helpful_count: u32,
    #[serde(default)]
    last_used_at: Option<u64>,
}
```

### What to Do from Day 1 (Costs Nothing)

1. Document `category: "outcome"` convention in server instructions. Agents start storing outcomes. Data accumulates for future retrospective.
2. Document `category: "lesson-learned"` convention. A's natural pattern becomes input data for C's retrospective later.
3. Keep confidence formula as a function, not inline math. Adding helpfulness_factor later = one function change.

---

## Part 7: Practical Agent Behavior — ADRs, CI/CD, and Behavioral Driving

### ADR Lookup

**Architect needs all ADRs (deterministic):**
```
context_lookup(category: "decision", tags: ["adr"])
```

**Architect needs ADRs for a specific feature:**
```
context_lookup(category: "decision", tags: ["adr", "nxs-010"])
```

**Rust-dev needs task-relevant ADRs (semantic):**
```
context_search(query: "how should auth tokens be validated and stored", category: "decision")
```

**Via briefing (most common path):**
```
context_briefing(role: "rust-dev", task: "implement auth middleware", feature: "nxs-010")
→ ADR-015 appears in "Decisions" section automatically
```

**How ADRs get stored (by architect during Phase 2):**
```
context_store(
  content: "ADR-015: JWT with RS256. Tokens in httpOnly cookies.
    Validation at gateway, claims propagated via X-Claims header.",
  topic: "auth",
  category: "decision",
  tags: ["adr", "architecture", "nxs-010"],
  source: "agent:ndp-architect"
)
```

### CI/CD and Test Procedures

**Storing (by whoever establishes them):**
```
context_store(
  content: "CI pipeline: cargo fmt --check → cargo clippy -- -D warnings →
    cargo test --workspace → cargo build --release. All must pass before merge.",
  topic: "cicd", category: "procedure",
  tags: ["ci", "rust", "pipeline"]
)

context_store(
  content: "Integration tests require Docker. Run: docker compose -f test-compose.yml up -d,
    then cargo test --features integration. Teardown: docker compose down -v.",
  topic: "cicd", category: "procedure",
  tags: ["integration-tests", "docker"]
)
```

**Retrieving:**
```
context_lookup(topic: "cicd", category: "procedure")
→ All CI/CD procedures (deterministic — tester wants full list)

context_search(query: "how to run integration tests for database layer")
→ Surfaces Docker-based procedure via semantic match
```

### Behavioral Driving — WHY Agents Do These Things

Three layers, each reinforcing:

**Layer 1: Server `instructions` field (70-85% reliability, zero config)**
```
Unimatrix is this project's knowledge engine. Before starting implementation
or design work, search for relevant conventions and patterns. After making
architectural decisions, discovering patterns, or establishing conventions,
store them. When corrected by the user, record the correction using context_correct.
```

**Layer 2: CLAUDE.md (pushes to ~90%)**
```markdown
## Unimatrix
Before starting implementation or design, search Unimatrix for relevant
conventions, decisions, and patterns. After establishing new conventions
or making architectural decisions, store them for future reference.
```

**Layer 3: Agent definition (specific trigger)**
```markdown
## Orientation (MANDATORY FIRST STEP)
Before starting any work, call:
  context_briefing(role: "ndp-architect", task: "<your assigned task>")

## Outcome Reporting (Before Handoff)
Store decisions, patterns, and conventions discovered during your work:
  context_store(topic: "<relevant topic>", category: "decision"|"pattern"|"convention")
```

**The behavioral chain:**
```
Server instructions → general intent ("search before work, store after decisions")
CLAUDE.md → reinforced intent ("search Unimatrix")
Agent file → specific action ("call context_briefing(role: X, task: Y)")
Agent calls tool → gets useful results → applies them
Agent encounters pattern → server instructions say "store it"
Agent stores → pattern persisted → next agent gets better briefings
```

**Reliability by behavior:**
- Search before work: ~90% (CLAUDE.md + server instructions + agent orientation)
- Store after decisions: ~75% (server instructions + agent outcome reporting)
- Store CI/CD procedures proactively: ~65% (weakest — agents don't always recognize operational knowledge is worth storing)
- Correction detection: ~60% (relies on dedup check catching it server-side)

---

## Part 8: Workflow Mapping Results

Two detailed workflow mappings were produced mapping the Spec-Driven Development with Risk-Based Testing workflow onto Proposals A and C. See:

- `proposals/a-knowledge-oracle/WORKFLOW-MAPPING.md`
- `proposals/c-workflow-aware-hybrid/WORKFLOW-MAPPING.md`

### Key Findings from Workflow Mapping

**Both proposals produce nearly identical .claude/ file structures** — same agents, same protocols, same gates. The difference is what flows through the system at runtime.

**Gate failure handling is the critical difference:**
- A: Validator finds issue → rework → lesson stored → human must proactively query to find pattern → human edits files
- C: Validator finds issue → outcome stored with gate tags → after N features, retrospective detects pattern → system proposes fix → human approves → process knowledge updates

**Data structure gaps shared by both:**
- No structured gate result tracking (unstructured markdown in entries)
- No risk-to-test traceability index
- Three source documents live on disk, not in Unimatrix

**C's unique data:**
- USAGE_LOG tracks every retrieval with helpful flag → feeds helpfulness_factor
- FEATURE_ENTRIES links features to entries used → enables cross-feature analysis
- OUTCOME_INDEX indexes outcome entries → enables retrospective aggregation

**Continuous tweaking:**
- A: Individual tweaks are cheap (edit a file), but discovering WHAT to tweak is expensive (human must proactively query)
- C: System surfaces what needs attention with evidence, human approves the change

**A's core tradeoff**: Knowledge accumulates silently. Process stays broken until a human intervenes.
**C's core tradeoff**: More moving parts. Protocol file can drift from Unimatrix process knowledge. Human must bridge file edits.

---

## Reference: Proposal File Locations

```
product/features/ass-007/
├── SCOPE.md
├── DESIGN-DISCUSSION.md              ← this file
├── research/
│   ├── TRACK-SYNTHESIS.md            ← D1-D6 consolidated findings
│   └── SCENARIO-ANALYSIS.md          ← 7-scenario design exploration
└── proposals/
    ├── workflow-example.md           ← Spec-Driven Development workflow
    ├── a-knowledge-oracle/
    │   ├── ASSUMPTIONS.md
    │   ├── CONTROL-STRUCTURE.md
    │   ├── INTERFACE.md
    │   ├── DATABASE.md
    │   ├── SCENARIOS.md
    │   └── WORKFLOW-MAPPING.md
    ├── b-dynamic-control-plane/      ← REJECTED
    │   ├── ASSUMPTIONS.md
    │   ├── CONTROL-STRUCTURE.md
    │   ├── INTERFACE.md
    │   ├── DATABASE.md
    │   └── SCENARIOS.md
    └── c-workflow-aware-hybrid/
        ├── ASSUMPTIONS.md
        ├── CONTROL-STRUCTURE.md
        ├── INTERFACE.md
        ├── DATABASE.md
        ├── SCENARIOS.md
        └── WORKFLOW-MAPPING.md
```
