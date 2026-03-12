# Security Review: nan-003-security-reviewer

## Risk Level: low

## Summary

This PR delivers two markdown skill files and extensive design documentation. No executable code, no Rust changes, no schema modifications, no new dependencies. The deliverables are Claude Code skill instructions (markdown) that guide model behavior for repository onboarding. The attack surface is limited to model instruction-following fidelity and CLAUDE.md file operations.

## Findings

### Finding 1: CLAUDE.md Overwrite Risk (Write vs Edit Semantics)

- **Severity**: medium
- **Location**: `.claude/skills/unimatrix-init/SKILL.md` Phase 3
- **Description**: The skill instructs Claude to append to CLAUDE.md using "Edit/append semantics -- do NOT overwrite the file." This is the correct instruction, and it is explicitly stated. However, the skill does not include a verification step after writing (e.g., "read CLAUDE.md after writing and verify all pre-existing content is preserved"). If the model misinterprets the instruction and uses Write instead of Edit, the entire CLAUDE.md is destroyed. The IMPLEMENTATION-BRIEF (C-07, function signatures table) reinforces "Edit (append, NOT Write/overwrite)" -- good.
- **Recommendation**: No change required. The instruction is clear and explicit. The risk is a platform constraint (model instruction fidelity), not a code defect. The RISK-TEST-STRATEGY R-12 covers this with test scenarios.
- **Blocking**: no

### Finding 2: Prompt Injection via Adversarial README Content

- **Severity**: low
- **Location**: `.claude/skills/unimatrix-seed/SKILL.md` Step 3 (Level 0 reads README.md)
- **Description**: `/unimatrix-seed` reads README.md and other repo files, then generates knowledge entries from their content. A malicious README could contain adversarial instructions designed to manipulate the model into storing harmful entries, bypassing the quality gate, or leaking context. The defense chain is: quality gate (What/Why/Scope) + human approval at every gate. This is documented in the RISK-TEST-STRATEGY security section.
- **Recommendation**: No change required. The human-in-the-loop approval at every gate is the correct mitigation for this threat model. The quality gate provides a first line of defense, and no entries are stored without explicit human approval.
- **Blocking**: no

### Finding 3: No Path Traversal Risk

- **Severity**: informational
- **Location**: Both skill files
- **Description**: Neither skill accepts user-provided file path parameters. `/unimatrix-init` globs a fixed pattern (`.claude/agents/**/*.md`) and writes to a fixed location (`CLAUDE.md`). `/unimatrix-seed` reads from a predetermined list of files (README.md, Cargo.toml, etc.) and stores via MCP `context_store`. No path traversal vectors exist.
- **Recommendation**: None.
- **Blocking**: no

### Finding 4: No Secrets or Credentials

- **Severity**: informational
- **Location**: Entire diff
- **Description**: Searched the full 4076-line diff for API keys, secrets, passwords, tokens, credentials, and bearer strings. Zero matches. Only external URLs are GitHub issue references to this project's own repository.
- **Recommendation**: None.
- **Blocking**: no

### Finding 5: No New Dependencies

- **Severity**: informational
- **Location**: Entire diff
- **Description**: No Cargo.toml, package.json, or any dependency manifest is modified. No new crates, no new npm packages. The deliverables are pure markdown files.
- **Recommendation**: None.
- **Blocking**: no

### Finding 6: Seed Entry Poisoning via context_store

- **Severity**: low
- **Location**: `.claude/skills/unimatrix-seed/SKILL.md` Step 4, Step 5, Step 7
- **Description**: Approved seed entries are stored via `context_store` and become visible to all future agents via `context_briefing` and `context_search`. If a low-quality or misleading entry passes human approval, it could provide incorrect guidance to future agents across the repository. The defense is the explicit human approval gate at every level, plus the quality gate (What/Why/Scope validation). The category restriction (convention/pattern/procedure only -- no decision/outcome/lesson-learned) limits the blast radius.
- **Recommendation**: No change required. The multi-layer defense (quality gate + human approval + category restriction) is appropriate for the threat model. Stored entries can be corrected or deprecated via existing MCP tools (context_correct, context_deprecate).
- **Blocking**: no

## Blast Radius Assessment

**Worst case if the fix has a subtle bug**: The most dangerous failure mode is `/unimatrix-init` using Write semantics instead of Edit/append, destroying existing CLAUDE.md content. This would cause loss of all project instructions for the target repository. The blast radius is limited to one file in one repository, and git history provides recovery. The skill instructions explicitly guard against this (repeated "do NOT overwrite" language), and dry-run mode exists for pre-verification.

For `/unimatrix-seed`, the worst case is storing low-quality entries that mislead future agents. The blast radius is bounded by: (a) human approval required for every entry, (b) entries can be deprecated/corrected after the fact, (c) entries only affect semantic search relevance, not hard-coded behavior.

Neither skill modifies any Rust code, schema, or runtime behavior of the Unimatrix server itself. The blast radius does not extend to the MCP server's operation.

## Regression Risk

**Minimal**. No existing code is modified. No existing files are changed. The two new skill files are additive -- they create new slash commands without touching the 11 existing skills. The only existing file referenced in git status as modified (`product/PRODUCT-VISION.md`) is NOT part of this PR diff, so no vision document regression.

The only theoretical regression: if the Claude Code skill loader has path conflicts or naming collisions, the new `unimatrix-init` and `unimatrix-seed` directories could interfere. This is unlikely given the existing `.claude/skills/` convention with 11 other skills already following the same pattern.

## PR Comments

- Posted 1 review comment on PR #212
- Blocking findings: no

## Knowledge Stewardship

- Stored: nothing novel to store -- this is a markdown-only delivery with no executable code. The prompt injection risk via README content is feature-specific and already documented in the RISK-TEST-STRATEGY. The Write-vs-Edit concern is a known platform constraint already captured in Unimatrix entry #550 (Markdown-Only Delivery Pattern). No 2+ feature evidence for a new pattern.
