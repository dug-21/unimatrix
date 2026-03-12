# Test Plan: C4 — Init Command

## Unit Tests (packages/unimatrix/test/init.test.js)

All tests use a temporary directory as project root with a `.git` directory created in setup.

### Project Root Detection

- `test_finds_git_in_current_dir`: Run init from a dir containing `.git`. Assert project root equals that dir.
- `test_walks_up_to_git`: Run init from a subdirectory (`project/src/`). Assert project root is `project/`.
- `test_no_git_errors_with_diagnostic`: Run init from a temp dir with no `.git` anywhere up the tree. Assert error message contains "could not find project root".
- `test_stops_at_filesystem_root`: No `.git` found before `/`. Assert error, not infinite loop.

### .mcp.json Writing

- `test_creates_mcp_json_on_clean_project`: No `.mcp.json` exists. After init, assert file exists with `mcpServers.unimatrix.command` containing the absolute binary path.
- `test_preserves_existing_servers`: `.mcp.json` has `{ "mcpServers": { "filesystem": { "command": "..." } } }`. After init, assert `filesystem` entry is unchanged and `unimatrix` entry is added.
- `test_updates_existing_unimatrix_entry`: `.mcp.json` has a stale `unimatrix` entry. After init, assert command path is updated.
- `test_preserves_nested_env_args_in_other_servers`: Other server entries with `env`, `args`, `cwd` fields are preserved exactly.

### Skill File Copying

- `test_copies_13_skill_dirs`: After init, `.claude/skills/` contains all 13 expected subdirectories each with `SKILL.md`.
- `test_overwrites_existing_unimatrix_skills`: Pre-create `.claude/skills/unimatrix-init/SKILL.md` with custom content. After init, assert it is overwritten with package content.
- `test_preserves_non_unimatrix_skills`: Pre-create `.claude/skills/custom-skill/SKILL.md`. After init, assert `custom-skill/` is untouched.

### DB Pre-Creation

- `test_calls_binary_with_project_dir`: Assert init invokes `execFileSync` with args `['version', '--project-dir', projectRoot]` (or equivalent). Mock the binary to capture args.

### Validation Step

- `test_validates_binary_by_running_version`: Assert init calls the binary with `version` subcommand and captures stdout.
- `test_reports_diagnostic_on_validation_failure`: Mock binary to exit with code 1 and stderr output. Assert init reports the stderr content in its error.

### Dry Run

- `test_dry_run_does_not_write_files`: Run init with `{ dryRun: true }`. Assert no `.mcp.json`, `.claude/settings.json`, or `.claude/skills/` created.
- `test_dry_run_prints_planned_actions`: Assert stdout contains descriptions of each planned action.

### Summary Output

- `test_prints_unimatrix_init_suggestion`: Assert init output contains `/unimatrix-init`.

## Integration Tests (Shell)

- `test_init_end_to_end`: In a temp dir with `.git`, install the package (or symlink), run `node bin/unimatrix.js init`. Verify `.mcp.json` exists with correct path, `.claude/settings.json` has 7 hook events, skills copied.
- `test_init_idempotent`: Run init twice. Diff `.claude/settings.json` before and after second run -- no semantic change. Parse JSON and assert exactly 7 unimatrix hook entries.

## Edge Cases

- `.claude/settings.json` is a directory (not file): init errors with diagnostic.
- Binary path contains spaces: JSON handles escaping, but hook command string must work in shell.
- Init from symlinked directory: project root resolves to real path.

## Risk Coverage

| Risk ID | Scenario | Test |
|---------|----------|------|
| R-02 | Init writes absolute path | `test_creates_mcp_json_on_clean_project` |
| R-04 | Duplicate hooks on repeated runs | `test_init_idempotent` |
| R-09 | .mcp.json drops existing servers | `test_preserves_existing_servers`, `test_preserves_nested_env_args_in_other_servers` |
| R-10 | Skill overwrite without warning | `test_overwrites_existing_unimatrix_skills`, `test_preserves_non_unimatrix_skills` |
| R-11 | JS/Rust project root divergence | `test_calls_binary_with_project_dir` (passes same root to binary) |
