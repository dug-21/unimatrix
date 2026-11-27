# Test Plan — C8 Claude hooks (`lib/merge-settings.js`) — extend `test/merge-settings.test.js`

Risks: R-02, R-03, R-15, R-16, R-05 (R4).

- `mergeSettings` into `settings.local.json` via `updateFileSync` with `containWithin:R`: created/updated/unchanged; re-mutate → `null` for tab/2/4 indent, CRLF, no newline.
- Foreign hooks in same event arrays preserved in order.
- `removeOwnedHooks` on `H/.claude/settings.json` (R4) with `CLAUDE_CONFIG_DIR=D`: owned removed, foreign kept (S-6, D-030); `CLAUDE_CONFIG_DIR=H` same-file → no wrongful removal.
- Unparseable project `settings.json`/`settings.local.json` → exit 1, file unchanged.
- `settings.local.json` linked outside R → `outside-project`; inside R → written through.
- Existing file mode 0644 kept after content change.
