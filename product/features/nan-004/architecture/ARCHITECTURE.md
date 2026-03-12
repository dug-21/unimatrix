# nan-004: Versioning & Packaging — Architecture

## System Overview

nan-004 introduces a distribution layer for Unimatrix, converting it from a build-from-source project into an installable npm package. The feature spans four distinct subsystems:

1. **npm package structure** — The esbuild/turbo optionalDependencies pattern for platform-specific binary distribution.
2. **CLI restructure** — Renaming `unimatrix-server` to `unimatrix` and adding the `init` subcommand for project wiring.
3. **Version synchronization** — Lockstep versioning across 9 Rust crates and npm packages, driven from `Cargo.toml`.
4. **Release pipeline** — GitHub Actions workflow for cross-compilation and npm publishing.

The feature does NOT touch the Unimatrix engine, storage, or MCP protocol internals. It wraps the existing binary in a distribution mechanism and adds a mechanical project setup command.

### Relationship to Existing Features

- **nan-003** (Onboarding Skills): nan-004 installs skill files mechanically; nan-003 skills run after to populate knowledge interactively.
- **nan-001/nan-002** (Export/Import): The `export` and `import` subcommands become `unimatrix export` and `unimatrix import` after the binary rename.
- **vnc-004** (PID lifecycle): The init command pre-creates the data directory and database, interacting with the same `~/.unimatrix/{hash}/` structure that PID guards protect.

## Component Breakdown

### C1: npm Package Structure (`packages/`)

**Responsibility**: Hold the distributable artifacts — JS shim, platform binaries, bundled skills, postinstall logic, and init command.

Files:
```
packages/
  unimatrix/                          # Root: @dug-21/unimatrix
    package.json                      # bin, optionalDependencies, postinstall, scripts
    bin/unimatrix.js                  # JS shim: resolve platform binary, exec with args
    postinstall.js                    # ONNX model pre-download (graceful failure)
    lib/
      resolve-binary.js              # Shared: locate platform binary from optionalDeps
      init.js                        # Init command: project wiring logic
      merge-settings.js              # settings.json merge (isolated for testability)
    skills/                           # Bundled copies of all 13 skill dirs
      unimatrix-init/SKILL.md
      unimatrix-seed/SKILL.md
      ... (all 13)
  unimatrix-linux-x64/               # Platform: @dug-21/unimatrix-linux-x64
    package.json                      # os: ["linux"], cpu: ["x64"]
    bin/unimatrix                     # Pre-compiled Rust binary (stripped)
```

### C2: JS Shim (`bin/unimatrix.js`)

**Responsibility**: Entry point registered in npm `bin` field. Resolves the platform-specific binary from optionalDependencies and exec's it, forwarding all arguments and stdio.

Behavior:
- Calls `resolve-binary.js` to find the platform binary path.
- Uses `child_process.execFileSync` with `{ stdio: 'inherit' }` for transparent passthrough.
- If `process.argv[2] === 'init'`, delegates to `lib/init.js` instead of exec'ing the Rust binary (init is JS-only logic).
- Exit code passthrough from the Rust binary.
- Clear error message if no platform binary found (lists supported platforms).

### C3: Binary Resolution (`lib/resolve-binary.js`)

**Responsibility**: Determine which platform package is installed and return the absolute path to its binary.

Strategy:
- Platform map: `{ "linux-x64": "@dug-21/unimatrix-linux-x64" }` (extensible for future darwin-arm64, darwin-x64).
- Resolution: `require.resolve('@dug-21/unimatrix-linux-x64/bin/unimatrix')`.
- Fallback: Check `UNIMATRIX_BINARY` environment variable (development/testing override).
- Error: Throw with platform info and supported list if resolution fails.

### C4: Init Command (`lib/init.js`)

**Responsibility**: Mechanical project wiring — deterministic, non-interactive, idempotent.

Steps (in order):
1. Resolve project root (walk up to `.git`, same algorithm as `detect_project_root` in Rust).
2. Resolve binary path via `resolve-binary.js`.
3. Write/merge `.mcp.json` — add/update `unimatrix` server entry with absolute binary path.
4. Write/merge `.claude/settings.json` — add/update hook entries for all 7 events (via `merge-settings.js`).
5. Copy skill files from `skills/` into `.claude/skills/` (overwrite existing, preserve non-unimatrix).
6. Pre-create data directory and database — exec `unimatrix --project-dir <root> version` (or a health subcommand) to trigger `ensure_data_directory` + `Store::open` + `migrate_if_needed`.
7. Validate — exec the binary with a version/health check to confirm it runs.
8. Print summary of actions taken.

Flags:
- `--dry-run`: Print actions without modifying files.
- No other flags. All paths are auto-detected.

### C5: Settings Merge (`lib/merge-settings.js`)

**Responsibility**: Merge Unimatrix hook configuration into `.claude/settings.json` without corrupting existing user settings.

Exported interface:
```javascript
/**
 * @param {string} filePath - Path to .claude/settings.json
 * @param {string} binaryName - Binary name for hook commands (e.g., "unimatrix")
 * @param {object} options - { dryRun: boolean }
 * @returns {{ actions: string[], content: object }}
 */
function mergeSettings(filePath, binaryName, options)
```

Merge semantics:
- Read existing JSON (or `{}` if absent/malformed — warn on malformed, do not crash).
- For each of 7 hook events: find existing unimatrix hook entry by matching command prefix (`unimatrix ` or `unimatrix-server `). If found, update command. If not, append to event's array.
- Identification heuristic: a hook entry is "unimatrix" if its `command` field starts with `unimatrix ` or `unimatrix-server ` or contains `unimatrix hook` or `unimatrix-server hook`.
- Preserve all non-unimatrix hooks, permissions, and other top-level keys.
- Write with 2-space indentation, stable key ordering (JSON.stringify replacer).

### C6: Postinstall (`postinstall.js`)

**Responsibility**: Pre-download the ONNX model after `npm install`.

Behavior:
- Exec `unimatrix model-download` (a new thin subcommand — see C8) to download the model.
- If the binary is not found (platform mismatch), warn and exit 0.
- If download fails (network), warn and exit 0.
- If model already cached (`~/.cache/unimatrix-embed/`), skip. Exit 0.
- No project file modifications.

### C7: Binary Rename (Rust changes)

**Responsibility**: Rename the binary from `unimatrix-server` to `unimatrix`. Add `init` routing and `version` subcommand.

Changes to `crates/unimatrix-server/Cargo.toml`:
```toml
[[bin]]
name = "unimatrix"
path = "src/main.rs"
```

Changes to `main.rs` CLI structure:
```rust
#[derive(Parser)]
#[command(name = "unimatrix", about = "Unimatrix knowledge engine")]
struct Cli {
    #[arg(long)]
    project_dir: Option<PathBuf>,

    #[arg(long, short)]
    verbose: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    Hook { event: String },
    Export { #[arg(short, long)] output: Option<PathBuf> },
    Import { #[arg(short, long)] input: PathBuf, ... },
    /// Print version and exit.
    Version,
    /// Download the ONNX model to cache (for postinstall).
    ModelDownload,
}
```

Default (no subcommand) remains the MCP server mode — no behavioral change to the server itself.

### C8: Model Download Subcommand (Rust)

**Responsibility**: Expose ONNX model download as a CLI subcommand for postinstall use.

Calls `unimatrix_embed::EmbedConfig::default()` then `ensure_model()` synchronously. Prints progress to stderr. Exits 0 on success, 1 on failure.

### C9: Version Synchronization

**Responsibility**: All crates use `version.workspace = true`. npm package versions match.

Changes:
- Root `Cargo.toml`: add `version = "0.5.0"` to `[workspace.package]`.
- All 9 crate `Cargo.toml` files: replace `version = "0.1.0"` with `version.workspace = true`.
- `crates/unimatrix-server/Cargo.toml`: also move `edition` and `rust-version` to workspace inheritance.
- npm `package.json` files: version `0.5.0`, updated by the `/release` skill before each release.

### C10: GitHub Actions Release Pipeline (`.github/workflows/release.yml`)

**Responsibility**: Build, package, and publish on tagged releases.

Trigger: `push.tags: ['v*']`

Jobs:
1. **build-linux-x64**: Native build on `ubuntu-latest`.
   - Install Rust 1.89 via `dtolnay/rust-toolchain`.
   - `cargo build --release` (native, no cross-compilation).
   - Strip binary: `strip target/release/unimatrix`.
   - Run tests: `cargo test --release`.
   - Upload binary as artifact.
2. **package-npm**: Depends on build job.
   - Download binary artifact.
   - Copy binary into `packages/unimatrix-linux-x64/bin/unimatrix`.
   - Set executable permission.
   - Copy skill files from `.claude/skills/` into `packages/unimatrix/skills/`.
   - Sync version from `Cargo.toml` to all `package.json` files.
   - `npm publish` for `@dug-21/unimatrix-linux-x64`.
   - `npm publish` for `@dug-21/unimatrix`.
3. **create-release**: Create GitHub Release with changelog.

### C11: `/release` Skill (`.claude/skills/release/`)

**Responsibility**: Human-initiated release workflow. Bumps versions, generates changelog, creates tag.

Steps:
1. Accept bump level (major/minor/patch) or explicit version.
2. Update `[workspace.package] version` in root `Cargo.toml`.
3. Update all npm `package.json` files.
4. Generate CHANGELOG.md entries from conventional commits since last `v*` tag.
5. Create release commit: `release: v{version}`.
6. Create git tag: `v{version}`.
7. Push commit + tag.

## Component Interactions

```
User runs: npm install @dug-21/unimatrix
  --> npm resolves optionalDependencies
  --> @dug-21/unimatrix-linux-x64 downloaded (binary)
  --> postinstall.js runs
      --> resolve-binary.js finds binary
      --> execs: unimatrix model-download
      --> ONNX model cached to ~/.cache/unimatrix-embed/

User runs: npx unimatrix init
  --> bin/unimatrix.js intercepts "init" argument
  --> lib/init.js runs:
      --> detect project root (.git walk)
      --> resolve-binary.js finds binary path
      --> write .mcp.json (absolute path to binary)
      --> merge-settings.js merges hooks into .claude/settings.json
      --> copy skills/ --> .claude/skills/
      --> exec: unimatrix version --project-dir <root>  (triggers db creation)
      --> exec: unimatrix version  (validates binary runs)
      --> print summary

User runs: npx unimatrix hook SessionStart  (or any other subcommand)
  --> bin/unimatrix.js exec's the Rust binary with args
  --> Rust binary handles normally

Hook fires (Claude Code shell):
  --> .claude/settings.json has absolute path command
  --> Shell executes: /abs/path/to/node_modules/.../unimatrix hook SessionStart
  --> Rust binary runs hook subcommand directly
```

## Technology Decisions

| Decision | Choice | ADR |
|----------|--------|-----|
| Hook PATH resolution | Absolute paths in settings.json | ADR-001 |
| Binary rename strategy | Single rename with compatibility shim | ADR-002 |
| Init command runtime | Node.js (not Rust) for init logic | ADR-003 |
| settings.json merge approach | Prefix-match identification, append-or-update | ADR-004 |
| Version source of truth | Cargo.toml workspace version, npm synced at release | ADR-005 |

## Integration Points

### Existing Crates (Changes Required)

| Crate | Change | Reason |
|-------|--------|--------|
| `unimatrix-server` | Rename binary, add `Version` and `ModelDownload` subcommands | C7, C8 |
| `unimatrix-embed` | Expose `ensure_model()` as public (if not already) | C8 postinstall needs model download |
| All 9 crates | `version.workspace = true` | C9 version sync |

### Existing Files (Changes Required)

| File | Change | Reason |
|------|--------|--------|
| `Cargo.toml` (root) | Add `version = "0.5.0"` to `[workspace.package]` | C9 |
| `.mcp.json` | Update command path (self-hosting) | C7 binary rename |
| `.claude/settings.json` | Update hook commands from `unimatrix-server` to `unimatrix` | C7 binary rename |

### New Files

| File | Component |
|------|-----------|
| `packages/unimatrix/package.json` | C1 |
| `packages/unimatrix/bin/unimatrix.js` | C2 |
| `packages/unimatrix/lib/resolve-binary.js` | C3 |
| `packages/unimatrix/lib/init.js` | C4 |
| `packages/unimatrix/lib/merge-settings.js` | C5 |
| `packages/unimatrix/postinstall.js` | C6 |
| `packages/unimatrix/skills/` (13 dirs) | C1 |
| `packages/unimatrix-linux-x64/package.json` | C1 |
| `.github/workflows/release.yml` | C10 |
| `.claude/skills/release/SKILL.md` | C11 |
| `CHANGELOG.md` | C11 |

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `ProjectPaths` struct | `{ project_root, project_hash, data_dir, db_path, vector_dir, pid_path, socket_path }` | `crates/unimatrix-engine/src/project.rs:13-28` |
| `detect_project_root(override_dir: Option<&Path>) -> io::Result<PathBuf>` | Walks up to `.git`, resolves worktrees | `crates/unimatrix-engine/src/project.rs:40` |
| `compute_project_hash(project_root: &Path) -> String` | SHA-256[0:16] of canonical path | `crates/unimatrix-engine/src/project.rs:126` |
| `ensure_data_directory(override_dir, base_dir) -> io::Result<ProjectPaths>` | Creates `~/.unimatrix/{hash}/` | `crates/unimatrix-engine/src/project.rs:142` |
| `Store::open(db_path) -> Result<Store>` | Opens SQLite + runs `migrate_if_needed()` | `crates/unimatrix-store/src/lib.rs` |
| `CURRENT_SCHEMA_VERSION` | `11` (u64) | `crates/unimatrix-store/src/migration.rs:18` |
| `EmbedConfig::default()` | Model name, cache dir, dimensions | `crates/unimatrix-embed/src/lib.rs` |
| Rust binary CLI | `unimatrix [hook|export|import|version|model-download]` | `crates/unimatrix-server/src/main.rs` |
| npm bin shim | `npx unimatrix [init|hook|export|import|version|...]` | `packages/unimatrix/bin/unimatrix.js` |
| `.mcp.json` server entry | `{ "mcpServers": { "unimatrix": { "command": "<abs-path>" } } }` | Project root |
| `.claude/settings.json` hook entry | `{ "type": "command", "command": "<abs-path> hook <Event>" }` | `.claude/settings.json` |
| `resolve-binary() -> string` | Returns absolute path to platform binary | `packages/unimatrix/lib/resolve-binary.js` |
| `mergeSettings(filePath, binaryName, options) -> { actions, content }` | Merge hooks into settings.json | `packages/unimatrix/lib/merge-settings.js` |

## Hook Command Format

After init, hook commands in `.claude/settings.json` use absolute paths:

```json
{
  "type": "command",
  "command": "/home/user/my-project/node_modules/@dug-21/unimatrix-linux-x64/bin/unimatrix hook SessionStart"
}
```

The `UserPromptSubmit` hook retains its tee-to-log pattern:
```json
{
  "type": "command",
  "command": "/home/user/my-project/node_modules/@dug-21/unimatrix-linux-x64/bin/unimatrix hook UserPromptSubmit | tee -a ~/.unimatrix/injections/hooks.log"
}
```

## Delivery Phasing

Recommended delivery order based on dependency graph and risk:

**Wave 1 — Foundation (parallel):**
- C7: Binary rename (Rust) — unblocks all downstream
- C9: Version synchronization (Cargo.toml changes) — independent

**Wave 2 — Package structure (depends on Wave 1):**
- C1: npm package structure (package.json files, directory layout)
- C2: JS shim
- C3: Binary resolution
- C8: Model download subcommand

**Wave 3 — Init command (depends on Wave 2):**
- C4: Init command
- C5: Settings merge
- C6: Postinstall

**Wave 4 — Release infrastructure (depends on Wave 1-3):**
- C10: GitHub Actions workflow
- C11: `/release` skill

## Open Questions

1. **ONNX shared library bundling**: The `ort` crate links the ONNX Runtime. Is it statically linked into the binary, or does it produce a separate `.so`? If separate, the platform package must bundle both. Validate with `ldd target/release/unimatrix` on the CI build.

2. **Binary size with ONNX**: SCOPE estimates ~20 MB. The ONNX runtime may inflate this. If the binary + ONNX runtime exceeds 50 MB, consider documenting this as a known trade-off.

3. **npm registry authentication**: The scope `@dug-21` requires registry authentication. The CI workflow needs an `NPM_TOKEN` secret. This is a configuration step, not an architecture decision, but must be documented in the release skill.

4. **`unimatrix version` subcommand output format**: Should it print just the version string (for scripting) or include build metadata (git SHA, platform)? Recommendation: `unimatrix version` prints `unimatrix 0.5.0` (human-readable), `unimatrix version --json` prints structured output.
