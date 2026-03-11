# Risk-Based Test Strategy: base-004

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Validator stewardship check parses `## Knowledge Stewardship` heading but agents write `## Stewardship` (or vice versa) -- heading mismatch causes false FAIL | High | High | Critical |
| R-02 | Agent report stewardship block omits required bullet prefix (`Stored:`, `Queried:`, `Declined:`) or uses variant casing/punctuation, causing validator to miss valid compliance | High | Med | High |
| R-03 | /store-pattern skill `why` validation threshold (10 chars) is too low to prevent noise entries or too high for legitimate terse patterns | Med | Med | Medium |
| R-04 | Active-storage agent classified as read-only (or vice versa) in validator gate check -- wrong tier expectation causes false PASS or false FAIL | High | Low | Medium |
| R-05 | Retro quality pass query misses entries stored during feature cycle because agents omitted `feature_cycle` tag (tag is recommended, not enforced) | Med | High | High |
| R-06 | Stewardship section exceeds 15-line NFR-01 budget for one or more agents, consuming context window tokens needed for the agent's primary task | Med | Med | Medium |
| R-07 | /store-pattern dedup check returns false positive (semantic match on unrelated pattern), causing agent to supersede a valid entry | Med | Low | Low |
| R-08 | Bugfix protocol causal linkage uses incorrect originating feature ID, creating misleading knowledge trail | Low | Med | Low |
| R-09 | Gate 3a composite check references architect agent but architect already has its own stewardship -- double enforcement or inconsistent expectations | Med | Med | Medium |
| R-10 | Specification agent listed as read-only in ADR-001 but FR-02 table says it stores `convention` entries via `/store-pattern` -- contradictory tier assignment | High | High | Critical |
| R-11 | Self-check items added to agent definitions are too generic ("stewardship complete") instead of actionable, providing no verification value | Low | Med | Low |
| R-12 | Retro quality pass deprecates entries that agents stored correctly, eroding agent trust in the stewardship system | Med | Med | Medium |

## Risk-to-Scenario Mapping

### R-01: Heading Mismatch Between Agent Report and Validator Parsing
**Severity**: High
**Likelihood**: High
**Impact**: Every agent report that uses the wrong heading will be flagged as REWORKABLE FAIL, blocking delivery. The Architecture (C2) uses `## Knowledge Stewardship` while the Specification FR-04 uses `## Stewardship`. This inconsistency in the design documents themselves is the root cause.

**Test Scenarios**:
1. Verify that all agent definition stewardship sections reference the SAME heading string for the report block.
2. Verify that the validator check description uses the SAME heading string as agent definitions.
3. Verify that the Architecture C2 heading and Specification FR-04 heading are consistent (currently they differ: `## Knowledge Stewardship` vs `## Stewardship`).

**Coverage Requirement**: Single grep across all modified files for the stewardship heading; every occurrence must use the identical string.

### R-02: Bullet Prefix Format Inconsistency
**Severity**: High
**Likelihood**: Med
**Impact**: Agents that comply with stewardship but use slightly different formatting (e.g., `Stored -` instead of `Stored:`, or `- stored:` lowercase) will fail validation.

**Test Scenarios**:
1. Verify that agent definitions show the exact bullet prefix format (e.g., `- Stored:` not `Stored:`).
2. Verify that the validator check description specifies exact matching rules (case-sensitive? prefix-only?).
3. Verify that the Architecture C2 format and the Specification FR-04 format show identical bullet/table syntax.

**Coverage Requirement**: Cross-reference the report format shown in each agent definition against the validator's parsing contract.

### R-03: /store-pattern Why Validation Threshold
**Severity**: Med
**Likelihood**: Med
**Impact**: Too-low threshold admits noise ("because bad"); too-high threshold rejects valid terse patterns.

**Test Scenarios**:
1. Verify the skill file specifies the 10-character minimum for `why` field.
2. Verify the skill file specifies the 200-character maximum for `what` field.
3. Check that the skill includes at least one example entry that demonstrates passing validation.

**Coverage Requirement**: Skill file content check for validation rules and examples.

### R-04: Agent Tier Misclassification in Validator
**Severity**: High
**Likelihood**: Low
**Impact**: Wrong tier means wrong validation -- an active agent treated as read-only would pass without storing anything.

**Test Scenarios**:
1. Cross-reference the agent tier table in Architecture (C1) against each gate check's agent list in the validator definition.
2. Verify that Gate 3a lists the correct agents (architect, risk-strategist, pseudocode) with correct tier expectations.
3. Verify that Gate 3b lists the correct agents (rust-dev, vision-guardian) with correct tier expectations.
4. Verify that Gate 3c lists the correct agents (tester) with correct tier expectations.

**Coverage Requirement**: Every agent in the tier table must appear in exactly one gate's check, with the correct tier expectation.

### R-05: Retro Quality Pass Misses Entries Due to Missing Tags
**Severity**: Med
**Likelihood**: High
**Impact**: Entries stored during a feature cycle but not tagged with `feature_cycle` are invisible to the retro quality pass. Quality review becomes incomplete, and low-quality entries persist.

**Test Scenarios**:
1. Verify that the /store-pattern skill explicitly instructs agents to include `feature_cycle` as a tag.
2. Verify that the retro quality pass query uses a method that can catch entries even without `feature_cycle` tags (e.g., searching by feature ID in content or title, not only by tag).
3. Verify consistency between the retro query method in the Architecture (C5) and the Specification (FR-07).

**Coverage Requirement**: Check that the retro skill's query strategy has a fallback beyond tag-only filtering.

### R-06: Stewardship Section Line Count Exceeds Budget
**Severity**: Med
**Likelihood**: Med
**Impact**: Agent context windows are consumed by stewardship instructions, leaving less room for the agent's primary task.

**Test Scenarios**:
1. Count lines in each agent definition's Knowledge Stewardship section (excluding heading and self-check items).
2. Verify active-storage agents are at 10-15 lines, read-only at 6-8 lines, exempt at 2 lines (per ADR-001).

**Coverage Requirement**: Line count check on every agent definition's stewardship section.

### R-07: /store-pattern Dedup False Positive
**Severity**: Med
**Likelihood**: Low
**Impact**: Agent supersedes a valid existing pattern, destroying knowledge.

**Test Scenarios**:
1. Verify the skill instructs agents to review search results before superseding (human-in-the-loop, not automatic).
2. Verify the dedup check uses both topic and category filters, not just content similarity.

**Coverage Requirement**: Skill file content check for dedup workflow.

### R-08: Incorrect Causal Feature Attribution
**Severity**: Low
**Likelihood**: Med
**Impact**: Misleading knowledge trail; retro may flag the wrong feature for design review.

**Test Scenarios**:
1. Verify the bug-investigator stewardship section includes guidance on identifying the originating feature.
2. Verify the `caused_by_feature` tag is optional (not enforced by validator), as stated in ADR-005.

**Coverage Requirement**: Bug-investigator agent def and bugfix protocol content checks.

### R-09: Architect Double Enforcement
**Severity**: Med
**Likelihood**: Med
**Impact**: Architect already has comprehensive stewardship. Gate 3a composite check may impose conflicting expectations or redundant validation.

**Test Scenarios**:
1. Verify the Gate 3a check acknowledges the architect's existing stewardship format and does not impose the new format on top of it.
2. Verify there is no conflict between the architect's existing `## Knowledge Stewardship` section and the new report block format.

**Coverage Requirement**: Compare architect's existing stewardship section against the new structured block format.

### R-10: Specification Agent Tier Contradiction
**Severity**: High
**Likelihood**: High
**Impact**: Architecture ADR-001 classifies `uni-specification` as read-only tier. Specification FR-02 table says it stores `convention` entries via `/store-pattern`. Implementers will not know which is correct, leading to either missing storage or incorrect validation.

**Test Scenarios**:
1. Check Architecture C1 tier table for `uni-specification` classification.
2. Check Specification FR-02 table for `uni-specification` storage expectations.
3. Verify the validator gate check for the spec agent matches whichever classification is authoritative.

**Coverage Requirement**: Cross-document consistency check on uni-specification's tier and storage expectations.

### R-11: Generic Self-Check Items
**Severity**: Low
**Likelihood**: Med
**Impact**: Self-check items that are too generic provide no verification value; agents check the box without actually verifying stewardship.

**Test Scenarios**:
1. Verify each agent's self-check item references a specific action (skill name, query target, or decline rationale).
2. Verify self-check items differ by agent tier (active agents reference storing, read-only reference querying).

**Coverage Requirement**: Content review of self-check items across all agent definitions.

### R-12: Retro Deprecates Valid Entries
**Severity**: Med
**Likelihood**: Med
**Impact**: If the retro quality pass applies the what/why/scope template too strictly, it may deprecate entries that were stored correctly by `/store-lesson` (which uses a different template: what-happened/root-cause/takeaway).

**Test Scenarios**:
1. Verify the retro quality pass applies category-appropriate templates (what/why/scope for patterns, what-happened/root-cause/takeaway for lessons, numbered steps for procedures).
2. Verify the retro does not apply the pattern template to non-pattern entries.

**Coverage Requirement**: Retro skill content check for category-aware quality assessment.

## Integration Risks

### Heading Contract Between C1, C2, and C3
The agent definition (C1) tells agents what heading to use in reports. The report format (C2) defines the heading. The validator (C3) parses the heading. All three must agree on the exact string. Current documents show inconsistency: Architecture uses `## Knowledge Stewardship`, Specification FR-04 uses `## Stewardship`. This is the highest-priority integration risk.

### Tier Classification Across Documents
The tier table in Architecture C1, the FR-02 table in the Specification, and the gate check descriptions in C3/FR-05 must all agree on which agents are active-storage, read-only, or exempt. The uni-specification agent is currently classified differently between Architecture (read-only) and Specification (active-storage with `convention` category).

### Skill-to-Agent Guidance Alignment
Each agent definition references a skill (e.g., `/store-pattern`). The skill must exist, accept the parameters the agent is told to provide, and produce output compatible with the report format. The /store-pattern skill must return an entry ID that agents can include in their `Stored:` line.

### Retro Phase Numbering
Architecture C5 inserts the quality pass between Phase 2 and Phase 3 (calling it "Phase 2b"). Specification FR-07 inserts it between Phase 1 and Phase 2 (calling it "Phase 1b"). Different insertion points affect what data is available to the quality pass.

## Edge Cases

1. **Agent stores zero entries legitimately**: Active-storage agent encounters nothing novel. Must write `Declined` with rationale. Validator must accept this as compliant.
2. **Multiple rust-dev agents in Gate 3b**: Composite check must verify ALL rust-dev reports, not just the first one found.
3. **Vision guardian not spawned**: Gate 3b check references vision-guardian "if spawned." Validator must handle the case where no vision-guardian report exists without failing.
4. **Agent report has stewardship heading but empty table**: Heading present, no rows. This should be REWORKABLE FAIL per FR-04, not PASS.
5. **Bugfix session with no identifiable causal feature**: `caused_by_feature` tag is optional. Investigator should be able to omit it without stewardship failure.
6. **Dedup check in /store-pattern finds similar but not identical pattern**: Agent must decide whether to supersede or create new. Skill should provide guidance, not auto-decide.

## Security Risks

This feature modifies markdown files only (agent definitions, skills, protocols). No untrusted external input is accepted, no executable code is added, no data model changes occur.

- **Input surface**: None. All modified files are developer-authored markdown consumed by Claude agents. No user-facing input, no network input, no file path construction from external data.
- **Blast radius**: If a malformed agent definition is committed, the worst case is agents receiving incorrect stewardship guidance. This would cause knowledge quality issues, not security issues.
- **Injection risk**: None. Skills instruct agents to call existing MCP tools with agent-determined parameters. No parameter concatenation, no shell execution, no path traversal.

Security risks are minimal for this workflow-only feature. No security-specific test scenarios required.

## Failure Modes

1. **Agent does not include stewardship block**: Validator issues REWORKABLE FAIL. Agent must re-run and add the block. Expected behavior -- this is the enforcement mechanism.
2. **Agent includes stewardship block with wrong heading**: Validator issues REWORKABLE FAIL (same as missing). Agent must fix the heading. This is a documentation/training problem, not a system failure.
3. **Retro quality pass finds no entries for a feature cycle**: This is valid if all agents declined storage. Retro should report "0 entries assessed" without error.
4. **/store-pattern rejects entry for short `why`**: Agent must provide a better `why` and retry. The skill should give a clear error message indicating the minimum length requirement.
5. **Validator cannot find agent reports**: Gate check fails with an error indicating missing reports. This is an existing failure mode unrelated to stewardship.

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (context window bloat) | R-06 | ADR-001 defines three tiers with line budgets: 10-15 (active), 6-8 (read-only), 2 (exempt). Quality enforcement pushed to skills. |
| SR-02 (brittle validator parsing) | R-01, R-02 | ADR-002 defines structured block with fixed heading and bullet prefixes. However, heading inconsistency between Architecture and Specification creates the exact risk SR-02 warned about. |
| SR-03 (CLAUDE.md discoverability) | -- | Architecture acknowledges this in Open Question #3. Recommends relaxing the "no CLAUDE.md changes" constraint. Not an architecture-level risk. |
| SR-04 (pattern vs lesson ambiguity) | R-07 | ADR-004 includes decision rule in both /store-pattern and /store-lesson documentation. |
| SR-05 (auto-extraction duplication) | -- | Not addressed in architecture. Accepted risk -- agent entries and auto-extracted entries coexist. Low severity per scope assessment. |
| SR-06 (feature_cycle tag inconsistency) | R-05 | ADR-004 puts `feature_cycle` tag instruction in the skill. But tag is recommended, not enforced. Retro query may miss untagged entries. |
| SR-07 (adoption friction) | R-01, R-02 | ADR-003 graduates enforcement: FAIL for missing block, WARN for thin content. Reduces friction while maintaining minimum bar. |
| SR-08 (incremental deployment) | -- | Specification NFR-04 recommends atomic deployment (single commit). Addressed at deployment level, not architecture level. |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 2 (R-01, R-10) | 6 scenarios |
| High | 2 (R-02, R-05) | 5 scenarios |
| Medium | 5 (R-03, R-04, R-06, R-09, R-12) | 10 scenarios |
| Low | 3 (R-07, R-08, R-11) | 5 scenarios |
