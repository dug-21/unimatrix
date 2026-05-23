## ADR-002: ORT Supply Chain Verification via SHA-256 Gate

### Context

SR-01 (High severity) identifies that ONNX Runtime is downloaded from GitHub Releases during `docker build` with no integrity verification. The existing `release.yml` has the same gap (Unimatrix entry #4274). A CDN outage, tag mutation, or MITM attack could inject a compromised ORT binary into the image.

SR-02 flags the same risk for `cargo-chef`, which is installed via `cargo install` inside the Dockerfile without version pinning or checksum verification.

The Dockerfile downloads ORT tarballs for two architectures, selected by the `TARGETARCH` build arg:
- `onnxruntime-linux-x64-1.20.1.tgz`
- `onnxruntime-linux-aarch64-1.20.1.tgz`

### Decision

**ORT verification**: Pin SHA-256 hashes as `ARG` values in the Dockerfile. After `curl` download, verify with `sha256sum -c` before extraction. Build fails immediately on hash mismatch.

```dockerfile
ARG ORT_VERSION=1.20.1
ARG ORT_SHA256_X64=<hash-captured-at-implementation>
ARG ORT_SHA256_ARM64=<hash-captured-at-implementation>
```

The verification script selects the correct hash based on `TARGETARCH`:

```dockerfile
RUN set -e && \
    ARCH=$([ "$TARGETARCH" = "arm64" ] && echo "aarch64" || echo "x64") && \
    HASH=$([ "$TARGETARCH" = "arm64" ] && echo "$ORT_SHA256_ARM64" || echo "$ORT_SHA256_X64") && \
    curl -fsSL "https://github.com/microsoft/onnxruntime/releases/download/v${ORT_VERSION}/onnxruntime-linux-${ARCH}-${ORT_VERSION}.tgz" -o ort.tgz && \
    echo "${HASH}  ort.tgz" | sha256sum -c - && \
    tar xzf ort.tgz && \
    cp onnxruntime-linux-${ARCH}-${ORT_VERSION}/lib/libonnxruntime.so* /usr/local/lib/ && \
    ldconfig && \
    rm -rf ort.tgz onnxruntime-linux-*
```

**cargo-chef verification**: Pin the cargo-chef version in the `cargo install` command:

```dockerfile
RUN cargo install cargo-chef --version 0.1.71 --locked
```

The `--locked` flag ensures the resolved dependencies match the published lockfile. Full binary hash verification is not practical for `cargo install` (builds from source), but version pinning + `--locked` prevents silent upgrades.

**Build resilience**: The Dockerfile uses `curl -fsSL` (fail on HTTP errors, silent, follow redirects, show errors). `-f` causes curl to exit non-zero on server errors (404, 500), preventing extraction of error pages as if they were tarballs.

### Consequences

- **Easier**: Supply chain integrity verified at build time. Tampered or wrong-version ORT binaries cause immediate, clear build failure.
- **Easier**: Closes the gap identified in Unimatrix entry #4274 for the Dockerfile path (the release.yml backport is a separate follow-up).
- **Harder**: ORT version upgrades require updating both the version string and both SHA-256 hashes. This is intentional — version upgrades should be deliberate, verified changes.
- **Harder**: cargo-chef version must be manually bumped. Acceptable: cargo-chef is a build tool, not a runtime dependency; updates are infrequent.
