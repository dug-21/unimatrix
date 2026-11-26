# infra-001: Dockerized Integration Test Harness
# Three-stage build: ORT libs artifact -> Rust compilation -> Python test runtime

# -- Stage 0: ort-libs (ORT shared library artifact) -------------------
# Dedicated artifact stage: fetches + SHA-256-verifies ONNX Runtime once (hash-pinned
# PyPI wheel, GH #1031), consumed by BOTH the builder and test-runtime stages (they
# cannot share a base image: builder is rust:*, runtime is python:*).
#
# SOURCE OF TRUTH: .github/scripts/ort-requirements.txt (version + per-arch hashes),
# shared with the root Dockerfile ort-download stage, release.yml and CI.
FROM python:3.12-slim-bookworm AS ort-libs
COPY .github/scripts/install-ort-linux.sh .github/scripts/ort-requirements.txt /ort/
RUN ORT_PREFIX=/opt/ort/lib ORT_LDCONFIG=0 bash /ort/install-ort-linux.sh && \
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
# dlopen during `cargo test --lib`. Mirrors the root Dockerfile builder stage.
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
