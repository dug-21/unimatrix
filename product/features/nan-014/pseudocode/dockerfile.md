# dockerfile: Three-Stage cargo-chef Dockerfile

## Purpose

Build the `unimatrix` binary with all workspace crates, install ORT with SHA-256 verification, download and bake ONNX models, produce a minimal distroless runtime image. Per FR-1, ADR-002, ADR-005, ADR-006.

## New File

**File**: `Dockerfile` (repo root)

## Pseudocode

### Build Arguments

```dockerfile
# Pinned versions — update deliberately
ARG RUST_VERSION=1.89
ARG ORT_VERSION=1.20.1
ARG CHEF_VERSION=0.1.71

# SHA-256 hashes for ORT tarballs (ADR-002).
# Captured at implementation time via:
#   curl -sL <url> | sha256sum
# Implementation agent must download both tarballs and record actual hashes.
ARG ORT_SHA256_X64=<captured-at-implementation-time>
ARG ORT_SHA256_ARM64=<captured-at-implementation-time>
```

### Stage 1: Planner

```dockerfile
FROM rust:${RUST_VERSION}-slim-bookworm AS planner

# Install cargo-chef (pinned + locked, ADR-006).
RUN cargo install cargo-chef --version ${CHEF_VERSION} --locked

WORKDIR /src

# Copy workspace manifests and patches FIRST for recipe extraction.
# Order matters: patches must be present for workspace resolution.
COPY Cargo.toml Cargo.lock ./
COPY .cargo/config.toml .cargo/config.toml
COPY patches/ patches/

# Copy all crate Cargo.toml files (workspace members).
# Each crate needs its manifest for cargo-chef to parse the workspace.
COPY crates/unimatrix-store/Cargo.toml crates/unimatrix-store/Cargo.toml
COPY crates/unimatrix-vector/Cargo.toml crates/unimatrix-vector/Cargo.toml
COPY crates/unimatrix-embed/Cargo.toml crates/unimatrix-embed/Cargo.toml
COPY crates/unimatrix-core/Cargo.toml crates/unimatrix-core/Cargo.toml
COPY crates/unimatrix-server/Cargo.toml crates/unimatrix-server/Cargo.toml
COPY crates/unimatrix-engine/Cargo.toml crates/unimatrix-engine/Cargo.toml
COPY crates/unimatrix-adapt/Cargo.toml crates/unimatrix-adapt/Cargo.toml
COPY crates/unimatrix-observe/Cargo.toml crates/unimatrix-observe/Cargo.toml
COPY crates/unimatrix-eval/Cargo.toml crates/unimatrix-eval/Cargo.toml

# Extract dependency recipe.
RUN cargo chef prepare --recipe-path recipe.json
```

### Stage 2: Builder

```dockerfile
FROM rust:${RUST_VERSION}-slim-bookworm AS builder

# Install build dependencies (curl for ORT download, pkg-config for system libs).
RUN apt-get update && apt-get install -y --no-install-recommends \
    curl \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

# Install cargo-chef (same pinned version as planner, ADR-006).
RUN cargo install cargo-chef --version ${CHEF_VERSION} --locked

WORKDIR /src

# Install ORT with SHA-256 verification (ADR-002).
# TARGETARCH is injected by BuildKit: "amd64" or "arm64".
ARG TARGETARCH
ARG ORT_VERSION
ARG ORT_SHA256_X64
ARG ORT_SHA256_ARM64

RUN set -e && \
    ARCH=$([ "$TARGETARCH" = "arm64" ] && echo "aarch64" || echo "x64") && \
    HASH=$([ "$TARGETARCH" = "arm64" ] && echo "$ORT_SHA256_ARM64" || echo "$ORT_SHA256_X64") && \
    curl -fsSL \
      "https://github.com/microsoft/onnxruntime/releases/download/v${ORT_VERSION}/onnxruntime-linux-${ARCH}-${ORT_VERSION}.tgz" \
      -o ort.tgz && \
    echo "${HASH}  ort.tgz" | sha256sum -c - && \
    tar xzf ort.tgz && \
    cp onnxruntime-linux-${ARCH}-${ORT_VERSION}/lib/libonnxruntime.so* /usr/local/lib/ && \
    ldconfig && \
    rm -rf ort.tgz onnxruntime-linux-*

# Set ORT build environment for cargo.
ENV ORT_LIB_LOCATION=/usr/local/lib \
    ORT_PREFER_DYNAMIC_LINK=1

# Copy recipe from planner and cook dependencies (cached layer).
COPY --from=planner /src/recipe.json recipe.json
COPY .cargo/config.toml .cargo/config.toml
COPY patches/ patches/
COPY Cargo.toml Cargo.lock ./

# Cook dependencies — this layer is cached until Cargo.toml/Cargo.lock change.
RUN cargo chef cook --release --recipe-path recipe.json

# Copy full source and build.
COPY crates/ crates/

ENV RUSTFLAGS="-C link-arg=-Wl,-rpath,\$ORIGIN"

RUN cargo build --release && \
    strip target/release/unimatrix

# Download models using the just-built binary.
# HOME=/data so models land at /data/.cache/unimatrix/models/
ENV HOME=/data
RUN mkdir -p /data && \
    LD_LIBRARY_PATH=/usr/local/lib target/release/unimatrix model-download && \
    LD_LIBRARY_PATH=/usr/local/lib target/release/unimatrix model-download --nli

# Create /data directory with correct ownership and permissions (WARN-2).
# UID 65534 = nonroot user in distroless.
RUN mkdir -p /data && \
    chown 65534:65534 /data && \
    chmod 0700 /data
```

### Stage 3: Runtime

```dockerfile
FROM gcr.io/distroless/cc-debian12:nonroot AS runtime

# Copy binary from builder.
COPY --from=builder --chown=65534:65534 /src/target/release/unimatrix /usr/local/bin/unimatrix

# Copy ORT shared library from builder.
COPY --from=builder /usr/local/lib/libonnxruntime.so* /usr/local/lib/

# Copy baked-in models from builder.
# Models were downloaded to /data/.cache/unimatrix/models/ (HOME=/data in builder).
COPY --from=builder --chown=65534:65534 /data/.cache/ /data/.cache/

# Copy /data directory with correct ownership.
COPY --from=builder --chown=65534:65534 /data /data

# Environment variables (ADR-005).
ENV HOME=/data \
    LD_LIBRARY_PATH=/usr/local/lib \
    UNIMATRIX_LOG=info \
    UNIMATRIX_CONFIG=/etc/unimatrix/config.toml

# Declare volume mount point.
VOLUME ["/data"]

# Health check (ADR-003).
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD ["unimatrix", "health", "--project-dir", "/data"]

# No EXPOSE — no HTTP listener until W2-2 (constraint C-10).

# Entry point: foreground mode with explicit project dir.
ENTRYPOINT ["unimatrix"]
CMD ["serve", "--foreground", "--project-dir", "/data"]
```

## Key Design Decisions

### ORT SHA-256 Verification (ADR-002)

- Hashes are `ARG` values, not hardcoded in RUN commands, for visibility and easy update.
- `sha256sum -c -` fails the build immediately on mismatch.
- `curl -fsSL` — `-f` exits non-zero on HTTP errors (prevents extracting error pages as tarballs).
- `set -e` ensures any command failure stops the build.

### cargo-chef (ADR-006)

- Installed in BOTH planner and builder (stages don't share filesystem).
- Pinned version + `--locked` for reproducibility.
- Recipe extraction in planner; cooking in builder — Docker layer caching separates dependency compilation from source compilation.

### Model Baking

- `HOME=/data` in builder so `model-download` writes to `/data/.cache/unimatrix/models/`.
- Models are copied to runtime stage via `COPY --from=builder /data/.cache/ /data/.cache/`.
- At runtime, `HOME=/data` means `EmbedConfig::resolve_cache_dir()` resolves to `/data/.cache/unimatrix/models/` — same path where models were baked.
- Air-gap capable: no runtime internet needed (NFR-5).

### /data Directory

- Created in builder with `chown 65534:65534` and `chmod 0700` (WARN-2).
- Copied to runtime via `COPY --from=builder --chown=65534:65534 /data /data`.
- `VOLUME ["/data"]` declares the mount point for named volumes.
- Named volumes inherit ownership from the image on first mount.

### No EXPOSE

- Constraint C-10: no HTTP listener exists until W2-2.
- Adding `EXPOSE 8443` before the listener exists is misleading.

## Error Handling

| Error | Build Stage | Behavior |
|-------|-------------|----------|
| ORT hash mismatch | builder | `sha256sum -c` fails, build aborts with clear "FAILED" message |
| ORT download failure | builder | `curl -f` exits non-zero, `set -e` aborts build |
| cargo-chef version unavailable | planner/builder | `cargo install` fails, build aborts |
| Model download failure | builder | `model-download` exits non-zero, build aborts |
| Missing workspace crate | planner | `cargo chef prepare` fails with "missing member" |
| patches/anndists missing | planner | `COPY patches/` fails with "not found" |

## Key Test Scenarios

1. **Successful build on x86_64**: `docker build -t unimatrix .` succeeds on x86_64. Image size under 350 MB.

2. **Successful build on ARM64**: `docker build -t unimatrix .` succeeds on ARM64. Image size under 350 MB.

3. **ORT tampered hash**: Modify one SHA-256 hash in the Dockerfile. `docker build` fails at `sha256sum -c` with a clear checksum error.

4. **Air-gap run**: `docker run --network=none -v unimatrix-data:/data unimatrix` starts successfully. Logs show both embedding and NLI models loaded (no download attempt).

5. **Non-root execution**: `docker top <container>` shows UID 65534 for PID 1.

6. **HEALTHCHECK reports healthy**: `docker inspect --format='{{.State.Health.Status}}'` shows `healthy` after start-period.

7. **Graceful shutdown**: `docker stop <container>` produces graceful shutdown logs (vector dump, DB compaction). Exit code 0.

## Open Questions for Implementation Agent

1. **ORT SHA-256 hashes**: Must be captured by downloading the actual tarballs at implementation time. The hash values in this pseudocode are placeholders.

2. **cargo-chef version**: Verify 0.1.71 is current latest. If a newer version exists, use that and update the ADR reference.

3. **Model download paths**: Verify that `model-download` with `HOME=/data` writes to `/data/.cache/unimatrix/models/`. If the path differs, update the `COPY --from=builder` source path in the runtime stage.

4. **Planner stage COPY**: The per-crate Cargo.toml COPY commands assume 9 workspace crates. If the workspace has changed, update the list. Consider using `COPY crates/*/Cargo.toml crates/` if Docker supports it (may not work due to directory structure requirements).
