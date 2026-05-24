# Test Plan: dockerfile

## Component

New file: `Dockerfile` (repo root). Three-stage cargo-chef build producing a distroless runtime image.

## Risk Coverage

| Risk | Scenario | Test |
|------|----------|------|
| R-04 (High) | ORT SHA-256 verification correct hash | AC-05 positive build test |
| R-04 (High) | ORT SHA-256 tampered hash fails build | AC-05 negative build test |
| R-04 (High) | TARGETARCH selects correct ORT tarball | Code review of arch conditional |
| R-07 (Med) | cargo-chef with workspace patch | AC-01 build test |
| R-08 (Med) | Model bake-in path mismatch | AC-02 + AC-11 (--network=none) |
| R-12 (Low) | Image exceeds 350 MB | AC-01 size check |
| R-14 (Low) | Distroless tag not pinned to Debian 12 | Code review of FROM directive |

## Shell Tests

All dockerfile tests are shell-based (docker build/run commands). These are executed during Stage 3c by the tester, not by `cargo test`.

### AC-01: Build succeeds, image under 350 MB

**Act**: `docker build -t unimatrix .` from repo root.

**Assert**:
- Build completes without error
- `docker images unimatrix --format '{{.Size}}'` reports under 350 MB
- All three stages (planner, builder, runtime) complete successfully

### AC-02: Container starts air-gapped with models

**Arrange**: Build image.

**Act**: `docker run --rm --network=none -v unimatrix-test:/data unimatrix`

**Assert**:
- Process starts (logs appear)
- `/data` directory structure created (`.unimatrix/{hash}/` tree)
- Logs show embedding model loaded (no download attempt)
- No network errors in logs

### AC-04: Non-root execution

**Arrange**: Start container.

**Act**: `docker top <container>`

**Assert**:
- UID column shows 65534 (nonroot user)
- No UID 0 process

### AC-05: ORT SHA-256 tampered hash (R-04 negative test)

**Arrange**: Copy Dockerfile, modify one SHA-256 ARG value (change last character).

**Act**: `docker build -f Dockerfile.tampered .`

**Assert**:
- Build fails at the `sha256sum -c` step
- Error output contains "FAILED" or "checksum" diagnostic

### AC-10: No enterprise code

**Act**: Review Dockerfile.

**Assert**:
- No reference to `unimatrix-collective`
- No enterprise feature flags in `cargo build` command
- Only the workspace at `.` is compiled (`cargo build --release` with no extra `--features`)

### AC-11: NLI model baked in

**Arrange**: Build image, run with `--network=none`.

**Act**: Inspect container logs.

**Assert**:
- Logs show NLI model loaded (e.g., "nli model loaded" or "cross-encoder" message)
- No download attempt for NLI model

### R-12: Image size verification

**Act**: `docker images unimatrix --format '{{.Size}}'`

**Assert**:
- Under 350 MB. Expected ~253 MB per NFR-1 budget.

### R-14: Distroless Debian version pinned

**Act**: `grep 'FROM.*distroless' Dockerfile`

**Assert**:
- Contains `cc-debian12:nonroot` (not `cc:nonroot` or `cc-debian13`)
- Debian 12 explicitly in the tag

## Build Validation Checklist (Code Review)

- [ ] `FROM rust:1.89-slim-bookworm` for planner and builder stages
- [ ] `FROM gcr.io/distroless/cc-debian12:nonroot` for runtime stage
- [ ] `cargo install cargo-chef --version X.Y.Z --locked` in builder
- [ ] `COPY patches/ patches/` before `cargo chef prepare`
- [ ] `COPY .cargo/ .cargo/` for ORT build config
- [ ] ORT SHA-256 ARGs defined for both x64 and aarch64
- [ ] `sha256sum -c` after curl download with `set -e`
- [ ] `RUN chmod 0700 /data` in builder stage (WARN-2)
- [ ] `ENTRYPOINT ["unimatrix"]` + `CMD ["serve", "--foreground", "--project-dir", "/data"]`
- [ ] `HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3`
- [ ] `VOLUME ["/data"]` declared
- [ ] No `EXPOSE` directive (C-10)
- [ ] `ENV HOME=/data LD_LIBRARY_PATH=/usr/local/lib UNIMATRIX_LOG=info UNIMATRIX_CONFIG=/etc/unimatrix/config.toml`

## Integration Tests

No infra-001 tests. Dockerfile validation is shell-based, not MCP-protocol-based.

## Edge Cases

- **Large Cargo.lock changes**: Verify cargo-chef recipe invalidation works (dependency stage rebuilds).
- **patches/anndists/target/ excluded**: The .dockerignore must exclude build artifacts but include source. If the build fails with "can't find crate for `anndists`", the .dockerignore is wrong.
