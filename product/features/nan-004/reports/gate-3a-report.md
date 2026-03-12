# Gate 3a Report: nan-004

> Gate: 3a (Design Review)
> Date: 2026-03-12
> Result: REWORKABLE FAIL

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | WARN | Pseudocode correctly implements resolved decisions; ARCHITECTURE.md and ADR-004 retain stale tee pipeline reference for UserPromptSubmit |
| Specification coverage | PASS | All FRs (FR-01 through FR-33), NFRs, and ACs have corresponding pseudocode |
| Risk coverage | PASS | All 15 risks (R-01 through R-15) mapped to test scenarios with appropriate emphasis |
| Interface consistency | FAIL | C3 pseudocode and C3 test plan contradict on UNIMATRIX_BINARY existence check behavior |
| Knowledge stewardship | FAIL | Architect report missing `## Knowledge Stewardship` section; pseudocode agent report missing `Stored:` entry |

## Detailed Findings

### Architecture Alignment
**Status**: WARN
**Evidence**: All 11 components (C1-C11) in pseudocode map 1:1 to architecture component breakdown. Delivery waves match. ADR decisions are followed in pseudocode:
- ADR-001 (absolute paths): C4 init pseudocode writes absolute binary path to `.mcp.json` and hook commands.
- ADR-002 (binary rename): C7 pseudocode renames binary in single commit, updates repo configs.
- ADR-003 (init in JS): C4 is JavaScript, delegates to Rust binary for DB creation.
- ADR-004 (prefix-match): C5 settings-merge uses the 4 regex patterns from ADR-004.
- ADR-005 (version source): C9 sets workspace version, all crates inherit.

**Issue (WARN)**: ARCHITECTURE.md lines 316-320 state: "The `UserPromptSubmit` hook retains its tee-to-log pattern" with a tee pipeline example. ADR-004 line 42 also states: "Special case: `UserPromptSubmit` retains the `| tee -a ~/.unimatrix/injections/hooks.log` suffix." These contradict the resolved decision (stated in the spawn prompt and correctly implemented in C7 pseudocode at lines 141-153): **NO tee pipeline for UserPromptSubmit**. The pseudocode is correct; the architecture and ADR-004 are stale. This is WARN because the pseudocode (the artifact being validated) is correct, but implementers reading the architecture could be confused.

### Specification Coverage
**Status**: PASS
**Evidence**:
- FR-01 to FR-05 (binary rename, CLI restructure): Covered by C7 pseudocode (binary rename, Version/ModelDownload subcommands, MCP server default).
- FR-06 to FR-09 (npm package structure): Covered by C1 (package.json files), C2 (JS shim), C3 (binary resolution).
- FR-10 to FR-19 (init command): Covered by C4 (init.js) and C5 (merge-settings.js). All 7 hook events listed. Dry-run flag present. Idempotency via merge semantics. Summary output suggests `/unimatrix-init`.
- FR-20 to FR-22 (postinstall): Covered by C6. All error paths exit 0.
- FR-23 to FR-25 (version management): Covered by C9 (workspace version) and C11 (release skill).
- FR-26 to FR-27 (changelog): Covered by C11 release skill pseudocode.
- FR-28 to FR-33 (release pipeline): Covered by C10 (GitHub Actions workflow).
- NFR-03 (postinstall resilience): C6 guarantees unconditional exit 0.
- NFR-06 (zero project-file mutation on install): C6 only touches `~/.cache/`, no project files.
- NFR-07 (backward compatibility): C7 updates `.claude/settings.json` and `.mcp.json` in the repo.
- No scope additions detected -- pseudocode implements only what is specified.

### Risk Coverage
**Status**: PASS
**Evidence**: Test plan OVERVIEW.md maps all 15 risks to specific test layers and components:
- R-01 (Critical, settings merge corruption): 7 merge scenarios in C5 test plan plus 3 additional edge cases.
- R-02 (Critical, absolute path invalidation): C3 and C4 test plans cover path verification.
- R-03 (High, binary runtime failure): C10 test plan includes `ldd` check and smoke test steps.
- R-04 (High, idempotency): C4 and C5 test plans have double-init and triple-merge tests.
- R-05 (Med, JS shim routing): C2 test plan has 7 routing tests and 3 exit code tests.
- R-06 (Med, version drift): C9 test plan has 7 version validation tests.
- R-07 (High, CI toolchain): C10 test plan verifies Rust 1.89 pin and patches assertion.
- R-08 (Med, postinstall failure): C6 test plan has 5 scenarios all asserting exit 0.
- R-09 (High, mcp.json merge): C4 test plan has 4 merge preservation tests.
- R-10 (Low, skill overwrite): C4 test plan covers skill counts and non-unimatrix preservation.
- R-11 (Med, root detection divergence): C4 test plan covers passing same root to binary.
- R-12 (Low, binary rename breakage): C7 test plan verifies binary name and existing subcommands.
- R-13 (Med, require.resolve failure): C3 test plan covers env fallback and error messages.
- R-14 (Med, malformed settings.json): C5 test plan has 3 error handling tests.
- R-15 (High, publish order): C10 test plan verifies platform-before-root ordering.
- Risk priorities reflected: R-01 (Critical) has the most extensive test coverage (7+ scenarios), R-10/R-12 (Low) have minimal focused tests.

### Interface Consistency
**Status**: FAIL
**Evidence**: Contradiction between C3 pseudocode and C3 test plan regarding `UNIMATRIX_BINARY` environment variable handling.

C3 pseudocode (`binary-resolution.md` lines 18-21) implements an existence check:
```
IF NOT fs.existsSync(envPath):
    THROW Error("UNIMATRIX_BINARY points to non-existent file: " + envPath)
END IF
RETURN fs.realpathSync(envPath)
```

C3 test plan (`binary-resolution.md` line 13) states the opposite:
```
test_env_override_with_nonexistent_path_still_returns_it:
  The function returns the env path without existence check (caller handles errors).
```

These directly contradict. Either the pseudocode should remove the existence check, or the test plan should test that the function throws on a non-existent path.

**Issue**: The pseudocode behavior (check existence, throw if missing) is the safer design. The test plan should be updated to match the pseudocode.

### Knowledge Stewardship
**Status**: FAIL
**Evidence**:

**Architect report** (`nan-004-agent-1-architect-report.md`): No `## Knowledge Stewardship` section present. The report has a `## Unimatrix Storage` section describing failed ADR store attempts, but this does not satisfy the Knowledge Stewardship requirement. The architect is an active-storage agent and must have `Stored:` or `Declined:` entries in a `## Knowledge Stewardship` section. **REWORKABLE FAIL**.

**Pseudocode agent report** (`nan-004-agent-1-pseudocode-report.md`): Has a `## Knowledge Stewardship` section with `Queried:` entry and deviation notes, but is missing a `Stored:` or "nothing novel to store -- {reason}" entry. The pseudocode agent is read-only and needs `Queried:` entries (present) but should also have a `Stored:` disposition. **WARN** -- the section exists with queried entries; the missing stored disposition is minor for a read-only agent.

**Test-plan agent report** (`nan-004-agent-2-testplan-report.md`): Proper `## Knowledge Stewardship` section with `Queried:` and `Stored: nothing novel to store -- {reason}`. **PASS**.

**Risk-strategist report** (`nan-004-agent-3-risk-report.md`): Proper `## Knowledge Stewardship` section with multiple `Queried:` entries and `Stored: nothing novel to store -- {reason}`. **PASS**.

## Rework Required (if REWORKABLE FAIL)

| Issue | Which Agent | What to Fix |
|-------|-------------|-------------|
| C3 test plan contradicts pseudocode on UNIMATRIX_BINARY existence check | nan-004-agent-2 (test-plan) | Update `test_env_override_with_nonexistent_path_still_returns_it` to test that the function throws on a non-existent path, matching the pseudocode behavior. Rename test to `test_env_override_with_nonexistent_path_throws`. |
| Architect report missing Knowledge Stewardship section | nan-004-agent-1 (architect) | Add `## Knowledge Stewardship` section to architect report with `Stored:` or "nothing novel to store -- {reason}" entry. The report already notes ADR store attempts failed due to permissions; this should be documented in the stewardship section. |
| Pseudocode agent report missing Stored disposition | nan-004-agent-1 (pseudocode) | Add `Stored: nothing novel to store -- {reason}` entry to the existing Knowledge Stewardship section. |

## Notes

- ARCHITECTURE.md lines 316-320 and ADR-004 line 42 retain stale tee pipeline references for UserPromptSubmit. These should be corrected but are not blocking because the pseudocode (the validated artifact) correctly implements the resolved decision (no tee). Implementers should follow the pseudocode, not the stale architecture text.
- Architecture Open Question #4 recommends `--json` for version output, but the resolved decision is plain text only. The pseudocode correctly implements plain text. The open question is clearly labeled as such in the architecture, so this is not a conflict.
- The pseudocode OVERVIEW.md correctly identifies the `ensure_model()` re-export gap in `unimatrix-embed` that C8 depends on.
