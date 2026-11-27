# Test Plan — C9 Codex (`lib/codex-install.js`) — extend `test/codex-install.test.js`, `codex-toml-surgical.test.js`

Risks: R-02, R-12, R-15, R-16, R-03.

- `codexHome(env,home)` re-exported, equals resolver output (existing D-085 tests unchanged).
- `hooks.json`/`config.toml` writes via `updateFileSync`; TOML with comments/odd spacing: write then re-mutate → `null`; bytes outside owned table identical.
- Unparseable Codex file → warning, `skipped-malformed`, exit 0.
- Injected `SafeWriteError` (rename fault) → rethrown, init exit 1 naming file (R-12, #4473).
- Project `.codex/*` link outside R → `outside-project`; `$CODEX_HOME` link outside home followed.
- Real-binary: none (D-012); test skip/notes name #1053.
