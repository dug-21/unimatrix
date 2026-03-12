# nan-004: Versioning & Packaging

## Problem Statement

Unimatrix currently requires building from source (`cargo build --release`) to produce the `unimatrix-server` binary. There is no distribution mechanism for users who want to adopt Unimatrix in their projects. The entire installation and wiring process is manual: compile the binary, download the ONNX model, configure `.mcp.json`, set up hooks in `.claude/settings.json`, copy skill files, and create the SQLite schema. This makes adoption impractical for anyone outside the development team.

The esbuild/turbo pattern (npm package with `optionalDependencies` for platform-specific binary packages) is a proven distribution mechanism for Rust/Go binaries in the JavaScript ecosystem. It provides a familiar `npm install` + `npx` workflow without requiring users to have a Rust toolchain.

## Goals

1. Publish a scoped npm package (`@scope/unimatrix`) that drops the platform-appropriate Rust binary and ONNX model into `node_modules` on `npm install` — no project file modifications during install.
2. Provide `npx unimatrix init` as a mechanical, deterministic project wiring command that configures MCP server, hooks, skill files, and pre-creates the database schema.
3. Establish a GitHub Actions release pipeline that cross-compiles platform binaries and publishes npm packages on tagged releases only.
4. Support semantic versioning with automatic schema migration on startup for version skew handling.
5. Start with linux-x64 only; design the package structure so darwin-arm64/x64 can be added without breaking changes.

## Non-Goals

- Interactive onboarding or conversational knowledge seeding — that is nan-003 (`/unimatrix-init` and `/unimatrix-seed` skills).
- Public npm registry publishing — initial release is private-scoped (`@username/unimatrix`).
- Windows support.
- Automatic updates or self-updating binaries.
- npm package for the library crates (only the server binary is distributed).
- macOS notarization or code signing (deferred to multi-platform expansion).
- CLAUDE.md content generation — `npx unimatrix init` does mechanical wiring only; the `/unimatrix-init` skill handles knowledge block authoring.

## Background Research

### Current Binary & Build Setup

- **Binary entry point**: `crates/unimatrix-server/src/main.rs` producing `unimatrix-server` binary.
- **CLI structure**: `clap` with subcommands: `hook <event>`, `export`, `import`. Server mode runs when no subcommand is given.
- **Rust edition**: 2024, `rust-version = "1.89"`, workspace has 9 crates.
- **Release binary size**: ~20 MB (linux aarch64, stripped). Acceptable for npm distribution.
- **ONNX runtime**: `ort = "=2.0.0-rc.9"` (pinned). Cross-compilation requires matching ONNX Runtime shared library per platform. This is the primary cross-compilation challenge.
- **Tokenizer**: `tokenizers = "0.21"` with `onig` feature — native C dependency (Oniguruma regex). Another cross-compilation consideration.
- **hf-hub**: Used at runtime for ONNX model download. Model is `sentence-transformers/all-MiniLM-L6-v2` (~90 MB, cached to `~/.cache/unimatrix-embed/`).
- **No existing CI/CD**: No `.github/workflows/` directory. No release infrastructure.
- **Package version**: All crate `Cargo.toml` files have `version = "0.1.0"`. No version synchronization mechanism.

### Current Project Wiring (What `npx unimatrix init` Must Reproduce)

**MCP server config** (`.mcp.json`):
```json
{
  "mcpServers": {
    "unimatrix": {
      "command": "/workspaces/unimatrix/target/release/unimatrix-server",
      "args": [],
      "env": {}
    }
  }
}
```
The `command` path must point to the installed binary (inside `node_modules`). The `npx unimatrix init` command must resolve this path dynamically.

**Hook configuration** (`.claude/settings.json`):
```json
{
  "hooks": {
    "SessionStart": [{ "matcher": "", "hooks": [{ "type": "command", "command": "unimatrix-server hook SessionStart" }] }],
    "Stop": [{ "matcher": "", "hooks": [{ "type": "command", "command": "unimatrix-server hook Stop" }] }],
    "UserPromptSubmit": [{ "matcher": "", "hooks": [{ "type": "command", "command": "unimatrix-server hook UserPromptSubmit | tee -a ~/.unimatrix/injections/hooks.log" }] }],
    "PreToolUse": [{ "matcher": "*", "hooks": [{ "type": "command", "command": "unimatrix-server hook PreToolUse" }] }],
    "PostToolUse": [{ "matcher": "*", "hooks": [{ "type": "command", "command": "unimatrix-server hook PostToolUse" }] }],
    "SubagentStart": [{ "matcher": "*", "hooks": [{ "type": "command", "command": "unimatrix-server hook SubagentStart" }] }],
    "SubagentStop": [{ "matcher": "*", "hooks": [{ "type": "command", "command": "unimatrix-server hook SubagentStop" }] }]
  }
}
```
Hook commands reference `unimatrix-server` by bare name — relies on the binary being on `$PATH` or using an absolute path. After npm install, `node_modules/.bin/unimatrix-server` will exist (via the npm `bin` field), but shell hooks execute outside `npx` context. The init command must either use absolute paths or create a shim.

**Skill files**: 13 skill directories under `.claude/skills/`. The init command must copy skill files into the target project. Source of truth for skill content is the npm package itself (bundled at publish time).

**Agent definitions**: 16 files under `.claude/agents/uni/`. These are Unimatrix's own agents (for Unimatrix development). They should NOT be copied to target projects — agents are project-specific.

**ONNX model**: Downloaded lazily by `ensure_model()` in `unimatrix-embed` on first server startup. Cache location: `~/.cache/unimatrix-embed/sentence-transformers_all-MiniLM-L6-v2/` containing `model.onnx` (~90 MB) and `tokenizer.json`. The postinstall phase should pre-download this to avoid first-run latency.

**Database schema**: Created automatically by `Store::open()` + `migrate_if_needed()` on first startup. Schema v11 (current). The init command should pre-create the database to validate the installation before the user starts a session.

### Schema Migration Architecture

- `CURRENT_SCHEMA_VERSION = 11` in `crates/unimatrix-store/src/migration.rs`.
- `migrate_if_needed()` runs on every `Store::open()` — already handles version skew on startup.
- Migration path covers v0 through v11 with idempotent guards (`IF NOT EXISTS`, column existence checks).
- Major migration: v5->v6 (bincode blobs to SQL columns) creates backups. v8->v9 (observation metrics normalization) runs outside main transaction.
- Schema migration is already production-ready for version upgrades. No new work needed beyond ensuring the migration path is tested across version gaps.

### esbuild/turbo optionalDependencies Pattern

The proven approach for distributing native binaries via npm:

1. **Root package** (`@scope/unimatrix`): Contains the JS shim that resolves the platform binary. Has `optionalDependencies` for each platform package.
2. **Platform packages** (`@scope/unimatrix-linux-x64`, `@scope/unimatrix-darwin-arm64`, etc.): Each contains only the pre-compiled binary. The `os` and `cpu` fields in `package.json` ensure npm only downloads the matching platform.
3. **`postinstall` script**: In the root package. Downloads/validates the ONNX model. Does NOT modify any project files.
4. **`bin` field**: In the root package. Points to a JS shim that exec's the platform binary.

### Existing nan-003 Boundary

nan-003 delivers `/unimatrix-init` (CLAUDE.md knowledge block + agent scan recommendations) and `/unimatrix-seed` (conversational knowledge population). These are interactive, human-directed skills. nan-004 is mechanical/automated only:
- nan-004 installs skill files into the project (including the nan-003 skills).
- nan-003 skills are used after nan-004's `npx unimatrix init` completes.
- The boundary: nan-004 places files and configs; nan-003 populates knowledge.

### Data Directory Structure

Per-project data stored in `~/.unimatrix/{hash}/`:
- `unimatrix.db` — SQLite database
- `vector/` — HNSW vector index files
- `unimatrix.pid` — PID file (flock-based)
- `unimatrix.sock` — Unix domain socket for hook IPC

Project hash is SHA-256(canonical project root path)[0:16]. The `npx unimatrix init` command can pre-create this structure and the database.

## Proposed Approach

### Package Structure

```
packages/
  unimatrix/                          # Root package: @scope/unimatrix
    package.json                      # bin, optionalDependencies, postinstall
    bin/
      unimatrix.js                    # JS shim: resolves platform binary, exec's it
    postinstall.js                    # ONNX model pre-download
    skills/                           # Bundled skill files (copied to projects)
      unimatrix-init/SKILL.md
      unimatrix-seed/SKILL.md
      store-adr/SKILL.md
      ...all 13 skills
    init/                             # Init command logic
      index.js                        # Project wiring: .mcp.json, settings.json, skills, schema
  unimatrix-linux-x64/               # Platform package: @scope/unimatrix-linux-x64
    package.json                      # os: ["linux"], cpu: ["x64"]
    bin/
      unimatrix-server                # Pre-compiled binary
```

### Two-Phase Install Model

**Phase 1: `npm install @scope/unimatrix`**
- npm resolves `optionalDependencies`, downloads platform-specific binary package.
- `postinstall` runs: pre-downloads ONNX model to `~/.cache/unimatrix-embed/`. No project files touched.

**Phase 2: `npx unimatrix init`**
- Resolves project root (walk up to `.git`).
- Writes `.mcp.json` with absolute path to the binary in `node_modules`.
- Merges hook configuration into `.claude/settings.json` (create if absent, merge if present — preserve existing hooks and permissions).
- Copies skill files from the package into `.claude/skills/`.
- Pre-creates `~/.unimatrix/{hash}/unimatrix.db` with current schema.
- Validates the installation: binary executes, ONNX model loads, database opens.
- Prints summary of what was done and suggests running `/unimatrix-init` next.

### GitHub Actions Release Pipeline

- Triggered on `v*` tags only (not PRs, not branch pushes).
- Matrix build: cross-compile `unimatrix-server` for each target platform.
- Linux x64 initially (via `cross` or `cargo-zigbuild` for glibc compatibility).
- Package each binary into its platform npm package.
- Publish all packages to npm registry.
- Version synchronized: Cargo.toml version drives npm package version.

### Versioning Strategy

- **Initial version**: 0.5.0 (reflects maturity — 44 features shipped, ~1700 tests).
- **Single source of truth**: Cargo workspace `version` in root `Cargo.toml`.
- **Sync mechanism**: Release process updates all crate `Cargo.toml` files + npm `package.json` files from the workspace version.
- **Semver policy**: MAJOR (breaking schema/API), MINOR (new features), PATCH (fixes). Pre-1.0, MINOR may include breaking changes per semver convention.
- **Version bump**: All crates move in lockstep (single workspace version).
- **CHANGELOG**: Generated from conventional commits since last tag. Grouped by type (features, fixes, breaking changes).

### `/release` Skill

A skill for repeatable, human-initiated releases:
1. Prompt for bump level (major/minor/patch) or explicit version.
2. Update version in root `Cargo.toml` workspace config — all crates inherit.
3. Update all npm `package.json` files (root + platform packages).
4. Generate/update `CHANGELOG.md` from conventional commits since last tag.
5. Create a release commit (`release: v{version}`).
6. Create git tag (`v{version}`).
7. Push commit + tag → triggers GitHub Actions release pipeline.

The skill is a nan-004 deliverable — it lives in `.claude/skills/release/`.

### settings.json Merge Strategy

The hook configuration in `.claude/settings.json` must be merged, not overwritten. Users may have existing hooks or permissions. The merge logic:
1. Read existing file (or start with `{}`).
2. For each hook event (SessionStart, Stop, etc.), check if a unimatrix hook already exists.
3. If not present, append the unimatrix hook entry to that event's array.
4. If already present, update the command path if it changed.
5. Preserve all non-unimatrix hooks and other settings (permissions, etc.).
6. Write back with stable key ordering.

## Acceptance Criteria

- AC-01: `npm install @scope/unimatrix` downloads the platform binary for the current OS/arch into `node_modules` without modifying any project files beyond `node_modules/` and `package.json`/`package-lock.json`.
- AC-02: The `postinstall` script pre-downloads the ONNX model (`all-MiniLM-L6-v2`) to the user cache directory (`~/.cache/unimatrix-embed/`). If download fails (no network), postinstall succeeds with a warning — the model will be downloaded on first server start.
- AC-03: `npx unimatrix init` writes `.mcp.json` with the correct absolute path to the `unimatrix-server` binary inside `node_modules`. If `.mcp.json` exists, the unimatrix server entry is added/updated without removing other MCP server entries.
- AC-04: `npx unimatrix init` merges Unimatrix hook configuration into `.claude/settings.json` for all 7 hook events (SessionStart, Stop, UserPromptSubmit, PreToolUse, PostToolUse, SubagentStart, SubagentStop). Existing hooks and permissions are preserved. If the file does not exist, it is created.
- AC-05: `npx unimatrix init` copies all skill files from the npm package into `.claude/skills/` in the target project. Existing skill files with the same name are overwritten (version upgrade path). Non-unimatrix skill files are preserved.
- AC-06: `npx unimatrix init` pre-creates the Unimatrix data directory (`~/.unimatrix/{hash}/`) and SQLite database with the current schema. If the database already exists (upgrade scenario), schema migration runs via the binary's startup path.
- AC-07: `npx unimatrix init` performs a validation step: executes the binary with a version check or health probe to confirm it runs on the current platform. Reports success or a diagnostic error.
- AC-08: `npx unimatrix init` is idempotent — running it twice on the same project updates paths and skill files without duplicating hooks or corrupting settings.
- AC-09: A JS shim (`bin/unimatrix.js`) resolves the platform-specific binary from `optionalDependencies` and exec's it with all passed arguments. If no platform binary is found, it prints a clear error listing supported platforms.
- AC-10: The GitHub Actions release workflow triggers only on `v*` tags, cross-compiles `unimatrix-server` for linux-x64, packages the binary into the platform npm package, and publishes both the root and platform packages to the npm registry.
- AC-11: The npm package version matches the version in `Cargo.toml`. A release script or CI step synchronizes versions before publishing.
- AC-12: The npm root package uses `optionalDependencies` with `os` and `cpu` fields on platform packages so that only the matching platform binary is downloaded.
- AC-13: `npx unimatrix init` prints a human-readable summary of all actions taken (files created, files updated, paths configured) and suggests running `/unimatrix-init` as the next step.
- AC-14: `npx unimatrix init` supports a `--dry-run` flag that prints what would be done without modifying any files.
- AC-15: All Cargo workspace crates use `version.workspace = true` inheriting from the root `Cargo.toml` workspace version. Initial version is `0.5.0`.
- AC-16: A `/release` skill exists in `.claude/skills/release/` that bumps version across Cargo.toml + package.json files, generates CHANGELOG entries, creates a release commit and git tag, and pushes to trigger the release pipeline.
- AC-17: The CHANGELOG.md is generated from conventional commits (feat, fix, breaking) grouped by type, covering commits since the last version tag.

## Constraints

- **Native CI build**: Linux x64 CI runner builds natively — no cross-compilation needed. ort and tokenizers native deps compile straightforwardly on the target platform.
- **Patched dependency**: `anndists` is patched locally (`patches/anndists`). The release build must include this patch. This works in CI since the patch is in the repo, but must be validated.
- **Rust edition 2024 / rust-version 1.89**: CI runners must have Rust 1.89+. This is newer than what most GitHub Actions runners provide by default — requires explicit toolchain installation.
- **ONNX model size**: ~90 MB download during postinstall. Must handle network failures gracefully (warn, don't fail). Must not re-download if already cached.
- **settings.json merge complexity**: The `.claude/settings.json` format supports nested structures (hooks with matchers, permissions with glob patterns). Merge logic must be precise to avoid corrupting user configurations.
- **Binary path stability**: The absolute path to the binary in `node_modules` changes if the project is moved or `node_modules` is rebuilt. Users must re-run `npx unimatrix init` after moving their project. This is inherent to the approach and should be documented.
- **npm scope**: Private scoped package requires `npm login` with the correct scope. Users must be authenticated to the registry. Public publishing is a later step.
- **No existing CI**: Everything (workflow files, release scripts, npm package structure) must be created from scratch.

## Resolved Questions

1. **Binary name**: RESOLVED — Rename to `unimatrix`. `unimatrix server` as default subcommand. `unimatrix hook <event>` for hooks. `unimatrix init` for project wiring.
2. **npm scope name**: RESOLVED — `@dug-21/unimatrix`. Platform packages: `@dug-21/unimatrix-linux-x64`, etc.
3. **ONNX model**: RESOLVED — Download in postinstall. Graceful failure (warn, don't fail). Lazy download on first server start as fallback.
4. **Build strategy**: RESOLVED — Native build on linux-x64 CI runner (no cross-compilation needed). Dev environment is Docker on Mac, target is the same linux container environment. Multi-platform builds deferred.
5. **Hook command paths**: RESOLVED — PATH-based via `node_modules/.bin/` (standard npm bin behavior). `npx unimatrix init` writes hook commands using bare `unimatrix` name.
6. **Which skills to bundle**: RESOLVED — All 13 skill directories.
7. **ONNX linking**: RESOLVED — Architect decides static vs dynamic based on CI build validation.
8. **UserPromptSubmit tee**: RESOLVED — Drop tee for distribution. Hook command is plain `unimatrix hook UserPromptSubmit` without tee pipeline.
9. **Version output**: RESOLVED — Plain text only (`unimatrix 0.5.0`). No `--json` flag.
10. **NPM_TOKEN**: RESOLVED — Already configured as GitHub repository secret for Actions.

## Tracking

https://github.com/dug-21/unimatrix/issues/220
