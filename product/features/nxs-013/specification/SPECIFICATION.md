# Specification: nxs-013 -- Co-locate Per-Project Config with Data Directory

## Objective

Align documentation, container configuration, and operational labeling with the reality that per-project `config.toml` is already the canonical configuration surface. The code loads per-project config from `data_dir.join("config.toml")` correctly; this feature removes the misleading `UNIMATRIX_CONFIG` ENV default from the Dockerfile, updates docker-compose.yml comments, relabels provenance log messages to reflect the per-project/global hierarchy, and updates documentation to present a single-directory operational model. No behavioral code changes to `load_config` merge semantics.

GitHub Issue: #637

---

## Functional Requirements

### FR-01: Remove UNIMATRIX_CONFIG ENV Default from Dockerfile

Remove the `UNIMATRIX_CONFIG=/etc/unimatrix/config.toml` line from the runtime ENV block in the Dockerfile (currently line 131). The `HOME=/data` ENV must remain. After removal, `docker inspect` on a built image must not show `UNIMATRIX_CONFIG` in the environment variables.

**Verification**: Build image, run `docker inspect --format '{{.Config.Env}}' <image>`, confirm `UNIMATRIX_CONFIG` is absent and `HOME=/data` is present.

### FR-02: Update docker-compose.yml Config Comments

Replace the current commented-out config bind mount block (lines 14-17) with comments that explain:
- Per-project config lives inside the `unimatrix-data` volume at the daemon's data directory path
- Config is written automatically on first run via `write_default_config_if_absent`
- `UNIMATRIX_CONFIG` env var can still be set explicitly for advanced use (e.g., Kubernetes ConfigMap mount)
- Backup = snapshot `unimatrix-data` volume (includes databases, vector indexes, config, and logs)

Remove any reference to `/etc/unimatrix/config.toml` bind mount patterns.

**Verification**: Read docker-compose.yml; confirm no `/etc/unimatrix/` references remain; confirm backup guidance is present in comments.

### FR-03: Update Provenance Log Labels in main.rs

In `log_config_provenance` (main.rs, currently lines 1347-1375), update the log message strings:

| Current Label | New Label |
|---------------|-----------|
| `"global config loaded"` | `"defaults config loaded (global)"` |
| `"global config not found; using compiled defaults"` | `"defaults config not found (global); using compiled defaults"` |
| `"project config loaded"` | `"primary config loaded (per-project)"` |
| `"project config not found; using compiled defaults"` | `"primary config not found (per-project); write default with 'unimatrix config'"` |

The `env_override` branch labels remain unchanged -- they already correctly identify UNIMATRIX_CONFIG.

**Verification**: Start daemon with both config files present, check log output contains "primary" and "defaults" labels. Start daemon with neither file, confirm fallback messages use the new labels.

### FR-04: Update README Configuration Section

In README.md, update the Configuration section (around line 238) to:
- Lead with per-project `~/.unimatrix/{hash}/config.toml` as the canonical, primary config location
- Present global `~/.unimatrix/config.toml` as the optional cross-project defaults layer
- Remove or update the line referencing "Optional config override via read-only bind mount at `/etc/unimatrix/config.toml`" in the container description (line 62)
- Preserve the existing explanation of replace semantics for list fields

**Verification**: Read README.md; confirm per-project is presented first; confirm no reference to `/etc/unimatrix/config.toml` as the primary container config pattern.

### FR-05: Update PRODUCT-VISION.md W2-1 Section

Update the W2-1 section (lines 448-459) to reflect the shipped single-volume model:
- Replace the two-volume description (`unimatrix-data` + `unimatrix-shared`) with a single `unimatrix-data` volume containing databases, vector indexes, config, and logs
- Remove the reference to `unimatrix-shared` for ONNX models + config.toml -- models are baked into the image, config lives in the data volume
- Preserve the backup = volume snapshot statement
- Update the `[Medium]` security requirement (line 456): remove "config.toml as read-only bind mount from secrets manager, not in data volume" — config now lives in the data volume by design. Replace with guidance that sensitive config values can be injected via `UNIMATRIX_CONFIG` env var pointing to a secrets-manager-provided path.
- Constrain edits to the W2-1 volume description only; do not revise other W2-1 content or adjacent sections (per SR-03)

**Verification**: Read PRODUCT-VISION.md W2-1 section; confirm single `unimatrix-data` volume described; confirm no `unimatrix-shared` reference for config; confirm `[Medium]` security requirement updated.

### FR-06: Update WAVE2-ROADMAP.md W2-1 Section

Update the W2-1 volume list (lines 39-43) to match the shipped single-volume model:
- Replace the three named volumes (`unimatrix-knowledge`, `unimatrix-analytics`, `unimatrix-shared`) with a single `unimatrix-data` volume
- Note that ONNX models are baked into the image and config lives in the data volume
- Add a brief annotation indicating this corrects the original planning text to match the shipped design (per SR-04 recommendation)
- Constrain edits to the volume description only

**Verification**: Read WAVE2-ROADMAP.md W2-1 section; confirm single volume described with correction annotation.

### FR-07: Update DEFAULT_CONFIG_TOML Header Comment

In `DEFAULT_CONFIG_TOML` (config.rs, lines 3130-3138), update the header comment to:
- Emphasize that the per-project file (`~/.unimatrix/{project-hash}/config.toml`) is the canonical, primary configuration
- Label the global file (`~/.unimatrix/config.toml`) explicitly as the optional cross-project defaults layer
- Preserve the existing explanation of replace semantics for list fields

The template content and field-level comments remain unchanged.

**Verification**: Run `unimatrix config` and inspect the generated file header; confirm canonical/defaults language is present.

---

## Non-Functional Requirements

### NFR-01: Zero Behavioral Change

The three-layer merge order (global defaults -> per-project overrides -> UNIMATRIX_CONFIG env override) in `load_config` must remain byte-identical. No changes to `load_config`, `merge_configs`, `write_default_config_if_absent`, or any config parsing/merging code.

### NFR-02: Test Suite Stability

All existing tests must pass without modification. The 7 provenance tests in config.rs (starting at line 9219) assert on `SourceStatus` enum variants (structural types), not on log message strings, so FR-03 label changes do not affect them. The 4 category authority tests are unaffected. This is verified by SR-06 analysis: provenance tests are type-based, not string-based.

### NFR-03: Container Startup Correctness

A container built from the modified Dockerfile must start correctly without `UNIMATRIX_CONFIG` set, loading config from the per-project path inside the data volume (resolved via `HOME=/data` per ADR-005, entry #4573). Verification is log-based only due to the distroless runtime (SR-02).

### NFR-04: Backward Compatibility for Explicit UNIMATRIX_CONFIG

Operators who explicitly set `UNIMATRIX_CONFIG` (via `docker run -e`, Kubernetes env, or docker-compose environment block) must experience no change. The env var override mechanism in `load_config` Step 0 is untouched. Only the Dockerfile default is removed.

### NFR-05: Edit Boundary Discipline

Documentation edits are surgical:
- PRODUCT-VISION.md: W2-1 volume description only (approximately lines 448-459)
- WAVE2-ROADMAP.md: W2-1 volume list only (approximately lines 39-43)
- README.md: Configuration section and container description line
- docker-compose.yml: Config-related comment block only
- config.rs: `DEFAULT_CONFIG_TOML` header comment and `log_config_provenance` function only

No other sections, functions, or files are modified beyond these boundaries.

---

## Acceptance Criteria

| AC-ID | Criterion | Verification Method |
|-------|-----------|-------------------|
| AC-01 | Dockerfile runtime ENV block does not set `UNIMATRIX_CONFIG`. The env var mechanism remains in `load_config` code for explicit operator use. | `docker inspect --format '{{.Config.Env}}'` on built image; grep Dockerfile for `UNIMATRIX_CONFIG` |
| AC-02 | docker-compose.yml does not contain a commented-out config bind mount to `/etc/unimatrix/config.toml`. Instead, comments explain that per-project config lives in the data volume. | File inspection |
| AC-03 | `log_config_provenance` labels per-project config as "primary" and global config as "defaults" in log messages. | Start daemon, inspect log output |
| AC-04 | README Configuration section presents per-project `~/.unimatrix/{hash}/config.toml` as the canonical config location, with global as the optional defaults layer. | File inspection |
| AC-05 | PRODUCT-VISION.md W2-1 section describes a single `unimatrix-data` volume containing databases, vector indexes, config, and logs. No reference to `unimatrix-shared` for config. | File inspection |
| AC-06 | WAVE2-ROADMAP.md W2-1 section matches the updated single-volume model. | File inspection |
| AC-07 | `DEFAULT_CONFIG_TOML` header comment emphasizes per-project as canonical and global as defaults. | `unimatrix config` output inspection; source code inspection |
| AC-08 | Backup documentation in docker-compose.yml volume comment states that backup = snapshot `unimatrix-data` volume (includes config). | File inspection |
| AC-09 | Merge semantics unchanged: global defaults -> per-project overrides -> UNIMATRIX_CONFIG env override. Verified by existing tests passing without modification. | `cargo test --workspace` -- all pass, zero test file changes in the PR |
| AC-10 | Container starts correctly without `UNIMATRIX_CONFIG` set, loading config from the per-project path inside the data volume. | Build image, run container with only `unimatrix-data` volume, inspect startup logs for "primary config" messages |

---

## Domain Models

### Key Terms

| Term | Definition |
|------|-----------|
| **Per-project config** (primary) | `~/.unimatrix/{project-hash}/config.toml` -- the canonical configuration file for a single project. Located inside the project's data directory. Written automatically by `write_default_config_if_absent` on first run. |
| **Global config** (defaults) | `~/.unimatrix/config.toml` -- optional cross-project defaults file. Values here are overridden by per-project config on a field-by-field basis (replace semantics). |
| **UNIMATRIX_CONFIG env override** | Environment variable pointing to an explicit config file path. Highest priority in the merge order. Used by operators for Kubernetes ConfigMap mounts or secrets managers. Not set by default after nxs-013. |
| **Data directory** | `~/.unimatrix/{project-hash}/` -- contains databases, vector indexes, config.toml, and logs for a single project. In the container, resolves to `/data/.unimatrix/{hash}/` via `HOME=/data`. |
| **Config merge order** | Step 0: UNIMATRIX_CONFIG env (if set) -> Step 1: global config -> Step 2: per-project config. Per-project overrides global; env overrides both. Replace semantics for all fields including lists. |
| **ConfigProvenance** | Structured type (from PR #636) recording which config sources were loaded, not found, or not applicable. Used by `log_config_provenance` to emit labeled log messages. |
| **SourceStatus** | Enum: `Loaded { path }`, `NotFound { path }`, `NotApplicable`. Provenance tests assert on these variants, not on log message strings. |

### Relationships

```
ConfigLoadResult
  +-- config: UnimatrixConfig (effective merged config)
  +-- provenance: ConfigProvenance
        +-- global: SourceStatus       -- "defaults" layer
        +-- project: SourceStatus      -- "primary" layer
        +-- env_override: SourceStatus -- explicit override

Data Volume (unimatrix-data)
  /data/.unimatrix/{hash}/
    +-- config.toml    (per-project, primary)
    +-- knowledge.db
    +-- analytics.db
    +-- vector indexes
    +-- logs
```

---

## User Workflows

### Container Operator (Personal Cloud)

1. Pull image, create `unimatrix-data` volume, run container
2. Daemon starts, `write_default_config_if_absent` writes default config.toml to data directory
3. Operator customizes config by editing the file inside the volume (or using `docker cp`)
4. Backup = snapshot `unimatrix-data` volume (includes config + databases + indexes)
5. No need to manage a separate config bind mount

### Advanced Operator (Kubernetes / ConfigMap)

1. Set `UNIMATRIX_CONFIG=/path/to/configmap/config.toml` in pod env
2. Daemon loads the explicit override path (Step 0 in merge order)
3. Behavior unchanged from before nxs-013

### Developer (Local)

1. Run `unimatrix serve` or via MCP bridge
2. Config loaded from `~/.unimatrix/{hash}/config.toml` (per-project, primary)
3. Optional: create `~/.unimatrix/config.toml` for cross-project defaults
4. No workflow change -- this was already the behavior

---

## Constraints

| ID | Constraint | Source |
|----|-----------|--------|
| C-01 | No behavioral change to `load_config` -- three-layer merge must remain identical | SCOPE.md line 108 |
| C-02 | Existing tests must pass unmodified -- zero test file changes in PR | SCOPE.md line 109, SR-06 (mitigated: tests are type-based) |
| C-03 | `UNIMATRIX_CONFIG` env var stays in code -- only Dockerfile default removed | SCOPE.md line 110 |
| C-04 | Distroless runtime -- verification via `docker inspect` and log output only | SCOPE.md line 111, SR-02 |
| C-05 | PR #636 provenance types are the foundation -- no changes to `ConfigLoadResult`, `ConfigProvenance`, or `SourceStatus` types | SCOPE.md line 112 |
| C-06 | Documentation edits constrained to specific sections -- no broad vision document revision | SR-03, SR-04 |
| C-07 | docker-compose.yml comments target new users setting up their first deployment — explain the correct pattern, not a migration path | ADR-003 |

---

## Dependencies

| Dependency | Status | Notes |
|-----------|--------|-------|
| PR #636 (ConfigLoadResult, ConfigProvenance, SourceStatus types) | Merged 2025-05-25 | Foundation for provenance label changes |
| ADR-005 (Container Data Path Resolution, entry #4573) | Established | `HOME=/data` ensures per-project config resolves inside the data volume |
| Two-Level TOML Config Merge pattern (entry #2395) | Established | Documents merge pipeline; unchanged by nxs-013 |

### Files Modified

| File | Change Type | Scope |
|------|------------|-------|
| `Dockerfile` | Remove line | Remove `UNIMATRIX_CONFIG=/etc/unimatrix/config.toml` from runtime ENV |
| `docker-compose.yml` | Comment rewrite | Replace config bind mount comments with in-volume config explanation |
| `crates/unimatrix-server/src/main.rs` | Log label update | `log_config_provenance` function only (lines 1347-1375) |
| `README.md` | Prose update | Configuration section (~line 238) and container description (~line 62) |
| `product/PRODUCT-VISION.md` | Prose update | W2-1 volume description (~lines 448-459) |
| `product/WAVE2-ROADMAP.md` | Prose update | W2-1 volume list (~lines 39-43) |
| `crates/unimatrix-server/src/infra/config.rs` | Comment update | `DEFAULT_CONFIG_TOML` header (lines 3130-3138) |

---

## NOT in Scope

- **Moving unimatrix.db** -- already in the correct location (`~/.unimatrix/{hash}/`)
- **Changing merge order** -- global -> per-project -> env override remains unchanged
- **Removing global config support** -- continues to work as defaults layer
- **Removing UNIMATRIX_CONFIG env var from code** -- remains for explicit operator use
- **Changing `write_default_config_if_absent` target path** -- already writes to per-project path
- **Schema changes or new CLI flags** -- no new Rust types, APIs, or migrations
- **Changing `ConfigLoadResult`, `ConfigProvenance`, or `SourceStatus` types** -- only log message strings change
- **Modifying any test files** -- all tests must pass as-is
- **Broad PRODUCT-VISION.md revision** -- only W2-1 volume description
- **Broad WAVE2-ROADMAP.md revision** -- only W2-1 volume list

---

## Open Questions (Resolved)

### OQ-01: docker-compose.yml UNIMATRIX_CONFIG example

**Resolution**: Include a commented example of `UNIMATRIX_CONFIG` usage in docker-compose.yml for advanced operators. This serves both the migration path (SR-01) and Kubernetes use case documentation. Captured in FR-02.

### OQ-02: WAVE2-ROADMAP.md as historical document

**Resolution**: Correct the volume description with a brief annotation noting the correction reflects the shipped design. The roadmap is a living planning document (it has been updated before -- it tracks ASS research spike status). Captured in FR-06.

### OQ-03: Effective merge summary log line

**Resolution**: Out of scope for nxs-013. The individual source lines from `log_config_provenance` are sufficient. A summary line would be a new feature, not a labeling change. Can be proposed as a follow-up if operators request it.
