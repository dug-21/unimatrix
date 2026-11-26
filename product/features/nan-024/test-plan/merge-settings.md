# Test Plan: merge-settings (`lib/merge-settings.js`)

Risks: R-03, R-08, R-12, R-13, R-17, R-19, AC-04/05/13. Files: update `merge-settings.test.js`, `dogfood-effect.test.js`, `init-remote.test.js` (counts non-decreasing; old-string tests updated in place); new `cross-scope-dedupe.test.js`.

- `buildShimCommand`: Unix `'<abs>' hook E`; Windows `"<abs fwd-slash>.cmd" hook E`; Codex appends ` --provider codex-cli`; refuses `'` (Unix), `%`/`"` (Windows); byte-fixed across simulated upgrade (D-028): init, bump version, re-init from another cwd -> `~/.codex/hooks.json` byte-identical; re-init over a current entry writes nothing. D-090 extension: the same byte-equality test also spans a `trust.js` refresh (v1 -> v2 upgrade replaces `bin/trust.js`; hook command in `settings.json`/`~/.codex/hooks.json` byte-identical, file mtime/bytes untouched).
- Claude hooks written only to `.claude/settings.local.json`; foreign hooks, extra keys, custom indent preserved; owned-free files byte+mtime unchanged; temp+rename; malformed JSON -> abort untouched.
- Table-driven legacy migration (AC-05): one case per L1..L6 + prior shim -> exactly one owned entry per event, foreign preserved, negative foreign cases untouched.
- Cross-scope (AC-13, D-010): seed owned entries in Claude project `settings.json`, `settings.local.json`, user settings; run init/wire sequences (init, wire, init+wire, wire+init, repeated); count across scopes <= 1 per event, all in `settings.local.json`; foreign diff empty.
- Local vs cloud byte-compare of whole written files per OS family (AC-01, R-12) in `wiring-e2e`.
- `.gitignore`: when `settings.local.json` not ignored, init and `--dry-run` print the warning; `.gitignore` bytes unchanged (R-27). `normalizeCommandSource` string arm deleted: test asserts export absent.

## Rework (D-080): `buildShimCommand(event, provider, platform, home)`
- `home` additive: Unix output `'<home>/.unimatrix/bin/unimatrix' hook E` for home `/tmp/fx/h` and for `/tmp/fx/my home` (space): round-trip: write into a temp settings file, parse, run via `sh -c` -> shim stub invoked with argv `hook E` (space survives single quotes). Windows: `"C:/Users/My User/.unimatrix/bin/unimatrix.cmd" hook E` (forward slashes, quoted) with `platform:'win32'`, executed in the win32 CI cell via `cmd /C`.
- `home` omitted -> falls back to `os.homedir()` (existing callers/tests unchanged and green); home containing `'` (Unix) or `%`/`"` (Windows) -> refuses (throws), nothing written.
- `buildShimCommand` output for given `(event, provider, platform, home)` is a pure function: same inputs byte-identical across calls (D-028 stability); `isUnimatrixCommand(buildShimCommand(...))` true for every event x provider x platform x home-with-space (round-trip with hook-ownership).
