# Test Plan Overview: nan-011 — Release Preparation

## Feature Summary

nan-011 is a documentation and distribution feature. No Rust code is written; no unit
tests exist in the traditional sense. All deliverables are static file edits: markdown
files, TOML configuration, skill files, protocol files, and npm packaging metadata.

"Testing" means executing shell verification commands and performing manual review
checks derived directly from ACCEPTANCE-MAP.md (AC-01 through AC-17).

---

## Risk-to-Test Mapping

| Risk ID | Priority | Risk Description | Test Plan File | AC-IDs Covered |
|---------|----------|-----------------|---------------|----------------|
| R-01 | Critical | config.toml defaults diverge from config.rs | config-toml.md | AC-08 |
| R-02 | Critical | boosted_categories/adaptive_categories show Rust Default instead of serde default | config-toml.md | AC-08 |
| R-03 | High | rayon_pool_size shown as fixed integer | config-toml.md | AC-08 |
| R-04 | Critical | Protocol dual-copy drift | protocols-dir.md | AC-15 |
| R-05 | High | Bare MCP invocations missed by grep | skills-audit.md | AC-10 |
| R-06 | High | config.toml parse errors | config-toml.md | AC-08 |
| R-07 | High | uni-init lists fewer than 14 skills | skills-audit.md | AC-11 |
| R-08 | Med | npm pack --dry-run not run from correct directory | npm-package.md | AC-13 |
| R-09 | Med | protocols/README.md missing context_cycle example | protocols-dir.md | AC-14 |
| R-10 | Med | Vision statement verbatim check fails | readme-vision.md | AC-01 |
| R-11 | Med | Stale NLI references survive | readme-vision.md, protocols-dir.md | AC-02, AC-12, AC-15 |
| R-12 | Med | uni-retro npm copy carries bare invocations | skills-audit.md | AC-10, AC-13 |
| R-13 | Low | PRODUCT-VISION.md wrong rows edited | readme-vision.md | AC-05 |
| R-14 | Low | uni-seed idempotency warning absent or misplaced | skills-audit.md | AC-16 |
| R-15 | Med | skills/ directory absent at repo root | npm-package.md | AC-13, AC-15 |

---

## Test Strategy

### Approach

All tests are shell verification commands and manual review steps. They are organized into
five component test plan files, one per architecture component. The tester in Stage 3c
executes each file's verification steps sequentially, then records results in
RISK-COVERAGE-REPORT.md.

### Test Categories

**Shell verification commands** — grep, diff, python3 TOML parse, npm pack --dry-run.
These produce machine-readable pass/fail output. Record the exact command and output.

**Manual review checks** — read file, confirm presence/content/order of specific text.
These are human-judgment checks where grep alone is insufficient (e.g., section ordering,
prose accuracy, comment quality). Record the judgment and evidence text.

**File existence checks** — confirm files exist at the correct path, are regular files
(not symlinks), and appear in npm pack output.

### Test Ordering

Critical-priority risks (R-01, R-02, R-04) must be verified first. If any Critical test
fails, flag immediately — these are blocking for PR merge.

Recommended component execution order:
1. config-toml.md (most risk-dense; R-01/R-02/R-03/R-06 are all Critical or High)
2. protocols-dir.md (R-04 is Critical; diff verification is fast)
3. skills-audit.md (R-05/R-07/R-12 are High; two-pass grep takes care)
4. readme-vision.md (R-10/R-11/R-13 are Med; quick greps and manual reads)
5. npm-package.md (R-08/R-15; depends on npm toolchain availability)

---

## Integration Harness Plan

**No integration suites apply to this feature.**

nan-011 introduces no new Rust types, no new MCP tools, no new tool parameters, and no
new lifecycle flows. The infra-001 integration harness exercises compiled binary behavior
through the MCP JSON-RPC protocol. Since nan-011 involves no Rust compilation, the
harness is not relevant.

- Smoke tests (`pytest -m smoke`): not applicable — no binary changes.
- All 8 suite files: not applicable.
- New integration tests to add: none — all verifiable behavior is in static file content.

**The tester in Stage 3c runs only the shell verification commands documented in the
per-component test plans. Do not run cargo or pytest for this feature.**

---

## Pre-Execution Environment Check

Before running any verification commands, confirm toolchain availability:

```bash
# Required for AC-08
python3 -c "import tomllib; print('tomllib ok')"

# Required for AC-13
node --version
npm --version

# Optional: confirm diff available (standard on Linux)
diff --version | head -1
```

If `python3`/`tomllib` is unavailable: AC-08 parse check is blocked — flag in report.
If `node`/`npm` is unavailable: AC-13 is blocked — flag in report, note SR-02 blocker.

---

## Cross-Component Dependencies

| Dependency | Detail |
|-----------|--------|
| protocols/ copies must be made after .claude/protocols/uni/ is corrected | Tester must verify source files first, then diff copies (AC-15 depends on both) |
| skills/uni-retro/SKILL.md must be the corrected copy | Verify .claude/skills/uni-retro/SKILL.md is clean before checking repo-root copy (R-12) |
| config.toml TOML validity before value checks | Parse check (AC-08 step 1) must pass before field-by-field comparison is meaningful |
| npm pack requires files to exist on disk | AC-13 depends on protocols/ and skills/uni-retro/ creation (AC-14, AC-15 must pass first) |

---

## Acceptance Criteria Coverage

| AC-ID | Component File | Priority |
|-------|---------------|----------|
| AC-01 | readme-vision.md | High (R-10) |
| AC-02 | readme-vision.md | Med (R-11) |
| AC-03 | readme-vision.md | Med |
| AC-04 | readme-vision.md | Med (R-11) |
| AC-05 | readme-vision.md | Low (R-13) |
| AC-06 | config-toml.md | High |
| AC-07 | config-toml.md | High |
| AC-08 | config-toml.md | Critical (R-01/R-02/R-03/R-06) |
| AC-09 | config-toml.md | High |
| AC-10 | skills-audit.md | High (R-05/R-12) |
| AC-11 | skills-audit.md | High (R-07) |
| AC-12 | skills-audit.md | Med (R-11) |
| AC-13 | npm-package.md | Med (R-08/R-15) |
| AC-14 | protocols-dir.md | Med (R-09) |
| AC-15 | protocols-dir.md | Critical (R-04/R-11) |
| AC-16 | skills-audit.md | Low (R-14) |
| AC-17 | skills-audit.md | Low |
