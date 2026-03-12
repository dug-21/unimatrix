# nan-003 Test Plan Overview

## Verification Approach

This feature delivers markdown skill files (no compiled code). All verification is manual — running the skills and inspecting behavior/output, plus static analysis of SKILL.md content.

### Verification Methods

| Method | Description |
|--------|-------------|
| **Content review** | grep/read SKILL.md files for required strings, structures, phrasing |
| **Manual execution** | Run the skill in a test repo and observe behavior |
| **Diff verification** | Compare file state before/after skill execution |
| **FR tracing** | Verify each functional requirement is addressed in SKILL.md instructions |

## Risk Coverage Mapping

| Risk | Priority | Test Plan File | Verification Method |
|------|----------|----------------|-------------------|
| R-01 (STOP gates not respected) | Critical | seed-state-machine.md | Content: verify STOP phrasing; Manual: test each gate |
| R-02 (Quality gate not enforced) | High | entry-quality-gate.md | Content: verify gate rules in SKILL.md |
| R-03 (Wrong categories) | High | entry-quality-gate.md | Content: verify category restriction text |
| R-04 (Sentinel missed) | Medium | unimatrix-init.md | Content: verify sentinel check logic |
| R-05 (MCP mid-session failure) | Medium | unimatrix-seed.md | Content: verify error handling instructions |
| R-06 (Dry-run violated) | Medium | unimatrix-init.md | Content: verify dry-run guard |
| R-07 (Depth limit bypassed) | Medium | seed-state-machine.md | Content: verify "no Level 3" text |
| R-08 (Approval mode inverted) | Medium | seed-state-machine.md | Content: verify batch vs individual instructions |
| R-09 (Pre-flight false success) | Low | unimatrix-seed.md | Content: verify status check instructions |
| R-10 (Near-duplicate re-run) | Low | unimatrix-seed.md | Content: verify existing-check threshold |
| R-11 (Agent scan false neg/pos) | Low | agent-scan.md | Content: verify check patterns |
| R-12 (CLAUDE.md corrupted) | Low | claude-md-template.md | Content: verify append semantics |
| R-13 (Prerequisites gap) | Low | unimatrix-init.md, unimatrix-seed.md | Content: verify prerequisites sections |

## Integration Verification

Since skills are markdown instructions (not code), there is no integration test harness. Integration verification is performed by:

1. Running `/unimatrix-init` on the Unimatrix repo itself and verifying output
2. Running `/unimatrix-seed` against a repo with an operational MCP server
3. Verifying MCP tool calls succeed when triggered by skill instructions

## Acceptance Criteria Mapping

| AC | Verified By | Test Plan |
|----|------------|-----------|
| AC-01 | Content review + manual run | unimatrix-init.md |
| AC-02 | Manual run (twice) + diff | unimatrix-init.md |
| AC-03 | Manual run (no CLAUDE.md) | unimatrix-init.md |
| AC-04 | Manual run + file timestamp check | agent-scan.md |
| AC-05 | Manual run (--dry-run) + diff | unimatrix-init.md |
| AC-06 | Manual run + context_status check | unimatrix-seed.md |
| AC-07 | Manual run through Level 0 | seed-state-machine.md |
| AC-08 | Manual run (reject at L0 + L1) | seed-state-machine.md |
| AC-09 | Manual run through L0->L1->L2 | seed-state-machine.md |
| AC-10 | grep for file paths + frontmatter | unimatrix-init.md, unimatrix-seed.md |
| AC-11 | Peer review of block content | claude-md-template.md |
| AC-12 | grep for "uni-init" in SKILL.md | unimatrix-init.md |
| AC-13 | Manual run with pre-existing entries | unimatrix-seed.md |
| AC-14 | grep for sentinel version string | unimatrix-init.md |
