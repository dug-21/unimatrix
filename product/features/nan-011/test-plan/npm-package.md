# Test Plan: npm Package Update

## Component Scope

Files under test:
- `packages/unimatrix/package.json` (files array update)
- `skills/uni-retro/SKILL.md` (new file at repo root)
- `.claude/skills/uni-release/SKILL.md` (Steps 7a and 7b)

Acceptance criteria covered: AC-13
Risks covered: R-08 (Med), R-15 (Med), R-12 (Med — npm copy cleanliness)

---

## Pre-Execution: Toolchain Check

Before any step in this file, confirm the npm toolchain is available:

```bash
node --version
# Expected: version string (e.g., v18.x.x or higher)

npm --version
# Expected: version string (e.g., 9.x.x or higher)
```

If either command fails or is not found: AC-13 is blocked by SR-02 (toolchain absent).
Record this blocker in RISK-COVERAGE-REPORT.md under Gaps. Do not fail the other ACs
in this feature because of a toolchain absence.

---

## AC-13: uni-release Steps + package.json Update + npm pack Confirmation

**Risk**: R-08 (Med/Med) — dry-run must be run from the correct directory; R-15 (skills/
directory must exist at repo root)

### Step 1 — uni-release SKILL.md: Steps 7a and 7b present

```bash
grep -n "7a\|7b\|protocols/\|uni-retro" /workspaces/unimatrix/.claude/skills/uni-release/SKILL.md | head -20
# Expected: Step 7a (copy protocols/ + diff verify) and Step 7b (copy uni-retro) are present
```

Read the updated sections of `.claude/skills/uni-release/SKILL.md`. Assert:
- Step 7a describes: copy `.claude/protocols/uni/` to `protocols/` and run diff verification
- Step 7b describes: copy `.claude/skills/uni-retro/SKILL.md` to `skills/uni-retro/SKILL.md`
- The git add step is updated to include the new artifacts (`protocols/` and
  `skills/uni-retro/SKILL.md`)
- The summary output step (Step 10 or equivalent) reflects the new artifacts

### Step 2 — package.json files array

```bash
grep -n '"protocols/"' /workspaces/unimatrix/packages/unimatrix/package.json
# Expected: one match showing "protocols/" in the files array
```

```bash
grep -n '"skills/"' /workspaces/unimatrix/packages/unimatrix/package.json
# Expected: one match showing "skills/" already present (not added by this feature, but must remain)
```

Assert the `files` array contains exactly these 5 entries (order may vary):
`"bin/"`, `"lib/"`, `"skills/"`, `"postinstall.js"`, `"protocols/"`

Assert `uni-release` is NOT in the files array:
```bash
grep -n "uni-release" /workspaces/unimatrix/packages/unimatrix/package.json
# Expected: zero matches in the files array context
```

### Step 3 — skills/ directory and uni-retro file exist at repo root (R-15)

```bash
ls /workspaces/unimatrix/skills/uni-retro/SKILL.md
# Expected: file found (no "No such file or directory")
```

```bash
ls -la /workspaces/unimatrix/skills/uni-retro/SKILL.md
# Check: first character of permissions is '-' (regular file), not 'l' (symlink)
```

Assert: `skills/uni-retro/SKILL.md` is a regular file at repo root, not inside `.claude/`.
A file at `.claude/skills/uni-retro/SKILL.md` alone does not satisfy this requirement.

### Step 4 — npm pack --dry-run (must run from packages/unimatrix/)

**Critical**: Run this command from `packages/unimatrix/`, not from repo root.
Running from repo root would succeed trivially and not exercise the `files` array.

```bash
cd /workspaces/unimatrix/packages/unimatrix && npm pack --dry-run 2>&1
```

Capture the full output. Assert:

(a) At least one file from `protocols/` appears in the output, e.g.:
    `protocols/README.md` or `protocols/uni-design-protocol.md`

(b) `skills/uni-retro/SKILL.md` appears in the output.

(c) `uni-release/SKILL.md` does NOT appear in the output (uni-release is internal tooling,
    not distributed). Check by scanning the output for the string `uni-release`.

```bash
# After running the dry-run, pipe through checks:
cd /workspaces/unimatrix/packages/unimatrix && npm pack --dry-run 2>&1 | grep "protocols/"
# Expected: at least one line

cd /workspaces/unimatrix/packages/unimatrix && npm pack --dry-run 2>&1 | grep "uni-retro"
# Expected: at least one line (skills/uni-retro/SKILL.md)

cd /workspaces/unimatrix/packages/unimatrix && npm pack --dry-run 2>&1 | grep "uni-release"
# Expected: zero lines (uni-release must NOT be distributed)
```

### Step 5 — Confirm npm copy of uni-retro is clean (R-12)

The npm pack output confirms the file is included. Also confirm the file is the corrected
version (not a pre-fix copy):

```bash
grep -rn 'context_[a-z_]*(' /workspaces/unimatrix/skills/uni-retro/SKILL.md | grep -v 'mcp__unimatrix__'
# Expected: zero matches (no bare invocations in the npm copy)
```

```bash
diff /workspaces/unimatrix/.claude/skills/uni-retro/SKILL.md \
     /workspaces/unimatrix/skills/uni-retro/SKILL.md
# Expected: no output (files are identical — copy was made from the corrected source)
```

**Pass criteria**: npm toolchain available; Step 7a and 7b present in uni-release;
package.json has "protocols/" in files array; skills/uni-retro/SKILL.md exists as a
regular file at repo root; npm pack --dry-run confirms protocols/ and uni-retro present
and uni-release absent; repo-root copy of uni-retro is identical to corrected source.
