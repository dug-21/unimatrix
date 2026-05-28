# nxs-013: Architecture — Co-locate Per-Project Config with Data Directory

## System Overview

nxs-013 aligns documentation, container defaults, and log labeling with the reality that per-project `config.toml` is the canonical configuration surface. The code already loads config from `data_dir.join("config.toml")` (per-project) with global config as the defaults layer. The Dockerfile, docker-compose.yml, log messages, and documentation still present the `/etc/unimatrix/config.toml` bind-mount pattern as the primary path. This feature resolves that inconsistency.

No Rust logic changes. No new types, APIs, or migrations. The three-layer merge (global -> per-project -> env override) is untouched.

### Relationship to Prior ADRs

- **ADR-005 (Unimatrix #4573)**: Established `HOME=/data` + `--project-dir /data` for container path resolution. Its "Harder" consequence noted: "Config bind mount at /etc/unimatrix/config.toml needs explicit config loading support." nxs-013 resolves this consequence — config lives in the data volume naturally; the bind mount is no longer the default path.
- **Two-Level TOML Config Merge pattern (Unimatrix #2395)**: Documents the merge pipeline. nxs-013 does not alter merge semantics.
- **Co-locate config pattern (Unimatrix #4626)**: Captures the general principle this feature implements.

## Component Breakdown

### C1: Dockerfile ENV Block

**Responsibility**: Remove `UNIMATRIX_CONFIG=/etc/unimatrix/config.toml` from the runtime ENV statement (line 131).

**Current state**: Line 128-131 sets four ENV vars. `UNIMATRIX_CONFIG` causes `load_config` Step 0 to always check `/etc/unimatrix/config.toml` first. When no bind mount exists, this is a harmless no-op (debug log + fallthrough), but it confuses operators who see it in `docker inspect`.

**Target state**: Three ENV vars remain: `HOME=/data`, `LD_LIBRARY_PATH=/usr/local/lib`, `UNIMATRIX_LOG=info`. Without `UNIMATRIX_CONFIG`, the daemon loads per-project config from `/data/.unimatrix/{hash}/config.toml` via `load_config` Step 2 — which is inside the data volume.

### C2: docker-compose.yml Comments

**Responsibility**: Replace the config bind-mount comment block with documentation explaining per-project config in the data volume.

**Current state**: Lines 14-17 contain a commented bind-mount example (`./config.toml:/etc/unimatrix/config.toml:ro`) and reference `UNIMATRIX_CONFIG`.

**Target state**: Comments explain that per-project config lives inside the data volume, is written automatically on first run, and that `UNIMATRIX_CONFIG` env var remains available for advanced use (Kubernetes ConfigMap, etc.).

### C3: log_config_provenance Labels (main.rs)

**Responsibility**: Update log message strings in `log_config_provenance` to reflect the canonical hierarchy: per-project is "primary", global is "defaults".

**Current state**: Messages say "global config loaded/not found" and "project config loaded/not found".

**Target state**: Messages say "defaults config loaded (global)" / "defaults config not found (global)" and "primary config loaded (per-project)" / "primary config not found (per-project)".

### C4: README Configuration Section

**Responsibility**: Update the Configuration section to present per-project config as the canonical location.

**Current state**: Line 240 leads with "two optional TOML files" without establishing primacy. Line 62 mentions "optional config override via read-only bind mount at `/etc/unimatrix/config.toml`" in the container quickstart.

**Target state**: Per-project config presented first as canonical. Global config presented as the optional defaults layer. Container quickstart sentence updated to reference config in the data volume.

### C5: PRODUCT-VISION.md W2-1 Section

**Responsibility**: Update the W2-1 volume description to reflect the single-volume model.

**Current state**: Lines 448-451 describe two volumes: `unimatrix-data` and `unimatrix-shared` (ONNX models + config.toml as read-only bind). This was never shipped — nan-014 delivered a single `unimatrix-data` volume with models baked in.

**Target state**: W2-1 describes a single `unimatrix-data` volume containing databases, vector indexes, config, and logs. No reference to `unimatrix-shared` for config.

### C6: WAVE2-ROADMAP.md W2-1 Section

**Responsibility**: Update the W2-1 volume list to reflect shipped reality.

**Current state**: Lines 39-43 list three named volumes including `unimatrix-shared`.

**Target state**: Single `unimatrix-data` volume. ONNX models baked into image. Config in data volume.

### C7: DEFAULT_CONFIG_TOML Header

**Responsibility**: Minor wording update to the config template header to emphasize per-project as canonical.

**Current state**: Header (lines 3130-3138) documents the two-level hierarchy correctly but neutrally.

**Target state**: Adds a line establishing per-project as the canonical location, global as the defaults layer.

## Component Interactions

The components are independent. No component depends on another component's output. All changes can be made in parallel.

```
C1 (Dockerfile)  ─── independent ──┐
C2 (compose)     ─── independent ──┤
C3 (main.rs)     ─── independent ──┤──> All verified by existing test suite + manual inspection
C4 (README)      ─── independent ──┤
C5 (VISION)      ─── independent ──┤
C6 (ROADMAP)     ─── independent ──┤
C7 (config.rs)   ─── independent ──┘
```

## Technology Decisions

See ADR-001 through ADR-004 below.

## Integration Points

### load_config (config.rs:2120)

The three-layer merge in `load_config` is NOT modified by nxs-013. The function reads:
1. Step 0: `UNIMATRIX_CONFIG` env var (highest priority)
2. Step 1: Global config from `home_dir.join(".unimatrix").join("config.toml")`
3. Step 2: Per-project config from `data_dir.join("config.toml")`

Returns `ConfigLoadResult { config, provenance }`. Merge semantics unchanged.

### log_config_provenance (main.rs:1347)

Consumes `ConfigProvenance` struct. Only the log message strings change (C3). The function matches on `SourceStatus` enum variants (`Loaded`, `NotFound`, `NotApplicable`) — these types are unchanged.

### Provenance Tests (config.rs:9219-9340)

**SR-06 Resolution**: The 7 provenance tests assert on **structured types** (`SourceStatus::Loaded`, `SourceStatus::NotFound`, `SourceStatus::NotApplicable`) and path values. They do **not** assert on log message strings. `log_config_provenance` is not tested directly — it produces `tracing` output consumed by the subscriber, not by test assertions.

Therefore: AC-09 ("existing tests pass unmodified") is compatible with AC-03 ("update log labels"). Zero test changes required.

### write_default_config_if_absent (main.rs:1407)

Already writes to `paths.data_dir.join("config.toml")`. No change needed.

## Integration Surface

| Integration Point | Type/Signature | Source | Changed by nxs-013? |
|---|---|---|---|
| `load_config(home_dir: &Path, data_dir: &Path) -> Result<ConfigLoadResult, ConfigError>` | Function | config.rs:2120 | No |
| `ConfigLoadResult { config: UnimatrixConfig, provenance: ConfigProvenance }` | Struct | config.rs:2092 | No |
| `ConfigProvenance { global: SourceStatus, project: SourceStatus, env_override: SourceStatus }` | Struct | config.rs:2078 | No |
| `SourceStatus { Loaded { path }, NotFound { path }, NotApplicable }` | Enum | config.rs:2067 | No |
| `log_config_provenance(provenance: &ConfigProvenance)` | Function | main.rs:1347 | Yes — log strings only |
| `write_default_config_if_absent(path: &Path, force: bool)` | Function | config.rs | No |
| `DEFAULT_CONFIG_TOML` | Static str | config.rs:3130 | Yes — header comment only |
| `UNIMATRIX_CONFIG` ENV in Dockerfile | Container env | Dockerfile:131 | Yes — removed |

## Resolved Open Questions

### OQ-01: docker-compose.yml commented UNIMATRIX_CONFIG example

**Recommendation: Include a commented `UNIMATRIX_CONFIG` environment variable example; remove the bind-mount example.**

Rationale: The bind-mount pattern (`./config.toml:/etc/unimatrix/config.toml:ro`) is the thing being eliminated as the default. However, advanced operators (Kubernetes ConfigMap, secrets manager) need to know that `UNIMATRIX_CONFIG` exists as an override mechanism. A commented environment variable example serves this purpose without re-introducing the split-location confusion. See ADR-003.

### OQ-02: WAVE2-ROADMAP.md — correct to match reality or leave as historical

**Recommendation: Correct the W2-1 section to match shipped reality. Add a one-line annotation.**

Rationale: WAVE2-ROADMAP.md is referenced as "the authoritative Wave 2 planning document" in PRODUCT-VISION.md (line 439). An authoritative document with factually incorrect volume descriptions is actively harmful — readers (human and agent) will design against the wrong model. A one-line annotation ("Updated to reflect nan-014 shipped design") preserves the edit trail. See ADR-004.

### OQ-03: log_config_provenance summary line

**Recommendation: No. Do not add a summary line.**

Rationale: The individual source lines already convey the hierarchy clearly with the updated labels ("primary config loaded (per-project)", "defaults config loaded (global)"). A summary line adds log noise without new information. The effective merge result is already logged separately at line 1280 (`"config loaded"` with preset). If operators need merge diagnostics, they read the individual lines. See ADR-002.

## Risk Mitigations

| Risk ID | Mitigation |
|---|---|
| SR-01 (breaking change for container users) | ADR-001 documents this as intentional. docker-compose.yml comments provide migration guidance. The change is additive for the operator — removing an ENV default does not break containers that never used the bind mount. Containers that DID use the bind mount can set `UNIMATRIX_CONFIG` explicitly in their compose override. |
| SR-02 (distroless verification) | Verification via `docker inspect` (confirm ENV block) and log output (confirm provenance labels). No shell access needed. |
| SR-03 (PRODUCT-VISION.md scope creep) | Edits constrained to W2-1 volume description only. No other sections touched. |
| SR-04 (WAVE2-ROADMAP.md purpose) | OQ-02 resolved: correct with annotation. Edits constrained to W2-1 volume list (lines 39-43). |
| SR-05 (unresolved OQs) | All three OQs resolved in this architecture document. |
| SR-06 (provenance test assertions) | Verified: tests assert on structured types, not log strings. Zero test changes required. |
| SR-07 (README merge conflicts) | Surgical edits to Configuration section only. Check for open PRs touching README before delivery. |
