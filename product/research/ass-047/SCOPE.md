# ASS-047: Core Scalability Strategy

**Date**: 2026-04-09
**Tier**: 1 (prerequisite for ASS-042 — control plane DB technology decision)
**Feeds**: W2-3 (control plane tech), W2-2 (connection limits), ASS-042 (integrating architecture)

---

## Question

What is the concurrency ceiling for Unimatrix under multi-agent, multi-repo enterprise load, and what is the minimum scalability investment required in Wave 2 to reach that ceiling while keeping the SaaS door open?

---

## Why It Matters

The enterprise deployment model changes the load profile fundamentally: multiple concurrent agents, multiple repos, a central server, OAuth validation on every request. The current architecture was designed for a single developer session. Before the control plane can be designed (ASS-042), we need to know where the bottlenecks are, what the concurrency ceiling is at current architecture, and what (if anything) must change before Wave 2 delivery to avoid hitting that ceiling at a realistic team size.

This spike also determines the control plane DB technology decision: SQLite (keeping the zero-infrastructure model) or PostgreSQL (for teams that will exceed SQLite's concurrent write limits).

---

## What to Explore

### 1. Current Write Path Analysis
Model the write path under concurrent multi-agent load:

- **`write_pool` (2 connections) + async write queue (batched ≤50 events / 500ms)**: Under N concurrent agents writing to the same repo, what is the queue depth and latency at N=5, 10, 20?
- Per-agent write rate: estimate the typical events per second per agent session (context_store, observations, co-access updates, audit log entries). Use current production usage patterns or realistic estimates.
- SQLite per-repo isolation: multiple agents writing to *different* repos are fully independent (no shared write lock). Multiple agents writing to the *same* repo share the write pool. Characterize: is same-repo concurrent write the expected case for an enterprise team, or are most concurrent writes cross-repo?
- Is the write queue's 500ms batching window acceptable for enterprise use cases (audit log writes, knowledge stores)? Or does enterprise latency expectation change this?

### 2. Control Plane DB Concurrency
The control plane DB is the single shared component across all agents and all repos. Unlike per-repo data plane DBs, it cannot benefit from per-repo isolation:

- Every MCP request that requires identity resolution hits the control plane.
- OAuth token validation (if server-side lookup): read per request.
- Role binding lookup: read per request for operators.
- Audit log writes: write per operation.
- Estimate: at a team of 10 concurrent agents, how many control plane reads/writes per second?
- SQLite WAL mode handles concurrent reads well. Concurrent writes are serialized. At what write rate does the control plane DB become a bottleneck?
- What read pool size is appropriate for the control plane? (Current data plane: 6–8 reads. Control plane may need a different sizing.)

### 3. PostgreSQL Migration Readiness
- The sqlx abstraction from W0-1 was designed for PostgreSQL compatibility. Validate: are there any SQLite-specific SQL constructs in the current codebase that would break under PostgreSQL? (SQLite-specific functions, `AUTOINCREMENT` vs. `SERIAL`, `UPSERT` syntax differences, JSON functions.)
- For the **control plane specifically**: what is the minimum Wave 2 change needed to ensure PostgreSQL is a config-swap, not a code rewrite?
- Threshold recommendation: at what concurrent agent count or write rate per second should the recommendation be "use PostgreSQL for the control plane"? This gives operators a clear upgrade trigger.
- For the **data plane** (per-repo SQLite DBs): these are unlikely to need PostgreSQL at enterprise team sizes. Confirm that per-repo SQLite remains appropriate at 20–50 concurrent sessions per repo.

### 4. Read Path Analysis
- Read pool (6–8 connections per repo DB). Under concurrent search + briefing requests across multiple agents on the same repo: at what agent count does the read pool become a bottleneck?
- The in-memory graph cache (`Arc<RwLock<_>>`): read lock contention under concurrent search. At what query rate does this become measurable latency?
- HNSW index under concurrent reads: is the current HNSW implementation thread-safe for concurrent readers? Document the current guarantee.

### 5. In-Memory Structures Under Multi-Repo Load
In a multi-repo enterprise deployment, the server hosts N repos simultaneously. Each repo has its own:
- In-memory graph (Arc<RwLock<TypedRelationGraph>>)
- PhaseFreqTable
- CoAccess affinities (if cached)
- GNN weight vector (when W3-1 ships)

At 10 repos, 20 repos, 50 repos: what is the memory envelope for these in-memory structures? Is lazy loading (only active repos loaded) needed, or is eager loading at startup acceptable?

### 6. Target Session Count for Wave 2
Define a concrete target: what is the number of concurrent agent sessions the enterprise server should support in Wave 2?
- Evaluate: 5 concurrent (small team), 20 (department), 50 (organization).
- Set a target that is achievable with the current architecture (possibly with minor changes) and defensible to an enterprise buyer.
- Document: above what session count does the architecture need a significant change (e.g., PostgreSQL for control plane, read replicas, connection pooling middleware)?

### 7. SaaS Optionality Assessment
Beyond `org_id` in the control plane schema (already decided), are there any other low-cost Wave 2 additions that would meaningfully simplify a future SaaS pivot?

Evaluate:
- **Per-org write queue**: should the write queue be keyed by org (future) vs. by repo (current)? Changing this later is a significant refactor.
- **Rate limiting per credential**: should there be per-credential request rate limiting in Wave 2? This is a SaaS requirement that is much cheaper to add early.
- **Metrics/observability hooks**: Prometheus endpoint, structured logging with org_id/project_id fields. Required for SaaS operations. Cheap to add in Wave 2.
- **Connection pool per org vs. shared**: at SaaS scale, per-org pool isolation prevents noisy-neighbor. In Wave 2 (one org), this is premature. But does the architecture foreclose it?

---

## Output

1. **Concurrency ceiling assessment** — per-component (write queue, control plane, read pool, graph cache) with the N-agent inflection point for each
2. **Target session count recommendation** — with rationale and documented ceiling above target
3. **Control plane DB recommendation** — SQLite-for-Wave-2 viable up to what team size? At what point does PostgreSQL become necessary? Minimum code changes for clean PostgreSQL migration.
4. **Any architectural changes required before Wave 2 delivery** — specifically: anything that would require a painful refactor post-Wave-2 if not done now
5. **SaaS optionality additions** — short list of low-cost Wave 2 additions worth including for SaaS viability, with effort estimate for each

---

## Constraints

- "Single binary" and "zero infrastructure" (container is optional) non-negotiables must hold
- SQLite per-repo for the data plane is fixed — this spike is about control plane concurrency and the per-repo write ceiling
- W0-1 (sqlx dual-pool + async write queue) is the foundation — build on it, do not re-evaluate it
- The scalability strategy must be honest about limits: better to say "supports 20 concurrent agents" and be right than claim 100 and be wrong under a real enterprise team's load
