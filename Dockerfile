# Unimatrix — Production Container Image
#
# Three-stage cargo-chef build:
#   1. planner  — extract dependency recipe
#   2. builder  — compile, install ORT (SHA-256 verified)
#   3. runtime  — distroless, nonroot, air-gap capable
#
# Build:  docker build -t unimatrix .
# Run:    docker run -v unimatrix-data:/data -v unimatrix-shared:/shared unimatrix

# --- Pinned versions (update deliberately) ---
ARG RUST_VERSION=1.89
ARG ORT_VERSION=1.20.1
ARG CHEF_VERSION=0.1.71

# ORT SHA-256 hashes (ADR-002). Captured from GitHub Releases.
# Update when bumping ORT_VERSION:
#   curl -fsSL <tarball-url> | sha256sum
ARG ORT_SHA256_X64=67db4dc1561f1e3fd42e619575c82c601ef89849afc7ea85a003abbac1a1a105
ARG ORT_SHA256_ARM64=ae4fedbdc8c18d688c01306b4b50c63de3445cdf2dbd720e01a2fa3810b8106a

# =============================================================================
# Stage 1: Planner — extract cargo-chef recipe
# =============================================================================
FROM rust:${RUST_VERSION}-slim-bookworm AS planner

ARG CHEF_VERSION
RUN cargo install cargo-chef --version ${CHEF_VERSION} --locked

WORKDIR /src

# cargo chef prepare runs cargo metadata, which needs full source to resolve
# workspace members. The planner only outputs recipe.json — source isn't cached.
COPY . .

RUN cargo chef prepare --recipe-path recipe.json

# =============================================================================
# Stage 2: Builder — compile binary, install ORT
# =============================================================================
FROM rust:${RUST_VERSION}-slim-bookworm AS builder

# Build dependencies.
RUN apt-get update && apt-get install -y --no-install-recommends \
    curl \
    pkg-config \
    libssl-dev \
    g++ \
    && rm -rf /var/lib/apt/lists/*

ARG CHEF_VERSION
RUN cargo install cargo-chef --version ${CHEF_VERSION} --locked

WORKDIR /src

# --- ORT install with SHA-256 verification (ADR-002) ---
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

# ORT build environment for cargo (matches .cargo/config.toml defaults).
ENV ORT_LIB_LOCATION=/usr/local/lib \
    ORT_PREFER_DYNAMIC_LINK=1

# --- Dependency compilation (cached layer) ---
COPY --from=planner /src/recipe.json recipe.json
COPY .cargo/config.toml .cargo/config.toml
COPY patches/ patches/
COPY Cargo.toml Cargo.lock ./

RUN cargo chef cook --release --recipe-path recipe.json

# --- Full source build ---
COPY crates/ crates/

ENV RUSTFLAGS="-C link-arg=-Wl,-rpath,\$ORIGIN"

RUN cargo build --release && \
    strip target/release/unimatrix

# --- Directory setup ---
# /data: integrity-critical persistent storage (databases, config, logs).
# /shared: re-downloadable assets (ONNX models). Separate volume for backup separation.
# UID 65532 = nonroot user in distroless/cc-debian12:nonroot.
RUN mkdir -p /data /shared/models && \
    chown -R 65532:65532 /data /shared && \
    chmod 0700 /data /shared

# =============================================================================
# Stage 3: Runtime — distroless, nonroot
# =============================================================================
FROM gcr.io/distroless/cc-debian12:nonroot AS runtime

# Binary.
COPY --from=builder --chown=65532:65532 \
    /src/target/release/unimatrix /usr/local/bin/unimatrix

# ORT shared library.
COPY --from=builder /usr/local/lib/libonnxruntime.so* /usr/local/lib/

# /data directory with correct ownership and permissions.
COPY --from=builder --chown=65532:65532 /data /data

# /shared directory with correct ownership and permissions.
COPY --from=builder --chown=65532:65532 /shared /shared

# Environment (ADR-005).
ENV HOME=/data \
    LD_LIBRARY_PATH=/usr/local/lib \
    UNIMATRIX_LOG=info \
    UNIMATRIX_MODEL_CACHE=/shared/models

# Volume mount point for persistent data.
#
# Bind-mount note (vnc-034 / FR-A9, AC-W1-S8): when /data is supplied as a host
# bind-mount instead of a named volume, the host directory MUST be writable by
# UID 65532 (the nonroot runtime user). Otherwise first-boot cert/token
# provisioning fails loud-and-actionable (the Rust binary reports the path + the
# UID-65532 requirement and exits non-zero — no shell, no silent fallback).
#   e.g.  mkdir -p ./data && sudo chown 65532:65532 ./data
VOLUME ["/data", "/shared"]

# Liveness probe (ADR-003).
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD ["unimatrix", "--project-dir", "/data", "health"]

# TLS serving port (vnc-034 / ADR-007, FR-A8, AC-W1-S2, R-12).
# ONLY the TLS port is exposed — there is NO plaintext port. The container serves
# pinned-TLS HTTPS only; OSS posture has no plaintext-to-client mode.
# The HTTP listener is gated on UNIMATRIX_HTTP_ENABLED=true (set in compose.yaml,
# ADR-007). The global binary default http.enabled=false is unchanged — flipping
# serving on is a container-scoped env concern, not a code-default change.
# TLS auto-enables from the first-boot-provisioned cert (provisioned in the Rust
# binary, since distroless has no shell). The token/cert stay 0600 files on the
# /data volume and are NEVER baked into an image layer (NFR-06).
EXPOSE 8443

ENTRYPOINT ["unimatrix"]
CMD ["--project-dir", "/data", "serve", "--foreground"]
