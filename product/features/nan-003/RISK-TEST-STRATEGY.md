# Risk-Based Test Strategy: nan-003 (Unimatrix Onboarding Skills)

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Model advances through seed levels without respecting STOP gates | High | High | Critical |
| R-02 | Quality gate (What/Why/Scope) not enforced — low-quality entries presented to human | High | Med | High |
| R-03 | Wrong category assigned to seed entry (e.g., `decision` or `outcome` stored) | High | Med | High |
| R-04 | Sentinel missed on large CLAUDE.md — double-initialization occurs | Med | Med | Medium |
| R-05 | MCP server unavailable mid-session after pre-flight passes | Med | Med | Medium |
| R-06 | Dry-run mode violated — CLAUDE.md written when `--dry-run` was passed | Med | Med | Medium |
| R-07 | Depth limit bypassed — skill offers or proceeds to Level 3 | Med | Low | Medium |
| R-08 | Batch vs individual approval mode inverted — batch at L1+, or per-entry at L0 | Med | Low | Medium |
| R-09 | Pre-flight false success — `context_status` returns healthy but `context_store` fails | Med | Low | Low |
| R-10 | Near-duplicate seed entries stored on re-run (server dedup doesn't catch them) | Low | Med | Low |
| R-11 | Agent scan false negatives — missing patterns not flagged, or non-patterns flagged | Low | Med | Low |
| R-12 | CLAUDE.md corrupted or truncated by append — existing content lost | Med | Low | Low |
| R-13 | Prerequisites gap leaves user stranded — no guided path to MCP wiring | Low | High | Low |

---

## Risk-to-Scenario Mapping

### R-01: Model Advances Through Seed Levels Without Respecting STOP Gates

**Severity**: High
**Likelihood**: High
**Impact**: Replicates the uni-init prototype failure (67 auto-generated entries, all deprecated). Human loses control of what gets stored; low-quality entries pollute the knowledge base. This is the primary differentiator between `/unimatrix-seed` and the failed prototype.

**Test Scenarios**:
1. Run `/unimatrix-seed` on a test repo; at GATE_0 provide no explicit "yes, go deeper" response — verify the skill stays at GATE_0 and does not auto-advance to Level 1.
2. At GATE_0, respond with "yes"; verify skill presents Level 1 menu and stops. Do not respond to the menu — verify skill does not begin Level 1 exploration autonomously.
3. Complete Level 1; at GATE_1, respond "no deeper" — verify skill transitions to DONE state and produces summary, not Level 2 exploration.
4. Complete Level 2 (GATE_2); verify no Level 3 menu is presented and session ends cleanly.

**Coverage Requirement**: Every state transition in the seed state machine (PREFLIGHT→EXISTING_CHECK→L0→GATE_0→L1→GATE_1→L2→GATE_2→DONE) must have a verified test showing the model pauses at each gate. The skill instruction STOP phrasing must be confirmed present in the delivered SKILL.md.

---

### R-02: Quality Gate Not Enforced — Low-Quality Entries Presented to Human

**Severity**: High
**Likelihood**: Med
**Impact**: Human is presented with tautological, missing-field, or over-long entries. The quality floor collapses — the same failure mode as the uni-init prototype but delayed to human-review stage rather than automatic storage.

**Test Scenarios**:
1. Invoke `/unimatrix-seed` on a repo where README contains only a project name (no `why` extractable) — verify the skill either proposes ≥1 high-quality entry using other sources or proposes 0 entries and explains why, NOT tautological entries.
2. Manually review each proposed entry in a test run for: `what` ≤ 200 chars, `why` ≥ 10 chars and non-tautological, `scope` present and non-empty.
3. Verify the skill's SKILL.md instruction explicitly states the What/Why/Scope gate with discard rule (not "ask human to fix") for failing entries.

**Coverage Requirement**: Quality gate must be documented in SKILL.md as a hard discard, not a suggestion. At least one test scenario with a sparse repo must validate that 0 entries are proposed rather than low-quality ones.

---

### R-03: Wrong Category Assigned to Seed Entry

**Severity**: High
**Likelihood**: Med
**Impact**: `decision` or `outcome` entries stored during seeding will be superseded immediately by real feature work, polluting the knowledge base and degrading search relevance. `lesson-learned` entries without actual failures are meaningless.

**Test Scenarios**:
1. Inspect all proposed entries in a test seed run — verify every entry has category in `{convention, pattern, procedure}` only.
2. Introduce a README section describing an architectural decision (e.g., "We chose PostgreSQL because...") — verify the skill does NOT propose a `decision` category entry; it must propose `convention` or `pattern` if anything.
3. Verify the skill's SKILL.md instruction explicitly lists excluded categories (`decision`, `outcome`, `lesson-learned`) with rationale.

**Coverage Requirement**: Category restriction must be verified in SKILL.md instruction text AND validated via inspection of all entries proposed in at least two test repo scenarios.

---

### R-04: Sentinel Missed on Large CLAUDE.md — Double-Initialization

**Severity**: Med
**Likelihood**: Med
**Impact**: CLAUDE.md receives a second Unimatrix block. The file now contains duplicate sections, causing confusion about which block is authoritative and breaking the `--update` path (ADR-002).

**Test Scenarios**:
1. Create a CLAUDE.md with the sentinel marker buried after 250 lines of content — run `/unimatrix-init`; verify "already initialized" is printed and no block is appended.
2. Create a CLAUDE.md with the sentinel at the very end (line 300+) — run `/unimatrix-init`; verify idempotency.
3. Run `/unimatrix-init` twice on a fresh repo — diff CLAUDE.md after first and second run; verify second run produces no diff.
4. Verify the SKILL.md instruction includes the head-check fallback: explicit instruction to check both start AND last 30 lines for large files (ADR-002).

**Coverage Requirement**: All three sentinel locations (beginning, middle, end of file) must be covered. File size threshold (>200 lines) must be documented in SKILL.md and tested.

---

### R-05: MCP Server Unavailable Mid-Session After Pre-Flight Passes

**Severity**: Med
**Likelihood**: Med
**Impact**: User completes Level 0 exploration and approves entries. `context_store` calls fail during storage. Entries are lost; user may not know what was stored vs. lost. Partial state is worse than no state.

**Test Scenarios**:
1. Verify the SKILL.md instruction includes explicit error handling for `context_store` failure: the skill must report which entries succeeded and which failed, not silently discard failures.
2. Verify pre-flight `context_status` call is the first action in `/unimatrix-seed` (before any file reads) — confirmed in SKILL.md instruction ordering.

**Coverage Requirement**: SKILL.md must document the MCP failure path explicitly. Pre-flight-first ordering must be verifiable by reading the SKILL.md instruction sequence.

---

### R-06: Dry-Run Mode Violated — CLAUDE.md Written When `--dry-run` Passed

**Severity**: Med
**Likelihood**: Med
**Impact**: The dry-run contract is broken. User reviewing changes before committing them receives false safety — the file was modified without consent.

**Test Scenarios**:
1. Run `/unimatrix-init --dry-run` on a repo without the sentinel — capture all terminal output; verify CLAUDE.md is unchanged after the run (file timestamp and content identical).
2. Run `/unimatrix-init --dry-run` on a repo without CLAUDE.md — verify CLAUDE.md was NOT created.
3. Verify the skill output during dry-run includes the text of what WOULD be written (allowing developer to review).

**Coverage Requirement**: Dry-run must be tested on both "CLAUDE.md exists" and "CLAUDE.md absent" paths. File state must be verifiably unchanged.

---

### R-07: Depth Limit Bypassed — Skill Offers or Proceeds to Level 3

**Severity**: Med
**Likelihood**: Low
**Impact**: Unbounded exploration; user ends up in an open-ended session rather than the known-length conversation the architecture contracts. Replicates the prototype's unbounded extraction failure.

**Test Scenarios**:
1. Complete a full Level 0 → Level 1 → Level 2 run — verify at GATE_2 the skill presents only a DONE summary, not a "continue to Level 3?" prompt.
2. Verify the SKILL.md instruction contains explicit text: "Level 2 is the final level. No further exploration is available after Level 2."

**Coverage Requirement**: Level 2 terminal state must be verified in SKILL.md instruction text and confirmed via a full run through all levels.

---

### R-08: Batch vs Individual Approval Mode Inverted

**Severity**: Med
**Likelihood**: Low
**Impact**: Level 0 uses per-entry approval (low risk → extra friction with no benefit) or Level 1+ uses batch approval (high stakes → individual accountability lost). The core quality control mechanism breaks.

**Test Scenarios**:
1. Run through Level 0 — verify all proposed entries are shown together with a single approve/reject prompt (batch), not individually.
2. Run through Level 1 — verify each proposed entry is presented separately with an individual approve/reject prompt.
3. Verify the SKILL.md instruction distinguishes batch (L0) from individual (L1+) approval with explicit language.

**Coverage Requirement**: Approval mode must be verified at both L0 and L1 in a test run. SKILL.md instruction must be readable to a reviewer confirming the mode per level.

---

### R-09: Pre-Flight False Success

**Severity**: Med
**Likelihood**: Low
**Impact**: `context_status` returns a healthy response but the server is in a degraded state (e.g., database locked, embedding pipeline stalled). Subsequent `context_store` calls fail after exploration is complete.

**Test Scenarios**:
1. Verify `context_status` response is non-empty and does not contain error indicators before the skill proceeds — SKILL.md instruction must check for an error-free status response, not just that the call completed.

**Coverage Requirement**: SKILL.md must document that a successful `context_status` response (not just non-failure) is required to proceed.

---

### R-10: Near-Duplicate Seed Entries on Re-Run

**Severity**: Low
**Likelihood**: Med
**Impact**: Second seeding run produces slightly-rephrased versions of existing entries. 0.92 cosine dedup misses them. Retrieval quality degrades as near-duplicates dilute rankings.

**Test Scenarios**:
1. Seed a repo; run `/unimatrix-seed` again; verify the existing-entries warning appears before Level 0 stores — not after.
2. Verify the warning threshold (≥3 active entries in convention/pattern/procedure) is documented in SKILL.md and the supplement vs. skip choice is presented to the human.

**Coverage Requirement**: Re-run warning flow must be exercised. Warning must appear before any entries are proposed or stored.

---

### R-11: Agent Scan False Negatives or False Positives

**Severity**: Low
**Likelihood**: Med
**Impact**: Agent files with `context_briefing` in a comment block are flagged as missing it; or agents that reference the skill by variable are not flagged. Recommendation report is misleading.

**Test Scenarios**:
1. Run `/unimatrix-init` on the Unimatrix repo itself (which has well-wired agents) — verify the report correctly identifies fully-wired agents vs. agents missing patterns.
2. Run on a repo with zero `.claude/agents/` files — verify "no agents found" note rather than an error or empty table.

**Coverage Requirement**: Both "agents present" and "no agents" cases must be tested.

---

### R-12: CLAUDE.md Content Corrupted by Append

**Severity**: Med
**Likelihood**: Low
**Impact**: Existing CLAUDE.md content is truncated, overwritten, or duplicated. Developer loses project instructions established before onboarding.

**Test Scenarios**:
1. Run `/unimatrix-init` on a repo with a large existing CLAUDE.md (500+ lines) — verify all pre-existing content is preserved and only the Unimatrix block was appended.
2. Verify the block is appended at the end, not inserted in the middle or at the start.

**Coverage Requirement**: Pre-existing CLAUDE.md content must be byte-for-byte preserved. Block placement (end of file) must be verified.

---

### R-13: Prerequisites Gap Leaves User Stranded

**Severity**: Low
**Likelihood**: High
**Impact**: User runs `/unimatrix-init` following documentation but has not wired the MCP server. The skill completes CLAUDE.md setup, but any future `/unimatrix-seed` attempt will fail at pre-flight with no guided path to fix the MCP wiring (nan-004 scope gap).

**Test Scenarios**:
1. Verify both SKILL.md files contain a "Prerequisites" section as the first non-frontmatter section, listing: (a) skill files copied to `.claude/skills/`, (b) MCP server wired in Claude settings.
2. Verify the prerequisites section references nan-004 or the installation documentation for MCP wiring.

**Coverage Requirement**: Prerequisites section presence verified by code review.

---

## Integration Risks

### MCP Server ↔ `/unimatrix-seed` State Machine

The seed state machine calls MCP at three points: pre-flight (`context_status`), existing-check (`context_search`), and storage (`context_store`). Failures at each point have different consequences:

- **Pre-flight failure**: Correct behavior — skill halts with error. Verify this path.
- **Existing-check failure**: Ambiguous — should the skill continue without the warning or abort? SKILL.md must specify behavior.
- **Storage failure on individual entry**: Should the skill report the failure and continue with remaining entries, or halt? Mid-batch failure is the highest-risk integration edge case.

### Claude Read/Glob ↔ Target Repo File System

The agent scan globs `.claude/agents/**/*.md` and reads each file. Edge cases:
- Symlinked agent files (potential infinite loop with glob)
- Binary files with `.md` extension
- Very large agent files causing context window pressure

The CLAUDE.md append assumes Write/Edit appends to the file end without replacing content. The skill must use Edit (append) semantics, not Write (overwrite) semantics.

### `/unimatrix-init` ↔ `/unimatrix-seed` Sequencing

The two skills are designed for sequential invocation (init first, then seed), but there is no enforcement. Running `/unimatrix-seed` before `/unimatrix-init` is valid (seed doesn't require the CLAUDE.md block to be present) and is a legal workflow. The risk is that the CLAUDE.md block is never written if the user only runs seed. This is acceptable by design but should be noted in documentation.

---

## Edge Cases

| Edge Case | Risk | Mitigation |
|-----------|------|------------|
| CLAUDE.md does not exist | `/unimatrix-init` must create it, not error | FR-03 specifies create behavior; verify |
| CLAUDE.md is a symlink | Append target may not be the intended file | Accept as known limitation; no mitigation required |
| `.claude/agents/` directory absent | Agent scan must handle gracefully | SKILL.md must instruct "if no agents found, skip scan and note" |
| README.md absent (Level 0) | Seed must continue with available manifests | SKILL.md must instruct partial-read behavior for Level 0 |
| All Level 0 entries rejected by human | Skill must halt cleanly with 0 entries stored | SKILL.md must specify DONE path on full batch rejection |
| `context_search` at existing-check returns 0 results | Clean first run — proceed to Level 0 normally | Verify no false "already seeded" warning on clean repos |
| User approves some, rejects some entries at Level 0 | Individual entries in the "batch" may need to be selectable | Level 0 batch: accept or reject the group; no partial batch — verify SKILL.md enforces this |
| Sentinel marker removed by user | Idempotency lost; next run will double-append | Accepted limitation (comment says DO NOT REMOVE); document as known edge case |
| `--dry-run` flag not recognized | Skill runs as normal init | Verify SKILL.md checks for `--dry-run` argument before Phase 1 |

---

## Security Risks

### `/unimatrix-init` — Agent File Reads

- **Untrusted input**: Agent files in `.claude/agents/**/*.md` are read and their content is scanned for pattern presence. A malicious or corrupted agent file could contain content designed to confuse the model's pattern detection (e.g., fake `context_briefing` markers, adversarial text).
- **Blast radius**: Low. The scan is read-only; output is terminal text. No file writes occur from scan results. The only write is to CLAUDE.md, which is determined by the skill template, not by agent file content.
- **Risk**: Minimal. The scan uses pattern presence (string matching), not execution. Model could be confused by adversarial content, but the output is only a recommendation report.

### `/unimatrix-init` — CLAUDE.md Write

- **Untrusted input**: None. The block template is fixed in the skill — it does not incorporate content from CLAUDE.md or agent files.
- **Blast radius**: CLAUDE.md is modified. Over-write (not append) would be a high-severity outcome. The skill must use append semantics.
- **Risk**: If the skill instruction uses Write (overwrite) instead of Edit/Append, all existing CLAUDE.md content is lost. This is R-12.

### `/unimatrix-seed` — Repository File Reads

- **Untrusted input**: README.md and package manifests are read from the target repo. A malicious README could contain content designed to extract, manipulate, or leak context (prompt injection).
- **Blast radius**: Moderate. The skill summarizes what it reads into Unimatrix entries via `context_store`. Adversarial README content could become a stored knowledge entry visible to future agents.
- **Path traversal**: Glob is limited to known files (README.md, Cargo.toml, etc.) at Level 0. Level 1+ explores module directories — paths are constrained by the directory structure Claude reads. No user-provided path parameters reduce path traversal risk.
- **Injection risk**: The `what`/`why`/`scope` quality gate filters entries. Adversarial content in README would need to pass the quality gate AND human approval to be stored. Human approval is the final defense.
- **Mitigation**: The quality gate + human approval chain provides adequate defense for the stated use case (developer-controlled repos).

### MCP Tool Calls

- **`context_store` with adversarial content**: Entries are stored as-is. No sanitization in the storage path. The quality gate is the primary defense — if it is bypassed (R-02), adversarial content could enter the knowledge base.
- **Blast radius**: Stored entries are visible to all future agents via `context_briefing` and `context_search`. A malicious entry could provide incorrect guidance to future agents.

---

## Failure Modes

| Failure | Expected Behavior | Unacceptable Behavior |
|---------|------------------|----------------------|
| `context_status` fails at pre-flight | Print clear error: "Unimatrix MCP not available. See prerequisites." and STOP | Silent fail; proceed with exploration; partial state |
| `context_store` fails during storage | Report per-entry success/failure; list which entries were stored | Silent fail; claim all entries stored; halt with no feedback |
| No agents found during scan | Print "No agents found at `.claude/agents/`" in report; continue to CLAUDE.md write | Error out; skip recommendation section without note |
| All Level 0 entries rejected | Print "0 entries stored. Re-invoke `/unimatrix-seed` with more specific guidance." and DONE | Offer to retry automatically; continue to Level 1 without approval |
| Large CLAUDE.md (>200 lines) | Perform tail check + full read; find or not-find sentinel; report accurately | Miss sentinel; double-append block |
| `--dry-run` flag passed | Print block content + recommendations; no file writes | Write CLAUDE.md; create CLAUDE.md; any file modification |
| Sentinel already present | Print "already initialized" and STOP immediately | Proceed with agent scan; re-append block |

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (Multi-turn gate state) | R-01, R-07, R-08 | ADR-001 addresses via explicit STOP gate phrasing + state machine model. Enforced in SKILL.md instruction text. Verified by manual test of each state transition. |
| SR-02 (Sentinel on large files) | R-04 | ADR-002 addresses via head-check fallback: skill explicitly checks last 30 lines when file > 200 lines. Reduces but does not eliminate risk for adversarially placed sentinel. |
| SR-03 (Model instruction fidelity) | R-01, R-02, R-03, R-06, R-07, R-08 | Accepted platform constraint. Mitigated by explicit STOP gate phrasing (ADR-001), quality gate enforcement (ADR-006), and per-level approval mechanics. All verification is manual. |
| SR-04 (Bootstrap paradox) | R-13 | Architecture mandates Prerequisites section in both SKILL.md files (ARCHITECTURE.md open question resolution). Addresses documentation gap; does not solve the install requirement itself (nan-004 scope). |
| SR-05 (uni-init name collision) | — | Addressed in specification (FR-07, AC-12): disambiguation notice required in SKILL.md. Not an architecture-level risk; no R-XX assigned. |
| SR-06 (MCP failure mid-session) | R-05, R-09 | ADR-003 addresses via pre-flight `context_status` call as the very first skill action, before any file reads. Mid-session failure (R-05) is a residual risk accepted as a platform limitation. |
| SR-07 (Near-duplicate entries) | R-10 | Architecture: EXISTING_CHECK state with ≥3 entry threshold and supplement warning. Server-side 0.92 cosine dedup is the second line of defense. Near-duplicate gap acknowledged in C-05. |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 1 (R-01) | 4 scenarios — all state transitions verified |
| High | 2 (R-02, R-03) | 3 scenarios each — quality gate + category restriction |
| Medium | 6 (R-04–R-08, R-12) | 2 scenarios each — sentinel, MCP failure, dry-run, depth, approval, append |
| Low | 4 (R-09–R-11, R-13) | 1–2 scenarios each — pre-flight, near-dup, scan, prerequisites |

---

## Knowledge Stewardship

- Queried: `/knowledge-search` for "lesson-learned failures gate rejection" — found #1006 (ADR-003 gate-check), #141 (glass box validation), #167 (gate result handling). No directly applicable lesson-learned entries; no previous skill quality failures recorded beyond the uni-init prototype referenced in SCOPE.md.
- Queried: `/knowledge-search` for "risk pattern skill markdown instruction following" — found #550 (Markdown-Only Delivery Pattern). Confirms platform constraint pattern is known; no instruction-fidelity risk patterns stored.
- Queried: `/knowledge-search` for "idempotency sentinel duplicate" — found #1091 (ADR-002 itself) and near-duplicate ADR entries. No broader pattern to store.
- Stored: nothing novel to store — the instruction-fidelity risk is feature-specific to this feature's markdown-only delivery. The pattern is already captured in #550. No 2+ feature evidence for a new pattern yet.
