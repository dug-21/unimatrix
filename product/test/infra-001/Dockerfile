# infra-001: Dockerized Integration Test Harness
# Two-stage build: Rust compilation -> Python test runtime

# -- Stage 1: Builder --------------------------------------------------
FROM rust:1.89-bookworm AS builder

WORKDIR /app

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
    wget \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Install ONNX Runtime 1.20.1 shared library (matches ort 2.0.0-rc.9)
RUN wget -q https://github.com/microsoft/onnxruntime/releases/download/v1.20.1/onnxruntime-linux-x64-1.20.1.tgz \
    && tar xzf onnxruntime-linux-x64-1.20.1.tgz \
    && cp onnxruntime-linux-x64-1.20.1/lib/* /usr/local/lib/ \
    && ldconfig \
    && rm -rf onnxruntime-linux-x64-1.20.1*

# Copy binary from builder
COPY --from=builder /app/target/release/unimatrix-server /usr/local/bin/unimatrix-server

# Environment for ONNX Runtime and binary location
ENV LD_LIBRARY_PATH=/usr/local/lib
ENV UNIMATRIX_BINARY=/usr/local/bin/unimatrix-server
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
