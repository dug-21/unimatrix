# dockerignore: Build Context Minimization

## Purpose

Exclude non-essential files from the Docker build context to keep it under 5 MB (NFR-2). The build context must contain only: source code, Cargo files, patches (source only), `.cargo/config.toml`, and the Dockerfile.

## New File

**File**: `.dockerignore` (repo root)

### Content

```
# Build artifacts
target/
patches/anndists/target/

# Version control
.git/
.gitignore

# Documentation and product files
product/
*.md
!README.md

# Development tooling
.claude/
.github/
.vscode/
.devcontainer/

# npm packages
packages/
node_modules/
package-lock.json

# Environment and secrets
.env
.env.*

# Test fixtures and data
tests/
test-data/
*.snap
*.snap.new

# Editor files
*.swp
*.swo
*~

# OS files
.DS_Store
Thumbs.db
```

### What MUST remain in the build context

| Path | Why |
|------|-----|
| `crates/` | All workspace crate source code |
| `Cargo.toml` | Workspace root manifest |
| `Cargo.lock` | Locked dependency versions |
| `.cargo/config.toml` | Contains `ORT_LIB_LOCATION` and `ORT_PREFER_DYNAMIC_LINK` build settings |
| `patches/anndists/Cargo.toml` | Workspace patch dependency manifest |
| `patches/anndists/src/` | Workspace patch dependency source |
| `Dockerfile` | The build file itself |
| `docker-compose.yml` | Compose config (not needed by build, but small) |

### What MUST be excluded

| Path | Why |
|------|-----|
| `target/` | Build artifacts (can be hundreds of MB) |
| `patches/anndists/target/` | Patch crate build artifacts |
| `.git/` | Git history (can be tens of MB) |
| `product/` | Feature docs, specs, pseudocode |
| `packages/` | npm package directories |
| `.claude/` | Agent definitions, protocols, skills |
| `.github/` | CI workflows, not needed in image |

### Critical Inclusion Check

The `.dockerignore` must NOT exclude:
- `.cargo/` — contains `config.toml` with ORT build flags
- `patches/` (top-level) — only `patches/anndists/target/` is excluded
- `Cargo.lock` — `*.lock` glob would break dependency pinning
- Any `Cargo.toml` in workspace crates

## Error Handling

Not applicable (static file). Build context errors surface as Docker build failures with missing-file errors.

## Key Test Scenarios

1. **Build context size**: Run `docker build` from repo root. Build output shows context size. Assert under 5 MB.

2. **`.cargo/config.toml` included**: Verify `COPY .cargo/config.toml` succeeds in the Dockerfile. If excluded, ORT build flags are missing and compilation fails with linker errors.

3. **patches/anndists source included**: Verify `COPY patches/ patches/` succeeds and `cargo chef prepare` resolves the workspace patch. If excluded, cargo fails with "failed to get patches/anndists".

4. **patches/anndists/target excluded**: Verify `patches/anndists/target/` is not in the build context (would bloat context size).

5. **All Cargo.toml files included**: Verify all 9 workspace crate Cargo.toml files are accessible in the build context. Missing any one causes `cargo chef prepare` to fail.
