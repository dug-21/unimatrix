# Unimatrix — Production Container Image
#
# Three-stage cargo-chef build:
#   1. planner  — extract dependency recipe
#   2. builder  — compile, install ORT (SHA-256 verified), bake models
#   3. runtime  — distroless, nonroot, air-gap capable
#
# Build:  docker build -t unimatrix .
# Run:    docker run -v unimatrix-data:/data unimatrix

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
# Stage 2: Builder — compile binary, install ORT, bake models
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

# --- Model bake-in ---
# HOME=/data so model-download writes to /data/.cache/unimatrix/models/.
ENV HOME=/data
RUN mkdir -p /data && \
    LD_LIBRARY_PATH=/usr/local/lib target/release/unimatrix model-download && \
    LD_LIBRARY_PATH=/usr/local/lib target/release/unimatrix model-download --nli && \
    rm -rf /data/.cache/huggingface

# --- /data directory setup (WARN-2) ---
# UID 65532 = nonroot user in distroless/cc-debian12:nonroot.
RUN chown -R 65532:65532 /data && \
    chmod 0700 /data

# =============================================================================
# Stage 3: Runtime — distroless, nonroot
# =============================================================================
FROM gcr.io/distroless/cc-debian12:nonroot AS runtime

# Binary.
COPY --from=builder --chown=65532:65532 \
    /src/target/release/unimatrix /usr/local/bin/unimatrix

# ORT shared library.
COPY --from=builder /usr/local/lib/libonnxruntime.so* /usr/local/lib/

# Baked-in models (embedding + NLI). Only copy the unimatrix model cache,
# not the huggingface hub cache (which has duplicate blobs and symlinks).
COPY --from=builder --chown=65532:65532 /data/.cache/unimatrix/ /data/.cache/unimatrix/

# /data directory with correct ownership and permissions.
COPY --from=builder --chown=65532:65532 /data /data

# Environment (ADR-005).
ENV HOME=/data \
    LD_LIBRARY_PATH=/usr/local/lib \
    UNIMATRIX_LOG=info

# Volume mount point for persistent data.
VOLUME ["/data"]

# Liveness probe (ADR-003).
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD ["unimatrix", "--project-dir", "/data", "health"]

# No EXPOSE — no HTTP listener until W2-2 (C-10).

ENTRYPOINT ["unimatrix"]
CMD ["--project-dir", "/data", "serve", "--foreground"]
