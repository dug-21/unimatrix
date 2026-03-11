# base-004: Mandatory Knowledge Stewardship

## Problem

Unimatrix is a self-learning knowledge engine, but the feedback loop is broken. Agents query Unimatrix at the start of their work (read side), then produce artifacts as files. Knowledge flows back into Unimatrix only when:

1. An agent voluntarily calls a store skill (optional, unenforced)
2. Someone manually runs `/retro` after a PR merge (manual, infrequent)

Result: implementation gotchas, gate failure patterns, security findings, vision alignment issues, and spec interpretation decisions are lost between features. The next agent on the next feature starts from zero on problems already solved.

### Agents With No Knowledge Stewardship

These agents currently have no mechanism to store findings back into Unimatrix:

| Agent | What Gets Lost |
|-------|---------------|
| `uni-vision-guardian` | Recurring misalignment patterns across features |
| `uni-specification` | AC interpretation decisions, domain model choices |
| `uni-synthesizer` | Conflict resolutions between architecture and spec |
| `uni-security-reviewer` | Recurring security anti-patterns across features |

### Agents With Optional/Unenforced Stewardship

These agents have Knowledge Stewardship sections but compliance is not verified:

| Agent | Stewardship Exists | In Self-Check | Validator Checks |
|-------|--------------------|---------------|-----------------|
| `uni-validator` | Yes | No | N/A (is the validator) |
| `uni-researcher` | Yes | No | No |
| `uni-risk-strategist` | Yes | No | No |
| `uni-rust-dev` | Yes (query only) | No | No |
| `uni-bug-investigator` | Yes | No | No |

---

## Design: Three-Layer Enforcement

### Layer 1: Agent Stewardship Sections (Universal)

Every agent definition gets a Knowledge Stewardship section. Not optional. The section specifies:

- **What to store**: category-specific guidance per agent role
- **How to store**: which skill to use, required fields
- **When to decline**: agents may legitimately find nothing novel — they note this in their report

#### Rust Developer Stewardship Model

The rust-dev is the strongest example of the pattern. It discovers implementation gotchas through trial and error — patterns invisible in source code that only surface when you hit them:

- "Don't hold `lock_conn()` across await points — deadlocks under concurrent requests"
- "bincode v2 `serde(default)` required on new fields or migration breaks silently"
- "redb transactions must be committed before `TableDefinition` reference drops"

**Storage model**: topic = crate name (e.g., `unimatrix-store`), category = `pattern` or `convention`.

**Content template** (enforced by skill):
- **What**: the pattern (one sentence)
- **Why**: what goes wrong without it
- **Scope**: which crate/module/context it applies to

This template is the quality floor. "I used Arc::clone" fails it — no "why." "Don't hold lock_conn() across await — deadlocks under concurrent MCP requests" passes naturally.

**Why not just grep the codebase?** The codebase shows how code works *now*. Unimatrix captures why and what bites you — traps invisible in source. You can't grep for "this compiles fine but deadlocks under load."

#### Per-Agent Stewardship Guidance

| Agent | Store What | Category | Topic Convention |
|-------|-----------|----------|-----------------|
| `uni-rust-dev` | Implementation gotchas, crate-specific traps | `pattern`, `convention` | Crate name |
| `uni-tester` | Test infrastructure patterns, fixture usage | `procedure`, `pattern` | `testing` or crate name |
| `uni-validator` | Recurring gate failure patterns | `lesson-learned` | `validation` |
| `uni-risk-strategist` | Risk patterns that recur across features | `pattern` | `risk` |
| `uni-researcher` | Problem space patterns, technical constraints | `pattern`, `convention` | Research area |
| `uni-bug-investigator` | Root cause patterns, debugging techniques | `lesson-learned` | Crate name |
| `uni-vision-guardian` | Recurring alignment variances | `pattern` | `vision` |
| `uni-specification` | AC interpretation precedents | `convention` | Domain area |
| `uni-synthesizer` | Arch-spec conflict resolution patterns | `pattern` | `synthesis` |
| `uni-security-reviewer` | Security anti-patterns | `lesson-learned` | Crate name or `security` |
| `uni-architect` | Already stores ADRs (mandatory) | `decision` | Feature/crate |

### Layer 2: Validator Gate Checks (Enforcement)

The validator already runs three gates per delivery session. Add one check to each relevant gate:

**Gate 3a (Design Validation)**:
- "Did the architect store ADRs in Unimatrix?" (already enforced)
- "Did the risk strategist query and store risk patterns?"

**Gate 3b (Implementation Validation)**:
- "Did each rust-dev agent store or explicitly decline to store implementation patterns?"
- "Did the pseudocode agent query patterns before designing?"

**Gate 3c (Test Validation)**:
- "Did the tester query procedures before designing test plans?"
- "Did the tester store any new test infrastructure patterns?"

**Enforcement rule**: The validator checks agent reports for evidence of stewardship. If an agent stored findings, the report will reference the stored entry. If an agent found nothing novel, the report should state that explicitly. Absence of either is a REWORKABLE FAIL.

### Layer 3: Retro Quality Pass (Curation)

The `/retro` skill already analyzes shipped features. Add a stewardship review step:

- Query all entries stored during the feature cycle (via `feature_cycle` tag)
- Assess quality: does each entry follow the content template?
- Deprecate low-value entries (noise that passed the gate check)
- Promote high-value entries (boost confidence on patterns confirmed by successful delivery)

This makes the retro the quality filter. Gates ensure agents *try*. Retros ensure the knowledge base stays clean.

---

## Skill: `/store-pattern` (Proposed)

A focused skill for implementation-level patterns. Distinct from `/store-procedure` (step-by-step how-tos) and `/store-lesson` (failure-driven).

**Required fields**:
- `topic`: crate or module name
- `what`: the pattern (one sentence)
- `why`: what goes wrong without it
- `scope`: where it applies

**Validation**: skill rejects entries missing `why` (the quality floor).

**Alternative**: extend `/store-procedure` with a `mode: pattern` that applies the template. Fewer skills to maintain. Trade-off is overloading an existing skill vs. clarity of purpose.

---

## Confidence Signal: Deliberate Retrieval Boost

Tracked in #199. When an agent does `context_get` or `context_lookup` (deliberate, targeted retrieval), this is a stronger relevance signal than appearing in a search result set. Currently both produce the same `access_count` increment.

Proposed: differentiate the signal so deliberate retrieval produces a stronger confidence boost. See #199 for implementation options.

---

## Staleness and the Codebase-vs-Unimatrix Tension

Concern: implementation patterns change as code evolves. Why query Unimatrix when you can read the source?

Resolution: Unimatrix stores *traps and gotchas*, not API documentation. The source shows how code works now. Unimatrix captures what will bite you — knowledge invisible in source. "This compiles but deadlocks under load" can't be grepped.

Staleness is handled by the confidence system:
- Patterns nobody queries → access_count flatlines → freshness decays → sinks in rankings
- Patterns invalidated by code changes → retro architect deprecates them
- Patterns confirmed by repeated use → confidence grows via retrieval signal (#199)

---

## Portability Considerations

This design has a portable core and a project-specific layer:

**Portable (ships with Unimatrix to any repo)**:
- Skills: `/query-patterns`, `/store-pattern`, `/store-lesson`, `/store-adr`, `/record-outcome`
- CLAUDE.md snippet: behavioral rules, stewardship expectations
- Content template for patterns (what/why/scope)
- Empty database — knowledge accumulates organically through use

**Project-specific (stays in this repo)**:
- Agent definitions with per-role stewardship guidance
- Validator gate check sets
- Swarm protocols and choreography
- Retro quality pass configuration

The skill set is the interface boundary. Keep it tight — every skill added is one more thing to document and maintain in the export package.

---

## Implementation Sequence

1. Add Knowledge Stewardship sections to agents that lack them (vision guardian, spec writer, synthesizer, security reviewer)
2. Add stewardship to rust-dev (crate-as-topic, pattern/convention categories, what/why/scope template)
3. Add validator gate checks for stewardship compliance
4. Create `/store-pattern` skill (or extend `/store-procedure`)
5. Add stewardship review step to `/retro` skill
6. Implement deliberate retrieval confidence boost (#199)

---

## References

- #199: Deliberate Retrieval Confidence Signal
- Confidence composite: `crates/unimatrix-engine/src/confidence.rs`
- Usage recording: `crates/unimatrix-server/src/services/usage.rs`
- Validator: `.claude/agents/uni/uni-validator.md`
- Retro skill: `.claude/skills/retro/SKILL.md`
- Existing agent defs: `.claude/agents/uni/`
