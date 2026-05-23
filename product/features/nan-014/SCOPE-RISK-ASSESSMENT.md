# Scope Risk Assessment: nan-014

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | ORT download from GitHub Releases during `docker build` is a supply chain SPOF — CDN outage, tag mutation, or MITM yields a broken or compromised image | High | Med | Architect should cache ORT tarballs as GHA artifacts or use a verified mirror as fallback. SHA-256 gate is necessary but not sufficient (ref #4274). |
| SR-02 | `cargo-chef` is a third-party build tool installed via `cargo install` inside the Dockerfile — no version pin or checksum means a compromised crate silently injects code into every build | High | Low | Pin cargo-chef version (`cargo install cargo-chef --version X.Y.Z`) and consider SHA verification of the installed binary. |
| SR-03 | Baking 166 MB of ONNX models into image layers increases build time and push/pull bandwidth; model updates require full image rebuild and re-push of those layers | Med | High | Architect should ensure model download is a separate Dockerfile stage so model layers cache independently of source changes (OQ-02 resolved, but verify stage isolation holds). |
| SR-04 | `gcr.io/distroless/cc-debian12:nonroot` is a mutable tag — Google can update the base image between builds, potentially changing glibc minor version or included libraries | Med | Med | Pin to a specific image digest (`@sha256:...`) in the Dockerfile for reproducible builds. Accept tag for convenience in compose docs. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-05 | SCOPE.md volume layout (single `/data`) diverges from WAVE2-ROADMAP.md (three named volumes: `unimatrix-knowledge`, `unimatrix-analytics`, `unimatrix-shared`). Resolved in SCOPE but architect must confirm the reconciliation does not create migration debt when W2-2/W2-3 land. | Med | Med | Document the single-to-multi volume migration path explicitly. Ensure `/data` internal directory structure matches the future volume split boundaries. |
| SR-06 | The `--foreground` code change touches the daemon startup path — a bug here breaks `serve --daemon` (the primary non-container mode) in addition to the new container mode | High | Med | Architect should ensure `--foreground` extracts the shared daemon logic into a common function called by both paths, not by duplicating or conditionally skipping steps in the existing daemon path. Test both modes in CI. |
| SR-07 | Non-goal boundaries with W2-2 (HTTPS) are clean at the Dockerfile level but the `EXPOSE 8443` directive already appears in ASS-043's sample Dockerfile — shipping it before W2-2 is confusing | Low | Med | Omit `EXPOSE` from nan-014 Dockerfile. Add it when W2-2 delivers the HTTPS listener. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-08 | Volume ownership with distroless `nonroot` (UID 65534): `COPY --chown=65534:65534` sets directory ownership in the image, but Docker named volumes only inherit image ownership on first mount — subsequent container updates leave existing volume ownership unchanged | Med | Med | Architect should verify that the daemon handles permission errors gracefully (clear error message) rather than panicking on volume write failure if ownership drifts. |
| SR-09 | PidGuard + flock behavior in container context: container restarts may leave stale PID files on the named volume. `handle_stale_pid_file` uses `/proc/{pid}/cmdline` which works inside the container (PID 1), but the stale-process-exit timeout (10s) may interact with Docker's stop timeout | Med | Low | Verify PidGuard cleanup is correct when the container's PID namespace resets on restart. PID 1 in a new container is not the same process as PID 1 in the old container. |
| SR-10 | CI pipeline adds 2 native-runner container build jobs + 1 manifest merge job to `release.yml`. If ARM64 runner availability is flaky (GHA ARM runners are newer infrastructure), the entire release pipeline blocks on container jobs | Med | Med | Architect should make container build jobs non-blocking for binary/npm releases (separate workflow or `needs` with `if: always()` on downstream jobs). |
| SR-11 | `unimatrix health` connects to UDS socket for liveness check. Inside the container, the socket path must match between the daemon and health subcommand — if `ProjectPaths` resolution differs between `serve --foreground` and `health`, the HEALTHCHECK silently fails | Med | Med | Both subcommands must resolve the same `ProjectPaths`. Architect should use an explicit socket path env var or shared constant rather than relying on identical path resolution logic. |

## Assumptions

- SCOPE.md assumes `rust:1.89-slim-bookworm` is available and glibc-compatible with `distroless/cc-debian12`. Both are Debian 12 today. If Rust 1.89 images shift to Debian 13 (trixie), glibc floor diverges. (Section: Constraints, glibc floor)
- SCOPE.md assumes ONNX models total ~166 MB and fit within a 350 MB image size budget. If NLI model quantization changes or a larger variant is selected, the budget breaks. (Section: AC-01, AC-11)
- SCOPE.md assumes `GITHUB_TOKEN` with `packages:write` is sufficient for GHCR push. If the repository is transferred or org-level GHCR policies restrict token-based pushes, a PAT or OIDC setup is required. (Section: OQ-03)
- SCOPE.md assumes the existing `tokio_main_daemon` function can run as PID 1 without modification beyond removing fork/setsid. Signal handling (SIGTERM) may need explicit registration since PID 1 in a container does not receive default signal behavior. (Section: OQ-01, Proposed Approach)

## Design Recommendations

- **SR-06 is the highest-blast-radius risk.** The foreground mode code change should be isolated: extract shared daemon logic into a `run_daemon_core()` function, called by both `--daemon` (via spawn-new-process child) and `--foreground` (directly). Do not modify the existing daemon-child path. (SR-06)
- **SR-01/SR-02**: Pin all build-time tool versions and verify checksums. The Dockerfile should not trust network-fetched binaries without hash verification — this applies to ORT and cargo-chef equally. (SR-01, SR-02)
- **SR-10**: Container CI jobs should not block binary/npm release artifacts. Use a separate workflow or make container jobs independent in the dependency graph. (SR-10)
