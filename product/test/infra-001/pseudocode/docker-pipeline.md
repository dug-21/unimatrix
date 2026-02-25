# Pseudocode: C1 — Docker Build Pipeline

## File: `Dockerfile`

```dockerfile
# ── Stage 1: Builder ─────────────────────────────────────
FROM rust:1.89-bookworm AS builder

WORKDIR /app

# Copy workspace files
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
COPY patches/ patches/

# Build release binary
RUN cargo build --release --package unimatrix-server

# Run unit tests as baseline gate
RUN cargo test --lib --workspace

# ── Stage 2: Test Runtime ────────────────────────────────
FROM python:3.12-slim-bookworm AS test-runtime

# Install system dependencies for ONNX Runtime
RUN apt-get update && apt-get install -y --no-install-recommends \
    wget \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Install ONNX Runtime 1.20.1 shared library (matches ort 2.0.0-rc.9)
# The binary links against libonnxruntime.so at runtime
RUN wget -q https://github.com/microsoft/onnxruntime/releases/download/v1.20.1/onnxruntime-linux-x64-1.20.1.tgz \
    && tar xzf onnxruntime-linux-x64-1.20.1.tgz \
    && cp onnxruntime-linux-x64-1.20.1/lib/* /usr/local/lib/ \
    && ldconfig \
    && rm -rf onnxruntime-linux-x64-1.20.1*

# Copy binary from builder
COPY --from=builder /app/target/release/unimatrix-server /usr/local/bin/unimatrix-server

# Set environment for ONNX Runtime
ENV LD_LIBRARY_PATH=/usr/local/lib
ENV UNIMATRIX_BINARY=/usr/local/bin/unimatrix-server
ENV ORT_DYLIB_PATH=/usr/local/lib/libonnxruntime.so

# Install Python test dependencies
RUN pip install --no-cache-dir \
    pytest \
    pytest-timeout \
    pytest-json-report

# Pre-download embedding model for offline tests (AC-17)
# Server lazy-loads from HuggingFace cache; we trigger download now
RUN mkdir -p /root/.cache/huggingface && \
    python3 -c "from huggingface_hub import snapshot_download; snapshot_download('sentence-transformers/all-MiniLM-L6-v2')" || \
    echo "Model pre-download optional — server will download on first use"

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
```

## File: `docker-compose.yml`

```yaml
version: "3.8"

services:
  test-runner:
    build:
      context: ../..
      dockerfile: product/test/infra-001/Dockerfile
      target: test-runtime
    environment:
      - TEST_SUITE=${TEST_SUITE:-all}
      - TEST_WORKERS=${TEST_WORKERS:-1}
      - PYTEST_ARGS=${PYTEST_ARGS:-}
      - RUST_LOG=${RUST_LOG:-info}
      - UNIMATRIX_BINARY=/usr/local/bin/unimatrix-server
    volumes:
      - test-results:/results
    tmpfs:
      - /tmp:size=512M

volumes:
  test-results:
```

## Key Design Decisions

- Build context is workspace root (`../..`) so Dockerfile can COPY Cargo.toml, crates/, patches/
- Multi-stage build: Rust compilation in builder, Python runtime in test-runtime
- ONNX Runtime installed as shared library matching ort 2.0.0-rc.9 (the version server was compiled against)
- Embedding model pre-downloaded via huggingface_hub for offline reproducibility
- tmpfs at /tmp:size=512M for test database I/O (avoids Docker volume overhead, ensures clean teardown)
- Single service: no separate server container (server is a subprocess per test)
- TEST_SUITE env var controls which suites run (mapped by run.sh)
- Results written to named volume for extraction after run

## .dockerignore (at workspace root)

The build context includes the full workspace for Rust compilation. A `.dockerignore` should exclude:

```
target/
.git/
product/features/
*.md
.claude/
```

However, since the Dockerfile only COPYs specific paths, large excluded directories just slow context sending. The `.dockerignore` is an optimization, not a correctness requirement.
