# Specification: base-004 Mandatory Knowledge Stewardship

## Objective

Close the knowledge feedback loop by making every swarm agent responsible for storing findings back into Unimatrix. Agents must either store knowledge using the appropriate skill or explicitly decline with rationale. Validator gate checks enforce compliance, and the retro skill curates stored entries for quality.

## Functional Requirements

### FR-01: Agent Stewardship Sections

Every agent definition in `.claude/agents/uni/` must have a Knowledge Stewardship section. Agents fall into two groups:

**FR-01a: Agents with store obligations** -- section specifies what to store, which category and topic convention, and which skill to invoke.

**FR-01b: Agents with no store obligation** -- section explicitly states "no storage expected" with rationale.

### FR-02: Per-Agent Stewardship Guidance

Each agent's Knowledge Stewardship section must include role-specific guidance per the table below. Guidance must be concise (maximum 15 lines per agent, excluding self-check items) to mitigate context window bloat (SR-01).

| Agent | Store What | Category | Topic Convention | Skill |
|-------|-----------|----------|-----------------|-------|
| `uni-rust-dev` | Implementation gotchas, crate-specific traps | `pattern`, `convention` | Crate name (e.g., `unimatrix-store`) | `/store-pattern` |
| `uni-tester` | Test infrastructure patterns, fixture usage | `pattern`, `procedure` | `testing` or crate name | `/store-pattern` or `/store-procedure` |
| `uni-validator` | Recurring gate failure patterns | `lesson-learned` | `validation` | `/store-lesson` |
| `uni-risk-strategist` | Risk patterns that recur across features | `pattern` | `risk` | `/store-pattern` |
| `uni-researcher` | Problem space patterns, technical constraints | `pattern`, `convention` | Research area | `/store-pattern` |
| `uni-bug-investigator` | Root cause patterns, debugging techniques | `lesson-learned` | Crate name | `/store-lesson` |
| `uni-vision-guardian` | Recurring alignment variances | `pattern` | `vision` | `/store-pattern` |
| `uni-specification` | AC interpretation precedents, domain modeling decisions | `convention` | Domain area | `/store-pattern` |
| `uni-security-reviewer` | Security anti-patterns, recurring vulnerability classes | `lesson-learned` | Crate name or `security` | `/store-lesson` |
| `uni-architect` | Already has mandatory stewardship (ADRs + patterns) | `decision`, `pattern` | Feature/crate | `/store-adr` |
| `uni-pseudocode` | No storage expected -- read-only; queries patterns before designing | N/A | N/A | N/A |
| `uni-synthesizer` | No storage expected -- compiles existing artifacts, produces no generalizable knowledge | N/A | N/A | N/A |

### FR-03: Stewardship Self-Check Items

Every agent definition with a Knowledge Stewardship section must include at least one stewardship-related item in its Self-Check list. The item must use the existing `- [ ] {statement}` format.

For agents with store obligations, the self-check item must reference the stewardship action (e.g., "Stored implementation patterns via /store-pattern or noted 'nothing novel' in report").

For agents with no store obligation (uni-pseudocode, uni-synthesizer), the self-check item must reference the query action (e.g., "Queried /query-patterns before designing").

### FR-04: Structured Stewardship Block in Agent Reports

Agent reports must include a structured `## Stewardship` section that the validator can parse reliably (mitigates SR-02). The format is:

```markdown
## Stewardship

| Action | Detail |
|--------|--------|
| Stored | entry #{id}: "{title}" via /store-pattern |
| Stored | entry #{id}: "{title}" via /store-lesson |
| Queried | /query-patterns for {domain} -- {N} results |
| Declined | Nothing novel to store -- {rationale} |
```

Rules:
- Every row starts with one of: `Stored`, `Queried`, `Updated`, `Declined`.
- `Stored` rows must include the entry ID returned by `context_store` and the skill used.
- `Declined` rows must include a rationale (not empty).
- At least one row is required. An empty Stewardship table is a validation failure.
- The table header and Action column values are the parsing contract. The validator matches on `## Stewardship` heading and `Stored`/`Queried`/`Updated`/`Declined` action keywords.

### FR-05: Validator Gate Stewardship Checks

The `uni-validator` agent definition must include one stewardship compliance check in each gate's check set.

**Gate 3a -- Stewardship compliance (design phase)**:
- Check: Agent reports from pseudocode agent and risk strategist include a Stewardship section.
- Pseudocode: must have at least one `Queried` row (queried patterns before designing).
- Risk strategist: must have at least one `Stored` or `Declined` row.
- Result: REWORKABLE FAIL if Stewardship section is missing or empty.

**Gate 3b -- Stewardship compliance (implementation phase)**:
- Check: Agent reports from each rust-dev agent include a Stewardship section.
- Each rust-dev: must have at least one `Stored` or `Declined` row.
- Result: REWORKABLE FAIL if Stewardship section is missing or empty.

**Gate 3c -- Stewardship compliance (test phase)**:
- Check: Agent report from tester includes a Stewardship section.
- Tester: must have at least one `Stored`, `Queried`, or `Declined` row.
- Result: REWORKABLE FAIL if Stewardship section is missing or empty.

The validator checks the structured table format, not free-form prose. Parsing rule: find `## Stewardship` heading, read the markdown table below it, verify at least one row with a valid Action value.

### FR-06: /store-pattern Skill

Create `/store-pattern` at `.claude/skills/store-pattern/SKILL.md` following the same structure as `/store-lesson` and `/store-procedure`.

**Required fields**:
- `topic`: crate or module name (string, required)
- `what`: the pattern in one sentence (string, required)
- `why`: what goes wrong without it (string, required)
- `scope`: where it applies -- crate, module, or context (string, required)

**Validation**:
- Reject entries missing the `why` field. This is the quality floor -- patterns without "why" are noise.
- Content must follow the what/why/scope template. The skill assembles these fields into the `content` parameter for `context_store`.

**Content assembly**: The skill combines fields into structured content:
```
What: {what}
Why: {why}
Scope: {scope}
```

**Deduplication check**: Before storing, the skill searches for existing patterns in the same topic/category:
```
mcp__unimatrix__context_search(query: "{what}", category: "pattern", k: 3)
```
If a matching pattern exists, the skill directs the agent to use `context_correct` to supersede it instead of creating a duplicate.

**Supersession flow**: If updating an existing pattern:
```
mcp__unimatrix__context_correct(original_id: {old ID}, content: "{updated content}", reason: "Updated: {reason}")
```

**Category and tags**:
- Category: `pattern` (default) or `convention` (if agent specifies)
- Tags: must include at least one domain tag; `feature_cycle` tag recommended for retro traceability (SR-06)

**Decision rule** (addresses SR-04): The skill includes guidance distinguishing `/store-pattern` from `/store-lesson`:
- If the finding was triggered by a failure and the takeaway is "don't do X": use `/store-lesson`
- If the finding is a reusable solution generalizable regardless of failure context: use `/store-pattern`

### FR-07: Retro Stewardship Quality Pass

Add a stewardship review step to the `/retro` skill between Phase 1 (data gathering) and Phase 2 (pattern extraction). This becomes Phase 1b.

**Phase 1b: Stewardship Quality Review**:

1. Query all entries stored during the feature cycle:
   ```
   mcp__unimatrix__context_search(query: "{feature-id}", k: 20)
   ```
   Filter results to entries with `feature_cycle` matching the feature ID or entries stored by agents with agent_id matching feature agent IDs.

2. For each entry found, assess quality against the what/why/scope template:
   - Does the `content` field include a clear "what" (the pattern)?
   - Does the `content` field include a clear "why" (the consequence)?
   - Does the `content` field include a clear "scope" (applicability)?
   - Is the entry properly categorized (pattern vs lesson vs procedure)?

3. Actions:
   - **Low quality** (missing why, vague what, wrong category): deprecate via `context_deprecate` with reason "Quality review: {specific issue}".
   - **High quality** (clear what/why/scope, correct category): note as validated. No confidence boost action (tracked in #199).
   - **Miscategorized** (e.g., lesson stored as pattern): supersede via `context_correct` with correct category.

4. Report stewardship quality findings in the retrospective summary:
   ```
   Stewardship review:
   - Entries assessed: {N}
   - Validated: {N}
   - Deprecated (low quality): {N}
   - Recategorized: {N}
   ```

### FR-08: Bugfix Protocol Stewardship

Add stewardship requirements to the bugfix protocol:

**FR-08a: Investigator stewardship** -- After diagnosis, the bug-investigator must store generalizable root cause patterns via `/store-lesson`. The investigator's GH Issue comment (diagnosis report) must include a Stewardship section.

**FR-08b: Rust-dev stewardship** -- After implementing the fix, the rust-dev must store implementation patterns discovered during the fix via `/store-pattern`. The agent report must include a Stewardship section.

**FR-08c: Causal feature linkage** -- The bugfix protocol must instruct agents to identify the feature that introduced the bug and tag stored entries with:
- `tags: ["bugfix", "{causal-feature-id}"]` where `{causal-feature-id}` is the feature whose implementation caused the bug.
- The investigator's diagnosis report must include a "Causal Feature" field identifying which feature introduced the defect. This enables the retro quality pass to connect bugfix lessons back to the originating feature's knowledge gaps.

**FR-08d: Validator bugfix gate** -- The bugfix validation gate check set must include a stewardship compliance check. The validator verifies that investigator and rust-dev agent reports include Stewardship sections with at least one `Stored` or `Declined` row.

### FR-09: Uni-Rust-Dev Stewardship (High-Value Target)

The `uni-rust-dev` stewardship section requires specific guidance because implementation gotchas are the most frequently lost knowledge:

- **When to store**: After encountering a non-obvious behavior, constraint, or trap during implementation that is not apparent from reading the source code.
- **Topic convention**: Use the crate name (e.g., `unimatrix-store`, `unimatrix-server`).
- **Content template**: what/why/scope enforced by `/store-pattern`.
- **Examples** (in the agent definition):
  - "Don't hold `lock_conn()` across await points -- deadlocks under concurrent requests" (topic: `unimatrix-store`)
  - "bincode v2 requires `serde(default)` on new fields or migration breaks silently" (topic: `unimatrix-store`)
- **Query before implementing**: The existing `/query-patterns` step is already in uni-rust-dev. No change needed.

### FR-10: Uni-Pseudocode Read-Only Stewardship

The `uni-pseudocode` agent stores no patterns. Its stewardship section:
- Confirms it queries `/query-patterns` before designing (already in its MANDATORY section).
- Notes in its report whether established patterns were followed or deviated from, with rationale.
- Stewardship report row: `Queried` with results summary. If deviation from an established pattern was necessary, a second row: `Declined` with rationale for deviation.

### FR-11: Uni-Tester Stewardship

The `uni-tester` agent stores test infrastructure patterns:
- **When to store**: New test fixtures, integration test techniques, or testing procedures discovered during test plan design or execution.
- **Topic convention**: `testing` for cross-cutting patterns, crate name for crate-specific test patterns.
- **Query before designing**: The existing `/knowledge-search` step is already in uni-tester. No change needed.

## Non-Functional Requirements

### NFR-01: Context Window Budget

Each agent's Knowledge Stewardship section must not exceed 15 lines of guidance text (excluding the self-check item). This limit mitigates SR-01 (context window bloat). The skill enforces structure and quality, not the agent definition.

Measurement: line count of the Knowledge Stewardship section in each agent `.md` file, excluding section heading and self-check items.

### NFR-02: Validator Parsing Reliability

The stewardship report format (FR-04) must be deterministically parseable by the validator without ambiguity. The parsing contract is:
- Heading: exactly `## Stewardship`
- Table: standard markdown table with `Action` and `Detail` columns
- Action values: exactly one of `Stored`, `Queried`, `Updated`, `Declined`

The validator does not need to call Unimatrix APIs to verify stewardship compliance. All evidence is in the agent report text (Constraint 6 from SCOPE.md).

### NFR-03: Backward Compatibility

Agent definitions must remain valid for agents that are mid-session when changes deploy. Changes are section additions only -- no structural changes to existing sections, no removal of existing content, no format changes to existing fields.

### NFR-04: Atomic Deployment

All agent definition changes should be committed in a single commit to avoid inconsistent stewardship expectations across agents during incremental deployment (SR-08).

## Acceptance Criteria

| AC-ID | Description | Verification Method | Verification Detail |
|-------|-------------|--------------------|--------------------|
| AC-01 | Every agent definition in `.claude/agents/uni/` has a Knowledge Stewardship section that specifies: what to store (or "no storage expected" with rationale), which category and topic convention to use, and which skill to invoke. | grep + file-check | Grep all `.md` files in `.claude/agents/uni/` for `## Knowledge Stewardship` or `### Knowledge Stewardship`. Every agent file must match. |
| AC-02 | Every agent definition with a Knowledge Stewardship section includes at least one stewardship-related item in its Self-Check list. | grep | Grep each agent `.md` for self-check items containing stewardship-related keywords (`store`, `pattern`, `stewardship`, `query-patterns`, `knowledge`). |
| AC-03 | The `uni-rust-dev` agent definition includes stewardship guidance with crate-as-topic convention and what/why/scope content template for implementation patterns. | file-check | Read `uni-rust-dev.md`, verify Knowledge Stewardship section references `/store-pattern`, crate name as topic, and includes example entries. |
| AC-04 | The `uni-validator` agent definition includes stewardship compliance checks in Gate 3a, Gate 3b, and Gate 3c check sets. | grep | Grep `uni-validator.md` for stewardship-related check items in each gate section. Each gate must have at least one stewardship check. |
| AC-05 | The `/retro` skill includes a stewardship quality pass step that queries entries stored during the feature cycle, assesses quality, and deprecates or promotes entries. | file-check | Read `.claude/skills/retro/SKILL.md`, verify a stewardship quality review phase exists with query, assessment, and deprecation steps. |
| AC-06 | A `/store-pattern` skill exists at `.claude/skills/store-pattern/SKILL.md` with required fields (topic, what, why, scope) and validation that rejects entries missing the "why" field. | file-check | Verify file exists. Read content, confirm required fields listed, `why` validation documented, content assembly format specified, deduplication check included. |
| AC-07 | Agent definitions that legitimately produce no generalizable knowledge (uni-synthesizer, uni-pseudocode) have a stewardship section that explicitly states "no storage expected" with rationale. | file-check | Read `uni-synthesizer.md` and `uni-pseudocode.md`, verify stewardship section contains "no storage expected" or equivalent with rationale. |
| AC-08 | The uni-pseudocode agent definition includes stewardship guidance for querying patterns before designing and noting deviations from established patterns. | file-check | Read `uni-pseudocode.md`, verify stewardship section references querying `/query-patterns` and reporting deviations in report. |
| AC-09 | The uni-tester agent definition includes stewardship guidance for storing new test infrastructure patterns and querying procedures before test plan design. | file-check | Read `uni-tester.md`, verify stewardship section references `/store-pattern` for test patterns and the existing query step. |

## Domain Models

### Entry

A knowledge record in Unimatrix. Fields relevant to stewardship:
- `title`: concise description
- `content`: structured text (what/why/scope for patterns; what-happened/root-cause/takeaway for lessons)
- `topic`: organizational grouping (crate name, domain area, or role name)
- `category`: one of `pattern`, `convention`, `procedure`, `lesson-learned`, `decision`
- `tags`: metadata array including domain, feature_cycle, consuming roles
- `agent_id`: the agent role that stored the entry

### Knowledge Stewardship

The practice of storing, querying, updating, and curating knowledge entries. Has three enforcement layers:
1. **Agent guidance** -- tells agents what and how to store
2. **Gate checks** -- validator enforces that agents made a stewardship decision
3. **Retro curation** -- quality review of entries stored during a feature cycle

### Stewardship Report Block

A structured markdown table in an agent's report that documents stewardship actions taken. Machine-parseable by the validator via heading match and action keyword matching.

### Content Template (What/Why/Scope)

The quality standard for pattern entries:
- **What**: the pattern itself, in one sentence
- **Why**: what goes wrong without following the pattern
- **Scope**: where the pattern applies (crate, module, context)

### Causal Feature Linkage

The practice of tagging bugfix knowledge entries with the feature ID that introduced the bug, enabling retroactive knowledge gap analysis.

## User Workflows

### Workflow 1: Agent Stores a Pattern

1. Agent encounters a non-obvious behavior during work.
2. Agent invokes `/store-pattern` with topic, what, why, scope fields.
3. Skill checks for existing patterns in the same topic (dedup).
4. If no duplicate: skill assembles content and calls `context_store`.
5. Agent records stored entry ID in report Stewardship table.

### Workflow 2: Agent Declines to Store

1. Agent completes work with no novel findings.
2. Agent adds a `Declined` row to report Stewardship table with rationale.
3. Validator reads the Stewardship section and accepts the explicit decline.

### Workflow 3: Validator Checks Stewardship

1. Validator reads agent reports for the gate being validated.
2. For each relevant agent, validator finds `## Stewardship` heading.
3. Validator reads the table and verifies at least one valid Action row.
4. If section is missing or empty: REWORKABLE FAIL.

### Workflow 4: Retro Quality Review

1. Retro skill queries entries stored during the feature cycle.
2. For each entry, assesses against what/why/scope template.
3. Deprecates low-quality entries, notes validated entries.
4. Reports quality summary in retrospective output.

### Workflow 5: Bugfix Stewardship with Causal Linkage

1. Investigator diagnoses root cause and identifies causal feature.
2. Investigator stores lesson via `/store-lesson` with `tags: ["bugfix", "{causal-feature-id}"]`.
3. Rust-dev implements fix and stores any implementation patterns.
4. Bugfix validator checks stewardship compliance in agent reports.

## Constraints

1. **File-only changes**: All modifications are to `.claude/agents/uni/*.md`, `.claude/skills/*/SKILL.md`, `.claude/skills/retro/SKILL.md`, and `.claude/protocols/uni/uni-bugfix-protocol.md`. No Rust code, no Cargo.toml, no schema changes.
2. **Backward compatibility**: Agent definitions must remain valid for mid-session agents. Only section additions; no structural changes to existing sections.
3. **Self-check format**: New self-check items use the existing `- [ ] {statement}` format.
4. **Skill structure**: `/store-pattern` follows the same directory and SKILL.md conventions as `/store-lesson` and `/store-procedure`.
5. **Validator check format**: New gate checks follow the existing numbered check format.
6. **Agent report evidence**: Stewardship compliance verifiable from agent reports without Unimatrix API calls. Agents include entry IDs or explicit decline in structured Stewardship section.
7. **Existing MCP tools**: No changes to `context_store`, `context_search`, `context_correct`, or `context_deprecate` tool signatures.

## Dependencies

- Existing Unimatrix MCP tools: `context_store`, `context_search`, `context_correct`, `context_deprecate`
- Existing skills: `/store-lesson`, `/store-procedure`, `/store-adr`, `/query-patterns`
- Agent definitions: all 12 files in `.claude/agents/uni/`
- Retro skill: `.claude/skills/retro/SKILL.md`
- Bugfix protocol: `.claude/protocols/uni/uni-bugfix-protocol.md`

## NOT in Scope

1. **No Rust code changes** -- no crate modifications, no Cargo.toml changes, no schema migrations.
2. **No deliberate retrieval confidence boost** -- tracked separately in #199.
3. **No MCP tool signature changes** -- existing tools are sufficient.
4. **No automated storage** -- agents decide what to store; the system enforces that they make a decision.
5. **No CLAUDE.md changes** -- already references stewardship skills.
6. **No uni-init changes** -- bootstrap agent is a one-time tool, not a per-feature participant.
7. **No advisory/warn-only rollout period** -- stewardship checks are REWORKABLE FAIL from the start. The quality bar is low (store or explicitly decline); agents that cannot meet it have a genuine gap.
8. **No auto-injection of feature_cycle tags** -- agents are responsible for including tags; the skill recommends but does not enforce feature_cycle tagging.
