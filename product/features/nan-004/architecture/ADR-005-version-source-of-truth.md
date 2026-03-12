## ADR-005: Cargo.toml Workspace Version as Single Source of Truth

### Context

nan-004 introduces two version domains: Rust crates (9 crates in the workspace) and npm packages (root + platform packages). These versions must stay synchronized. The current state is:

- All 9 Rust crates have `version = "0.1.0"` hardcoded individually.
- No npm packages exist yet.
- The SCOPE resolves initial version as `0.5.0` and requires lockstep versioning.

Cargo workspaces support version inheritance: `version.workspace = true` in each crate's `Cargo.toml` inherits from `[workspace.package] version` in the root `Cargo.toml`.

### Decision

The root `Cargo.toml` `[workspace.package] version` is the single source of truth for the project version.

**Rust side:**
- Add `version = "0.5.0"` to `[workspace.package]` in root `Cargo.toml`.
- Change all 9 crate `Cargo.toml` files from `version = "0.1.0"` to `version.workspace = true`.
- The `unimatrix-server` crate also moves `edition` and `rust-version` to workspace inheritance (it currently has these hardcoded while other crates already inherit).

**npm side:**
- `packages/unimatrix/package.json` and `packages/unimatrix-linux-x64/package.json` have `version: "0.5.0"`.
- The `/release` skill reads the version from `Cargo.toml` and writes it to all `package.json` files before committing.
- The CI pipeline validates that Cargo.toml version matches npm package.json versions before publishing.

**Version bump flow:**
1. `/release` skill prompts for bump level.
2. Updates `[workspace.package] version` in root `Cargo.toml`.
3. Updates all npm `package.json` `version` fields.
4. Generates CHANGELOG entries.
5. Commits, tags, pushes.

**`env!("CARGO_PKG_VERSION")` usage:**
The existing codebase uses `env!("CARGO_PKG_VERSION")` in `main.rs:278` to report the server version. With workspace version inheritance, this compile-time macro automatically reflects the workspace version. No code change needed.

### Consequences

**Easier:**
- Single place to check/change the version (root `Cargo.toml`).
- `cargo metadata` and `env!("CARGO_PKG_VERSION")` automatically stay in sync.
- All crates always have the same version — no version matrix to manage.
- The `/release` skill has a clear, mechanical workflow.

**Harder:**
- npm versions are a derived artifact — they can drift if someone edits `package.json` manually without updating `Cargo.toml`. The CI validation step catches this.
- If individual crates ever need independent versioning (for external consumers), this lockstep model must be revisited. Acceptable because there are no external crate consumers and the SCOPE explicitly excludes library crate distribution.
