# Proposal A: Knowledge Oracle -- End-to-End Scenarios

## Scenario 1: Agent Orientation (Rust Dev Starting Implementation)

**Setup:** The scrum-master is spawning an `ndp-rust-dev` for Wave 2 of feature `nxs-003`. The project has 400 entries accumulated over 3 prior features.

**What's in .claude/ files (static, human-authored):**
- `agents/ndp/ndp-rust-dev.md` -- role definition, scope, self-check gates
- `protocols/implementation-protocol.md` -- wave sequence, gate conditions
- `rules/rust-workspace.md` -- Rust-specific constraints (file-pattern triggered)

**What's in Unimatrix (dynamic, agent-accumulated):**
- 45 entries tagged `topic: "rust"` (error handling conventions, testing patterns, crate preferences)
- 8 entries from `nxs-003` architect decisions (ADRs stored during planning)
- 12 entries `category: "lesson-learned"` from prior features

**The scrum-master builds the spawn prompt:**

```
Step 1: Scrum-master calls context_briefing
--> context_briefing(role: "rust-dev", task: "implement HNSW index wrapper with FilterT support", feature: "nxs-003")

Server internally executes:
  lookup(topic: "rust-dev", category: "convention")  -> 3 entries (error handling, testing, naming)
  lookup(topic: "nxs-003", category: "decision")     -> 2 ADRs (DistDot choice, dimension validation)
  search(query: "HNSW index wrapper FilterT", k: 3)  -> 3 patterns (hnsw_rs usage, closure patterns, filter construction)

Response (~800 tokens):
  ## Briefing for rust-dev: implement HNSW index wrapper with FilterT support

  ### Conventions
  1. [e:12] Use anyhow for app errors, thiserror for library errors. Never unwrap in prod.
  2. [e:34] Every public fn returning Result needs error-path tests.
  3. [e:56] Prefer newtype wrappers over type aliases for domain IDs.

  ### Decisions (nxs-003)
  1. [e:201] ADR-003: Use DistDot distance. Pre-normalize all vectors before insertion.
  2. [e:205] ADR-007: Dimension validated at our layer, not hnsw_rs. Panic on mismatch.

  ### Relevant Patterns
  1. [e:89] hnsw_rs FilterT pattern (0.91): Build closure capturing HashSet<usize> of valid d_ids.
  2. [e:145] RwLock<Hnsw> pattern (0.87): Use RwLock for search mode toggle.
  3. [e:167] spawn_blocking bridge (0.83): Wrap hnsw_rs ops in tokio::task::spawn_blocking.

Step 2: Scrum-master composes spawn prompt (from .claude/ file + Unimatrix result)
  prompt = read("agents/ndp/ndp-rust-dev.md")       // static role definition
         + "Your agent ID: nxs-003-agent-3-rust-dev"
         + "Task: Implement HNSW index wrapper..."
         + briefing_result                            // pasted from context_briefing
         + "Read these files: [component-specific pseudocode, test-plan]"
```

**The subagent receives:** Static role definition (from file) + dynamic knowledge context (from Unimatrix) + task-specific file paths. It never calls Unimatrix directly. The orchestrator-passes-context pattern bypasses MCP inheritance bugs entirely.

**During work, if the rust-dev discovers a pattern:**

```
The orchestrator (scrum-master) stores it after the agent returns:
--> context_store(
      content: "hnsw_rs search_filter returns neighbors sorted by distance ascending. Map to similarity via 1.0 - distance. Pre-sort is guaranteed.",
      topic: "hnsw-rs",
      category: "convention",
      tags: ["rust", "search", "hnsw"],
      source: "agent:ndp-rust-dev"
    )
<-- Stored entry e:210 "hnsw_rs Search Result Ordering" [topic: hnsw-rs, category: convention]
    Confidence: 0.70 (new, unvalidated)
```

---

## Scenario 2: Knowledge Accumulation Over Feature Iterations

**Setup:** The team has completed features nxs-001, nxs-002, nxs-003. Starting nxs-004. Each feature stored 15-30 entries. The knowledge base has 250 entries, ~200 active.

**How knowledge compound-returns work:**

```
nxs-001 (storage traits):
  Stored: redb table layout patterns, bincode serialization convention, Arc<Database> pattern
  These were discovered fresh -- no prior knowledge existed

nxs-002 (embedding pipeline):
  context_search("redb table patterns") returned nxs-001 entries
  Architect applied known patterns, stored new: embedding model selection, dimension validation
  Knowledge: 15 inherited from nxs-001 + 20 new = 35 relevant entries

nxs-003 (HNSW integration):
  context_search("hnsw_rs with redb") returned entries from BOTH nxs-001 and nxs-002
  Rust-dev found conflict: nxs-001 used sync redb, nxs-002 used spawn_blocking
  --> context_correct(original_id: 15, content: "Always use spawn_blocking for redb in async context", reason: "sync calls block tokio runtime")
  Knowledge: 35 inherited + 1 corrected + 25 new = 61 relevant entries

nxs-004 (MCP server):
  context_briefing returns entries spanning all 3 prior features
  The corrected redb pattern surfaces with CORRECTED annotation
  New agent doesn't repeat the sync-vs-async mistake
```

**The compounding effect:** Each feature starts with more context than the last. Corrections propagate forward. Without Unimatrix, each agent starts from scratch (or from whatever the human remembered to put in file-based docs).

**What Unimatrix does NOT do here:** It does not update the `.claude/rules/rust-workspace.md` file with the spawn_blocking convention. If the human reads the correction and decides it's important enough, they add it to the rules file manually. If they don't, it still surfaces via `context_search` but with less authority than a rule.

---

## Scenario 3: The 12-Release Retrospective

**Setup:** Feature `col-001` (orchestration engine) took 12 releases to complete. The team wants to learn from this. Under Proposal A, Unimatrix captures the lessons; humans fix the process.

**Step 1: Scrum-master stores lessons at learning gate**

After each release, the scrum-master stores observations:

```
--> context_store(
      content: "col-001 release 4: Scope creep in wave 2. Architect added 3 unplanned components. Wave exceeded 2-iteration drift budget. Root cause: SCOPE.md acceptance criteria were ambiguous on integration boundaries.",
      topic: "col-001", category: "lesson-learned",
      tags: ["scope-creep", "planning", "wave-management"],
      source: "agent:ndp-scrum-master"
    )

--> context_store(
      content: "col-001 release 8: Test failures cascaded because wave 1 components had no integration test gate. Unit tests passed but cross-component calls failed. Root cause: test-plan lacked integration test requirement.",
      topic: "col-001", category: "lesson-learned",
      tags: ["testing", "integration", "gates"],
      source: "agent:ndp-scrum-master"
    )

--> context_store(
      content: "col-001 total: 12 releases for a feature scoped at 3. Contributing factors: ambiguous AC, missing integration gate, architect scope creep in waves 2 and 3, insufficient pseudocode specificity.",
      topic: "col-001", category: "lesson-learned",
      tags: ["retrospective", "planning", "process"],
      source: "agent:ndp-scrum-master"
    )
```

**Step 2: Human reviews lessons**

```
Human asks Claude: "What lessons did we learn from col-001?"

Claude calls:
--> context_lookup(topic: "col-001", category: "lesson-learned")
<-- Returns all 12 lesson entries, sorted by confidence

Human reads and identifies 3 actionable process changes:
  1. Add scope-check gate between wave 1 and wave 2
  2. Require integration test plan in test-plan/OVERVIEW.md
  3. Architect scope limited to components listed in SCOPE.md
```

**Step 3: Human edits .claude/ files**

```
Human edits .claude/protocols/implementation-protocol.md:
  + ## Scope Check Gate (between Wave 1 and Wave 2)
  + Before spawning Wave 2 agents, scrum-master verifies:
  + - No new components added beyond SCOPE.md
  + - Each Wave 2 task maps to an accepted component
  + - If scope expanded, STOP and consult primary agent

Human edits .claude/agents/ndp/ndp-architect.md:
  + ## Scope Boundary
  + Your architecture work is LIMITED to components listed in SCOPE.md.
  + If you identify a needed component not in SCOPE.md, document it as
  + a scope expansion request in your return -- do NOT add it to the architecture.

Human edits .claude/agents/ndp/ndp-tester.md:
  + ## Integration Test Requirement
  + Every test-plan/OVERVIEW.md MUST include an Integration Test section
  + listing cross-component interactions to verify.
```

**Step 4: Next feature benefits**

When `col-002` starts, the updated protocols and agent definitions take effect immediately. Additionally, if someone searches for planning patterns:

```
--> context_search(query: "scope management in multi-wave features")
<-- Returns col-001 lessons with context on WHY the process was changed
```

**What Proposal A does well here:** The lessons are preserved with full context. The human has clear, actionable information. The process changes are deliberate and auditable (git diff shows exactly what changed in protocols).

**What Proposal A does poorly here:** If the human doesn't act, nothing changes. The lessons sit in the database. The next feature with scope creep will have the same problem, even though the knowledge exists that it's a known issue. The system knows but cannot self-correct.

---

## Scenario 4: Cross-Agent Knowledge Transfer in a Swarm

**Setup:** Planning swarm for `nxs-005`. The architect, spec writer, and tester are running in Wave 1. The architect makes a decision that the tester needs.

**Timeline:**

```
T+0: Scrum-master spawns Wave 1 agents (architect, spec, tester) in parallel
     Each receives context_briefing in spawn prompt

T+2min: Architect completes, stores ADR:
--> context_store(
      content: "ADR-012: Use channel-based flow for embedding pipeline. Producer reads entries from redb, sends through bounded channel (cap 1000), consumer embeds and inserts to hnsw_rs. Backpressure via channel capacity.",
      topic: "nxs-005", category: "decision",
      tags: ["architecture", "embedding", "channels"],
      source: "agent:ndp-architect"
    )
<-- Stored e:301

T+3min: Tester completes. Did NOT have the ADR (spawned before it was stored).
     Tester's test plan tests "batch embedding" not "channel-based flow."

T+4min: Scrum-master runs drift check, queries:
--> context_lookup(topic: "nxs-005", category: "decision")
<-- Returns e:301 (the channel-based flow ADR)

Scrum-master compares ADR against tester's test plan.
Identifies mismatch: test plan doesn't cover channel backpressure.
Spawns tester for iteration 2 with ADR in prompt:
  "Update test plan. New architecture decision: [e:301 content]. Add channel backpressure tests."
```

**The key insight:** Knowledge transfer between agents in a swarm is mediated by the scrum-master via store/lookup, not by direct agent-to-agent communication. The scrum-master's drift check is the synchronization point. This is already how the protocol works (read `.claude/protocols/implementation-protocol.md`); Unimatrix adds the *knowledge content* that flows through these checkpoints.

**Without Unimatrix:** The architect writes the ADR to a file. The scrum-master reads the file and pastes it into the tester's re-spawn prompt. Functionally identical for this single feature, but the ADR is not searchable across features and doesn't benefit from confidence scoring, correction chains, or semantic retrieval.

**With Unimatrix:** The ADR is both in the feature directory (as a file for human review) AND in Unimatrix (for cross-feature retrieval, semantic search, lifecycle management). The file is the snapshot; the database is the living record.
