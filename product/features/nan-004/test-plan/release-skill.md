# Test Plan: C11 — Release Skill

## File Presence Validation

- `test_skill_file_exists`: Assert `.claude/skills/release/SKILL.md` exists.
- `test_skill_content_has_version_bump_steps`: Assert the SKILL.md content mentions updating `Cargo.toml` workspace version and npm `package.json` files.
- `test_skill_content_has_changelog_step`: Assert SKILL.md mentions generating CHANGELOG.md from conventional commits.
- `test_skill_content_has_tag_step`: Assert SKILL.md mentions creating a `v{version}` git tag.

## CHANGELOG Generation

- `test_changelog_groups_by_type`: After generating a changelog from test commits (`feat:`, `fix:`, `docs:`), assert CHANGELOG.md contains `### Features`, `### Fixes` sections.
- `test_changelog_includes_commit_references`: Assert entries include short commit hash or PR number.
- `test_changelog_covers_range_since_last_tag`: With a prior `v0.5.0` tag and new commits, assert only new commits appear.

## Integration (Manual)

The `/release` skill is a human-guided skill invoked in Claude Code. It cannot be fully automated. Manual verification:
1. Invoke `/release` with `patch` bump.
2. Verify Cargo.toml updated to `0.5.1`.
3. Verify all package.json files updated to `0.5.1`.
4. Verify CHANGELOG.md generated.
5. Verify commit message is `release: v0.5.1`.
6. Verify git tag `v0.5.1` created.

## Risk Coverage

| Risk ID | Scenario | Test |
|---------|----------|------|
| R-06 | Version drift | `test_skill_content_has_version_bump_steps` (skill guides sync) |
