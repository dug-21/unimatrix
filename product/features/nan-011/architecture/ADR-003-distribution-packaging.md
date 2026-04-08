## ADR-003: Distribution Packaging — protocols/ Directory and uni-retro

### Context

SR-03 (High/High risk) identified that the `protocols/` directory at repo root will be independent copies of `.claude/protocols/uni/` files and that symlinks do not survive `npm pack`. The dual-copy maintenance obligation is easy to miss and creates permanent drift. Additionally, the `package.json` `files` array must be updated to include both the `protocols/` directory and the `uni-retro` skill path.

The current `packages/unimatrix/package.json` `files` array contains: `["bin/", "lib/", "skills/", "postinstall.js"]`. The `skills/` directory already exists in the package, so the path convention for distributing `uni-retro` must align with that existing structure.

### Decision

**1. protocols/ directory structure**

Create at repo root: `protocols/`
Contents:
- `uni-design-protocol.md` (copy of `.claude/protocols/uni/uni-design-protocol.md`)
- `uni-delivery-protocol.md` (copy of `.claude/protocols/uni/uni-delivery-protocol.md`)
- `uni-bugfix-protocol.md` (copy of `.claude/protocols/uni/uni-bugfix-protocol.md`)
- `uni-agent-routing.md` (copy of `.claude/protocols/uni/uni-agent-routing.md`)
- `README.md` (new file; covers context_cycle integration pattern)

The `.claude/protocols/uni/` directory remains the source of truth for internal project use. `protocols/` at root is the distribution copy.

**2. Source of truth and copy direction**

`.claude/protocols/uni/` is the source of truth. When corrections are made during validation (AC-15), edits are applied to `.claude/protocols/uni/` first, then copied to `protocols/`. The `uni-release` skill update must include an explicit step: "diff `.claude/protocols/uni/*.md` against `protocols/*.md` and confirm they are identical before committing."

**3. uni-retro distribution path**

The existing `files` array contains `"skills/"` which maps to a `skills/` directory at repo root (not `.claude/skills/`). The implementer must verify whether a `skills/` directory exists at repo root in the current package structure before writing to it.

The target path for uni-retro distribution is: `skills/uni-retro/SKILL.md` at repo root.

This file is a copy of `.claude/skills/uni-retro/SKILL.md`. The `.claude/skills/uni-retro/SKILL.md` is the source of truth.

**4. package.json `files` array update**

The `files` array in `packages/unimatrix/package.json` must include:
- `"protocols/"` — to include the protocols directory
- `"skills/"` — already present; covers `skills/uni-retro/SKILL.md`

If `skills/` is already in the array, no array change is needed for uni-retro — only the file creation at `skills/uni-retro/SKILL.md` is required. If `protocols/` is not in the array, it must be added.

**5. uni-release skill update**

The `uni-release` SKILL.md must gain two new steps inserted before the "Create Release Commit" step (currently Step 7):

- Step 7a: Copy protocol files from `.claude/protocols/uni/` to `protocols/`, verify diff is empty.
- Step 7b: Copy `uni-retro` skill from `.claude/skills/uni-retro/SKILL.md` to `skills/uni-retro/SKILL.md`.
- Update the `git add` command in Step 7 to include `protocols/` and `skills/uni-retro/`.
- Update the summary output (Step 10) to list the new files.

**6. AC-13 verification: npm pack --dry-run**

The implementer must run `npm pack --dry-run` from the `packages/unimatrix/` directory after updating `package.json` and confirm that both `protocols/README.md` (or any protocol file) and `skills/uni-retro/SKILL.md` appear in the output.

### Consequences

- Dual-copy obligation is explicit: source in `.claude/protocols/uni/`, copy to `protocols/` as part of release. Drift is prevented by the diff-verification step in `uni-release`.
- `uni-release` is not itself distributed (per SCOPE.md non-goals) — the skill updates are to the internal tooling, not the npm package.
- `protocols/README.md` is a net-new file with no source in `.claude/` — it is authored once in `protocols/`.
- The `skills/` directory at repo root may not exist yet; the implementer must create it if absent.
- Future skills added to the distribution package follow the same pattern: copy to `skills/{skill-name}/SKILL.md` at repo root.
