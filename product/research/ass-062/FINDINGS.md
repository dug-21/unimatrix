# FINDINGS: Release and Versioning Strategy for Dual-Artifact Delivery

**Spike**: ass-062
**Date**: 2026-05-28
**Approach**: investigation
**Confidence**: directional

---

## Findings

### Q1: Versioning Model

**Answer**: Use **unified semver** -- one `vX.Y.Z` for both npm package and container image. Container-only rebuilds (base image CVE, model bundle change) bump the patch version on the unified semver, just like any other patch.

**Evidence**:

| Option | User confusion | Automation complexity | Compat tracking | Container-only patch handling |
|--------|---------------|----------------------|-----------------|-------------------------------|
| **Unified semver** | Lowest -- one version to check | Lowest -- existing pipeline unchanged | None | Patch bump on the unified version |
| Unified + build metadata (`v1.2.3+build.4`) | Low but tooling issues -- Docker Hub strips build metadata, `docker pull` cannot reference `+build.4` | Medium -- need separate tag logic for rebuilds | Minimal | Build metadata tag, but Docker tooling ignores it |
| Independent semver | Highest -- "which container works with which npm?" | Highest -- two version tracks, compatibility matrix, separate changelogs | Heavy -- N x M matrix | Clean, but at high cognitive cost |
| Unified + suffix (`v1.2.3-container.1`) | Medium -- semver pre-release semantics are confusing for post-release patches | Medium -- new tag pattern, suffix counter management | Moderate | Works but violates semver semantics (pre-release != post-release patch) |

Container-only patch frequency: The container image is deterministic (pinned Rust, pinned ORT, SHA-256 verified, distroless base). Base image CVEs require a rebuild, but distroless images have minimal attack surface. ONNX model changes always accompany code changes. Estimated frequency: 1-2 container-only patches per year -- not enough to justify the complexity of divergent versioning.

The project already has ADR-005 (Unimatrix #1202) establishing Cargo.toml workspace version as the single source of truth, with lockstep versioning across 9 crates and 2 npm packages. Adding the container as a third artifact in the same lockstep is the natural extension. A patch bump for a container-only change communicates "something changed" to all users; the alternative (silent container rebuild at the same version) violates the principle of least surprise.

**Recommendation**: Unified semver. One `vX.Y.Z` for npm package, container image, and all Rust crates. Container-only changes bump the patch version via the normal uni-release flow. No build metadata, no suffixes, no independent versioning.

---

### Q2: Tag Conventions and Pipeline Triggers

**Answer**: Keep the single `v*` tag trigger. Both pipeline branches (binary/npm and container) fire on the same tag. No separate container-only tag namespace. Pre-release tags (`v1.0.0-rc.1`) produce pre-release artifacts on both branches.

**Evidence**:

Current state from release.yml: `push.tags: ['v*']` triggers both the binary/npm branch and the container branch independently (ADR-004, Unimatrix #4572). Container-only rebuilds do not exist under unified semver -- a base image CVE fix bumps the patch, runs uni-release, and both branches execute. The binary/npm branch publishes a functionally identical npm package (same code), which is harmless.

Pre-release tags: `v1.0.0-rc.1` should trigger both branches. The current pipeline sets `prerelease: false` unconditionally -- this needs updating.

Container image tag scheme (for the multi-arch manifest):

| Tag | Meaning | Behavior |
|-----|---------|----------|
| `:v0.7.2` | Exact version | Immutable -- never overwritten |
| `:v0.7` | Minor float | Updated on each `v0.7.x` release |
| `:v0` | Major float | Updated on each `v0.x.y` release |
| `:latest` | Latest stable | Updated on each non-prerelease tag |
| `:v1.0.0-rc.1` | Pre-release exact | Immutable -- never overwritten |

Currently the pipeline only produces `:v{version}` and `:latest`. The minor and major float tags are missing. The `docker/metadata-action@v5` supports these natively:

```yaml
tags: |
  type=semver,pattern=v{{version}}
  type=semver,pattern=v{{major}}.{{minor}}
  type=semver,pattern=v{{major}}
  type=raw,value=latest,enable=${{ !contains(github.ref_name, '-') }}
```

**Recommendation**: Keep single `v*` tag trigger for both branches. Add minor/major float tags to container manifest creation. Add pre-release detection (`contains(github.ref_name, '-')`) to suppress `:latest` tag on pre-release builds and set `prerelease: true` on the GitHub Release. No separate container-only tag namespace.

---

### Q3: Container Image Variants

**Answer**: One image variant. No slim, no GGUF variant.

**Evidence**:

ASS-061 (Q4) established:
- ONNX models (embedding + NLI, ~75 MB combined): baked into the container image
- GGUF models (1-8 GB): separate `unimatrix-models` named volume with init-container download pattern

This eliminates the need for `:slim` (no models) and `:gguf` (includes GGUF) variants. Total image size: ~155 MB (distroless ~30 MB + binary ~30 MB + ORT ~20 MB + ONNX models ~75 MB) -- lean for a container image.

A `:slim` variant without ONNX models would save ~75 MB but break core functionality unless users manually provide models via volume mount, creating a "broken by default" experience that contradicts the "no enrollment ceremony" principle.

If W2-6/Wave 3 adds an embedded SPA dashboard via `rust-embed`, the image grows by 1-5 MB -- negligible and not worth a separate variant.

**Recommendation**: Ship one container image variant with ONNX models baked in. No slim or GGUF variants. GGUF users mount the models volume separately per the init-container pattern from ASS-061.

---

### Q4: Schema Migration and Container Upgrades

**Answer**: Forward-only migration on startup (current design is correct). Add a forward compatibility guard that refuses to start if the volume's schema version exceeds the binary's `CURRENT_SCHEMA_VERSION`. No automated rollback -- document manual recovery via export/import.

**Evidence**:

Current migration mechanism (from `migration.rs`):
- `migrate_if_needed()` runs from `SqlxStore::open()` on a dedicated non-pooled connection
- Reads `schema_version` from the `counters` table
- If `current_version >= CURRENT_SCHEMA_VERSION`, returns immediately (idempotent)
- Runs all migrations in a transaction; rolls back on failure
- Currently at schema version 27 with a linear migration chain

**Forward compatibility guard** (currently missing): If a user runs container v0.8.0 (schema version 30), then downgrades to v0.7.2 (schema version 27), the binary sees `current_version (30) >= CURRENT_SCHEMA_VERSION (27)` and returns Ok -- silently running against a schema it does not understand. The migration code should change from:

```rust
// Current: treats "too new" the same as "up to date"
if current_version >= CURRENT_SCHEMA_VERSION {
    return Ok(());
}
```

To a three-way check:

```rust
if current_version > CURRENT_SCHEMA_VERSION {
    return Err(StoreError::Migration {
        source: format!(
            "Database schema version {} is newer than binary schema version {}. \
             Upgrade the binary or restore from backup.",
            current_version, CURRENT_SCHEMA_VERSION
        ).into(),
    });
}
if current_version == CURRENT_SCHEMA_VERSION {
    return Ok(());
}
```

**Rollback story**: Schema migrations are forward-only. No automated rollback -- most migrations are additive (ADD COLUMN, CREATE TABLE, CREATE INDEX). The two destructive migrations (v5->v6, v8->v9) create backup files. Export/import (nxs-012, 11-table coverage with hash chain validation) provides a safe recovery path.

Recovery procedure for a bad upgrade:
1. Stop the container: `docker compose down`
2. If backup file exists: restore it (`cp unimatrix.db.v26-backup unimatrix.db`)
3. If no backup: re-import from the last known-good export
4. Start the container with the previous image version

**Recommendation**: Add a forward compatibility guard in `migrate_if_needed()` that refuses to start when the volume's schema version exceeds `CURRENT_SCHEMA_VERSION`. Document the recovery procedure. Do not build automated rollback.

---

### Q5: Compatibility Matrix (Server-Client Version Contract)

**Answer**: MCP protocol provides the stability layer. The server already advertises its version in the MCP `initialize` handshake. Adopt same-major-version compatibility: server vX.a.b works with any client built for vX.c.d. No minimum client version enforcement.

**Evidence**:

Current version advertisement: The MCP `initialize` response includes `ServerInfo { server_info: Implementation { name: "unimatrix", version: env!("CARGO_PKG_VERSION") } }`. Every MCP client that connects receives the server version.

MCP protocol stability: Tools are discovered via `tools/list` and schemas are self-describing. A client that discovers `context_search` with its current schema can call it regardless of the server version. New tools added in a server upgrade are additional entries in `tools/list` -- old clients ignore them.

Breaking changes analysis:
- **Tool removal**: Would break clients. Has never happened (14 tools, all additive). Rule: never remove a tool in a minor version.
- **Tool schema change**: Would break clients. Has not happened -- all changes additive (new optional parameters with defaults). Rule: new required parameters are a major version bump.
- **Response format change**: Low risk. MCP tool responses are text content -- clients present them to the LLM, which is format-tolerant.

Pre-1.0 compatibility contract:
- Patch (`0.7.1` -> `0.7.2`): No breaking changes.
- Minor (`0.7.x` -> `0.8.0`): New tools may be added. Existing tools maintain backward-compatible schemas.
- Breaking tool contract change: Bump minor version and document in changelog (pre-1.0 equivalent of major bump).

Post-1.0: Same major version = compatible. Major version bump = tool contract changed, clients must upgrade.

No minimum client version enforcement: MCP clients are diverse (Claude Code, Codex CLI, Gemini CLI, custom integrations) and do not report Unimatrix client version -- they report their own product version in `ClientInfo`.

**Recommendation**: Same-major-version compatibility contract. Never remove tools or add required parameters in minor releases. Add `version` and `schema_version` fields to the future W2-2 HTTP health endpoint. No client enforcement mechanism.

---

### Q6: Changelog and Release Notes

**Answer**: One changelog. Container-specific changes use scoped conventional commits (`fix(container):`, `feat(container):`). The `uni-release` skill needs three updates.

**Evidence**:

One changelog is correct because: (1) users reference one CHANGELOG.md, (2) unified semver means one changelog entry per version, (3) container-specific changes are rare.

Container-specific changes should use scoped conventional commits and render under a `### Container` subsection:

```
## [0.8.1] - 2026-06-15

### Fixes
- search: improve NLI re-ranking threshold (#700)

### Container
- fix(container): update distroless base for CVE-2026-XXXX (#701)
```

Updates needed for `uni-release` skill:

1. **Pre-release support**: Accept `--pre-release rc.1` argument. Produce tag `v0.8.0-rc.1`. Set `prerelease: true` in GitHub Release.
2. **Container changelog section**: When commits matching `(container)` scope exist, render under `### Container` subsection.
3. **release.yml updates**: Add minor/major float tags to container metadata-action. Add pre-release detection to suppress `:latest` on RC builds.

**Recommendation**: Keep one CHANGELOG.md. Use scoped conventional commits for container changes. Update uni-release for pre-release support, container changelog section, and release.yml float tags.

---

## Unanswered Questions

None. All six bounded questions answered at directional confidence.

---

## Out-of-Scope Discoveries

1. **Forward compatibility guard missing in migration.rs**: `migrate_if_needed()` at line 66 returns `Ok(())` when `current_version >= CURRENT_SCHEMA_VERSION`, not distinguishing "up to date" (==) from "too new" (>). A binary running against a database migrated by a newer version could encounter unknown schema structures. Concrete bug -- file as standalone fix.

2. **Pre-release pipeline path not implemented**: release.yml sets `prerelease: false` unconditionally and does not detect pre-release tags. Blocks any RC/alpha release workflow.

3. **Container image lacks OCI version label**: Dockerfile does not set `org.opencontainers.image.version` explicitly. The `docker/metadata-action` generates labels but they are not verified post-build.

4. **Health endpoint does not report version**: Current health check (health.rs) is a binary liveness probe (UDS connect/fail). W2-2 HTTP health endpoint should include `version` and `schema_version` fields.

5. **Changelog generator does not handle scoped commits**: Current changelog generation in release.yml strips `feat: ` and `fix: ` prefixes but does not render conventional commit scopes like `(container)`.

---

## Recommendations Summary

| Question | Recommendation |
|----------|---------------|
| **Q1 -- Versioning Model** | Unified semver -- one `vX.Y.Z` for npm, container, and all crates; container-only changes bump the patch version |
| **Q2 -- Tag Conventions** | Keep single `v*` tag trigger for both pipeline branches; add minor/major float tags (`:v0.7`, `:v0`); add pre-release detection to suppress `:latest` on RC builds |
| **Q3 -- Container Variants** | One image variant with ONNX baked in; no slim or GGUF variants; GGUF uses separate volume per ASS-061 |
| **Q4 -- Schema Migration** | Add forward compatibility guard (refuse start when volume schema > binary schema); document recovery via export/import; no automated rollback |
| **Q5 -- Compatibility Contract** | Same-major-version compatibility; never remove tools or add required params in minor releases; add version to future W2-2 health endpoint; no client enforcement |
| **Q6 -- Changelog** | One CHANGELOG.md; scoped conventional commits for container changes; update uni-release for pre-release support and container changelog section |
