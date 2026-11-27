# Test Plan — C10 OpenCode (`lib/opencode-install.js`) — extend `test/opencode-install.test.js`

Risks: R-05, R-12, R-15, R-16, R-19, R-20. AC-2(a).

- `OPENCODE_CONFIG_DIR=C` → entry at `C/opencode.json`; owned twin at `H/.config/opencode/opencode.json` removed, foreign keys deep-equal.
- `XDG_CONFIG_HOME=X` → `X/opencode/opencode.json`; both set → `OPENCODE_CONFIG_DIR` wins, `X/opencode` untouched.
- FR-13/D-015: owned twin only, no `--harness opencode` → resolved file written, twin removed.
- Same-file (`XDG_CONFIG_HOME=H/.config`, `OPENCODE_CONFIG_DIR=H/.config/opencode`) → entry survives, no removal leg.
- Unparseable user `opencode.json` → warning, exit 0, twin kept (R-11) (D-024).
- Unparseable project `opencode.json` during RT-08/09 removal → exit 1 naming file, untouched (D-029).
- G3 legacy `plugin[]` removal in same mutate as project `mcp.unimatrix` removal; order preserved; `[]` stays `[]`; non-array no leg.
- Rename fault → rethrow, exit 1 (R-12).
- Foreign at `mcp.unimatrix` → kept + warning.
- Indent detection reused; re-mutate → `null`.
