# Wave 2 — Enterprise Deployment Roadmap

**Date**: 2026-04-09 (updated 2026-04-10)
**Prior roadmap**: ASS-040 (self-learning knowledge engine) — COMPLETE
**Schema version**: v22
**Eval baseline**: MRR=0.2558, 2,096 scenarios, snapshot a03bdd8f1fcb (2026-04-08)

**Strategic intent**: Wave 2 is the SOC 2 Type I readiness phase and the ISO/IEC 42001 enabler. It establishes the architectural and audit foundations — three-role RBAC, structured audit log with full attribution, OAuth 2.1, HTTPS — that make both certifications achievable in subsequent phases. The compliance controls are a commercial asset, not just a technical requirement.

---

## Prior Waves — Completion Status

| Wave | Scope | Status |
|------|-------|--------|
| Wave 0 | Daemon mode, sqlx dual-pool, config externalization | **COMPLETE** |
| Wave 1 | Typed relationship graph, rayon ML pool, eval harness, NLI cross-encoder, observation pipeline generalization | **COMPLETE** |
| Wave 1A | Formula fusion (WA-0), phase signals (WA-1), session context (WA-2), proactive delivery (WA-4), PreCompact restoration (WA-5) | **COMPLETE** |
| ASS-040 Groups 1–6 | Formula cleanup, tick decomp, graph enrichment (S1/S2/S8/cosine Supports), PPR expander (+0.0122 MRR), behavioral signal infra, goal-conditioned briefing | **COMPLETE** |
| ASS-040 Groups 7–8 | Data hygiene, co_access→PPR migration, intelligence-driven retention | **COMPLETE** (carry-forwards: #477, #471 open, non-blocking) |
| ASS-040 Groups 9–10 | Explicit read logging, phase-conditioned category affinity | **COMPLETE** |

Intelligence-pipeline carry-forwards that do not block Wave 2: #477 (quarantine guard at co_access write), #471 (orphaned-edge compaction), #510 (context_purge).

---

## Working Hypotheses

These are strong, reasoned positions — not immovable objects. Every item below is in pencil until research validates or contradicts it. Researchers are expected to challenge these, not work around them. The goal is the best possible product, not consistency with prior assumptions.

Hard constraints (genuinely fixed — changing these requires rewriting shipped code):
- Rust codebase
- SQLite per-repo isolation for data plane (schema v22, live in production)
- sqlx abstraction layer (already in place)
- Existing MCP tool API surface

Everything else below is a hypothesis.

### 1. OSS / Enterprise Product Bifurcation
- **OSS tier (MIT/Apache)**: STDIO transport, single-project, local daemon. Everything shipped through Wave 1A. No network exposure.
- **Enterprise tier (BSL-1.1)**: HTTPS-only, OAuth, multi-project, multi-agent, container deployment. Wave 2.
- **Rationale**: STDIO is the community on-ramp. Enterprise features require the security model that mandates the licensing boundary. HTTPS implies OAuth. There is no secure halfway point.  BSL still allows individual/personal projects to utilize for free, but not commercial use.

### 2. Admin XOR Operator Identity Model
- Credentials are mutually exclusive: Admin or Operator, never both.
- Enforced at credential issuance, not at runtime capability check.
- Identity unit: one credential set = one role. Multiple terminals in one IDE share credentials. A separate machine gets its own credentials and may hold a different role.
- **Rationale**: Separation of duties at the architectural level. A Claude instance doing development work cannot self-elevate to admin. The architecture enforces this — not convention.

### 3. Control Plane / Data Plane Separation
- **Control plane DB** (one per deployment): agent registry, role bindings, project registrations, org record(s), audit log.
- **Data plane DBs** (one per repo, unchanged): knowledge + analytics. Same per-repo SQLite isolation as today.
- TenantRouter mediates all data plane access. Control plane validates access before routing.

### 4. SaaS Optionality via org_id
- `org_id` field in the control plane schema from day one.
- Wave 2 writes exactly one org value. SaaS adds rows.
- No multi-org logic built in Wave 2 — the hook is the schema field, not an implementation.
- **Rationale**: Retrofitting org-level isolation after the fact is expensive. Adding a column costs nothing now.

### 5. HTTPS-Only Enterprise Transport
- No plain HTTP. No `--insecure` flag. Refuse HTTP connections entirely — no redirect.
- Unimatrix terminates TLS directly OR binds `127.0.0.1` behind a reverse proxy. Both supported; startup config determines which.
- Admin port (8444) separate from content port (8443). Admin port never load-balancer-exposed.

### 6. Per-Repo DB Isolation — Unchanged
- Enterprise extends today's model: each repo has its own `knowledge.db` + `analytics.db`.
- Repo identity: git hash of repository name (unchanged from OSS).
- Cross-repo leakage is prevented architecturally, not by policy.

### 7. Admin Console as Primary Management Surface
- All admin operations are available via the admin API (OAuth-scoped `unimatrix:admin`).
- The admin console is the human-facing surface on top of that API.
- Programmatic admin requires explicit scope configuration — not default.
- Wave 2 admin console scope: agent enrollment/management, project/repo registration, role binding management, audit log viewer, system health. Full knowledge explorer is Matrix phase (deferred).

---

## Wave 2 Projected Path

*Goal statements are provisional — subject to refinement by research spikes ASS-041 through ASS-047.
Items marked (🔬) have direct research dependencies.*

### W2-0: Product Bifurcation + Licensing (🔬 ASS-045)
**Goal**: Clean OSS/Enterprise boundary in the codebase. BSL-1.1 applied to enterprise features with finalized Change Date, conversion license, and Additional Use Grant. CLA decision made. Build pipeline produces separate OSS and enterprise artifacts.

**Resolved by ASS-045**: Codebase split strategy. BSL specifics. CLA requirement. Distribution artifact plan.

---

### W2-1: Container Packaging (🔬 ASS-043)
**Goal**: BSL-licensed enterprise container. Single-image deployment containing the Unimatrix daemon with enterprise features, ONNX runtime, and control plane DB initialization.

Named volumes:
- `unimatrix-control` — control plane DB (integrity-critical)
- `unimatrix-knowledge` — per-repo knowledge DBs (integrity-critical, back up frequently)
- `unimatrix-analytics` — per-repo analytics DBs (self-healing)
- `unimatrix-shared` — ONNX models + `config.toml` as read-only bind

Non-root container user. HEALTHCHECK on daemon liveness + schema version. Air-gap deployable (no runtime internet dependencies).

**Resolved by ASS-043**: ONNX Runtime packaging approach. Base image selection. Multi-arch strategy. Secrets injection pattern.

---

### W2-2: HTTPS Transport (🔬 ASS-041)
**Goal**: HTTPS-only enterprise transport alongside existing UDS/stdio. Two listeners: content port (8443) and admin port (8444). Bearer token validation at transport layer; capability checks enforced at service layer (unchanged). TLS non-negotiable — no `--insecure` flag; no HTTP mode.

Max request body ≤1MB. Connection timeout 30s. Max concurrent connections enforced.

**Resolved by ASS-041**: rmcp 0.16 HTTP transport readiness. Library selections for TLS, request handling, bearer token validation.

---

### W2-3: Enterprise Identity Model (🔬 ASS-042, ASS-047)
**Goal**: OAuth 2.0 client credentials flow. Admin XOR Operator mutual exclusivity enforced at credential issuance. Control plane DB (agents, role bindings, project registrations, `org_id`). JWT `sub` → `agent_id` attribution. `unimatrix_project` claim → data plane routing validated against registered project allowlist. Per-repo operator scope binding. Bootstrap flow for first admin credential on fresh deployment.

JWT algorithm allowlist: RS256/ES256 only. `exp`/`iss`/`aud` enforced. `sub` claim validated `^[a-zA-Z0-9_-]{1,64}$`. OAuth client secrets never stored.

**Resolved by ASS-042**: Identity enforcement mechanism. Role binding data model location (JWT vs. server-side). Control plane schema with SaaS `org_id`. Bootstrap flow design. Multi-agent content integrity augmentations.

**Resolved by ASS-047**: Control plane DB technology (SQLite-for-Wave-2 with PostgreSQL-ready abstraction). Concurrent write ceiling at target agent count.

---

### W2-4: Admin Console (🔬 ASS-044)
**Goal**: Minimum viable enterprise admin UI serving as the primary human management surface. Authenticates via the same OAuth flow as other clients.

Wave 2 scope: agent enrollment + promotion/revocation, project/repo registration, role binding management (operator→project assignment), audit log viewer (paginated), system health (schema version, daemon uptime, entry counts per project).

Deferred to Matrix phase: knowledge explorer, entry browser, graph visualization, feature drilldown, prompt debugger.

**Resolved by ASS-044**: Technology choice (single binary compatibility is a hard constraint). Framework selection. API surface (MCP tools vs. separate admin REST). Build pipeline integration.

---

### W2-5: GGUF Module — Conditional (🔬 ASS-046)
**Goal**: Optional `unimatrix-infer` capability behind Cargo feature flag. Local GGUF inference on a dedicated rayon pool (separate from ONNX pool). When present: upgrades `context_cycle_review` recommendations, `context_status` explanations, contradiction explanation, background synthesis.

SHA-256 hash-pinned model file required in config. LLM input length-limited (~4,000 tokens). LLM output passes content scanner before storage or return.

**Gate**: ASS-046 must return a go recommendation with proof-of-concept validation before this item is scoped for delivery. If ASS-046 returns unfavorable, W2-5 defers to a post-Wave-2 wave.

---

## Research Prerequisites

Eight research spikes are required before Wave 2 can be formally scoped for delivery. ASS-048 is Tier 0 — it produces the enterprise security requirements that drive ASS-041 and ASS-042. ASS-042 is the integrating architecture document.

### ASS-048 Findings Summary (2026-04-10) — `product/research/ass-048/FINDINGS.md`

**Q1 — Auth model**: OAuth 2.1 client credentials is the correct M2M choice and aligns with the MCP spec's June 2025 mandate. The hypothesis is confirmed but incomplete — must also enforce token expiry, audience claims, and scope validation. Proxy-terminated TLS is standard enterprise practice and must be a documented deployment option. Do not implement mTLS in Wave 2.

**Q2 — RBAC**: The Admin/Operator two-role model is **contradicted** — it fails SOC 2 CC6.3 duty segregation requirements at the first enterprise security review. Three roles are required: Admin, Operator, and **Auditor** (read-only). Auditor is the blocking gap for enterprise deployment approval.

**Q3 — Compliance**: SOC 2 Type II is confirmed as the correct primary target. Wave 2 should be designed for SOC 2 Type I readiness: three-role RBAC, structured audit log, OAuth 2.1, HTTPS. Type II requires 12 months of operation post-controls. ISO 27001, FedRAMP, and ISO/IEC 42001 are post-Wave 2.

**Q4 — AI-specific risks**: Unimatrix's RAG/vector architecture faces four high-severity AI-native risks per OWASP LLM Top 10 2025 and MITRE ATLAS v5.4.0: RAG poisoning via write access (LLM08), indirect prompt injection via stored entries (LLM01), excessive agency from over-permissioned agents (LLM06), and credential harvesting from stored context (ATLAS). Wave 2 mitigations: per-tool write authorization in RBAC, audit log with agent attribution, sensitive content ingestion policy, rate limiting per token. The existing crt-003 contradiction detection is a defensible MITRE ATLAS mitigation asset.

**Q5 — BSL procurement risk**: BSL-1.1 creates moderate procurement friction due to the Terraform/HashiCorp 2023 precedent and non-OSI recognition. Risk is manageable if the Additional Use Grant explicitly permits internal developer tool use. Feed to ASS-045: grant must include "internal software development, AI agent pipelines, CI/CD use is permitted regardless of commercial relationship" language; consider a dual-licensing path (BSL + commercial) for enterprise contracts.

| Spike | Title | Tier | Feeds |
|-------|-------|------|-------|
| ASS-048 | Enterprise Security Requirements | 0 — **COMPLETE** | ASS-041 (auth model), ASS-042 (role model), ASS-045 (licensing risk) |
| ASS-041 | Transport + Auth Stack Evaluation | 1 | W2-2, W2-3 |
| ASS-042 | Enterprise Security Model Architecture | 1 (integrator) | W2-3, W2-0 |
| ASS-043 | Container + Packaging Strategy | 2 | W2-1 |
| ASS-044 | Admin UI Architecture | 2 | W2-4 |
| ASS-045 | Licensing + Codebase Structure | 1 | W2-0, all items |
| ASS-046 | GGUF Feasibility | 3 | W2-5 go/no-go |
| ASS-047 | Core Scalability Strategy | 1 | W2-3 (control plane tech), W2-2 (connection limits) |

---

## Dependency Map

```
Tier 0 (run first — no dependencies, unblocks Tier 1):
  ASS-048: Enterprise Security Requirements ─────────────┐
                                                          │ feeds auth model + role model + licensing risk
                                                          ▼
Tier 1 (run in parallel after ASS-048):
  ASS-041: Transport + Auth Stack ──────────────────────┐
  ASS-045: Licensing + Codebase ────────────────────────┤──► ASS-042: Security Architecture
  ASS-047: Core Scalability ────────────────────────────┘    (integrates all Tier 0 + 1 findings)

ASS-042 output unblocks:
  ├── W2-3 delivery scoping (identity model, control plane schema, bootstrap)
  └── W2-0 delivery scoping (codebase boundary confirmed by ASS-045 input)

Tier 2 (independent — can run in parallel with Tier 0 + 1):
  ASS-043 ──► W2-1 delivery scoping (packaging decisions)
  ASS-044 ──► W2-4 delivery scoping (UI technology + scope)

Tier 3 (deferred — does not block other Wave 2 items):
  ASS-046 ──► W2-5 go/no-go
```

---

## What These Spikes Will Determine

After all Tier 1 + 2 spikes complete, the real Wave 2 delivery roadmap will specify:

1. **Delivery sequence** — whether W2-0 → W2-2 → W2-3 → W2-1 → W2-4 is right, or whether container and licensing must ship first as an outer shell
2. **Effort estimates** — currently unestimatable without transport library selection, RBAC design, and scalability ceiling confirmed
3. **W2-3 scope refinement** — per-repo RBAC binding location, control plane schema detail, bootstrap flow
4. **W2-5 disposition** — in-wave or deferred, based on ASS-046 finding
5. **SaaS optionality completeness** — `org_id` is confirmed; ASS-047 may surface additional low-cost SaaS hooks worth including in Wave 2

---

## Wave 3 — Unchanged

W3-1 (GNN session-conditioned relevance function) and W3-2 (knowledge synthesis) remain deferred pending:
- ASS-029 architecture spike (not yet started — can begin during Wave 2 delivery)
- Behavioral signals accumulating in production (2–4 weeks active daemon use)
- Groups 9 + 10 signal quality confirmed via live retrospectives

Wave 3 scoping can proceed in parallel with Wave 2 delivery. ASS-029 has no Wave 2 dependencies.
