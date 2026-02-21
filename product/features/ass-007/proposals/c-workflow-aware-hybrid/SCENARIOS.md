# Proposal C: End-to-End Scenarios

## Scenario 1: Agent Orientation with Thin Shell + Unimatrix Pull

### The `.claude/` file (thin shell)

```markdown
# ndp-rust-dev (45 lines -- identity only)
[frontmatter: name, type, scope]

## Orientation (MANDATORY FIRST STEP)
Before starting any work, call:
  context_briefing(role: "ndp-rust-dev", task: "<your assigned task>")

## Design Principles
[6 stable philosophical principles]

## Self-Check
[5 gates -- cargo build, cargo test, no todos, scope check, reflexion]

## Outcome Reporting
Call context_store(category: "outcome") before handoff.
```

### The runtime flow

**Scrum-master spawns ndp-rust-dev with task: "implement EmbeddingStore trait for redb"**

Agent's first action (driven by the file's Orientation section):

```
Tool call: context_briefing
  params: { role: "ndp-rust-dev", task: "implement EmbeddingStore trait for redb", phase: "implementation", feature: "nxs-012" }
```

Server-side execution:
```
1. lookup(topic: "ndp-rust-dev", category: "convention") -> 3 entries
2. lookup(topic: "ndp-rust-dev", category: "duties")     -> 1 entry
3. lookup(category: "process", tags: ["phase:implementation"]) -> 2 entries (approved process knowledge)
4. search(query: "implement EmbeddingStore trait for redb", k: 3) -> 3 entries
5. lookup(category: "convention", tags: ["recent-correction"], limit: 2) -> 1 entry
```

Response (<1800 tokens):
```
## Your Conventions
- Use `anyhow` for app errors, `thiserror` for library errors
- All data stores implement core traits (Domain Adapter Pattern)
- Use `spawn_blocking` with `Arc<Database>` for redb async

## Process Knowledge
- Wave 2: max 3 parallel agents (approved 2026-02-15)
- Implementation phase: run `cargo clippy` before each commit, not just at end

## Relevant Patterns
1. redb Write Pattern (0.91): Single writer via write transaction, read via read transaction...
2. Trait Implementation Pattern (0.87): Define trait in core/src/traits.rs, implement in domain module...
3. Test Pattern for Storage (0.82): Use tempdir for redb in tests, cleanup on drop...

## Recent Corrections
- CORRECTED 2026-02-18: Storage traits now return `Result<T, StorageError>` not `Result<T, CoreError>`
```

**What's in the file vs. what came from Unimatrix:**

| Source | Content |
|--------|---------|
| `.claude/` file | Design principles, self-check gates, the instruction to call `context_briefing` |
| Unimatrix | Conventions, process knowledge, relevant patterns, corrections |

**Usage tracking**: All 10 returned entries get logged in `USAGE_LOG` with `feature_id: "nxs-012"`, `agent_role: "ndp-rust-dev"`. Added to `FEATURE_ENTRIES` multimap.

---

## Scenario 2: The 12-Release Retrospective

**Context**: Team has completed 12 features (nxs-001 through nxs-012). Human wants to understand process effectiveness and improve.

### Step 1: Trigger retrospective

```
Tool call: context_retrospective
  params: { feature: "nxs-012", compare_with: ["nxs-010", "nxs-011", "nxs-009", "nxs-008"] }
```

### Step 2: System aggregates outcome data

From `OUTCOME_INDEX` for nxs-012:
```
- 4 outcome entries: completion (8 days, 3 waves, 5 agents), quality (2 post-merge bugs),
  efficiency (12 entries, 9 helpful), blocker (wave 2 merge conflicts on trait files)
```

From `FEATURE_ENTRIES` for nxs-012:
```
- 12 entries were retrieved during this feature's lifecycle
- USAGE_LOG shows: 9 marked helpful, 2 marked irrelevant, 1 marked outdated
```

Cross-feature comparison:
```
nxs-008: 5 days, 2 waves, 3 agents, 0 bugs, 8/10 helpful
nxs-009: 6 days, 3 waves, 4 agents, 1 bug, 7/9 helpful
nxs-010: 7 days, 3 waves, 4 agents, 1 bug, 10/12 helpful
nxs-011: 6 days, 2 waves, 3 agents, 0 bugs, 9/11 helpful
nxs-012: 8 days, 3 waves, 5 agents, 2 bugs, 9/12 helpful
```

### Step 3: Gap detection + proposal generation

System identifies:
- Features with 4+ agents in any wave have higher bug rates (nxs-009, nxs-010, nxs-012)
- ndp-rust-dev searched for "embedding store" patterns 3 times across nxs-010/011/012, found nothing useful
- Entry #42 ("redb write pattern") was used in all 5 features, always helpful

System creates two `process-proposal` entries:

**PP-001** (stored in ENTRIES, status: PendingReview):
```
PROPOSAL: Cap any single wave at 3 parallel agents.
EVIDENCE: Features with 4+ agents in a wave averaged 1.3 bugs vs 0.2 bugs
for features with <=3. Merge conflict rate correlates with agent count.
SUGGESTED ACTION: Add constraint to protocols/planning.md wave definitions.
AFFECTED: All features, wave planning phase.
```

**PP-002**:
```
PROPOSAL: Create seed pattern for embedding storage implementations.
EVIDENCE: "embedding store" searched 3 times in recent features with no useful results.
Agents spent avg 45 min building from scratch each time.
SUGGESTED ACTION: Store canonical embedding storage pattern as convention entry.
AFFECTED: Any feature involving vector storage.
```

### Step 4: Human reviews

```
Tool call: context_lookup
  params: { category: "process-proposal", status: "pending-review" }

Response:
  2 pending proposals:

  1. [PP-001] Cap waves at 3 agents (evidence: 5 features)
     > Features with 4+ agents averaged 1.3 bugs vs 0.2...

  2. [PP-002] Add embedding storage pattern (evidence: 3 searches, 0 results)
     > "embedding store" searched 3 times...
```

**Human approves PP-001:**
```
CLI: unimatrix approve PP-001
  # or MCP: context_correct(original_id: "PP-001", content: "Wave constraint: max 3 parallel agents per wave...", reason: "Approved based on evidence")
```

Result: New entry created with `category: "process"`, `status: Active`, `supersedes: PP-001`. PP-001 marked `Deprecated` with `superseded_by`. Future `context_briefing` calls with `phase: "planning"` will include this constraint.

**Human modifies PP-002 before approving:**
```
CLI: unimatrix approve PP-002 --content "Store embedding storage pattern. Use the pattern from nxs-012 as baseline, not a generic template."
```

**Human notes**: "I should also update `protocols/planning.md` to add the wave cap." Unimatrix suggested the action but doesn't touch the file. Human edits it.

---

## Scenario 3: Knowledge Accumulation Across Feature Cycles

### Feature nxs-010 (early)

ndp-rust-dev discovers a pattern while implementing:
```
Tool call: context_store
  params: { content: "For redb multimap tables, iterate with .range() not .get() -- .get() only returns first match",
            topic: "redb", category: "pattern", tags: ["redb", "multimap", "gotcha"] }
```
Entry created. Confidence: 0.6 (new, no usage data).

### Feature nxs-011 (middle)

Different ndp-rust-dev agent retrieves this via search while working on tag indexing:
```
Tool call: context_search
  params: { query: "redb multimap iteration", topic: "redb" }
```
Returns the entry. Agent uses it, reports helpful via reflexion. Usage count: 1, helpful: 1. Confidence rises to 0.72.

### Feature nxs-012 (later)

Third agent retrieves it via `context_briefing`. Still helpful. Usage: 2, helpful: 2. Confidence: 0.81.

Meanwhile, ndp-rust-dev on nxs-012 stores a new pattern:
```
{ content: "redb MultimapTable::insert doesn't check for duplicates -- always call remove() first if updating",
  topic: "redb", category: "pattern", tags: ["redb", "multimap", "gotcha"] }
```

Dedup check: similarity with existing multimap entry is 0.78 (below 0.92 threshold). Stored as separate entry -- they're related but different gotchas.

### Retrospective after nxs-012

Both multimap entries show high helpfulness. No process proposal needed -- the knowledge layer is working. The outcome report shows: "redb patterns: 2 entries, 4 total uses, 100% helpful rate."

### Feature nxs-015 (much later)

A redb API update changes multimap behavior. Agent stores correction:
```
Tool call: context_correct
  params: { original_id: <first_entry_id>, content: "redb 2.x: MultimapTable.range() renamed to .iter(). Use .iter() for multimap traversal.", reason: "redb 2.x API change" }
```
Old entry deprecated, new entry active. Correction chain preserved. Any future search for "redb multimap" returns the corrected version with annotation: "Supersedes earlier pattern (redb 1.x API)."

---

## Scenario 4: New Agent Type Added

**Context**: Team needs a `ndp-security-auditor` agent. Human sets it up.

### Step 1: Human creates thin-shell file

Human writes `/workspaces/unimatrix/.claude/agents/ndp/ndp-security-auditor.md`:

```markdown
---
name: ndp-security-auditor
type: reviewer
scope: security
description: Reviews code and architecture for security vulnerabilities
---

# Security Auditor

You review code and architecture decisions for security vulnerabilities.

## Orientation (MANDATORY FIRST STEP)

Before starting any review, call:
  context_briefing(role: "ndp-security-auditor", task: "<your review scope>")

## Design Principles

1. Defense in Depth -- assume every layer can be bypassed
2. Least Privilege -- minimize access at every boundary
3. Fail Secure -- errors should deny access, not grant it

## Self-Check

- [ ] Every finding has severity (critical/high/medium/low) and remediation
- [ ] No false sense of security -- state what was NOT checked
- [ ] Cross-reference with known vulnerability patterns

## Outcome Reporting

Call context_store(category: "outcome") with findings summary and review coverage.
```

**What the human wrote**: ~30 lines. Identity only.

### Step 2: Human seeds expertise in Unimatrix

```bash
# Via CLI or by asking Claude to store these
unimatrix store --topic "ndp-security-auditor" --category "convention" \
  --content "Check all user input boundaries for injection. SQL, command, path traversal." \
  --tags "security,input-validation"

unimatrix store --topic "ndp-security-auditor" --category "convention" \
  --content "Review all authentication flows. Check token expiry, refresh rotation, revocation." \
  --tags "security,auth"

unimatrix store --topic "security" --category "checklist" \
  --content "OWASP Top 10 review checklist: injection, broken auth, sensitive data exposure..." \
  --tags "security,checklist,owasp"
```

### Step 3: First deployment

Scrum-master spawns `ndp-security-auditor` for nxs-013 review:
```
context_briefing(role: "ndp-security-auditor", task: "review auth middleware implementation", feature: "nxs-013")
```

Response includes the seeded conventions plus any project-wide security-related entries from prior features. Thin -- but functional.

### Step 4: Expertise grows

After 3 features, the security auditor has stored findings:
```
{ topic: "security", category: "pattern", content: "This project uses JWT RS256 -- check key rotation...", tags: ["auth", "jwt"] }
{ topic: "security", category: "finding", content: "Missing rate limiting on /api/auth endpoint", tags: ["rate-limiting", "nxs-013"] }
```

By feature 5, `context_briefing` for the security auditor returns rich context without the human having added anything more to the `.claude/` file.

### What went where

| Content | Location | Who created it |
|---------|----------|---------------|
| Role name, principles, self-checks | `.claude/` file | Human (once) |
| Security conventions | Unimatrix | Human (seed) + agent (discovered) |
| Project-specific findings | Unimatrix | Agent (during reviews) |
| Process improvements (e.g., "security review should happen in wave 1, not wave 3") | Unimatrix | System (proposed), human (approved) |
| Update to `protocols/planning.md` adding security review to wave 1 | `.claude/` file | Human (manually, after approving proposal) |
