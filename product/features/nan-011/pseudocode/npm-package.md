# Component 5 — npm Package Update

## Purpose

Update `packages/unimatrix/package.json` to include the `protocols/` directory in the
npm distribution, and create the distributable `skills/uni-retro/SKILL.md` at repo root.
Verify with `npm pack --dry-run`.

---

## Dependencies

This component MUST run AFTER:
- Component 3 (Skills MCP Format Audit): `.claude/skills/uni-retro/SKILL.md` must be
  corrected before it is copied to `skills/uni-retro/SKILL.md` at repo root.
- Component 4 (protocols/ Directory): `protocols/` must exist and be populated before
  `npm pack --dry-run` can verify it.

Do NOT copy `skills/uni-retro/SKILL.md` until Component 3 is complete.
Do NOT run `npm pack --dry-run` until `protocols/` is populated.

---

## Pre-Work: Environment Check

Before any edits, verify the npm toolchain is available:

```bash
node --version
npm --version
```

If either command fails, the `npm pack --dry-run` verification (AC-13) is blocked.
Document the failure in the agent report and do not skip the other steps — the file
edits can still be made; only the verification is deferred.

---

## Pre-Work: Check skills/ Directory at Repo Root

```bash
ls /workspaces/unimatrix/skills/ 2>/dev/null || echo "ABSENT"
```

Two cases:
- **`skills/` exists**: Create `skills/uni-retro/` subdirectory inside it.
- **`skills/` is absent**: Create `skills/uni-retro/` — this implicitly creates `skills/`.

The `package.json` already contains `"skills/"` in the `files` array. No array change
is needed for uni-retro. Only the file creation is required.

---

## Operation N1: Create skills/uni-retro/SKILL.md at Repo Root

PRECONDITION: Component 3 (skills-audit.md) must be complete. `.claude/skills/uni-retro/SKILL.md`
must have been corrected (bare invocations fixed).

```bash
mkdir -p /workspaces/unimatrix/skills/uni-retro
cp .claude/skills/uni-retro/SKILL.md skills/uni-retro/SKILL.md
```

VERIFY the copy is a regular file (not a symlink):

```bash
ls -la skills/uni-retro/SKILL.md
# Output must show a regular file (-rw-...), NOT a symlink (lrwxrwxrwx)
```

VERIFY the copy is identical to the corrected source:

```bash
diff .claude/skills/uni-retro/SKILL.md skills/uni-retro/SKILL.md
# Must produce zero output
```

VERIFY the npm dist copy is also clean (MCP format):

```bash
# Pass 1 on the dist copy
grep -n '`context_[a-z_]*(' skills/uni-retro/SKILL.md

# Pass 2 on the dist copy
grep -n 'context_[a-z_]*(' skills/uni-retro/SKILL.md | grep -v 'mcp__unimatrix__'
```

Both passes must return zero uninvestigated matches.

---

## Operation N2: Update packages/unimatrix/package.json

LOCATE: `packages/unimatrix/package.json`

READ the current `files` array:

```json
"files": [
  "bin/",
  "lib/",
  "skills/",
  "postinstall.js"
]
```

CHANGE to add `"protocols/"`:

```json
"files": [
  "bin/",
  "lib/",
  "skills/",
  "postinstall.js",
  "protocols/"
]
```

This is the ONLY change to package.json in this component. Do not modify:
- `version`
- `name`
- `optionalDependencies`
- `scripts`
- Any other field

VERIFY the change:

```bash
grep -A 10 '"files"' packages/unimatrix/package.json
# Must show "protocols/" in the array
```

VERIFY `uni-release` does not appear in the files array:

```bash
grep "uni-release" packages/unimatrix/package.json
# Must return zero matches
```

---

## Operation N3: npm pack --dry-run Verification

Run from the `packages/unimatrix/` directory (NOT repo root):

```bash
cd packages/unimatrix && npm pack --dry-run 2>&1
```

The output lists all files that would be included in the published package.

ASSERT the following entries appear in the output:
- At least one file from `protocols/` (e.g., `protocols/README.md`)
- `skills/uni-retro/SKILL.md`

ASSERT the following does NOT appear:
- Any file containing `uni-release/SKILL.md`

Record the full dry-run output in the PR description or delivery checklist as
required by NFR-3.

If `npm pack --dry-run` fails or is unavailable:
- Document the failure reason in the agent report
- Note which assertions cannot be verified
- Do not block the PR on npm toolchain absence — file a follow-on issue if needed

---

## Final State After This Component

```
packages/unimatrix/package.json   (files array includes "protocols/")
skills/
  uni-retro/
    SKILL.md                      (regular file, copy of .claude/skills/uni-retro/SKILL.md)
protocols/                        (populated by Component 4 — verified by npm pack here)
  uni-design-protocol.md
  uni-delivery-protocol.md
  uni-bugfix-protocol.md
  uni-agent-routing.md
  README.md
```

---

## Error Handling

If the diff between `skills/uni-retro/SKILL.md` (dist) and `.claude/skills/uni-retro/SKILL.md`
(source) is non-empty: the copy was made before Component 3 completed, or an edit was
made after the copy. Re-run the copy and re-verify.

If `npm pack --dry-run` output does not list `protocols/` files: the directory may
not exist at the path npm resolves from `packages/unimatrix/`. npm resolves `files`
entries relative to the repo root (where package.json anchor is). Verify that
`protocols/` exists at repo root, not inside `packages/unimatrix/`.

If `skills/uni-retro/SKILL.md` is a symlink (e.g., created by accident with `ln -s`):
remove it and re-create as a regular file copy. Symlinks do not survive `npm pack`.

---

## Key Test Scenarios

1. `ls -la skills/uni-retro/SKILL.md` shows regular file (not symlink).
2. `diff .claude/skills/uni-retro/SKILL.md skills/uni-retro/SKILL.md` returns zero output.
3. Pass 1 and Pass 2 MCP format grep on `skills/uni-retro/SKILL.md` return zero matches.
4. `packages/unimatrix/package.json` `files` array contains `"protocols/"`.
5. `grep "uni-release" packages/unimatrix/package.json` returns zero matches.
6. `npm pack --dry-run` from `packages/unimatrix/` lists `protocols/README.md`.
7. `npm pack --dry-run` from `packages/unimatrix/` lists `skills/uni-retro/SKILL.md`.
8. `npm pack --dry-run` output does NOT list `uni-release/SKILL.md`.
