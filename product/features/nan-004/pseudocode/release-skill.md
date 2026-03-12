# C11: Release Skill — Pseudocode

## Purpose

Human-initiated `/release` skill for Claude Code. Guides the maintainer through version bump, changelog generation, commit, tag, and push. The tag push triggers the CI pipeline (C10).

## New File: .claude/skills/release/SKILL.md

```markdown
# /release — Create a Unimatrix Release

## Steps

1. Ask the user for the bump level (major/minor/patch) or an explicit version string.

2. Read the current version from `[workspace.package] version` in root `Cargo.toml`.

3. Compute the new version:
   - If bump level: increment the appropriate semver component.
   - If explicit version: validate it is valid semver and greater than current.

4. Update root `Cargo.toml`:
   - Set `[workspace.package] version = "{new_version}"`.

5. Update all npm `package.json` files:
   - `packages/unimatrix/package.json` -> set `version` to new_version.
   - `packages/unimatrix-linux-x64/package.json` -> set `version` to new_version.
   - `packages/unimatrix/package.json` -> set `optionalDependencies.@dug-21/unimatrix-linux-x64` to new_version.

6. Generate CHANGELOG.md entries:
   - Find the previous `v*` tag (or first commit if no prior tag).
   - Parse conventional commits between previous tag and HEAD.
   - Group by type: Features (feat:), Fixes (fix:), Breaking Changes (BREAKING CHANGE or !).
   - Prepend a new section to CHANGELOG.md:
     ```
     ## [new_version] - YYYY-MM-DD

     ### Features
     - commit message (PR #NNN)

     ### Fixes
     - commit message (PR #NNN)
     ```
   - If CHANGELOG.md does not exist, create it with a header.

7. Verify the changes compile:
   - Run `cargo check` to ensure the workspace version change does not break builds.

8. Create release commit:
   - Stage: root Cargo.toml, all npm package.json files, CHANGELOG.md.
   - Commit message: `release: v{new_version}`

9. Create git tag:
   - `git tag v{new_version}`

10. Push commit and tag:
    - `git push origin HEAD`
    - `git push origin v{new_version}`
    - The tag push triggers the release workflow (C10).

11. Print summary:
    - New version
    - Files modified
    - Tag created
    - CI pipeline URL (if available)
```

## New File: CHANGELOG.md (initial, created by first release)

```markdown
# Changelog

All notable changes to Unimatrix are documented here.
Format based on [Keep a Changelog](https://keepachangelog.com/).
```

## Changelog Entry Format

```
## [0.5.0] - 2026-03-12

### Features
- npm distribution via @dug-21/unimatrix (#NNN)
- Binary renamed from unimatrix-server to unimatrix (#NNN)
- Version subcommand: unimatrix version (#NNN)
- Model download subcommand for postinstall (#NNN)
- Init command for project wiring (#NNN)

### Breaking Changes
- Binary renamed from unimatrix-server to unimatrix
```

## Version Bump Logic

```
FUNCTION computeNewVersion(current: string, bump: string) -> string:
    LET [major, minor, patch] = current.split(".").map(Number)

    MATCH bump:
        "major" -> RETURN (major + 1) + ".0.0"
        "minor" -> RETURN major + "." + (minor + 1) + ".0"
        "patch" -> RETURN major + "." + minor + "." + (patch + 1)
        explicit version string -> validate semver, RETURN it
    END MATCH
```

## Conventional Commit Parsing

```
FUNCTION parseConventionalCommits(range: string) -> { features: string[], fixes: string[], breaking: string[] }:
    LET log = git log $range --format="%H %s"
    LET features = [], fixes = [], breaking = []

    FOR EACH line IN log:
        LET [hash, ...messageParts] = line.split(" ")
        LET message = messageParts.join(" ")

        IF message starts with "feat:":
            features.push(message.replace("feat: ", ""))
        ELSE IF message starts with "fix:":
            fixes.push(message.replace("fix: ", ""))
        END IF

        // Check body for BREAKING CHANGE
        LET body = git log -1 $hash --format="%b"
        IF body contains "BREAKING CHANGE" OR message contains "!:":
            breaking.push(message)
        END IF
    END FOR

    RETURN { features, fixes, breaking }
```

## Error Handling

| Condition | Behavior |
|-----------|----------|
| No bump level or version provided | Prompt user to provide one |
| Invalid explicit version (not semver) | Error with diagnostic |
| New version <= current version | Error: "new version must be greater than current" |
| cargo check fails after version update | Error: "build check failed, review changes" |
| git tag already exists | Error: "tag v{version} already exists" |
| Uncommitted changes in worktree | Error: "clean worktree required for release" |

## Key Test Scenarios

1. Patch bump: 0.5.0 -> 0.5.1. Root Cargo.toml and all package.json updated.
2. Minor bump: 0.5.0 -> 0.6.0.
3. Major bump: 0.5.0 -> 1.0.0.
4. Explicit version: "0.7.0".
5. CHANGELOG.md created on first release.
6. CHANGELOG.md prepended (not appended) on subsequent releases.
7. Release commit message: `release: v{version}`.
8. Git tag created: `v{version}`.
9. optionalDependencies version in root package.json updated.
