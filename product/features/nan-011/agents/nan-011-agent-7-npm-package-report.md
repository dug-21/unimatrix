# Agent Report: nan-011-agent-7-npm-package

Component: npm Package Update
Feature: nan-011

## Files Created/Modified

- `packages/unimatrix/package.json` — added `"protocols/"` to files array
- `packages/unimatrix/skills/uni-retro/SKILL.md` — new, copy of corrected source (for npm distribution)
- `packages/unimatrix/protocols/README.md` — new, copy from repo root protocols/ (for npm distribution)
- `packages/unimatrix/protocols/uni-design-protocol.md` — new, copy (for npm distribution)
- `packages/unimatrix/protocols/uni-delivery-protocol.md` — new, copy (for npm distribution)
- `packages/unimatrix/protocols/uni-bugfix-protocol.md` — new, copy (for npm distribution)
- `packages/unimatrix/protocols/uni-agent-routing.md` — new, copy (for npm distribution)
- `skills/uni-retro/SKILL.md` — new at repo root, regular file copy of corrected source

## Verification Results

### Diff (source vs repo-root copy)
`diff .claude/skills/uni-retro/SKILL.md skills/uni-retro/SKILL.md` — zero output (IDENTICAL)

### Diff (source vs package copy)
`diff .claude/skills/uni-retro/SKILL.md packages/unimatrix/skills/uni-retro/SKILL.md` — zero output (IDENTICAL)

### package.json files array
```json
["bin/", "lib/", "skills/", "postinstall.js", "protocols/"]
```

### uni-release absent
`grep "uni-release" packages/unimatrix/package.json` — zero matches (PASS)

### MCP format passes on dist copy
- Pass 1 (backtick-wrapped bare): CLEAN
- Pass 2 (bare invocations without prefix): CLEAN

### npm pack --dry-run (from packages/unimatrix/)
PASS — 25 files, 171.6 kB unpacked:
- `protocols/README.md` — PRESENT
- `protocols/uni-agent-routing.md` — PRESENT
- `protocols/uni-bugfix-protocol.md` — PRESENT
- `protocols/uni-delivery-protocol.md` — PRESENT
- `protocols/uni-design-protocol.md` — PRESENT
- `skills/uni-retro/SKILL.md` — PRESENT (11.3kB)
- `uni-release` — ABSENT (confirmed)

## Design Document Discrepancy Noted

The pseudocode's error handling section states: "npm resolves `files` entries relative to the repo root (where package.json anchor is)." This is factually incorrect. npm resolves `files` relative to the directory containing `package.json`, which is `packages/unimatrix/`. Verified by creating a test file at repo-root `protocols/` and confirming npm pack did not include it.

**Resolution**: Created `packages/unimatrix/skills/uni-retro/SKILL.md` and `packages/unimatrix/protocols/` (copying from repo-root `protocols/` created by Component 4) so that npm pack picks up the files. The repo-root `skills/uni-retro/SKILL.md` is also created as specified by the spawn prompt (useful as a git-tracked reference, but not what npm packages).

This creates a three-level copy chain for protocols:
1. `.claude/protocols/uni/` (source of truth)
2. `protocols/` (repo root, git-tracked distribution reference, created by Component 4)
3. `packages/unimatrix/protocols/` (actual npm distribution copy, created here)

The `uni-release` SKILL.md copy steps (7a/7b) should document that the copy target for npm is `packages/unimatrix/protocols/` and `packages/unimatrix/skills/uni-retro/SKILL.md`, not just repo-root paths.

## Test Results

All 8 key test scenarios from the test plan pass:
1. Regular file (not symlink): PASS (`-rw-r--r--`)
2. diff returns zero output: PASS
3. MCP format passes 1 and 2: PASS (both CLEAN)
4. package.json "protocols/" present: PASS
5. uni-release absent from package.json: PASS
6. npm pack lists protocols/README.md: PASS
7. npm pack lists skills/uni-retro/SKILL.md: PASS
8. npm pack does not list uni-release/SKILL.md: PASS

## Issues / Blockers

None. npm toolchain available (node v24.13.1, npm 11.8.0).

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing — returned ADR-003 (entry #4267) as top result confirming distribution packaging decisions; no additional context needed beyond what was already in the pseudocode.
- Stored: nothing novel to store — the npm resolution discrepancy (files resolved relative to package.json directory, not repo root) is a factual correction to the design docs rather than a reusable implementation pattern. The three-level copy chain is specific to this package layout. Filing in agent report for the gate validator to assess whether the uni-release SKILL.md update (Component 3) needs a correction pass to specify the right target paths.
