# FINDINGS-RAW: Multi-Project Data Architecture for Containerized Deployment

**Spike**: ASS-060
**Date**: 2026-05-28
**Approach**: investigation
**Confidence**: directional

---

## Findings

### Q1: Project Identity in a Container

**Question**: ADR-004 uses `SHA-256(project_root_path)` for project isolation — no equivalent concept in a container where there is no local project root. What replaces it?

**Answer**: Use explicit project slugs declared in a `[[projects]]` config table, with the slug itself as the filesystem directory name (replacing the path-hash). The path-hash mechanism (ADR-004) is retained unchanged for local UDS mode — it continues to work exactly as today. The slug mechanism is additive for HTTP-routed multi-project containers.

**Evidence**:

The current `project.rs` (line 131) computes `SHA-256(path_as_utf8)[..16]` from a canonical filesystem path. In a container, `--project-dir /data` produces a single deterministic hash. There is no concept of "which remote project is this request for" — the container serves exactly one project store per `--project-dir` value.

The `ensure_data_directory()` function (lines 146-187) uses `base_dir / project_hash` as the data directory. This maps cleanly to a slug-based model: `base_dir / slug` instead of `base_dir / hash`.

Evaluated options:

| Option | Determinism | Collision safety | Ergonomics | Enterprise upgrade |
|--------|------------|-----------------|------------|-------------------|
| **Explicit slug in config** | Deterministic (operator-declared) | Validated uniqueness at registration | High — human-readable, operator controls naming | JWT `unimatrix_project` claim maps directly to slug |
| Client-declared header | Per-request (no server-side registry) | None — any client can typo a new project into existence | Low — every client must know and declare correctly | JWT claim replaces header; but migration requires client config changes |
| Path-prefix routing | Per-request (embedded in URL) | None — same typo problem | Medium — visible in URLs but clutters the API surface | JWT claim replaces path prefix; but API path changes are breaking |

**Slug rules**: `^[a-z0-9][a-z0-9_-]{0,63}$` (lowercase, DNS-label-safe, max 64 chars). This matches the `source_domain` validation pattern already used in the observation pipeline (line 185 of config.toml). The slug doubles as the filesystem directory name under `/data/.unimatrix/`, so filesystem safety is required.

**Registration model**: Projects are declared in config, not created dynamically by clients. This prevents accidental project creation via typo and makes the server authoritative over its project set.

```toml
# Container multi-project config
[[projects]]
slug = "research-repo"
description = "Research knowledge base"

[[projects]]
slug = "main-dev"
description = "Primary development repo"
```

When `[[projects]]` is absent (default), the server operates in single-project mode using `--project-dir` path-hash — identical to today. This is the zero-config personal cloud path.

**Enterprise upgrade path**: The JWT `unimatrix_project` claim (W2-3 vision spec) validates against the registered project slugs. The `TenantRouter` resolves `slug -> Arc<Store>` pair. The slug IS the project identity at every tier.

**Recommendation**: Use explicit project slugs declared in `[[projects]]` config. Retain path-hash for local/single-project mode. The slug replaces the hash as the filesystem directory name in multi-project containers. Validation: `^[a-z0-9][a-z0-9_-]{0,63}$`.

---

### Q2: Volume Layout for N Projects

**Question**: Current W2-1 spec has 1 named volume (post-nan-014). With N projects, what layout?

**Answer**: Subdirectories within the single `unimatrix-data` volume, one subdirectory per project slug. NOT separate volumes per project. NOT shared databases across projects.

**Evidence**:

The current shipped layout (nan-014, Dockerfile line 133-134, docker-compose.yml):
```
/data/                              # VOLUME mount point
/data/.unimatrix/{hash}/            # Single project data dir
/data/.unimatrix/{hash}/unimatrix.db
/data/.unimatrix/{hash}/vector/
/data/.unimatrix/{hash}/config.toml
/data/.cache/unimatrix/             # Baked-in models (shared)
```

Multi-project layout extends this naturally:
```
/data/                              # Single VOLUME mount point
/data/.unimatrix/research-repo/     # Project 1
/data/.unimatrix/research-repo/unimatrix.db
/data/.unimatrix/research-repo/vector/
/data/.unimatrix/research-repo/config.toml
/data/.unimatrix/main-dev/          # Project 2
/data/.unimatrix/main-dev/unimatrix.db
/data/.unimatrix/main-dev/vector/
/data/.unimatrix/main-dev/config.toml
/data/.cache/unimatrix/             # Models (shared, read-only at runtime)
```

Evaluated trade-offs:

| Layout | Isolation | Backup granularity | Ops complexity | Compose complexity |
|--------|-----------|-------------------|----------------|-------------------|
| **Subdirs in one volume** | Per-project DB files (SQLite file-level isolation) | Per-project: `tar czf backup.tar.gz /data/.unimatrix/research-repo/` | Low — single volume to manage | Zero change — same `docker-compose.yml` |
| N separate volumes | Maximum — volume-level | Per-volume snapshot (native Docker) | High — N volume declarations, N mount points | Linear growth in compose file |
| One DB with schema isolation | None — shared write lock, shared WAL | Impossible per-project | Lowest ops | Zero change |

**Per-project backup** (subdirectory model): `VACUUM INTO` already works per-database (snapshot.rs uses it). Back up one project: copy its subdirectory. Back up all: snapshot the entire volume. The existing backup documentation in docker-compose.yml (lines 31-33) continues to work unchanged for full-volume backup.

The shared-DB option violates the architectural invariant from PRODUCT-VISION.md Section "Architectural Decisions Required Before Wave 2" — Decision 3: "Per-project `knowledge.db` + `analytics.db` for all tiers. Shared `analytics.db` across projects is a cross-project observation leakage risk. Per-project for both is the only safe model."

The container config co-location pattern (Unimatrix entry #4626) is preserved: each project's `config.toml` lives in its data subdirectory.

**Recommendation**: Use subdirectories within the single `unimatrix-data` volume, keyed by project slug. Each project gets its own `unimatrix.db`, `vector/`, and `config.toml`. Models remain shared at `/data/.cache/unimatrix/`. No docker-compose.yml changes required.

---

### Q3: Request Routing

**Question**: How does the HTTP server resolve which project store handles a request?

**Answer**: Use path-prefix routing: `/v1/{project-slug}/tools/...`. The slug in the URL path selects the project store. Compose with StaticTokenAuth as an outer layer (token validates first, then path resolves project).

**Evidence**:

The current server (server.rs) holds a single `Arc<Store>` and `Arc<VectorIndex>`. Multi-project requires a routing layer that resolves the correct store pair per request.

The rmcp `transport-streamable-http-server` feature uses Tower middleware. HTTP requests arrive via the rmcp transport layer, which dispatches to the `ServerHandler` implementation.

Evaluated routing options:

| Mechanism | StaticTokenAuth compose | JWT compose | Client ergonomics | MCP compatibility |
|-----------|------------------------|-------------|-------------------|-------------------|
| **Path-prefix** `/v1/{slug}/tools/...` | Token validates before routing (outer middleware) | JWT claim overrides or must match path slug | Clear — URL encodes project; client configures one MCP server URL per project | Each project = separate MCP server entry in client config (natural) |
| Header `X-Unimatrix-Project` | Token validates before routing | JWT claim replaces header | Requires custom header support in every client | MCP spec has no custom header mechanism; requires `claude mcp add -H` workaround |
| Per-project bearer token | Token IS the routing key (lookup in registry) | Not composable — two auth mechanisms compete | Simplest for single-project; N projects = N tokens to manage | Each project = separate MCP entry with different token |

**Path-prefix is the strongest option** because:

1. **MCP client compatibility**: Each project is configured as a separate MCP server entry with a distinct URL (`https://host:8443/v1/research-repo/`). Claude Code, Codex CLI, and Gemini CLI all support per-server URLs. No custom headers needed.

2. **StaticTokenAuth composition**: The bearer token validates the caller at the Tower middleware layer (outer). The path prefix routes to the correct project store (inner). These are cleanly separated concerns. A single token grants access to all projects on this instance — appropriate for the personal cloud tier where the operator owns all projects.

3. **Enterprise JWT composition**: The JWT `unimatrix_project` claim (W2-3 vision spec) can be validated against the path slug. If the claim is present, it MUST match the path slug (defense in depth). If absent, the path slug is authoritative. The `TenantRouter` from the vision spec resolves `slug -> Arc<Store>`.

4. **Single-project backward compatibility**: When `[[projects]]` is absent in config, the server exposes `/v1/tools/...` (no slug prefix) and routes all requests to the single project store. Zero breaking change.

**Router implementation sketch** (additive to existing server.rs):

```rust
struct ProjectRouter {
    stores: HashMap<String, Arc<ProjectContext>>,  // slug -> (Store, VectorIndex, ...)
    default_project: Option<String>,                // single-project fallback
}
```

**Recommendation**: Use path-prefix routing (`/v1/{project-slug}/tools/...`). Bearer token validates at the outer middleware layer. Path slug resolves project store at an inner routing layer. Single-project mode omits the slug prefix. Enterprise JWT `unimatrix_project` claim validates against path slug.

---

### Q4: Local-to-Cloud Migration Path

**Question**: A developer has `~/.unimatrix/{hash}/` locally. They spin up a cloud instance. What's the import path?

**Answer**: Use the existing CLI `export` then `import` pipeline with a project-slug target. The export/import system (nxs-012) already covers 11 tables with full data fidelity including GRAPH_EDGES, observations, and cycle_events. The gap is not in data coverage but in the migration UX.

**Evidence**:

The export system (export.rs) covers 11 tables: entries, entry_tags, co_access, feature_entries, outcome_index, agent_registry, audit_log, graph_edges, observations, cycle_events, plus counters. Format version 2 (current). Snapshot-consistent reads via `BEGIN DEFERRED` transaction. Hash chain validation on import.

The import system (import/mod.rs) opens the target database via `project::ensure_data_directory(project_dir, base_dir)`, validates the header, ingests all rows, validates hashes, and runs `reconstruct_embeddings()` to rebuild the HNSW index. This is a complete restore.

**GH #631 context**: The issue title references missing GRAPH_EDGES + observations in export/import. This was resolved by nxs-012 (PR #646, recently shipped — visible in git log `03cb7b01`). The export now includes `export_graph_edges`, `export_observations`, and `export_cycle_events`. Import handles all three. Data fidelity is complete.

**Migration path** (local to cloud):

1. **Export locally**: `unimatrix export --output my-project.jsonl`
2. **Transfer**: `scp my-project.jsonl cloud-host:/tmp/`
3. **Import to cloud**:
   ```bash
   docker run --rm -v unimatrix-data:/data -v /tmp/my-project.jsonl:/import.jsonl \
     ghcr.io/dug-21/unimatrix:latest --project-dir /data/.unimatrix/my-project \
     import --input /import.jsonl --force
   ```

**Embedding reconstruction**: The import pipeline (line 228) calls `reconstruct_embeddings()` after DB commit. This re-embeds all entries using the container's baked-in ONNX model. The local machine's embeddings are not transferred — they are recomputed. This is correct: embedding consistency within a deployment matters more than preserving stale embeddings from a different model version.

**Full DB file copy**: Also works for same-binary-version migrations. Copy `~/.unimatrix/{hash}/unimatrix.db` and `~/.unimatrix/{hash}/vector/` to `/data/.unimatrix/{slug}/`. Faster (skips re-embedding) but fragile across binary versions.

**API-driven sync**: Not recommended for V1. Export/import is batch; real-time sync introduces conflict resolution complexity unjustified for personal cloud.

**Missing convenience**: A `unimatrix migrate --from local --to https://host:8443/v1/my-project/ --token <token>` command would wrap export + transfer + import. This is a delivery item, not a research blocker.

**Recommendation**: Use CLI export/import (already complete with nxs-012). For multi-project cloud, `--project-dir` targets the slug directory. Add a `migrate` convenience subcommand for the export-transfer-import workflow. Full DB file copy supported as fast-path for same-version deployments.

---

### Q5: Cross-Project Knowledge Sharing (Scoping Decision)

**Question**: The vision doc describes an "owner store" for cross-project conventions. Is this in scope for OSS or enterprise-only?

**Answer**: No sharing in the OSS tier. Cross-project knowledge sharing is enterprise-only. OSS projects are fully isolated — each has its own `unimatrix.db`, own integrity chain, own graph.

**Evidence**:

The vision doc (PRODUCT-VISION.md, W2-3 section) describes a two-tier store model: owner store for cross-project conventions, project store for project-specific knowledge, fan-out at query time.

Implementing this in OSS creates problems:

1. **Integrity chain breakage**: Each project has its own hash chain. Cross-project reads surface entries from a different chain, breaking per-project integrity guarantees. The hash chain is described as the product's "defensible moat" (PRODUCT-VISION.md).

2. **Observation leakage**: PRODUCT-VISION.md Decision 3 explicitly states shared analytics across projects is a cross-project observation leakage risk.

3. **Complexity**: Read fan-out requires the search pipeline (HNSW -> NLI re-rank -> co-access boost -> category affinity) to query multiple stores, merge results, and apply cross-store relevance ranking — a significant architectural change.

| Option | Complexity | Isolation | OSS appropriateness |
|--------|-----------|-----------|-------------------|
| **No sharing** | Zero | Complete | Correct for personal cloud |
| Read-only fan-out | High | Compromised | Enterprise needs RBAC to control access |
| Explicit promotion | Medium | Preserved (copied entry has own chain) | Possible but premature |

**The personal cloud use case does not require sharing.** A developer with research + dev repos wants them isolated. If a pattern is truly cross-cutting, the developer can use `context_store` to create it in each project.

**Recommendation**: No cross-project sharing in OSS. Full project isolation. Enterprise adds the owner store with OAuth-scoped fan-out. OSS invariant: project slug = isolation boundary.

---

### Q6: Enterprise Compatibility Contract

**Question**: Define what must be true about the OSS data model so enterprise multi-tenant is an additive layer, not a rewrite.

**Answer**: Seven invariants the OSS data model establishes. Enterprise inherits all seven and adds OAuth identity, RBAC, and the owner store layer.

**Evidence**:

Analyzed the codebase, ASS-050 findings, and W2-3 vision spec.

**OSS Invariants (enterprise inherits unchanged):**

**Invariant 1: Project slug is the isolation boundary.**
Every `Store`, `VectorIndex`, `AuditLog`, and `SessionRegistry` instance is scoped to exactly one project slug. No SQL query, no HNSW search, no graph traversal crosses the slug boundary. Enterprise adds: JWT validates `unimatrix_project` claim against slug; RBAC policies are per-slug.

**Invariant 2: Per-project databases with independent schemas.**
Each project has its own `unimatrix.db`. Schema versions are per-project (migration runs independently per DB). Enterprise adds: the "owner store" is itself another project slug with special RBAC rules.

**Invariant 3: Hash chain integrity is per-project.**
`content_hash` and `previous_hash` are computed within a single project's entry set. Enterprise adds: cross-project promotion creates a new entry in the target project with its own hash chain position.

**Invariant 4: Audit log carries credential_type and agent_attribution.**
The ASS-050 schema migration (4 new columns) is designed for extensibility. OSS writes `credential_type = "bearer_static"` for HTTP, `"uds_local"` for UDS. Enterprise writes `"jwt_oauth"`. Same schema, same append-only triggers.

**Invariant 5: BearerValidator trait for pluggable auth.**
OSS ships `StaticTokenAuth`. Enterprise ships `JwtBearerAuth`. Same trait interface. No OSS code changes when enterprise auth is swapped in.

**Invariant 6: Capability checks at the service layer.**
Whether the caller arrives via UDS, bearer token, or JWT, `ServiceLayer` checks capabilities identically. Enterprise derives capabilities from JWT scopes instead of StaticTokenAuth's implicit full-access grant.

**Invariant 7: ProjectRouter resolves stores at request dispatch.**
The `ProjectRouter` maps `slug -> (Store, VectorIndex, ServiceLayer)`. Everything downstream operates on a single-project `ServiceLayer` and is unaware of multi-tenancy. Enterprise JWT `unimatrix_project` claim validated against registered slugs before routing.

**What enterprise adds (NOT in OSS):** OAuth 2.1 (`JwtBearerAuth`), three-role RBAC, owner store (distinguished slug with fan-out), control plane DB, admin console.

**What enterprise does NOT rewrite:** Store schema, hash chain, confidence/NLI/graph pipelines, export/import format, CLI tools.

**Recommendation**: Enforce these seven invariants in OSS code review gates. Enterprise compatibility is guaranteed as long as they hold. Critical OSS implementation items: (1) project slug as first-class identity, (2) `ProjectRouter` as routing seam, (3) `BearerValidator` trait (ASS-050 specified).

---

## Unanswered Questions

None. All six bounded questions answered at directional confidence.

---

## Out-of-Scope Discoveries

1. **Per-project config overlay model**: With N projects each having `config.toml`, a three-level hierarchy (compiled defaults -> server-wide config -> per-project config) may be needed for containers. Warrants a design decision during W2-3 delivery.

2. **Health check per project**: Current `HEALTHCHECK` checks daemon liveness. Multi-project should include per-project schema version checks. Delivery detail for the multi-project container spec.

3. **Project lifecycle CLI**: Creating, deleting, and listing projects via CLI or API. Config-driven registration handles creation; deletion and listing need CLI support. Delivery scope for W2-3.

4. **Backup/restore API endpoint**: No HTTP API for triggering export/import; currently CLI-only requiring container exec. An admin endpoint would improve cloud operations. W2-3 or post-W2 delivery item.
