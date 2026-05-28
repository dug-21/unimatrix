# FINDINGS: Multi-Project Data Architecture for Containerized Deployment

**Spike**: ASS-060
**Date**: 2026-05-28
**Approach**: investigation
**Confidence**: directional
**Source**: FINDINGS-RAW.md (single input; no inter-track tensions to resolve)

---

## Findings

### Q1: Project Identity in a Container

**Question**: ADR-004 uses `SHA-256(project_root_path)` for project isolation — no equivalent concept in a container where there is no local project root. What replaces it?

**Answer**: Explicit project slugs declared in a `[[projects]]` config table, with the slug as the filesystem directory name. The path-hash mechanism (ADR-004) is retained unchanged for local UDS mode. The slug mechanism is additive for HTTP-routed multi-project containers.

**Evidence**: The current `project.rs` computes `SHA-256(path_as_utf8)[..16]` from a canonical filesystem path. In a container, `--project-dir /data` produces a single deterministic hash with no concept of "which remote project." The `ensure_data_directory()` function uses `base_dir / project_hash` as the data directory, which maps cleanly to `base_dir / slug`.

Evaluation against criteria:

| Option | Determinism | Collision safety | Ergonomics | Enterprise upgrade |
|--------|------------|-----------------|------------|-------------------|
| **Explicit slug in config** | Deterministic (operator-declared) | Validated uniqueness at registration | High — human-readable | JWT `unimatrix_project` claim maps directly to slug |
| Client-declared header | Per-request (no server-side registry) | None — typo creates new project | Low — every client must declare | JWT claim replaces header; migration requires client config changes |
| Path-prefix routing only | Per-request (embedded in URL) | None — same typo problem | Medium — visible but clutters API | JWT claim replaces path; API path changes are breaking |

Slug validation: `^[a-z0-9][a-z0-9_-]{0,63}$` (lowercase, DNS-label-safe, max 64 chars). Projects are declared in config, not created dynamically by clients.

```toml
[[projects]]
slug = "research-repo"
description = "Research knowledge base"

[[projects]]
slug = "main-dev"
description = "Primary development repo"
```

When `[[projects]]` is absent, the server operates in single-project mode using path-hash — identical to today.

**Recommendation**: Use explicit project slugs in `[[projects]]` config. Retain path-hash for local/single-project mode. Slug replaces hash as filesystem directory name in multi-project containers. Validation: `^[a-z0-9][a-z0-9_-]{0,63}$`.

---

### Q2: Volume Layout for N Projects

**Question**: Current W2-1 spec has 1 named volume. With N projects, what layout?

**Answer**: Subdirectories within the single `unimatrix-data` volume, one subdirectory per project slug. Not separate volumes per project. Not shared databases across projects.

**Evidence**: The shipped layout (nan-014) already uses `/data/.unimatrix/{hash}/` subdirectories. Multi-project extends this:

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

| Layout | Isolation | Backup granularity | Ops complexity | Compose complexity |
|--------|-----------|-------------------|----------------|-------------------|
| **Subdirs in one volume** | Per-project DB files (SQLite file-level) | Per-project via subdirectory copy | Low | Zero change |
| N separate volumes | Maximum — volume-level | Per-volume snapshot | High — N volume declarations | Linear growth |
| One DB with schema isolation | None — shared WAL | Impossible per-project | Lowest | Zero change |

The shared-DB option violates PRODUCT-VISION.md Decision 3: per-project databases for both knowledge and analytics to prevent cross-project observation leakage. Per-project backup works via `VACUUM INTO` (already in snapshot.rs) or subdirectory copy. Config co-location pattern (Unimatrix #4626) is preserved.

**Recommendation**: Subdirectories within the single `unimatrix-data` volume, keyed by project slug. Each project gets its own `unimatrix.db`, `vector/`, and `config.toml`. Models shared at `/data/.cache/unimatrix/`. No docker-compose.yml changes required.

---

### Q3: Request Routing

**Question**: How does the HTTP server resolve which project store handles a request?

**Answer**: Path-prefix routing: `/v1/{project-slug}/tools/...`. The slug in the URL path selects the project store. StaticTokenAuth validates at the outer middleware layer before routing.

**Evidence**: The current server.rs holds a single `Arc<Store>` and `Arc<VectorIndex>`. Multi-project requires a routing layer.

| Mechanism | StaticTokenAuth compose | JWT compose | Client ergonomics | MCP compatibility |
|-----------|------------------------|-------------|-------------------|-------------------|
| **Path-prefix** | Token validates before routing (outer) | JWT claim must match path slug | URL encodes project; one MCP server entry per project | Natural — each project = separate MCP entry |
| Header `X-Unimatrix-Project` | Token validates before routing | JWT claim replaces header | Requires custom header in every client | MCP spec has no custom header mechanism |
| Per-project bearer token | Token IS routing key | Not composable — two auth mechanisms compete | N tokens for N projects | Separate MCP entry per token |

Path-prefix wins because: (1) MCP clients support per-server URLs natively, (2) bearer token and path routing are cleanly separated concerns, (3) JWT `unimatrix_project` claim validates against path slug for defense in depth, (4) single-project backward compatibility — when `[[projects]]` absent, `/v1/tools/...` works with no slug prefix.

Router implementation seam:

```rust
struct ProjectRouter {
    stores: HashMap<String, Arc<ProjectContext>>,  // slug -> (Store, VectorIndex, ...)
    default_project: Option<String>,                // single-project fallback
}
```

**Recommendation**: Use path-prefix routing (`/v1/{project-slug}/tools/...`). Bearer token validates at outer middleware. Path slug resolves project store at inner routing layer. Single-project mode omits slug prefix. Enterprise JWT claim validates against path slug.

---

### Q4: Local-to-Cloud Migration Path

**Question**: A developer has `~/.unimatrix/{hash}/` locally. They spin up a cloud instance. What's the import path?

**Answer**: CLI export then import with project-slug target. The export/import system (nxs-012) already covers 11 tables with full data fidelity including GRAPH_EDGES, observations, and cycle_events. GH #631 is resolved.

**Evidence**: Export covers 11 tables (entries, entry_tags, co_access, feature_entries, outcome_index, agent_registry, audit_log, graph_edges, observations, cycle_events, plus counters). Format version 2. Snapshot-consistent reads via `BEGIN DEFERRED`. Hash chain validation on import. Embedding reconstruction via `reconstruct_embeddings()` after DB commit — embeddings are recomputed in the target environment rather than transferred.

Migration steps:
1. Export locally: `unimatrix export --output my-project.jsonl`
2. Transfer: `scp my-project.jsonl cloud-host:/tmp/`
3. Import to cloud container targeting the slug directory

Full DB file copy also works for same-binary-version migrations (faster, skips re-embedding, fragile across versions). API-driven sync not recommended for V1 due to conflict resolution complexity.

Missing convenience: a `unimatrix migrate` subcommand wrapping export-transfer-import. Delivery item, not a research blocker.

**Recommendation**: Use CLI export/import (complete with nxs-012). For multi-project cloud, `--project-dir` targets the slug directory. Add a `migrate` convenience subcommand. Full DB file copy supported as fast-path for same-version deployments.

---

### Q5: Cross-Project Knowledge Sharing (Scoping Decision)

**Question**: The vision doc describes an "owner store" for cross-project conventions. Is this in scope for OSS or enterprise-only?

**Answer**: No sharing in OSS. Cross-project knowledge sharing is enterprise-only. OSS projects are fully isolated.

**Evidence**: Implementing cross-project sharing in OSS creates three problems:

1. **Integrity chain breakage**: Cross-project reads surface entries from a different hash chain, breaking per-project integrity guarantees.
2. **Observation leakage**: PRODUCT-VISION.md Decision 3 explicitly calls shared analytics a cross-project leakage risk.
3. **Complexity**: Read fan-out requires the search pipeline (HNSW -> NLI re-rank -> co-access boost -> category affinity) to query multiple stores and merge results.

| Option | Complexity | Isolation | OSS appropriateness |
|--------|-----------|-----------|-------------------|
| **No sharing** | Zero | Complete | Correct for personal cloud |
| Read-only fan-out | High | Compromised | Enterprise needs RBAC to control access |
| Explicit promotion | Medium | Preserved (copied entry has own chain) | Possible but premature |

The personal cloud use case does not require sharing. Cross-cutting patterns can be manually created in each project via `context_store`.

**Recommendation**: No cross-project sharing in OSS. Full project isolation. Enterprise adds owner store with OAuth-scoped fan-out. OSS invariant: project slug = isolation boundary.

---

### Q6: Enterprise Compatibility Contract

**Question**: Define what must be true about the OSS data model so enterprise multi-tenant is an additive layer, not a rewrite.

**Answer**: Seven invariants the OSS data model establishes. Enterprise inherits all seven and adds OAuth identity, RBAC, and the owner store layer.

**OSS Invariants:**

1. **Project slug is the isolation boundary.** Every Store, VectorIndex, AuditLog, and SessionRegistry is scoped to exactly one slug. No query crosses the slug boundary. Enterprise adds: JWT validates `unimatrix_project` claim against slug; RBAC policies per-slug.

2. **Per-project databases with independent schemas.** Each project has its own `unimatrix.db`. Schema versions migrate independently per DB. Enterprise adds: the "owner store" is itself another project slug with special RBAC rules.

3. **Hash chain integrity is per-project.** `content_hash` and `previous_hash` are computed within a single project's entry set. Enterprise adds: cross-project promotion creates a new entry in the target with its own chain position.

4. **Audit log carries credential_type and agent_attribution.** The ASS-050 schema migration (4 new columns) is extensible. OSS writes `bearer_static` or `uds_local`. Enterprise writes `jwt_oauth`. Same schema, same triggers.

5. **BearerValidator trait for pluggable auth.** OSS ships StaticTokenAuth. Enterprise ships JwtBearerAuth. Same trait interface.

6. **Capability checks at the service layer.** ServiceLayer checks capabilities identically regardless of transport. Enterprise derives capabilities from JWT scopes instead of StaticTokenAuth's implicit full-access.

7. **ProjectRouter resolves stores at request dispatch.** Maps `slug -> (Store, VectorIndex, ServiceLayer)`. Everything downstream operates on a single-project ServiceLayer and is unaware of multi-tenancy.

**What enterprise adds (not in OSS):** OAuth 2.1 (JwtBearerAuth), three-role RBAC, owner store (distinguished slug with fan-out), control plane DB, admin console.

**What enterprise does NOT rewrite:** Store schema, hash chain, confidence/NLI/graph pipelines, export/import format, CLI tools.

**Recommendation**: Enforce these seven invariants in OSS code review gates. Enterprise compatibility is guaranteed as long as they hold. Critical implementation items: (1) project slug as first-class identity, (2) ProjectRouter as routing seam, (3) BearerValidator trait per ASS-050.

---

## Unanswered Questions

None. All six bounded questions answered at directional confidence.

---

## Out-of-Scope Discoveries

1. **Per-project config overlay model**: With N projects each having `config.toml`, a three-level hierarchy (compiled defaults -> server-wide config -> per-project config) may be needed. Design decision for W2-3 delivery.

2. **Health check per project**: Current HEALTHCHECK checks daemon liveness. Multi-project should include per-project schema version checks. Delivery detail for multi-project container spec.

3. **Project lifecycle CLI**: Creating, deleting, and listing projects via CLI or API. Config-driven registration handles creation; deletion and listing need CLI support. Delivery scope for W2-3.

4. **Backup/restore API endpoint**: No HTTP API for triggering export/import; currently CLI-only requiring container exec. An admin endpoint would improve cloud operations.

---

## Recommendations Summary

| Question | Recommendation |
|----------|---------------|
| **Q1 — Project Identity** | Explicit project slugs in `[[projects]]` config; retain path-hash for local mode |
| **Q2 — Volume Layout** | Subdirectories within single volume keyed by slug; per-project DB + vector + config |
| **Q3 — Request Routing** | Path-prefix `/v1/{project-slug}/tools/...`; bearer token outer, slug resolution inner |
| **Q4 — Migration Path** | CLI export/import (nxs-012, complete); add `migrate` convenience subcommand |
| **Q5 — Cross-Project Sharing** | No sharing in OSS; enterprise adds owner store with OAuth fan-out |
| **Q6 — Enterprise Contract** | Seven OSS invariants; enterprise is additive, not a rewrite |
