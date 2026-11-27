# Unimatrix V2 — Product Vision

> **Status: DRAFT.** Nothing references this document yet. Two-level authority, OIDC user seam, Auditor role in Stage 2, and principles P12–P14 are human direction beyond ass-111 Rev 4. Derived from ass-111 S7 (draft goal set, principles, root vision — pending D5) and the human's proposed V2 vision notes. V1 vision remains authoritative in `PRODUCT-VISION.md` until adopted.

---

## Vision

Unimatrix is a workflow-aware, self-learning knowledge engine, delivered as a hosted multi-tenant service. It retains, curates and retrieves trustworthy knowledge for any client — agents in a delivery workflow, autonomous task agents and pipelines (e.g. GitHub Actions), and chat applications over MCP.

Agents search, store, and correct knowledge as a normal part of doing work: decisions get attributed, lessons get captured, patterns get refined. Every write is attributed to an authenticated principal, hash-chained, and correctable with full provenance.

When a client declares its workflow context — the cycle, phase and goal it is working in — Unimatrix uses that context to deliver the right knowledge before it is asked for. **Workflow-awareness is the differentiator, and it is additive:** knowledge retained and retrieved outside any workflow gets the full integrity and retrieval engine, and never pays for, or is distorted by, workflow features it does not use.

Each project's knowledge, audit trail and learning are isolated by construction. Sharing happens only through explicit, enumerated grants.

**Boundary.** Unimatrix is not an orchestration engine. It does not coordinate agents, schedule work, hold task state or manage workflows. It answers questions; it never initiates actions, and no workflow depends on state only Unimatrix holds.

**Key mental model — three layers.**
- *Harness definitions* (workflows, protocols, agents, skills) are released artifacts, distributed through the harness's own channels. Unimatrix references them by version and digest and never hosts them.
- *Living knowledge* (decisions, patterns, lessons, findings) evolves with every cycle. Unimatrix manages it.
- *Engine configuration* (categories, relationship vocabulary, domain packs) is each project's versioned contract with the engine (the PCD), managed through Unimatrix.

**Unimatrix is a platform.** Domain solutions — a delivery harness, a research harness, a medical-research aggregator — are built *on* it, not *into* it. They configure a project's PCD, plug their identity and policy into presented seams, and consume memory over MCP and the Node SDK. The extension surface is documented, versioned, and presented as a product.

---

## What changes from V1

| V1 | V2 |
|---|---|
| Self-hosted: one container, one bearer token | Hosted multi-tenant SaaS; local N=1 remains only as exit, migration and dev path (unsupported) |
| UDS / stdio / HTTPS | HTTPS only; Node stdio→HTTPS bridge stays a client adapter |
| Operator registers slugs + restart | Owner self-serves via `npx unimatrix init` |
| Bearer tokens, cert pinning, client bundles | OAuth 2.1 for humans, OIDC exchange for CI, scoped tokens for services |
| Self-asserted `agent_id` grants capability | Role ∩ credential ceiling ∩ tier gate ∩ agent-type policy; agent identity only narrows |
| One flat role model | Two authority levels: **Platform** (operator: Platform Owner, Platform Auditor) and **Tenant** (Owner, Writer, Reader, Auditor) |
| Fire-and-forget audit (written outside the transaction, errors swallowed) | **Trusted audit** — three classes: state change (same transaction, fail-closed), security event (synchronous before response), access (drained, loss-counted); per-store hash chain anchored in the control plane |
| One box, scaled vertically | Horizontal scale-out: project-placed, tenant-affine cells; add cells, never grow one |
| Agentic delivery clients only | Client classes K1–K5: harness agents, autonomous agents/CI, chat apps, public read |
| Manual TOML config | Versioned, validated, audited PCD per project with domain-pack templates; runtime taxonomy discovery |
| One project scope | Intra-tenant shared spaces via explicit grants |
| Learning always on | Learning as a per-store mode: adaptive, curated, or deterministic |

---

## Story

Unimatrix began in agentic software delivery, where the problem was specific: AI agents forget, contradict each other, and confidently repeat mistakes. We built a knowledge engine where nothing is merely stored — everything is attributed, hash-chained, scored by real usage, and correctable with full provenance.

That foundation became a platform: a typed knowledge graph, a confidence system that learns from usage, an observation pipeline fed by any event source, and any knowledge-intensive domain configured rather than rebuilt.

V2 takes it to a service. The same engine, hosted, serving many tenants with isolation by construction — so any developer, pipeline, or application can attach in minutes, and every project's knowledge improves with use without ever leaking into another's.

---

## Strategic Goals (draft — pending D5)

| Goal | Tag | Summary | From V1 |
|------|-----|---------|---------|
| Knowledge integrity | `integrity` | Every entry and state change attributed to an authenticated principal, tamper-evident, and current; retractions propagate; contradictions served by declared policy; inferred links distinguishable; tenants verify their own chain heads. **Trusted audit**: every authoritative change audited in-transaction (fail-closed), per-store audit hash chain anchored outside the store, losses never silent; tenants can verify chain heads without trusting the operator | Survives, extended |
| Self-learning intelligence | `self-learning` | Every adaptive project measurably improves from its own use; learning is a per-store mode (adaptive / curated / deterministic); never crosses a project or tenant boundary; every signal attributed | Survives, restated |
| Proactive knowledge delivery | `proactive-delivery` | Knowledge arrives before it is asked for, conditioned on declared workflow context in any domain; without context, core-engine results | Survives, restated as additive |
| Hosted service | `hosted-service` | One operator runs a multi-tenant service at stated cost, durability (RPO/RTO) and fairness; scales horizontally by adding cells, never by growing one box; sign-in to attached project in minutes, nothing to run, no secrets in the repo; every tenant can export and leave | New — replaces `personal-cloud` |
| Trusted identity | `trusted-identity` | Every call by an authenticated principal (user / workload / service), authorized server-side by role ∩ credential ceiling, at two authority levels — Platform and Tenant. Users sign in through OAuth 2.1 with a pluggable OIDC identity seam (GitHub first; enterprise IdPs later). Agent identity harness-attested and tiered (T1–T3), only ever narrows; claims worded per tier | Promoted; absorbs authN/authZ |
| Extensible platform | `platform` | Any domain on the same engine, configured via versioned PCD; solutions extend presented seams (L1 PCD, L2 identity/attestation, L3 MCP + Node SDK); validated on delivery, research, medical-research aggregation | Merges `platform` + `domain-agnostic` |

---

## Architectural Principles (draft — pending D5)

Non-negotiable across all V2 work:

1. **Hash chain integrity is immutable.** `content_hash` and `previous_hash` on every entry — never skipped, backdated or made optional. Each store owns its chains; no link crosses stores. Cross-store lineage (publish, fork, migration) is hash-pinned provenance metadata.

2. **The audit log is append-only, complete and tamper-evident.** Every committed change to authoritative state — in a knowledge store or the control plane, by a client, a background job or an operator — writes its audit entry in the same transaction, or does not commit. Security events that change no state (authentication failures, denials, rejected attestations) are recorded before the response. Access to knowledge is recorded as access; any loss is itself recorded. Derived and learned state (indexes, confidence, co-access, telemetry) is not audited state. Each store's audit log is a hash chain whose head is anchored outside the store. Each entry is attributed to the authenticated principal, credential type and attestation tier, kept separate from the self-asserted agent annotation. DDL triggers forbid update, delete and non-monotonic insert. Fire-and-forget is acceptable for no audit class. The audit log is never a learning source. *(ass-111 S9.)*

3. **Capability checks at the service layer, after identity resolution.** Every call is an authenticated principal over HTTPS. Tenant and project come from the URL; authorization from the control plane — never from the payload, which may narrow but never widen. Effective capability = role ∩ credential ceiling ∩ tier gate ∩ agent-type policy. Two authority levels never mix: **Platform** roles (Owner, Auditor) govern the service; **Tenant** roles (Owner, Writer, Reader, Auditor) govern a tenant's projects. A platform role grants no access to tenant knowledge. Only human Owners hold Admin at either level. Auditor roles are read-only over audit, never over knowledge content unless also granted. Identity providers plug in behind an OIDC seam. A cell trusts only an authenticated router assertion.

4. **Typed relationship graph.** Relationships are explicit, typed and traversable; agents declare edges at write time. Machine-inferred edges carry their origin and can be excluded from traversal and serving. No edge crosses stores.

5. **Graceful degradation.** Every ML capability has a defined fallback. Cold start, slow shared store, or missing ML worker degrades service (plain ranking, home-only results) rather than failing it. In deterministic mode, optional ML is off, not degraded. Integrity guarantees are never degraded: a write whose state-change audit row fails does not commit (Principle 2).

6. **One binary, minimal required infrastructure.** The same binary fills every role (router, cell, control plane, inference worker, local single-project). Roles communicate only through their published interfaces, so running several in one process is a deployment choice configuration can undo, never an architectural coupling. The data plane needs only the binary, local disk, and an object store for backup — no external database, queue or cache. Identity is delegated to an external authorization server so keys and secrets stay out of Unimatrix. Clients are adapters: the Node edge client, never the binary.

7. **In-memory hot path, per resident project.** All ranking-relevant derived state is served from memory, rebuildable from the project's durable store without ML inference. Projects become resident on first request within a stated cold-start bound, stay current via change-driven rebuilds, and are evicted under a per-cell memory budget without losing durable state. A project is resident on at most one cell at a time — the cell holding its current assignment epoch. Derived state is never shared or replicated between cells. Query-path inference runs in the serving cell; background inference may run in a separate worker tier. Background work is change-scheduled and fair-shared; one tenant never degrades another beyond a stated, measured bound.

8. **No secrets in any database.** OAuth client secrets, signing keys, API keys, TLS private keys and crypto-shred keys never enter any Unimatrix database, the control plane included. Only token hashes and public keys are stored.

9. **Isolation by construction.** The project (or shared space) is the physical store — own database, vector index, chains, audit log and learned state. A tenant is a control-plane grouping. No SQL spans two stores. Cross-store references are qualified `(store_id, entry_id)`. Learning lands only in the addressed store, or flows one way into the reader's store under an explicit grant. Every isolation gate is tested in both directions.

10. **Workflow-awareness is additive.** Without declared workflow context, every surface returns core-engine behaviour. Parity is tested.

11. **Unimatrix answers and never initiates.** It never initiates an action and never holds state whose loss would break a workflow. No locks, claims, queues or notifications.

12. **Inference is a generic, workload-scaled seam.** Background inference sits behind one task-typed contract (e.g. embed, score, generate/extract), independent of model, runtime or format. The default implementation runs in-process, in the same container. Moving it to a separate worker tier is a deployment choice, not a redesign, so heavy CPU/GPU work scales with its own workload, never with — or at the cost of — user-facing traffic. Query-path embedding stays in the serving cell. V2 builds the seam, not any specific model capability.

13. **Scale out, never up.** The project is the placement unit; tenant affinity is policy, not schema. Capacity grows by adding standard cells. Every component is N-wide from day one — proven by a two-cell CI topology even while production runs one cell.

14. **Current dependencies.** *(Proposed — not from research.)* V2 runs on maintained, current dependency versions; pre-release pins and known-vulnerable versions are tracked debt with a dated exit, not steady state.

---

## Domain Validation

| Domain | Status | Evidence |
|--------|--------|----------|
| Agentic software delivery | Validated (V1) | Waves 0–1B, ASS-040 |
| Autonomous research | Validated (V1) | ASS-057 |
| Medical-research aggregation (Med Search) | Target — β tenant | K4 ingest, K5 public chat, deterministic mode, retraction (ass-111 S4) |
| Autonomous task agents / CI | Target — α | Service tokens → GitHub OIDC exchange |
| Chat applications over MCP | Target — α (optional) | claude.ai custom connector |

---

## Out of Scope for V2

- Transient workflow session memory (H6 rejected)
- Hosting harness definitions (referenced by version + digest only)
- Cross-tenant sharing (post-GA)
- Supported self-hosting / air-gap product
- Webhooks or any Unimatrix-initiated action
- Kubernetes / serverless cells (revisit beyond ~10 cells)
- Per-project read replicas and intra-project sharding

**Named, deferred to GA / post-GA:** Auditor audit-viewing interface (the role itself ships in Stage 2); sealed audit archival; tenant admin console; SOC 2 readiness; enterprise IdP federation (SAML/SCIM) beyond the OIDC seam.

---

## Roadmap

See `product/V2-DeliveryPlan.md`. Research: `product/research/ass-110/`, `product/research/ass-111/` (incl. `ass-111-s2-architect-review-2.md` — horizontal scale-out). Trusted audit: `ass-111-s9-findings.md`.
