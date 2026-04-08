---
name: "uni-release"
description: "Version bump, changelog generation, tag, and push to trigger the release pipeline."
---

# /uni-release — Create a Unimatrix Release

## Inputs

From the invoker:
- Bump level: `major`, `minor`, or `patch` — OR an explicit semver string (e.g. `0.7.0`).

If no bump level is provided, ask the user before proceeding.

---

## Pre-flight Checks

1. Ensure the worktree is clean (no uncommitted changes):
   ```bash
   git status --porcelain
   ```
   If output is non-empty, stop with: **"Clean worktree required for release. Commit or stash changes first."**

2. Ensure you are on a branch that can be pushed (typically `main` or a release branch).

---

## Step 1: Read Current Version

Read `[workspace.package] version` from the root `Cargo.toml`:

```bash
grep -m1 'version' Cargo.toml | head -1
```

Look inside the `[workspace.package]` section for the `version = "X.Y.Z"` line. Parse the current version string.

---

## Step 2: Compute New Version

- **If bump level** (`major` / `minor` / `patch`):
  - Parse current version as `MAJOR.MINOR.PATCH`.
  - `patch` -> `MAJOR.MINOR.(PATCH+1)`
  - `minor` -> `MAJOR.(MINOR+1).0`
  - `major` -> `(MAJOR+1).0.0`

- **If explicit version string**:
  - Validate it matches `X.Y.Z` where X, Y, Z are non-negative integers.
  - Validate it is strictly greater than the current version.
  - If invalid, stop with a diagnostic error.

Check that the git tag `v{new_version}` does not already exist:
```bash
git tag -l "v{new_version}"
```
If it exists, stop with: **"Tag v{new_version} already exists."**

---

## Step 3: Update Root Cargo.toml

Edit the `[workspace.package]` section in the root `Cargo.toml`:
- Change `version = "{old_version}"` to `version = "{new_version}"`.

All 9 workspace crates use `version.workspace = true` and inherit automatically.

---

## Step 4: Update npm package.json Files

Update these files with the new version:

1. **`packages/unimatrix/package.json`**:
   - Set `"version"` to `"{new_version}"`.
   - Set `"optionalDependencies"."@dug-21/unimatrix-linux-x64"` to `"{new_version}"`.
   - Set `"optionalDependencies"."@dug-21/unimatrix-linux-arm64"` to `"{new_version}"`.

2. **`packages/unimatrix-linux-x64/package.json`**:
   - Set `"version"` to `"{new_version}"`.

3. **`packages/unimatrix-linux-arm64/package.json`**:
   - Set `"version"` to `"{new_version}"`.

---

## Step 5: Generate CHANGELOG.md

1. Find the previous release tag:
   ```bash
   git describe --tags --abbrev=0 --match "v*" 2>/dev/null
   ```
   If no prior tag exists, use the first commit as the range start.

2. Collect conventional commits in the range `{previous_tag}..HEAD`:
   ```bash
   git log {previous_tag}..HEAD --format="%H %s"
   ```

3. Classify each commit:
   - Starts with `feat:` or `feat(` -> **Features** (strip prefix for display).
   - Starts with `fix:` or `fix(` -> **Fixes** (strip prefix for display).
   - Contains `BREAKING CHANGE` in the body OR has `!:` in the subject -> **Breaking Changes**.
   - All other prefixes (`docs:`, `test:`, `chore:`, etc.) -> skip.

4. Build the new changelog section:
   ```
   ## [{new_version}] - {YYYY-MM-DD}

   ### Breaking Changes
   - {message}

   ### Features
   - {message}

   ### Fixes
   - {message}
   ```
   Omit any section that has zero entries. Use today's date.

5. If `CHANGELOG.md` does not exist, create it with this header:
   ```
   # Changelog

   All notable changes to Unimatrix are documented here.
   Format based on [Keep a Changelog](https://keepachangelog.com/).
   ```

6. **Prepend** the new section after the header (before any existing release sections).

---

## Step 6: Verify Build

Run a build check to confirm the version change does not break compilation:
```bash
cargo check --workspace
```
If this fails, stop with: **"Build check failed after version update. Review changes before releasing."**

---

## Step 7a: Sync protocols/ Distribution Copy

Copy the four protocol files from the internal `.claude/protocols/uni/` directory
to the distributable `protocols/` directory at repo root:

```bash
cp .claude/protocols/uni/uni-design-protocol.md protocols/uni-design-protocol.md
cp .claude/protocols/uni/uni-delivery-protocol.md protocols/uni-delivery-protocol.md
cp .claude/protocols/uni/uni-bugfix-protocol.md protocols/uni-bugfix-protocol.md
cp .claude/protocols/uni/uni-agent-routing.md protocols/uni-agent-routing.md
```

Verify each copy is identical to its source:

```bash
diff .claude/protocols/uni/uni-design-protocol.md protocols/uni-design-protocol.md
diff .claude/protocols/uni/uni-delivery-protocol.md protocols/uni-delivery-protocol.md
diff .claude/protocols/uni/uni-bugfix-protocol.md protocols/uni-bugfix-protocol.md
diff .claude/protocols/uni/uni-agent-routing.md protocols/uni-agent-routing.md
```

All four diffs must produce zero output. If any diff shows differences, resolve them
before proceeding. The `.claude/protocols/uni/` directory is the source of truth —
apply any needed corrections there first, then re-copy.

---

## Step 7b: Sync uni-retro Distribution Copy

Copy the uni-retro skill to the distributable `skills/` directory at repo root:

```bash
cp .claude/skills/uni-retro/SKILL.md skills/uni-retro/SKILL.md
```

Verify the copy is identical to its source:

```bash
diff .claude/skills/uni-retro/SKILL.md skills/uni-retro/SKILL.md
```

The diff must produce zero output.

---

## Step 7: Create Release Commit

Stage only the release-related files:
```bash
git add Cargo.toml packages/unimatrix/package.json packages/unimatrix-linux-x64/package.json packages/unimatrix-linux-arm64/package.json CHANGELOG.md protocols/ skills/uni-retro/
```

Commit with the release message:
```bash
git commit -m "release: v{new_version}"
```

---

## Step 8: Create Git Tag

```bash
git tag "v{new_version}"
```

---

## Step 9: Push Commit and Tag

```bash
git push origin HEAD
git push origin "v{new_version}"
```

The tag push triggers the release pipeline defined in `.github/workflows/uni-release.yml`.

---

## Step 10: Print Summary

```
Release v{new_version} complete.

Version: {old_version} -> {new_version}

Files modified:
  - Cargo.toml (workspace version)
  - packages/unimatrix/package.json
  - packages/unimatrix-linux-x64/package.json
  - packages/unimatrix-linux-arm64/package.json
  - CHANGELOG.md
  - protocols/ (synced from .claude/protocols/uni/)
  - skills/uni-retro/SKILL.md (synced from .claude/skills/uni-retro/)

Git:
  - Commit: release: v{new_version}
  - Tag: v{new_version}
  - Pushed to origin

CI pipeline: https://github.com/anthropic/unimatrix/actions
```

---

## Error Reference

| Condition | Action |
|-----------|--------|
| No bump level or version provided | Ask the user to specify one |
| Invalid explicit version (not semver) | Stop with diagnostic |
| New version <= current version | Stop: "New version must be greater than {current}" |
| Git tag already exists | Stop: "Tag v{version} already exists" |
| Uncommitted changes in worktree | Stop: "Clean worktree required for release" |
| `cargo check` fails | Stop: "Build check failed, review changes" |
