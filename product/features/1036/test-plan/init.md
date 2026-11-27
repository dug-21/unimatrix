# Test Plan — C13 init (`lib/init.js`) — extend `test/init-integration.test.js`, `init.test.js`, `init-remote.test.js`

Risks: R-01, R-03, R-07, R-08, R-14, R-25. Discharges AC-1(c), AC-2(b), AC-3 (project), AC-5, AC-8, AC-9, S-2, S-3. All spawned via `bin/unimatrix.js` with `scrubbedEnv`.

## Skills / remote subtree
- `cleanStaleRemoteSubtree` and skill install via writer; skills with `containWithin:R`.
- `.claude/skills/<n>/SKILL.md -> H/victim.txt` → exit 1 `outside-project`, victim unchanged; in-root link written through.

## D-026 spawned (AC-3 project)
- `settings.local.json`, `.mcp.json`, `opencode.json`, `.codex/hooks.json`, `.codex/config.toml` → outside-R targets: exit 1 naming link+target, unchanged, no temp — real run and `--dry-run`.
- `.claude -> /outside`, prefix sibling, root via symlink (inside OK), dangling (`dangling-symlink`), link to dir (`not-regular`).
- Grep: no `isWithinProject` remains.

## D-025 (S-3, R-01 scenarios 1-9, named)
- `remote.json` 0644 credential changed → 0600 + new bytes; credential unchanged → 0600, bytes equal, leg `updated`.
- `bin/shim-entry.js`, `bin/trust.js`, shim at 0775 → 0700; subsequent hook event through installed shim passes trust check.
- `install.json`, `trust.json`, root marker → 0600; trust check passes.
- Control: `settings.local.json`, `~/.claude.json`, `opencode.json`, skill file keep 0644/0664.
- umask 000/002/022; second init no chmod/rename.

## AC-5
- Spawn init twice in real git repo; ×{no override, each override}, ×{`--bundle`}, twins seeded before run 1. Run 2: every leg `unchanged`; bytes/`mtimeMs`/mode equal for all resolved, twin, project, skill, exclude and `~/.unimatrix/*` files; no D-013 notice. Mode-drift fixtures converge to `unchanged` on run after repair.

## AC-8
- Spawned init in repo, worktree, submodule, non-repo, no-git PATH (see git-exclude.md); porcelain clean; diffs clean.

## AC-9 parity harness
- Helper `parity(fixture)`: tree hash + modes before; spawn `--dry-run`; assert hash/modes unchanged, no temp/dir; capture manifest, action lines, `resolved:` lines, D-013 notice; spawn real init; assert manifest equality on (harness, surface, action, path, reason, entry/command); changed-file set == non-unchanged non-skipped dry-run paths. Allowed diff: `appended-not-effective`. Run over every AC-1..AC-8 fixture incl. mode-drift and outside-root (exit 1 both modes).

## Real binaries (R-25)
- AC-1(c): HOME=tmp, `CLAUDE_CONFIG_DIR=<tmp>/cfg`, cwd=R; after init `claude mcp get unimatrix` exit 0, stdout contains `unimatrix`; default `~/.claude.json` seeded entry still present. Skip: `claude not on PATH; real-binary check tracked in #1053`.
- AC-2(b): `OPENCODE_CONFIG_DIR=<tmp>/cfg` and XDG-only cases + both-set precedence; `opencode debug config --standalone` (helper asserts flag), bounded timeout, lenient match of `path` and `mcp.servers.unimatrix|mcp.unimatrix`; no owned entry in `<tmp>/.config/opencode/opencode.json` doc; parse failure fails with raw output. Skip text names opencode + #1053.

## S-2
- Each var × {"", "  ", ~/x, cfg, NUL, abs/}: one stderr warning naming var (none for ""), manifest == no-override manifest.
