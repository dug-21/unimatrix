# Test Plan: config.js (resolution + root walk + hash)

Oracle: `project.rs:130-136` (hash), `lib/init.js:26-41` (`detectProjectRoot`). ADR-006.
Risks: R-09 (High), R-14, R-16. Suite: `test/hook-client/config.test.js` (temp-dir fixtures).

## FR-06 Resolution Matrix (R-09 — every no-config row also proves NO network via stub request log == empty, plus breadcrumb written, stderr line, exit 0, no stdout)

| # | Test | Setup | Expected |
|---|---|---|---|
| 1 | `test_env_pair_wins_over_file` | both env vars + valid file with different values | env values used; file not consulted for url/token |
| 2 | `test_partial_env_pair_is_misconfig` | only `UNIMATRIX_REMOTE_URL` (then only `UNIMATRIX_REMOTE_TOKEN`) | misconfiguration: breadcrumb class `auth`, exit 0, no network |
| 3 | `test_file_resolution_happy` | no env; `{root}/.claude/settings.local.json` with `unimatrix.remote` | url+token resolved |
| 4 | `test_missing_file` | no env, no file | no network, breadcrumb, exit 0 |
| 5 | `test_file_without_remote_key` | file present, no `unimatrix.remote` | same as 4 (Claude Code key-drop simulation: write key, remove it, next spawn degrades silently; init re-run restores — paired with init-remote.md idempotency) |
| 6 | `test_malformed_settings_json` | unparseable file | no throw, no network, breadcrumb |
| 7 | `test_subdirectory_cwd` | spawn cwd = `{root}/a/b/c` | root found via `.git` walk; config read from `{root}` |
| 8 | `test_stdin_cwd_overrides_process_cwd` | stdin `cwd` ≠ `process.cwd()`; also stdin `cwd` empty → fallback | root derived from stdin `cwd` when non-empty |

## Root Walk + Hash (split-brain prevention)

- `test_nested_git_monorepo_nearest_root_wins` — `{outer}/.git` and `{outer}/pkg/.git`, cwd inside `pkg/sub` → root = `{outer}/pkg`. **Split-brain assertion**: the root string used for config lookup is `===` the string fed to the state-dir hash.
- `test_no_git_root_is_resolved_cwd` — no `.git` anywhere up to fs root → root = resolved cwd; one stat-walk, one file read (fs spy: no multi-location probing).
- `test_hash_parity_with_rust` — `compute hash("…path…")` = first 16 hex of SHA-256 of the root path string; golden values generated from `project.rs::compute_project_hash` for ≥3 paths (ASCII, non-ASCII, trailing-slash-normalized as Rust does) committed as fixtures — no hand-written expected values (#2984).
- `test_state_dir_path_shape` — `~/.unimatrix/{hash}/hook-client/`.

## Cross-Platform (R-14 — included in OS-matrix CI run)

- `test_root_walk_windows_separators` — walk works with `\` separators and drive-letter roots (Windows runner).
- `test_homedir_resolution` — `os.homedir()` used; `HOME` unset → no throw, breadcrumb path degraded gracefully (see state.md `~`-unresolvable case).

## Security (R-16 adjunct)

- Timeout-override block in `settings.local.json` parsed; non-numeric overrides ignored, defaults (750/2000/3000) retained — defaults are ACCEPTED, no test questions the values.
- Resolved token never echoed: assert token string absent from stderr output across matrix rows.

## Concrete Assertions

- `resolveConfig(input)` returns `{url, token, timeouts}` or a typed "unconfigured"/"misconfigured" result — never throws.
- Misconfigured (partial pair) and unconfigured both exit 0 with no network, distinct breadcrumb classes (`auth` vs absent-config behavior per pseudocode).
