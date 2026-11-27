# Test Plan — C11 ownership (`lib/hook-ownership.js`) — extend `test/hook-ownership.test.js`

Risk: R-20. AC-4 support.

- `isLegacyOpencodePlugin` true: `@dug-21/unimatrix-opencode-plugin`, `...@1.2.3`, `...@latest`, `...@^0.4`.
- false: `...-extra`, `@other/unimatrix-opencode-plugin`, `unimatrix-opencode-plugin`, `file:...`, trailing `@`, `...@a/b`, `null`, `42`, object.
- Existing ownership predicates unchanged (all prior tests green).
