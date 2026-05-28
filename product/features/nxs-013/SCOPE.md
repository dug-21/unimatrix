# nxs-013: Co-locate Per-Project Config with Data Directory

## Problem Statement

Per-project `config.toml` is the primary configuration surface (categories, weights, boosted_categories, adaptive_categories, domain packs, preset), yet documentation, container design, and operational tooling treat it inconsistently. The Dockerfile sets `UNIMATRIX_CONFIG=/etc/unimatrix/config.toml` as an external bind-mount path, creating a split where config lives outside the data directory that contains everything else about the project. This complicates backup/recovery (must snapshot two locations), container volume reasoning (config vs. data separation), and operational mental model ("where is my project's state?"). Wave 2 personal cloud deployment amplifies this: individual developers need a single-directory model for backup, migration, and container lifecycle.

## Goals

1. Establish `~/.unimatrix/{hash}/config.toml` as the canonical, documented config location for per-project configuration.
2. Remove `UNIMATRIX_CONFIG=/etc/unimatrix/config.toml` from the Dockerfile ENV defaults so the daemon naturally loads the per-project config from the data volume.
3. Update container design so backup = snapshot `unimatrix-data` volume (now includes config alongside databases, vector indexes, and logs).
4. Update provenance log labels to reflect per-project config as "primary" and global config as "defaults".
5. Update documentation (README, config.toml header, PRODUCT-VISION.md, feature graph) to present per-project config as canonical.
6. Clarify that global `~/.unimatrix/config.toml` remains supported as a cross-project defaults layer, and `UNIMATRIX_CONFIG` env var remains available as an explicit override mechanism.

## Non-Goals

- **Moving unimatrix.db** — already in the correct location (`~/.unimatrix/{hash}/`).
- **Changing merge order** — global defaults -> per-project overrides -> UNIMATRIX_CONFIG env override remains unchanged.
- **Removing global config support** — global config continues to work as a defaults layer.
- **Removing UNIMATRIX_CONFIG env var** — it remains available for operators who want an explicit override (e.g., Kubernetes ConfigMap mount). It just stops being the default container path.
- **Changing `write_default_config_if_absent` target path** — it already writes to `data_dir.join("config.toml")` (the per-project path). No change needed.
- **Schema changes or new CLI flags** — this is a documentation, container config, and log labeling change. No new Rust types or APIs.

## Background Research

### Code Verification

**`load_config` (config.rs:2120)**: Already loads per-project config from `data_dir.join("config.toml")` in Step 2. Global config from `home_dir.join(".unimatrix").join("config.toml")` in Step 1. `UNIMATRIX_CONFIG` env var as highest-priority override in Step 0. The three-layer merge is correct and unchanged.

**`write_default_config_if_absent` (config.rs:3353)**: Takes an explicit `path` parameter. The sole call site (main.rs:1407-1408) passes `paths.data_dir.join("config.toml")` — already writes to the per-project directory. No change needed.

**`log_config_provenance` (main.rs:1347-1375)**: Currently labels sources as "global config loaded/not found" and "project config loaded/not found". The issue requests labeling per-project as "primary" and global as "defaults" to reflect the canonical hierarchy.

**`DEFAULT_CONFIG_TOML` (config.rs:3130-3138)**: Header comment already documents the two-level hierarchy with per-project overriding global. No substantive change needed to the template content — the file path comment on line 3132 already says `~/.unimatrix/{project-hash}/config.toml`.

### Container Design (Current State)

**Dockerfile (line 131)**: Sets `UNIMATRIX_CONFIG=/etc/unimatrix/config.toml` in the runtime ENV block. This means the daemon always checks `/etc/unimatrix/config.toml` first (Step 0 in load_config). When no bind mount exists at that path, load_config logs a debug message and falls through to the per-project path. The env var is harmless when unused but creates confusion: operators see it in `docker inspect` and think config must go to `/etc/unimatrix/`.

**docker-compose.yml (lines 14-17)**: Documents a commented-out bind mount for `./config.toml:/etc/unimatrix/config.toml:ro`. This pattern assumes config lives outside the data volume.

**ADR-005 (Unimatrix entry #4573)**: Documents `HOME=/data` so all data lands under `/data`. Notes that `dirs::config_dir()` also resolves under `/data`. The "Harder" consequence explicitly calls out: "Config bind mount at /etc/unimatrix/config.toml needs explicit config loading support." With nxs-013, this consequence is resolved by removing the bind mount pattern as the default — config lives in the data volume naturally.

### Product Vision / Wave 2 Context

**PRODUCT-VISION.md W2-1 (lines 448-456)**: Describes two named volumes: `unimatrix-data` for databases and `unimatrix-shared` for ONNX models + config.toml. The actual shipped nan-014 container simplified this to a single `unimatrix-data` volume with models baked into the image. The vision text still references the two-volume model and describes config as belonging in `unimatrix-shared` with a read-only bind mount. This needs updating.

**WAVE2-ROADMAP.md W2-1 (lines 39-43)**: Lists three named volumes including `unimatrix-shared` for ONNX models + config.toml as read-only bind. Also out of date with the shipped single-volume design.

### PR #636 Dependency (Merged 2025-05-25)

Added `ConfigLoadResult`, `ConfigProvenance`, and `SourceStatus` types. `load_config` now returns structured provenance metadata alongside the effective config. `log_config_provenance` logs each source status at appropriate levels. This is the foundation nxs-013 builds on — the provenance labels are the thing being updated.

### Unimatrix Knowledge Relevant Entries

- **Entry #2395 (Two-Level TOML Config Merge pattern)**: Documents the merge pipeline, replace semantics, validation ordering. No conflict with nxs-013.
- **Entry #4573 (ADR-005: Container Data Path Resolution)**: Documents `HOME=/data` strategy. Explicitly notes config bind mount needs at `/etc/unimatrix/config.toml` — nxs-013 eliminates this need.
- **Entry #4551 (Goal: developer-friendly deployment)**: nxs-013 directly serves this goal by simplifying the operational model.

## Proposed Approach

### Phase 1: Container Config Simplification

1. **Remove `UNIMATRIX_CONFIG=/etc/unimatrix/config.toml` from Dockerfile ENV** — the daemon will naturally load per-project config from `/data/.unimatrix/{hash}/config.toml` (which is inside the data volume). The `UNIMATRIX_CONFIG` env var mechanism remains in `load_config` code for operators who explicitly set it.

2. **Update docker-compose.yml comments** — remove the config bind mount example (lines 14-17). Replace with a comment explaining that per-project config lives inside the data volume and is written automatically on first run. Add a note that `UNIMATRIX_CONFIG` can still be set explicitly for advanced use.

### Phase 2: Provenance Log Label Update

3. **Update `log_config_provenance` in main.rs** — change log messages from "project config loaded" to "primary config loaded (per-project)" and "global config loaded" to "defaults config loaded (global)". This makes the hierarchy visible in logs.

### Phase 3: Documentation Updates

4. **README Configuration section** — lead with per-project config as the canonical location. Present global config as the optional defaults layer.

5. **PRODUCT-VISION.md W2-1 section** — update volume descriptions to reflect the single-volume model with config inside the data volume. Remove references to `unimatrix-shared` for config.

6. **WAVE2-ROADMAP.md W2-1 section** — same update as PRODUCT-VISION.md.

7. **DEFAULT_CONFIG_TOML header** — minor wording: emphasize that the per-project file is canonical and global is the defaults layer.

### Rationale

The code already works correctly — `load_config` loads per-project config from `data_dir.join("config.toml")`, and `write_default_config_if_absent` writes there. The changes are:
- One line removed from Dockerfile (ENV)
- Comment updates in docker-compose.yml
- Log label changes in main.rs
- Documentation prose updates

No merge semantics change. No new code paths. No migration. This is a documentation/config/labeling alignment with reality.

## Acceptance Criteria

- AC-01: Dockerfile runtime ENV block does not set `UNIMATRIX_CONFIG`. The env var mechanism remains in `load_config` code for explicit operator use.
- AC-02: docker-compose.yml does not contain a commented-out config bind mount to `/etc/unimatrix/config.toml`. Instead, comments explain that per-project config lives in the data volume.
- AC-03: `log_config_provenance` labels per-project config as "primary" and global config as "defaults" in log messages.
- AC-04: README Configuration section presents per-project `~/.unimatrix/{hash}/config.toml` as the canonical config location, with global as the optional defaults layer.
- AC-05: PRODUCT-VISION.md W2-1 section describes a single `unimatrix-data` volume containing databases, vector indexes, config, and logs. No reference to `unimatrix-shared` for config.
- AC-06: WAVE2-ROADMAP.md W2-1 section matches the updated single-volume model.
- AC-07: `DEFAULT_CONFIG_TOML` header comment emphasizes per-project as canonical and global as defaults.
- AC-08: Backup documentation in docker-compose.yml volume comment states that backup = snapshot `unimatrix-data` volume (includes config).
- AC-09: Merge semantics unchanged: global defaults -> per-project overrides -> UNIMATRIX_CONFIG env override. Verified by existing tests passing without modification.
- AC-10: Container starts correctly without `UNIMATRIX_CONFIG` set, loading config from the per-project path inside the data volume.

## Constraints

- **No behavioral change to `load_config`** — the three-layer merge (global -> per-project -> env override) must remain identical. Only log labels change.
- **Existing tests must pass unmodified** — AC-09 is a regression gate. The 7 provenance tests and 4 category authority tests from PR #636 must continue to pass.
- **`UNIMATRIX_CONFIG` env var stays in code** — operators who explicitly set it (Kubernetes ConfigMap, secrets manager) must not break. Only the Dockerfile default is removed.
- **Distroless runtime** — no shell access in the container image. Changes must be verifiable via `docker inspect` and log output, not shell commands.
- **PR #636 is the dependency** — already merged (2025-05-25). No blocking dependencies remain.

## Open Questions

- OQ-01: Should the docker-compose.yml include a commented example of `UNIMATRIX_CONFIG` for advanced users who want to inject config from outside the volume? Or is removing all mention of it sufficient for the personal cloud tier?
- OQ-02: The WAVE2-ROADMAP.md still references `unimatrix-shared` volume for "ONNX models + config.toml as read-only bind". Should this be corrected to reflect that models are baked into the image and config is in the data volume, or is the roadmap a historical document that should not be revised?
- OQ-03: Should `log_config_provenance` also log the effective merge result (e.g., "effective config: per-project primary, global defaults applied") as a single summary line, or are the individual source lines sufficient?

## Tracking

GitHub Issue: #637
