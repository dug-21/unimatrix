## ADR-003: Init Command Implemented in JavaScript, Not Rust

### Context

The `npx unimatrix init` command must wire `.mcp.json`, `.claude/settings.json`, copy skill files, and pre-create the database. This logic could live in the Rust binary (new `Init` subcommand) or in JavaScript (executed by the npm shim).

Arguments for Rust:
- Single binary, no Node.js dependency for init.
- Can reuse `detect_project_root` and `ensure_data_directory` directly.
- Type safety.

Arguments for JavaScript:
- Init needs to resolve its own npm package location (`require.resolve`) to find bundled skills and determine the binary's absolute path.
- JSON merge for `.claude/settings.json` is trivial in JS, complex in Rust (serde_json preserves order but array manipulation is verbose).
- The init command only runs once per project setup — performance is irrelevant.
- The Rust binary does not know where it is installed within `node_modules` (it has no awareness of the npm package structure).

### Decision

Implement the init command in JavaScript (`packages/unimatrix/lib/init.js`).

The JS shim (`bin/unimatrix.js`) intercepts `process.argv[2] === 'init'` and delegates to `lib/init.js` instead of exec'ing the Rust binary. All other subcommands pass through to the Rust binary.

The init command delegates to the Rust binary for operations that benefit from Rust code:
- Database pre-creation: `unimatrix version --project-dir <root>` triggers `Store::open()` + migration.
- Validation: `unimatrix version` confirms the binary executes on this platform.

The init command handles in JavaScript:
- Project root detection (walk up to `.git` — mirrors the Rust algorithm).
- Binary path resolution (`require.resolve`).
- `.mcp.json` creation/merge (JSON manipulation).
- `.claude/settings.json` merge (complex JSON merge via `merge-settings.js`).
- Skill file copying (fs operations).
- Summary output.

### Consequences

**Easier:**
- `require.resolve` gives the exact npm package location — no path guessing.
- JSON merge logic is straightforward in JavaScript.
- Skill files are co-located with the init script in the npm package.
- No Rust compilation dependency for init logic changes.

**Harder:**
- Project root detection is duplicated (JS mirrors Rust). If the algorithm changes, both must be updated. This is acceptable because the algorithm is stable (walk up to `.git`) and simple.
- The `npx unimatrix init` invocation goes through Node.js, not directly to the Rust binary. This adds ~100ms startup but init runs once per project setup.
- The JS shim must distinguish `init` from all other subcommands (simple string check on argv).
