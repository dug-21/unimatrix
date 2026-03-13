# Test Plan Overview: nan-005 — Documentation & Onboarding

## Feature Nature

nan-005 is documentation-only. There is no Rust code to compile or unit-test.
Testing means content verification: shell commands that assert the produced
markdown files match codebase reality and spec requirements.

---

## Test Strategy

| Layer | Approach | Gate |
|-------|----------|------|
| Factual accuracy | Shell grep/count commands against live codebase | Gate 3c (critical — blocks merge) |
| Structural completeness | Section presence + non-empty content checks | Gate 3c (critical — blocks merge) |
| Content correctness | Absence checks for banned terms + aspirational language | Gate 3c (high priority) |
| Protocol integration | Position verification via line-number grep in modified protocol | Gate 3c (high priority) |
| Agent definition | File existence + required-field grep | Gate 3c (high priority) |

**No unit tests are run.** `cargo test` is not relevant — nan-005 touches no Rust code.

**No integration harness (infra-001) suites apply.** The integration harness exercises
the compiled binary through MCP JSON-RPC. Documentation files have no MCP-visible effect.
The smoke gate (`pytest -m smoke`) is SKIPPED for this feature per suite selection rules
("documentation-only feature — no code changes").

---

## Risk-to-Test Mapping

| Risk ID | Priority | Risk | Test File | Key Scenarios |
|---------|----------|------|-----------|---------------|
| R-01 | Critical | README factual error at ship | readme-rewrite.md | Facts 1-7: grep/count against codebase |
| R-02 | Critical | Fact verification step skipped | readme-rewrite.md | Verified values present; no SCOPE.md estimates used |
| R-03 | High | `maintain` misdocumented | readme-rewrite.md | Grep for silent-ignore language; grep against active language |
| R-04 | High | Tool count discrepancy | readme-rewrite.md | Table row count == `grep -c '#\[tool('` result |
| R-05 | High | uni-docs behavioral gaps | uni-docs-agent.md | Fallback chain, scope boundary, no-source-code all present |
| R-06 | High | Protocol step at wrong position | delivery-protocol-mod.md | Line-number check: uni-docs before /review-pr |
| R-07 | Med | Trigger criteria absent/ambiguous | delivery-protocol-mod.md | MANDATORY list + skip list both present |
| R-08 | Med | Aspirational content | readme-rewrite.md | Grep for forward-looking language |
| R-09 | Med | Inconsistent terminology | readme-rewrite.md | Grep for variant spellings/forms |
| R-10 | Med | Security section unimplemented features | readme-rewrite.md | Grep for OAuth/HTTPS/_meta |
| R-11 | Med | Skills table misclassification | readme-rewrite.md | Row count vs filesystem; /uni-git classification documented |
| R-12 | Low | uni-docs reads source code | uni-docs-agent.md | Grep for explicit no-source-code constraint |
| R-13 | Low | Acknowledgments removed | readme-rewrite.md | Grep for credits |

---

## Cross-Component Dependencies

The three components form an implicit contract:
- **README section headers** become the update target for uni-docs in future deliveries.
  If a header name changes post-merge, uni-docs agents will misidentify targets.
  Test: section headers in README match the names referenced in uni-docs.md inputs.

- **Trigger criteria** in the delivery protocol must match the categories uni-docs
  is instructed to detect. Test: mandatory conditions in protocol are consistent
  with mandatory conditions documented in uni-docs agent definition.

- **uni-docs spawn template** in the protocol must supply the inputs the agent expects
  (feature ID, SCOPE.md path, SPECIFICATION.md path, README.md path). Test: grep
  protocol for these four inputs in the spawn block.

---

## Fact Baseline (pre-authoring verification commands)

These commands must be run against the worktree BEFORE the implementation agent
authors the README. The Stage 3c tester re-runs them to validate the README matches.

```bash
WORKTREE=/workspaces/unimatrix-nan-005

# Tool count
grep -c '#\[tool(' $WORKTREE/crates/unimatrix-server/src/mcp/tools.rs

# Skill count
ls $WORKTREE/.claude/skills/ | wc -l

# Crate count
ls $WORKTREE/crates/ | wc -l

# Schema version
grep 'CURRENT_SCHEMA_VERSION' $WORKTREE/crates/unimatrix-store/src/migration.rs

# SQLite table count
grep -c 'CREATE TABLE IF NOT EXISTS' $WORKTREE/crates/unimatrix-store/src/db.rs

# Rust version
grep 'rust-version' $WORKTREE/Cargo.toml

# npm package name
grep '"name"' $WORKTREE/packages/unimatrix/package.json | head -1

# Hook event names
grep -h 'UserPromptSubmit\|PreCompact\|Stop\|PostToolUse\|PreToolUse' \
  $WORKTREE/crates/unimatrix-server/src/uds/hook.rs

# maintain behavior
grep -A5 'maintain' $WORKTREE/crates/unimatrix-server/src/mcp/tools.rs | head -20

# CLI subcommands (Command enum)
grep -A2 'enum Command\|hook\|export\|import\|version\|model.download' \
  $WORKTREE/crates/unimatrix-server/src/main.rs | head -30

# Category names
grep -v '//' $WORKTREE/crates/unimatrix-server/src/infra/categories.rs | \
  grep '"' | head -20
```

---

## Integration Harness Plan

**Determination**: No infra-001 suites apply to nan-005.

**Rationale**: The integration harness exercises the compiled `unimatrix-server`
binary through MCP JSON-RPC. nan-005 produces three markdown files:

1. `README.md` — static documentation
2. `.claude/agents/uni/uni-docs.md` — agent definition file
3. `.claude/protocols/uni/uni-delivery-protocol.md` — protocol edit

None of these files are loaded by the binary at runtime. None affect any MCP tool
behavior, schema, confidence scoring, security enforcement, or storage layout.
There is no mechanism through which a README edit or protocol modification would
be visible to the harness.

**Smoke gate status**: SKIP — documentation-only feature per USAGE-PROTOCOL.md
suite selection rule: "documentation-only feature — no code changes."

**New integration tests**: None required. No new MCP-visible behavior exists.

**Future note**: If a future feature adds a tool that surfaces README metadata
(e.g., `context_help`), that feature's test plan would add to `test_tools.py`.
nan-005 does not introduce that capability.
