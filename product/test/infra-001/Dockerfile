# infra-001: Dockerized Integration Test Harness
# Three-stage build: ORT libs artifact -> Rust compilation -> Python test runtime

# -- Stage 0: ort-libs (ORT shared library artifact) -------------------
# Dedicated artifact stage: downloads + SHA-256-verifies ONNX Runtime once,
# consumed by BOTH the builder and test-runtime stages (they cannot share a
# base image: builder is rust:*, runtime is python:*).
#
# SOURCE OF TRUTH: /workspaces/unimatrix/Dockerfile lines 11-20 pin
# ORT_VERSION + ORT_SHA256_X64/ARM64. These values are duplicated here because
# Dockerfiles cannot import ARGs across files — a future ORT bump MUST update
# both the root Dockerfile and this file in lockstep (version-drift warning).
FROM debian:bookworm-slim AS ort-libs

# ARGs are per-stage in BuildKit (not inherited) and docker-compose.yml passes
# no build.args — so each MUST carry a baked default or it resolves empty and
# breaks the download / checksum. Defaults mirror root Dockerfile:12-20.
ARG TARGETARCH
ARG ORT_VERSION=1.20.1
ARG ORT_SHA256_X64=67db4dc1561f1e3fd42e619575c82c601ef89849afc7ea85a003abbac1a1a105
ARG ORT_SHA256_ARM64=ae4fedbdc8c18d688c01306b4b50c63de3445cdf2dbd720e01a2fa3810b8106a

RUN apt-get update && apt-get install -y --no-install-recommends \
    curl \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# TARGETARCH is injected by BuildKit: "amd64" or "arm64".
# sha256sum -c fails the build (set -e) on mismatch -> fail-closed (ADR-002).
RUN set -e && \
    ARCH=$([ "$TARGETARCH" = "arm64" ] && echo "aarch64" || echo "x64") && \
    HASH=$([ "$TARGETARCH" = "arm64" ] && echo "$ORT_SHA256_ARM64" || echo "$ORT_SHA256_X64") && \
    curl -fsSL \
      "https://github.com/microsoft/onnxruntime/releases/download/v${ORT_VERSION}/onnxruntime-linux-${ARCH}-${ORT_VERSION}.tgz" \
      -o ort.tgz && \
    echo "${HASH}  ort.tgz" | sha256sum -c - && \
    tar xzf ort.tgz && \
    mkdir -p /opt/ort/lib && \
    cp -a onnxruntime-linux-${ARCH}-${ORT_VERSION}/lib/*.so* /opt/ort/lib/ && \
    rm -rf ort.tgz onnxruntime-linux-* && \
    test -e /opt/ort/lib/libonnxruntime.so

# -- Stage 1: Builder --------------------------------------------------
FROM rust:1.89-bookworm AS builder

WORKDIR /app

# ONNX Runtime for compile-time linking. `ort` is default-features=false
# (download-binaries OFF — the project dynamic-links a local ORT), so ort-sys
# needs the lib present AND ORT_LIB_LOCATION set at compile time or it emits a
# sentinel link flag and the link fails. Both ldconfig and ORT_LIB_LOCATION are
# required (not redundant): ORT_LIB_LOCATION is read by the ort-sys build script
# at compile; ldconfig refreshes the loader cache for the link step and for
# dlopen during `cargo test --lib`. Mirrors root Dockerfile:56-77.
COPY --from=ort-libs /opt/ort/lib/ /usr/local/lib/
RUN ldconfig && test -e /usr/local/lib/libonnxruntime.so
ENV ORT_LIB_LOCATION=/usr/local/lib \
    ORT_PREFER_DYNAMIC_LINK=1

# Copy workspace files needed for compilation
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
COPY patches/ patches/

# Build release binary
RUN cargo build --release --package unimatrix-server

# Run unit tests as baseline gate
RUN cargo test --lib --workspace

# -- Stage 2: Test Runtime ---------------------------------------------
FROM python:3.12-slim-bookworm AS test-runtime

# Install system dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# ONNX Runtime shared library (SHA-256-verified in the ort-libs stage).
# Replaces the previous unverified wget download.
COPY --from=ort-libs /opt/ort/lib/ /usr/local/lib/
RUN ldconfig && test -e /usr/local/lib/libonnxruntime.so

# Copy binary from builder (bin target renamed unimatrix-server -> unimatrix in nan-004;
# the cargo package above is still named unimatrix-server)
COPY --from=builder /app/target/release/unimatrix /usr/local/bin/unimatrix

# Environment for ONNX Runtime and binary location
ENV LD_LIBRARY_PATH=/usr/local/lib
ENV UNIMATRIX_BINARY=/usr/local/bin/unimatrix
ENV ORT_DYLIB_PATH=/usr/local/lib/libonnxruntime.so

# Install Python test dependencies
RUN pip install --no-cache-dir \
    pytest \
    pytest-timeout \
    pytest-json-report

# Pre-download embedding model for offline tests (AC-17)
RUN pip install --no-cache-dir huggingface_hub && \
    python3 -c "from huggingface_hub import snapshot_download; snapshot_download('sentence-transformers/all-MiniLM-L6-v2')" || \
    echo "WARN: Model pre-download failed; server will download on first use"

# Copy test harness
WORKDIR /test
COPY product/test/infra-001/harness/ harness/
COPY product/test/infra-001/suites/ suites/
COPY product/test/infra-001/fixtures/ fixtures/
COPY product/test/infra-001/scripts/ scripts/
COPY product/test/infra-001/pytest.ini pytest.ini

# Make scripts executable
RUN chmod +x scripts/run.sh scripts/report.sh

# Results directory
RUN mkdir -p /results/logs

# Entrypoint
ENTRYPOINT ["scripts/run.sh"]
