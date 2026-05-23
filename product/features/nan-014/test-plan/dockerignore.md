# Test Plan: dockerignore

## Component

New file: `.dockerignore` (repo root). Minimizes build context to source + patches + Cargo files.

## Risk Coverage

| Risk | Scenario | Test |
|------|----------|------|
| R-09 (Med) | .dockerignore excludes required file | Build context content verification |
| R-09 (Med) | .cargo/config.toml NOT excluded | Grep assertion |
| R-09 (Med) | patches/anndists source included, target excluded | Grep assertion |
| R-09 (Med) | All workspace Cargo.toml included | Grep assertion |

## Shell Tests

### AC-09: Context size under 5 MB

**Act**: `docker build .` and observe context size in output (line: "Sending build context to Docker daemon X.XXB").

**Assert**:
- Context size under 5 MB
- If using BuildKit (default), size is not printed directly -- verify by listing context contents or comparing with/without .dockerignore

### Required files NOT excluded

**Act**: Grep the `.dockerignore` file.

**Assert** each of these is NOT in the exclusion list (or has a negation pattern):
- `.cargo/config.toml` (contains ORT_LIB_LOCATION, ORT_PREFER_DYNAMIC_LINK)
- `patches/anndists/Cargo.toml`
- `patches/anndists/src/`
- `Cargo.toml` (root)
- `Cargo.lock`
- All `crates/*/Cargo.toml` files

### Required exclusions present

**Act**: Grep the `.dockerignore` file.

**Assert** each of these IS excluded:
- `target/`
- `.git/`
- `product/`
- `packages/`
- `.claude/`
- `.github/`
- `patches/anndists/target/`
- `*.md` (documentation, except Cargo-relevant)
- `.env`

### Build succeeds with .dockerignore active

**Arrange**: Ensure `.dockerignore` is in place.

**Act**: `docker build -t unimatrix .`

**Assert**:
- Build succeeds (no missing file errors)
- No `COPY` step fails due to excluded files
- Specifically: `COPY .cargo/ .cargo/` and `COPY patches/ patches/` succeed

## Validation Checklist (Code Review)

- [ ] `target/` excluded (large build artifacts)
- [ ] `.git/` excluded (repository history)
- [ ] `product/` excluded (feature docs, test plans)
- [ ] `packages/` excluded (npm packages)
- [ ] `.claude/` excluded (agent definitions, skills)
- [ ] `.github/` excluded (CI workflows)
- [ ] `patches/anndists/target/` excluded (patch build artifacts)
- [ ] `.cargo/config.toml` NOT excluded (needed for ORT build config)
- [ ] `Dockerfile` itself is NOT excluded (BuildKit needs it in context)
- [ ] No overly broad globs that accidentally exclude needed files

## Integration Tests

No infra-001 tests. Build context is a Docker concern, not an MCP protocol concern.

## Edge Cases

- **New crate added to workspace**: If a future feature adds `crates/unimatrix-foo/`, the .dockerignore must not exclude it. Verify no `crates/` exclusion pattern exists.
- **`*.toml` glob**: If someone adds `*.toml` to .dockerignore, it would exclude `Cargo.toml` and `.cargo/config.toml`. Assert this pattern is NOT present.
