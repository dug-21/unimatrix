# Test Plan: merge-settings (`lib/merge-settings.js`)

Risks: R-03, R-08, R-12, R-13, R-17, R-19, AC-04/05/13. Files: update `merge-settings.test.js`, `dogfood-effect.test.js`, `init-remote.test.js` (counts non-decreasing; old-string tests updated in place); new `cross-scope-dedupe.test.js`.

- `buildShimCommand`: Unix `'<abs>' hook E`; Windows `"<abs fwd-slash>.cmd" hook E`; Codex appends ` --provider codex-cli`; refuses `'` (Unix), `%`/`"` (Windows); byte-fixed across simulated upgrade (D-028): init, bump version, re-init from another cwd -> `~/.codex/hooks.json` byte-identical; re-init over a current entry writes nothing.
- Claude hooks written only to `.claude/settings.local.json`; foreign hooks, extra keys, custom indent preserved; owned-free files byte+mtime unchanged; temp+rename; malformed JSON -> abort untouched.
- Table-driven legacy migration (AC-05): one case per L1..L6 + prior shim -> exactly one owned entry per event, foreign preserved, negative foreign cases untouched.
- Cross-scope (AC-13, D-010): seed owned entries in Claude project `settings.json`, `settings.local.json`, user settings; run init/wire sequences (init, wire, init+wire, wire+init, repeated); count across scopes <= 1 per event, all in `settings.local.json`; foreign diff empty.
- Local vs cloud byte-compare of whole written files per OS family (AC-01, R-12) in `wiring-e2e`.
- `.gitignore`: when `settings.local.json` not ignored, init and `--dry-run` print the warning; `.gitignore` bytes unchanged (R-27). `normalizeCommandSource` string arm deleted: test asserts export absent.
