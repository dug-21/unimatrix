# nan-004: Versioning & Packaging — Specification

## Objective

Establish npm/npx distribution of the Unimatrix Rust binary so that adopters can install Unimatrix via `npm install` and wire it into their project via `npx unimatrix init` without requiring a Rust toolchain. This includes the esbuild/turbo `optionalDependencies` pattern for platform-specific binaries, a GitHub Actions release pipeline triggered by version tags, semantic versioning with lockstep workspace crates, and a deterministic project wiring command that configures MCP server, hooks, skills, and database schema.

## Functional Requirements

### Binary Rename & CLI Restructure

- FR-01: The binary name changes from `unimatrix-server` to `unimatrix`. The `clap` `#[command(name)]` updates to `"unimatrix"`.
- FR-02: When no subcommand is provided, the binary runs in MCP server mode (existing behavior, unchanged semantics).
- FR-03: The `hook`, `export`, and `import` subcommands retain their current behavior and argument signatures under the renamed binary.
- FR-04: A new `init` subcommand is added. When invoked, it performs deterministic project wiring (see FR-10 through FR-18). This subcommand runs synchronously with no tokio runtime, matching the hook/export/import paths.
- FR-05: A new `version` subcommand (or `--version` flag) prints the binary version derived from `CARGO_PKG_VERSION`. Used by the init validation step and by users for diagnostics.

### npm Package Structure

- FR-06: A root npm package `@dug-21/unimatrix` is created under `packages/unimatrix/` in the repository. It contains the JS binary shim, postinstall script, init logic, and bundled skill files.
- FR-07: A platform npm package `@dug-21/unimatrix-linux-x64` is created under `packages/unimatrix-linux-x64/`. Its `package.json` declares `"os": ["linux"]` and `"cpu": ["x64"]`. It contains only the pre-compiled `unimatrix` binary.
- FR-08: The root package declares the platform package as an `optionalDependency`. npm resolves the correct platform package based on the installing system's OS and CPU architecture.
- FR-09: The root package `bin` field maps `"unimatrix"` to a JS shim (`bin/unimatrix.js`) that resolves the platform-specific binary path from the installed `optionalDependencies` and calls `child_process.execFileSync` (or `execvp` equivalent) with all forwarded arguments. If no platform binary is found, the shim prints an error listing supported platforms and exits with code 1.

### `npx unimatrix init` — Project Wiring

- FR-10: The init command resolves the project root by walking up from the current working directory to find a `.git` directory. If no `.git` is found, it errors with a diagnostic message.
- FR-11: The init command writes or merges `.mcp.json` at the project root. The `unimatrix` server entry uses the absolute path to the `unimatrix` binary resolved from `node_modules/.bin/`. If `.mcp.json` already exists, other MCP server entries are preserved; only the `unimatrix` key is added or updated.
- FR-12: The init command merges Unimatrix hook configuration into `.claude/settings.json` at the project root. It configures all 7 hook events: SessionStart, Stop, UserPromptSubmit, PreToolUse, PostToolUse, SubagentStart, SubagentStop. Hook commands use the absolute path to the `unimatrix` binary (mitigating SR-09 — bare-name PATH resolution is unreliable in shell hook context outside npm/npx execution environment).
- FR-13: When `.claude/settings.json` already exists, the merge preserves all existing content (non-unimatrix hooks, permissions, other settings). For each hook event, if a unimatrix hook entry already exists it is updated in place; if absent it is appended. Unimatrix hook entries are identified by command string containing `unimatrix` (case-insensitive match on the binary name portion).
- FR-14: The init command copies all 13 skill directories from the npm package's bundled `skills/` directory into `.claude/skills/` at the project root. Existing skill files with the same directory name are overwritten (upgrade path). Non-unimatrix skill files in `.claude/skills/` are preserved.
- FR-15: The init command computes the project hash (SHA-256 of canonical project root path, first 16 hex chars) and pre-creates the data directory at `~/.unimatrix/{hash}/`. It then invokes the `unimatrix` binary to initialize the database (which triggers `Store::open()` + `migrate_if_needed()`).
- FR-16: The init command performs a validation step after wiring: executes `unimatrix --version` to confirm the binary is functional on the current platform. Reports success with the version string, or a diagnostic error.
- FR-17: The init command prints a human-readable summary listing every action taken: files created, files updated, paths configured, database location. The final line suggests running `/unimatrix-init` as the next step.
- FR-18: The init command supports a `--dry-run` flag that prints all actions that would be taken without writing any files.
- FR-19: The init command is idempotent. Running it twice on the same project updates paths and skill files without duplicating hook entries, corrupting settings, or creating duplicate MCP server entries.

### Postinstall — ONNX Model Pre-download

- FR-20: The root npm package includes a `postinstall` script that pre-downloads the ONNX model (`sentence-transformers/all-MiniLM-L6-v2`) to `~/.cache/unimatrix-embed/sentence-transformers_all-MiniLM-L6-v2/`. It downloads both `model.onnx` (~90 MB) and `tokenizer.json`.
- FR-21: If the model files already exist at the cache location, the postinstall skips the download.
- FR-22: If the download fails (network unavailable, firewall, proxy), the postinstall completes successfully with a warning printed to stderr. The server's `ensure_model()` handles lazy download on first startup as fallback.

### Version Management

- FR-23: All 9 Cargo workspace crates use `version.workspace = true`, inheriting from the root `Cargo.toml` `[workspace.package]` version field. The initial version is set to `0.5.0`.
- FR-24: All npm `package.json` files (root + platform packages) carry the same version as the Cargo workspace version.
- FR-25: A `/release` skill is created at `.claude/skills/release/SKILL.md`. It guides the human through: selecting bump level (major/minor/patch) or explicit version; updating the root `Cargo.toml` workspace version; updating all npm `package.json` versions; generating CHANGELOG.md entries from conventional commits since last tag; creating a release commit (`release: v{version}`); creating a git tag (`v{version}`); pushing to trigger the CI pipeline.

### CHANGELOG Generation

- FR-26: `CHANGELOG.md` is generated at the repository root. Entries are grouped by type: Features (`feat:`), Fixes (`fix:`), Breaking Changes (`BREAKING CHANGE` or `!` suffix). Each entry includes the short commit message and issue/PR reference if present.
- FR-27: The changelog covers commits between the previous version tag and the current release tag. For the initial release (no prior tag), all conventional commits are included.

### GitHub Actions Release Pipeline

- FR-28: A workflow file `.github/workflows/release.yml` triggers on `v*` tags pushed to the repository. It does not trigger on PRs or branch pushes.
- FR-29: The workflow installs Rust 1.89 toolchain using `dtolnay/rust-toolchain` with explicit version pin (mitigating SR-04).
- FR-30: The workflow builds the `unimatrix` binary natively on the linux-x64 runner. It verifies the `patches/anndists` directory exists before building (mitigating SR-03).
- FR-31: The workflow strips the binary (`strip`) and packages it into the `@dug-21/unimatrix-linux-x64` npm package directory.
- FR-32: The workflow publishes both npm packages (`@dug-21/unimatrix` and `@dug-21/unimatrix-linux-x64`) to the npm registry. Version is validated to match the Cargo workspace version before publishing.
- FR-33: The workflow creates a GitHub Release with the CHANGELOG entries for the tagged version.

## Non-Functional Requirements

- NFR-01: **Install time** — `npm install @dug-21/unimatrix` (excluding ONNX model download) completes in under 30 seconds on a broadband connection. The platform binary package should be under 25 MB.
- NFR-02: **Init execution time** — `npx unimatrix init` completes in under 5 seconds (excluding ONNX model download if triggered).
- NFR-03: **Postinstall resilience** — The postinstall script never causes `npm install` to fail. Network errors, permission issues, and disk space problems produce warnings only.
- NFR-04: **Binary portability** — The linux-x64 binary must run on Ubuntu 22.04 LTS and later. The build must link against a glibc version no newer than Ubuntu 22.04's (glibc 2.35) or use static linking for critical dependencies (mitigating SR-02).
- NFR-05: **CI build time** — The release pipeline completes in under 20 minutes for a single-platform build.
- NFR-06: **Zero project-file mutation on install** — `npm install` modifies only `node_modules/`, `package.json`, and `package-lock.json`. No other project files are touched until `npx unimatrix init` is explicitly run.
- NFR-07: **Backward compatibility** — The binary rename from `unimatrix-server` to `unimatrix` must be accompanied by updating all hook references in this repository's `.claude/settings.json` and `.mcp.json` to use the new name. Existing data directories and databases are unaffected (binary name is not part of the data model).

## Acceptance Criteria

| AC-ID | Criterion | Verification Method |
|-------|-----------|-------------------|
| AC-01 | `npm install @dug-21/unimatrix` downloads the platform binary for the current OS/arch into `node_modules` without modifying any project files beyond `node_modules/` and `package.json`/`package-lock.json`. | Manual test: install in a clean project, verify no other files changed via `git status`. |
| AC-02 | The postinstall script pre-downloads the ONNX model to `~/.cache/unimatrix-embed/`. If download fails, postinstall succeeds with a warning. | Test with network available (files appear) and with network blocked (exit code 0, warning on stderr). |
| AC-03 | `npx unimatrix init` writes `.mcp.json` with the correct absolute path to the binary. Existing MCP server entries are preserved on merge. | Test on clean project (file created) and on project with existing `.mcp.json` containing other servers (servers preserved). |
| AC-04 | `npx unimatrix init` merges hook config into `.claude/settings.json` for all 7 events. Existing hooks and permissions preserved. File created if absent. | Test on clean project (file created with 7 events); test on project with existing hooks (non-unimatrix hooks preserved); test on project with permissions block (permissions unchanged). |
| AC-05 | `npx unimatrix init` copies all 13 skill files into `.claude/skills/`. Existing same-name skills overwritten; other skills preserved. | Test with clean skills dir (13 dirs created); test with pre-existing unimatrix skill (overwritten); test with non-unimatrix skill (preserved). |
| AC-06 | `npx unimatrix init` pre-creates `~/.unimatrix/{hash}/` and SQLite database with current schema. Existing database triggers migration via binary startup. | Test on fresh project (directory + db created); test with existing db at older schema (migration runs). |
| AC-07 | `npx unimatrix init` validates the binary by executing `unimatrix --version` and reporting success or diagnostic error. | Test with valid binary (success printed); test with missing/corrupt binary (error with diagnostic). |
| AC-08 | `npx unimatrix init` is idempotent — running twice does not duplicate hooks or corrupt settings. | Run init twice, diff `.claude/settings.json` and `.mcp.json` — second run produces no semantic changes. |
| AC-09 | JS shim resolves platform binary and exec's it. Prints clear error on unsupported platform. | Test on linux-x64 (binary runs); test with platform package removed (error message lists supported platforms). |
| AC-10 | GitHub Actions release workflow triggers on `v*` tags, builds linux-x64 binary, packages it, and publishes both npm packages. | Push a test tag, observe workflow execution and npm registry state. |
| AC-11 | npm package version matches Cargo.toml version. | Inspect published package.json versions vs workspace Cargo.toml. |
| AC-12 | Root package uses `optionalDependencies` with `os`/`cpu` fields so only the matching platform binary downloads. | Install on linux-x64: only linux-x64 package present in node_modules. |
| AC-13 | Init prints human-readable summary of all actions and suggests `/unimatrix-init` as next step. | Visual inspection of init output. |
| AC-14 | Init supports `--dry-run` that prints planned actions without modifying files. | Run with `--dry-run`, verify no files created or modified. |
| AC-15 | All 9 Cargo crates use `version.workspace = true` inheriting from root. Initial version is `0.5.0`. | `grep "version.workspace = true"` across all crate Cargo.toml files; verify root version. |
| AC-16 | `/release` skill exists and guides version bump, changelog, commit, tag, and push. | Skill file present at `.claude/skills/release/SKILL.md`; dry-run a release to verify steps. |
| AC-17 | CHANGELOG.md generated from conventional commits grouped by type. | Generate changelog; verify feat/fix/breaking grouping and commit references. |

## Domain Models

### Key Entities

- **Root Package** (`@dug-21/unimatrix`): The npm package users install. Contains the JS binary shim, postinstall script, bundled skills, and init logic. Does not contain the native binary itself.
- **Platform Package** (`@dug-21/unimatrix-linux-x64`): Contains only the pre-compiled native binary for a specific OS/CPU combination. Selected automatically by npm based on `os` and `cpu` fields in `package.json`.
- **JS Shim** (`bin/unimatrix.js`): A thin Node.js script that resolves the platform binary path from the installed optional dependency and exec's it with forwarded arguments. The entry point for `npx unimatrix` invocations.
- **Init Command**: The `unimatrix init` subcommand (routed through the JS shim for npx, or directly via the binary). Performs deterministic, mechanical project wiring.
- **Project Root**: The directory containing `.git`. Resolved by walking up from CWD. All wiring targets (`.mcp.json`, `.claude/settings.json`, `.claude/skills/`) are relative to this root.
- **Project Hash**: `SHA-256(canonical_project_root_path)[0:16]` in hex. Determines the per-project data directory path (`~/.unimatrix/{hash}/`).
- **Release Pipeline**: The GitHub Actions workflow triggered by `v*` tags. Builds, packages, and publishes.
- **Workspace Version**: The single version string in root `Cargo.toml` `[workspace.package].version` that all crates and npm packages inherit.

### Ubiquitous Language

| Term | Definition |
|------|-----------|
| Platform binary | The compiled `unimatrix` executable for a specific OS/CPU target |
| Wiring | The act of configuring a project to use Unimatrix: MCP server, hooks, skills, database |
| Postinstall | npm lifecycle script that runs after `npm install`; used for ONNX model pre-download |
| Hook event | One of the 7 Claude Code lifecycle events that Unimatrix instruments |
| Bare name | Using `unimatrix` without a path prefix, relying on PATH resolution |
| Absolute path | Full filesystem path to the binary; used in hook commands and `.mcp.json` to avoid PATH resolution issues (SR-09) |

## User Workflows

### Workflow 1: First-Time Adoption

1. User runs `npm install @dug-21/unimatrix` in their project.
2. npm downloads the root package and the matching platform binary package.
3. Postinstall pre-downloads the ONNX model (or warns if offline).
4. User runs `npx unimatrix init`.
5. Init resolves project root, writes `.mcp.json`, merges hooks into `.claude/settings.json`, copies 13 skills, creates database.
6. Init prints summary and suggests running `/unimatrix-init`.
7. User starts a Claude Code session and runs `/unimatrix-init` (nan-003) for knowledge seeding.

### Workflow 2: Version Upgrade

1. User runs `npm update @dug-21/unimatrix` (or `npm install @dug-21/unimatrix@latest`).
2. New platform binary replaces old one in `node_modules`.
3. User runs `npx unimatrix init` again.
4. Init updates `.mcp.json` path (same absolute path, new binary), updates skill files (overwrite), runs schema migration on existing database.
5. Init prints summary showing updated files.

### Workflow 3: Creating a Release (Maintainer)

1. Maintainer invokes `/release` skill in Claude Code.
2. Skill prompts for bump level (major/minor/patch) or explicit version.
3. Skill updates root `Cargo.toml` workspace version, all npm `package.json` versions.
4. Skill generates CHANGELOG.md entries from conventional commits.
5. Skill creates release commit and git tag.
6. Maintainer pushes (or skill pushes after confirmation) — tag triggers CI.
7. CI builds binary, packages, publishes to npm.

### Workflow 4: Dry Run (Cautious Adopter)

1. User runs `npx unimatrix init --dry-run`.
2. Init prints every action it would take (file paths, merge operations) without modifying anything.
3. User reviews, then runs `npx unimatrix init` to execute.

## Constraints

- **C-01: Patched dependency** — The `anndists` crate is patched locally at `patches/anndists`. CI builds must check out the full repository including this patch directory. The workflow must assert patch presence before building (SR-03).
- **C-02: Rust 1.89 toolchain** — CI runners must explicitly install Rust 1.89+ since it is newer than GitHub Actions default toolchains (SR-04). Use `dtolnay/rust-toolchain@stable` with an explicit override or `dtolnay/rust-toolchain@1.89`.
- **C-03: ONNX runtime native dependency** — `ort =2.0.0-rc.9` bundles or links the ONNX Runtime shared library. The CI build must validate that the resulting binary is self-contained (no missing shared libraries) on a clean Ubuntu 22.04 container (SR-01).
- **C-04: Oniguruma C dependency** — The `tokenizers` crate with `onig` feature compiles Oniguruma. The binary must link statically or target glibc >= 2.35 (Ubuntu 22.04 LTS baseline) (SR-02).
- **C-05: Hook PATH resolution** — Hook commands execute in shell context outside npm/npx, where `node_modules/.bin/` is not on PATH (SR-09). The init command must write absolute paths to the binary in hook commands and `.mcp.json`, not bare names. This means re-running `npx unimatrix init` is required after moving the project or reinstalling `node_modules`.
- **C-06: settings.json merge precision** — The `.claude/settings.json` format contains nested structures (hooks with matchers, permissions with globs). The merge must be a deep, structure-aware operation that never drops or reorders existing content. Edge cases: empty file, malformed JSON (error with diagnostic, do not overwrite), conflicting hook matchers, existing permissions block (SR-08).
- **C-07: Private npm scope** — `@dug-21/unimatrix` is a private-scoped package. Users must be authenticated to the npm registry with the correct scope. The postinstall and init must not assume public registry access.
- **C-08: No existing CI** — All workflow files, npm package structure, and release scripts are created from scratch. There is no prior CI infrastructure to build on.
- **C-09: Binary rename is a breaking change** — Renaming from `unimatrix-server` to `unimatrix` breaks existing hook configurations referencing the old name (SR-06). This repository's `.claude/settings.json` and `.mcp.json` must be updated atomically with the rename.
- **C-10: Schema version 11** — The current schema version is 11. The init command database pre-creation must produce a schema v11 database. Future version upgrades are handled by `migrate_if_needed()` on server startup.

## Dependencies

### Rust Crates (Existing)

- `clap` — CLI parsing (already used; new `init` and `version` subcommands added)
- `unimatrix-store` — `Store::open()` + `migrate_if_needed()` for database pre-creation
- `unimatrix-server` — `project::ensure_data_directory()` for project hash computation

### Rust Crates (New)

- None anticipated. The init subcommand uses existing project and store infrastructure.

### npm Packages (New, Created by This Feature)

- `@dug-21/unimatrix` — root distribution package
- `@dug-21/unimatrix-linux-x64` — linux x64 platform binary package

### External Services

- **npm registry** — Package publishing target (private scope)
- **GitHub Actions** — CI/CD for release pipeline
- **Hugging Face Hub** — ONNX model download (`sentence-transformers/all-MiniLM-L6-v2`)

### Existing Components Modified

- `crates/unimatrix-server/src/main.rs` — Binary rename, new `init` and `version` subcommands
- `Cargo.toml` (root) — Workspace version change from `0.1.0` to `0.5.0`, add `version` field to `[workspace.package]`
- All 9 `crates/*/Cargo.toml` — Change to `version.workspace = true`
- `.claude/settings.json` — Update hook commands from `unimatrix-server` to absolute path of `unimatrix`
- `.mcp.json` — Update binary path reference

## NOT in Scope

- **Interactive onboarding or knowledge seeding** — That is nan-003 (`/unimatrix-init` and `/unimatrix-seed` skills). This feature does mechanical wiring only.
- **Public npm registry publishing** — Initial release is private-scoped `@dug-21/unimatrix`.
- **Windows support** — No Windows platform package or CI build.
- **macOS platform packages** — darwin-arm64 and darwin-x64 are deferred. The package structure supports adding them without breaking changes, but they are not built or published in this feature.
- **macOS notarization or code signing** — Deferred to multi-platform expansion.
- **Automatic updates or self-updating binaries** — Users run `npm update` manually.
- **npm package for library crates** — Only the server binary is distributed.
- **CLAUDE.md content generation** — `npx unimatrix init` does not write or modify CLAUDE.md. The `/unimatrix-init` skill (nan-003) handles that.
- **Agent definition copying** — Agent files under `.claude/agents/uni/` are Unimatrix-project-specific and are not copied to target projects.
- **Cross-compilation** — linux-x64 builds natively on the CI runner. Cross-compilation tooling (cross, cargo-zigbuild) is deferred to multi-platform expansion.
- **Bundling ONNX model in the npm package** — Model is downloaded on postinstall or lazily on first server start. Bundling ~90 MB in the npm package is not viable.
- **Shell profile modification for PATH** — Absolute paths in hook commands eliminate the need for PATH modification (SR-09 mitigation).

## Open Questions

1. **Init subcommand location** — The scope proposes `npx unimatrix init` which routes through the JS shim to the Rust binary's `init` subcommand. Alternatively, the init logic could be implemented entirely in Node.js (`packages/unimatrix/init/index.js`). The architect should decide: Rust implementation (reuses `project::ensure_data_directory` and `Store::open` natively) vs. Node.js implementation (simpler file manipulation, invokes the binary only for validation and DB creation). Recommendation: Rust for database/project operations, with the JS shim dispatching to it.

2. **ONNX shared library bundling** — SR-01 flags that `ort =2.0.0-rc.9` may require a separate shared library (`.so`) alongside the binary. If the ONNX runtime is dynamically linked, the platform package must include it, potentially increasing package size beyond 25 MB. The architect must validate whether the release build statically links the ONNX runtime or requires `.so` co-packaging.

3. **Total platform package size** — SCOPE estimates ~20 MB for the binary. If ONNX runtime `.so` must be bundled, the package could reach 100+ MB. The architect should measure actual size after a release build and assess whether this is acceptable for npm distribution.

4. **UserPromptSubmit hook tee command** — The current `UserPromptSubmit` hook pipes through `tee -a ~/.unimatrix/injections/hooks.log`. Should the init command reproduce this `tee` pipeline, or should it use the plain command form matching the other 6 hooks? The architect should determine if the `tee` is a debugging artifact or a required production behavior.

5. **npm authentication in CI** — The workflow needs npm registry credentials for publishing. The architect should specify how credentials are provided (GitHub Secrets, OIDC, npm token) and whether `npm publish --access restricted` or `--access public` is used.

## Knowledge Stewardship

- Queried: /query-patterns for packaging, npm distribution, versioning -- not available in agent context (deferred tool). Specification based on SCOPE.md, risk assessment, and codebase inspection.
