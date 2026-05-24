# Alignment Report: nan-014 (Container Packaging — MIT Image)

> Reviewed: 2026-05-23
> Artifacts reviewed:
>   - product/features/nan-014/architecture/ARCHITECTURE.md
>   - product/features/nan-014/specification/SPECIFICATION.md
>   - product/features/nan-014/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md
> Roadmap source: product/WAVE2-ROADMAP.md
> Scope source: product/features/nan-014/SCOPE.md
> Scope risk: product/features/nan-014/SCOPE-RISK-ASSESSMENT.md

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Delivers W2-1 goal: containerized daemon, ONNX, air-gap, non-root, health check |
| Milestone Fit | PASS | Correctly targets W2-1; no future-milestone over-build |
| Scope Gaps | WARN | Two vision items not fully addressed (see below) |
| Scope Additions | PASS | No unauthorized scope additions detected |
| Architecture Consistency | PASS | Architecture, spec, and risk are internally consistent |
| Risk Completeness | PASS | 14 risks, 30 scenarios, scope-risk traceability matrix present |

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Simplification | Volume layout | Vision says two named volumes (`unimatrix-data` + `unimatrix-shared`); SCOPE.md resolves to single `/data` volume + optional config bind mount + models baked in image. Rationale documented in SCOPE "Volume Layout Reconciliation." Acceptable — simplifies ops, achieves same isolation goals. |
| Simplification | HEALTHCHECK scope | Vision says "HEALTHCHECK verifies daemon liveness and schema version currency." Source docs implement liveness only (UDS socket connect). Schema version check is not included. |
| Simplification | Volume permissions | Vision says `chmod 0700` on named volumes. Source docs use distroless `:nonroot` (UID 65534) with `COPY --chown`. No explicit `chmod 0700` step. Functional equivalence — distroless nonroot achieves non-root isolation without chmod. |
| Simplification | Non-root user naming | Vision says `USER unimatrix`. Source docs use UID 65534 (distroless `nonroot` default). No named `unimatrix` user. Functionally equivalent — non-root execution achieved. |
| Gap | Schema version in HEALTHCHECK | Vision specifies HEALTHCHECK verifies "schema version currency." Neither architecture, spec, nor risk docs address schema version verification in the health check. |
| Gap | `chmod 0700` on volumes | Vision specifies `[High] Named volumes owner-only at container build time (chmod 0700)`. Source docs achieve non-root isolation via UID 65534 but do not enforce 0700 mode on the volume directory. |

## Variances Requiring Approval

### 1. HEALTHCHECK does not verify schema version currency

**What**: The product vision states HEALTHCHECK verifies "daemon liveness and schema version currency." The source documents implement only liveness verification (UDS socket connect, exit 0/1). Schema version currency — verifying that the database schema matches the binary's expected version — is not part of the `unimatrix health` subcommand.

**Why it matters**: Schema version mismatch after a container upgrade is a real operational scenario. If a newer binary runs against an older database, migrations run automatically (existing behavior). But if a newer database schema is encountered by an older binary (downgrade), the daemon should fail. The RISK-TEST-STRATEGY mentions this edge case (under "Pre-existing `/data` volume from older version") but the health check does not verify it. The daemon's startup path likely catches this, but the HEALTHCHECK does not independently verify schema currency.

**Classification**: WARN

**Recommendation**: Accept for nan-014 delivery. The daemon's startup path already validates schema compatibility — a schema mismatch would prevent startup, which the health check would detect indirectly (daemon not running = unhealthy). Adding schema version to the health check response is a reasonable follow-up enhancement but not a functional gap. Document as a follow-up.

---

### 2. Volume permissions: functional equivalence vs. vision literal

**What**: Vision says `[High] Named volumes owner-only at container build time (chmod 0700)`. Source docs use distroless `:nonroot` (UID 65534) with `COPY --chown=65534:65534`. The `/data` directory is owned by UID 65534, but no explicit `chmod 0700` is applied. Default directory permissions in Docker are typically 0755 (world-readable).

**Why it matters**: In a container context, the security boundary is the container namespace, not Unix file permissions. However, if the `/data` volume is bind-mounted from a host directory shared between containers, 0755 would allow other processes to read the knowledge database. For named Docker volumes (the documented deployment), this is less of a concern since Docker manages access. The vision's `chmod 0700` is a defense-in-depth measure.

**Classification**: WARN

**Recommendation**: Accept with a note for the implementation agent to add `RUN chmod 0700 /data` in the builder stage alongside the `COPY --chown`. This is a one-line addition that satisfies the vision's security requirement literally and adds defense-in-depth. Low cost, no architectural impact.

## Detailed Findings

### Vision Alignment

The source documents deliver the W2-1 vision goal: "Knowledge survives infrastructure changes — production-grade deployment with clean backup, recovery, and standard container lifecycle."

Evidence of alignment:
- **Containerized daemon**: Dockerfile + docker-compose deliver single-command deployment (SCOPE AC-02, AC-03; Spec FR-1, FR-2)
- **ONNX runtime**: ORT 1.20.1 installed with SHA-256 verification; both models (embedding + NLI) baked into image (SCOPE AC-05, AC-11; Spec FR-1.3, FR-1.4)
- **Air-gap capability**: No runtime internet dependency; `--network=none` verification specified (Spec NFR-5; SCOPE AC-02)
- **Non-root execution**: UID 65534 via `gcr.io/distroless/cc-debian12:nonroot` (Spec NFR-3; SCOPE AC-04)
- **HEALTHCHECK**: `unimatrix health` subcommand with UDS liveness check (Spec FR-5; SCOPE AC-07)
- **Backup/recovery**: Volume snapshot of `unimatrix-data` is the backup strategy. PidGuard and SQLite WAL recovery handle ungraceful shutdowns (Risk-Test "Failure Modes" section)

Vision non-negotiables preserved:
- **Hash chain integrity**: Not touched by nan-014 (correct — infrastructure feature)
- **Single binary**: Container runs one `unimatrix` process, no sidecars (Spec C-5)
- **Graceful degradation**: Absent model file falls back to cosine (existing behavior, not modified)
- **Zero infrastructure optional**: "container is optional; daemon + UDS works without it" — preserved; `--foreground` is additive, `--daemon` unchanged (Arch ADR-001; Spec FR-4.5)

### Milestone Fit

nan-014 correctly targets W2-1 (Container Packaging) within Wave 2 (Personal Cloud Delivery). No over-build into future milestones detected.

Evidence:
- **No W2-2 content**: No `EXPOSE` directive (Spec C-10), no HTTP listener, no TLS, no bearer token. SCOPE non-goals explicitly exclude these.
- **No W2-3 content**: Single-project mode only. Multi-project routing deferred to W2-3 TenantRouter. SCOPE "Multi-Project Statement" explicitly addresses this.
- **No W2-4 content**: No GGUF model baking, no llama.cpp. SCOPE non-goal explicitly excludes this.
- **No enterprise content**: MIT-only crates. License boundary enforced by repository separation (SCOPE AC-10).

The WAVE2-ROADMAP note "W2-1 wraps W2-2 + W2-3" could imply nan-014 should deliver after W2-2/W2-3, but SCOPE explicitly analyzes this dependency and concludes the Dockerfile and CI pipeline are independent of transport/auth implementation. This is a reasonable interpretation — the container skeleton is the infrastructure that W2-2 and W2-3 features are deployed on, not dependent on them.

### Architecture Review

The architecture document is well-structured with 6 components, 6 ADRs, clear integration surfaces, and a volume layout diagram.

**Strengths**:
- ADR-001 (foreground mode) correctly identifies `tokio_main_daemon` as already self-contained — no unnecessary refactoring proposed
- ADR-005 (data path resolution) correctly identifies `--project-dir /data` as the container path override mechanism
- Component interaction diagram clearly shows the dependency chain
- Error boundary table covers all failure modes

**Consistency check**: Architecture and specification agree on all technical decisions. The architecture's "Open Questions for Implementation Agents" (ORT hashes, cargo-chef version, distroless digest, model baking path) are appropriately deferred to implementation time — these are build-time constants, not design decisions.

**Volume layout divergence between architecture and specification**: The architecture shows models under `/data/unimatrix/models/` (via `XDG_CACHE_HOME=/data`), while the specification shows models at `/usr/local/share/unimatrix/models/` (baked into image layer). The specification's OQ-1 acknowledges this as an open question for the architect. This is an internal inconsistency, but it is explicitly flagged — the implementation agent will resolve it. Not a vision alignment issue.

### Specification Review

The specification is thorough: 6 functional requirement groups, 9 non-functional requirements, 12 acceptance criteria, 10 constraints, and 3 open questions.

**FR-4.5 vs Architecture ADR-001**: The specification says "shared daemon logic must be extracted into a common function callable by both `--foreground` and `--daemon`" (SR-06 mitigation). The architecture says "No shared logic extraction is needed because `tokio_main_daemon` IS the shared logic." These are consistent — the architecture identifies that `tokio_main_daemon` already is the shared function; the specification's constraint is satisfied without code extraction. The risk strategy correctly traces this (SR-06 -> R-01 -> ADR-001).

**Spec C-9 vs Arch Component 4**: Both address the same SR-06 risk but with different framing. Spec C-9 says "extract into a common function." Arch Component 4 says "no extraction needed — `tokio_main_daemon` already is the common function." This is a communication gap, not a technical gap. The implementation agent reading both will understand the intent.

**Multi-arch support**: Correctly specified with native GHA runners (no QEMU), matching the existing `release.yml` binary build pattern. CI independence from binary/npm release is specified (NFR-9, FR-6.7, FR-6.8).

### Risk Strategy Review

14 risks identified, 30 test scenarios, scope-risk traceability matrix present. All 11 scope risks (SR-01 through SR-11) mapped to architecture risks.

**Strongest coverage**: R-02 (PidGuard PID 1 self-SIGTERM race) receives detailed analysis of the container restart race condition. This is the most likely container-specific bug and receives appropriate attention.

**R-13 (HOME=/data config resolution)** is an important catch: if `HOME=/data`, `dirs::config_dir()` would resolve to `/data/.config/`, potentially missing the bind-mounted `/etc/unimatrix/config.toml`. The risk strategy correctly identifies this and requires tracing the config loading path. The specification's OQ on model bake-in path is related.

**Integration risks section**: Identifies 4 cross-component risks (ProjectPaths resolution, ORT loading, model download in builder, PidGuard + flock). All are genuine integration concerns. The ORT loading chain (builder COPY -> LD_LIBRARY_PATH -> runtime) is correctly identified as a subtle failure mode.

**Edge cases**: 6 edge cases identified, including empty volume first run, schema downgrade, socket path length, concurrent container starts, startup health check timing, and SIGKILL WAL recovery. Comprehensive for an infrastructure feature.

### Multi-Project Statement Alignment with W2-3 TenantRouter

The SCOPE "Multi-Project Statement" correctly characterizes the relationship to W2-3:

> "nan-014 deliverables (Dockerfile, volume layout, foreground mode, health check, CI pipeline) require no restructuring when multi-project lands — the changes are additive at the daemon's service layer, not the container's infrastructure."

This is architecturally sound. The volume layout (`/data/projects/{hash}/`) already anticipates multiple project directories. W2-3's TenantRouter operates at the service layer, resolving `Arc<Store>` pairs per request — it does not change the container's volume structure. The container image is the same; the daemon's internal routing changes.

The specification's volume layout diagram shows `projects/{project-hash}/` under `/data/`, which is the correct structure for multi-project. No restructuring needed when W2-3 lands.

### MIT/Enterprise Boundary

Clean separation verified:
- SCOPE explicitly states "zero commercial code, zero enterprise volume layout, zero OAuth infrastructure" as non-goals
- AC-10: "The image contains zero enterprise/commercial code. Only the 9 MIT-licensed workspace crates are compiled."
- Architecture: no reference to `unimatrix-collective` repository or enterprise feature flags
- Specification "NOT in Scope" section: "Enterprise image: Ships from private `unimatrix-collective` repository. nan-014 delivers zero commercial code."
- The W2-2 admin port (8444) is correctly absent — it is described as an "enterprise extension point" in the roadmap, and nan-014 does not include it

No commercial code leakage detected.

## Knowledge Stewardship

- Queried: /uni-query-patterns for vision alignment patterns -- found #3742 (optional future branch must match scope intent) and #2298 (config key semantic divergence). #3742 is relevant: the architecture's model bake-in path (XDG_CACHE_HOME) vs specification's baked-in image path (/usr/local/share/) is an "optional future branch" pattern where docs diverge on implementation but flag it as an open question. #2298 is marginally relevant (no config key divergence in nan-014).
- Stored: nothing novel to store -- the WARNs found (HEALTHCHECK schema version omission, chmod 0700 literal compliance) are feature-specific and do not generalize to a recurring pattern. The volume layout simplification (two volumes -> one + baked models) is a one-time reconciliation, not a repeating pattern.
