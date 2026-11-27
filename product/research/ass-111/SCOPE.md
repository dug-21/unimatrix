# ASS-111 — Unimatrix V2 Feasibility: Single Multi-Tenant SaaS

**Mode**: Campaign (`uni-research-sm`). Spikes S1–S6 + Synthesis S7.
**Source**: `product/research/ass-111/Proposed V2 ProductVision.md` (human draft) + uni-zero session 2026-11-26.
**Decision this campaign enables**: whether Unimatrix V2 becomes one multi-tenant SaaS (V1 frozen/retired/maintained), and if so the architecture, goal set, and migration path. **V1 goals and capability map are not modified by this campaign.**

## Campaign framing

- V1 has effectively one user, multiple projects (the human). Portability and self-host breadth carry low strategic weight — challengeable, but weigh accordingly.
- Human has mutiple projects using unimatrix today... some development focused, some research based.  Med Search (human's other project) is the **second validating tenant**: medical research aggregation, frequent contradiction/retraction, learning-off (deterministic) mode, public web chat via MCP that must not influence learning or access patterns.
- Elements of prior "enterprise version" discussion (multi-tenant, OAuth, RBAC, shared knowledge) are deliberately in scope; prior "out of scope: enterprise" boundaries in V1 goal entries are **not** binding here.
- One of the many value props, but enabling knowledge storage retention outside of workflow context may open up new use cases capabilities (med-search) is one such example that has both types workflor oriented AND non-workflow oriented value.  

## Campaign-wide constraints

**Hard** (non-negotiable; findings may not propose violating them):
- Architectural Principles 1 (hash-chain integrity), 2 (append-only complete audit), 3 (capability checks at service layer after identity resolution), 8 (no secrets in any database).
- Tenant isolation: no tenant can read, write, or influence learning of another tenant except via an explicitly granted shared scope.  a 'Shared' scope may not be shared by every client.
- Graceful degradation (Principle 5) for all ML components.
- Unimatrix remains a knowledge engine, not an orchestration engine.

**Hypothesis** (human positions — challenge them):
- H1: One hosted multi-tenant SaaS replaces local deployment entirely.
- H2: HTTPS is the only transport; OAuth added alongside bearer/scoped tokens.
- H3: Deeper integration with one open harness (OpenCode) may be worth giving up harness ubiquity, if it delivers non-forgeable identity and cross-project coordination.
- H4: A Shared scope (readable by ≥1 tenant/project in addition to their own) is needed for enterprise shared knowledge.
- H5: Principles 6 (single binary, zero infra) and 7 (in-memory hot path) can be restated for SaaS without losing their intent.
- H6: Transient workflow-session memory and/or shared harness definitions (protocols, agents, skills) may belong in Unimatrix.

## Execution order

```
S1 ──┬──► S3 (also needs S2) ──┐
     ├──► S4 ──────────────────┤
S2 ──┘ (parallel with S1, S6)  ├──► S7 Synthesis
S6 (parallel; feeds S1 final)  │
S5 (independent) ──────────────┘
```
S2 findings get a review by `uni-architect` before S3 starts (storage is the most expensive decision to reverse). S6 runs in parallel with S1; the SM routes S6 output into S1's identity conclusions before S1 is finalized.

## Intelligence level

Agents run at their definition defaults. **S7 Synthesis: the SM MUST spawn with `effort: "max"`** — explicit human instruction (uni-zero session 2026-11-26).

## Prior art (all spikes)

- Goal entries (Unimatrix, category `goal`): personal-cloud, trusted-identity, platform, integrity, domain-agnostic — especially the "enterprise extends, never re-architects" clause and the ass-060 seven OSS invariants.
- ASS-060 (enterprise-compatibility invariants), ASS-100/101 (edge identity, root of trust), ASS-106 (OpenCode out-of-band agent-type stamping), ASS-108 (#1014, claude.ai OAuth connector), ASS-109 (#1020), ASS-049 (multi-LLM), ASS-057 (research domain), ASS-104 (#962 background-processing target state).
- vnc-050 (#1010): scoped tokens, `read` mode neither writes nor feeds learning; headless attach (C19).
- V1 capabilities directly relevant: DA1 (#835 runtime taxonomy discovery), DA10 (#1011 project-declared edges), SL-OPTOUT (#1015), KI-CONTRADICT, KI-QUARANTINE, PL-3/PL-4 (#5705 identity seam, fail-closed), C3/C4 (`resolve_store` single isolation seam).

---

## S1 — Tenancy & identity model

**Goal**
1. What is the identity hierarchy (tenant → project → scope, plus shared scope) and where does each identifier come from (transport, token claim, attestation — never payload)?
2. Does the existing `resolve_store` seam and (tenant, project) keying extend to multi-tenant without re-architecture? Where does it break?
3. What authentication mix is required: OAuth 2.1 (interactive + claude.ai connector), scoped bearer tokens, headless/CI (GitHub Actions) credentials, public-chat access? What is the minimum role model?
4. Can agent identity become non-forgeable (harness-attested) for at least one harness, and what does capability enforcement look like when it is (PL-4 fail-closed)?
5. What is the tenant onboarding flow (signup → project → client attach) at minimum friction?

**Breadth**: code+ecosystem · **Approach**: evaluation · **Confidence**: directional
**Target outputs**: identity/tenancy model (diagram + rules), auth-method matrix per client class, role model, gap list against current seams, onboarding flow sketch.
**Constraints**: campaign Hard; H2, H3 challengeable.
**Dependencies**: consumes S6 (harness attestation). Unblocks S3, S4, S7.
**Prior art**: ASS-060, ASS-100/101, ASS-106, ASS-108, vnc-050, PL-3/PL-4, trusted-identity goal.

## S2 — Storage, scale & hot path

**Goal**
1. Per-tenant SQLite (+ per-tenant vector index) vs shared Postgres (+ pgvector or equivalent) vs hybrid: which meets isolation, hash-chain/audit guarantees (DDL triggers), backup/restore per tenant, and cost at 10 / 100 / 1,000 tenants?
2. What replaces or restates Principle 7 (in-memory hot path rebuilt by tick) when N tenants share a fleet — per-tenant caches, eviction, lazy load? Same for the background tick and ML models (ONNX/NLI) per tenant, same for graph in memory updates.
3. What is the horizontal-scale model (stateless API + tenant-pinned workers? sharding?) and its operational burden for a solo operator?
4. What does per-tenant data export/offboarding and residency require?

**Breadth**: code+ecosystem · **Approach**: evaluation (measurement where cheap) · **Confidence**: directional; empirical for any cost/latency claim used to rank options
**Target outputs**: ranked storage options with trade-offs, recommended architecture, Principle 6/7 restatement draft, cost model, reversibility assessment.
**Constraints**: campaign Hard; H1, H5 challengeable.
**Dependencies**: none to start. Reviewed by `uni-architect` before S3. Unblocks S3, S7 (migration).
**Prior art**: current store/vector crates, ASS-047 (scalability), ASS-104, RETAIN, BACKUP-RESTORE, NX-DURABILITY.

## S3 — Shared scope semantics

**Goal**
1. What may a tenant/project read from a shared scope, and how do shared entries merge into its retrieval, briefing, and injection ranking?
2. Who may write to a shared scope, and whose hash chain and audit log does a shared write enter? How are corrections/deprecations of shared entries propagated?
3. Does shared-scope usage feed learning — the shared scope's, the reader's, both, neither — without cross-tenant leakage?
4. How does "1 client : 1 project integrity boundary" evolve (server-enforced per-project claim)?

**Breadth**: code+ecosystem · **Approach**: evaluation · **Confidence**: directional
**Target outputs**: shared-scope model (read/write/learning rules), integrity design, go/no-go on H4 for V2 initial release.
**Constraints**: campaign Hard (isolation, Principles 1–2); H4 challengeable.
**Dependencies**: S1, S2 (+ architect review). Unblocks S7.
**Prior art**: personal-cloud goal (1:1 integrity boundary clause), KI-CHAIN/KI-CHAIN-XV, SL1/SL2/SL4.

## S4 — Client classes & learning isolation

**Goal**
1. What client classes must V2 serve — interactive harness, autonomous agent (GitHub Actions / task-oriented), chat application over MCP, public web chat — and what does each need (capabilities, hooks, identity)?
2. How is each class's traffic kept out of (or selectively into) learning and access-pattern signals? Is vnc-050 `read` mode sufficient for public chat, and how is anonymous/public access reconciled with "every port authenticated"?
3. What does a deterministic (learning-off) project mode require end to end — votes, co-access, confidence evolution, graph self-enrichment, NLI, tick — so the same query yields the same results?
4. What does Med Search need for frequent contradiction/retraction (KI-CONTRADICT, quarantine, NLI) and is it a V2 requirement or a domain extension?

**Breadth**: code+ecosystem · **Approach**: evaluation · **Confidence**: directional
**Target outputs**: client-class matrix, learning-isolation rules, deterministic-mode definition, Med Search requirement list mapped to V1 capabilities.
**Constraints**: campaign Hard; knowledge-engine-not-orchestrator.
**Dependencies**: S1. Unblocks S7.
**Prior art**: vnc-050, C19, SL-OPTOUT #1015, KI-CONTRADICT, KI-QUARANTINE, ASS-057, DA1/DA10.

## S5 — Engine boundary: session memory & harness definitions

**Goal**
1. Is there a real need for transient workflow-session memory (state shared across independent agents, retained until the workflow session ends)? What would it hold that knowledge entries and existing session context do not?
2. If built, what constraints keep it on the knowledge-engine side of "not an orchestration engine" (TTL, scope, no coordination semantics, never a learning signal)?
3. Should harness definitions (protocols, agent, skill definitions) be stored in a shared scope to integrate more closely with a harness? Distinguish distribution/versioning of artifacts from retrieval-as-knowledge. What breaks in the static/dynamic mental model?
4. Should onboarding/configuration (domain packs, categories, edges today hand-edited in TOML) be a tenant-facing configuration surface, and how does it relate to DA1/DA10/PL-2?
5. Unimatrix stays **workflow-aware AND** serves non-workflow knowledge (retention and retrieval outside any workflow context — e.g. Med Search aggregation, public chat). How do the two coexist: which engine features (phase conditioning, proactive delivery, session signals) apply to non-workflow knowledge and clients, what degrades gracefully, and how is the vision restated so workflow-awareness remains an explicit, named differentiator — additive, never diluted?

**Breadth**: code+industry · **Approach**: evaluation · **Confidence**: directional
**Target outputs**: go/no-go per item (session memory, harness definitions, config surface) with boundary rules; workflow-aware AND non-workflow coexistence model; vision-text recommendation that keeps workflow-awareness named.
**Constraints**: knowledge-engine-not-orchestrator is Hard; H6 challengeable.
**Dependencies**: none. Unblocks S7.
**Prior art**: product vision "key mental model", ASS-063 (#649 Unimatrix as protocol/workflow harness), PL-2, DA1, DA10.

## S6 — Harness depth vs ubiquity

**Goal**
1. What does OpenCode-first deep integration concretely enable: non-forgeable agent identity, per-agent-type capability policy, coordination across project scopes? Which parts are proven vs speculative?
2. What do Claude Code and Codex lose or keep in V2 (retrieval-only? full hooks?) if OpenCode is primary?
3. What is OpenCode's maturity, adoption trajectory, and extension stability risk as a strategic dependency?

**Breadth**: code+ecosystem · **Approach**: evaluation (proof-of-concept if attestation feasibility is uncertain) · **Confidence**: directional; validated for any attestation claim S1 depends on
**Target outputs**: capability matrix per harness for V2, recommendation on H3, dependency-risk assessment.
**Constraints**: H3 challengeable.
**Dependencies**: none to start. Feeds S1 before S1 finalizes. Unblocks S7.
**Prior art**: ASS-106, #998, C14, C18, nan-024.

## S7 — Synthesis: the V1/V2 decision

**Goal**
1. Is single multi-tenant SaaS feasible for a solo operator at acceptable cost and risk? Go / no-go / conditional.
2. V1 disposition: freeze, retire, or maintain — and what that costs.
3. V1 → V2 migration path (data, hash chains, audit, clients) for existing projects.
4. Draft V2 strategic goal set (which V1 goals survive, merge, die, or are new) and restated architectural principles.
5. Sequenced V2 roadmap at capability altitude (what must be proven first).

**Breadth**: all S1–S6 findings · **Approach**: evaluation · **Confidence**: directional
**Target outputs**: decision memo in `ass-111-findings.md` with go/no-go, V1 disposition, migration plan, draft goals + principles, ordered capability roadmap, unresolved risks.
**Constraints**: all campaign constraints; must not edit V1 goals, capability map, or PRODUCT-VISION.md (draft only).
**Dependencies**: S1–S6. **Spawned with `effort: "max"`.**
**Prior art**: S1–S6 findings, `Proposed V2 ProductVision.md`.

## S8 — V1 refactor debt (add-on, human-approved 2026-11-26)

**Evidence** (wc -l, `crates/**/*.rs`): 138/460 files exceed the 500-line limit in `.claude/rules/rust-workspace.md`; 52 >1000, 26 >2000; 71% of all lines live in over-limit files. Worst: unimatrix-server `mcp/tools.rs` 13,808; `infra/config.rs` 12,886; `uds/listener.rs` 10,219; `services/search.rs` 6,051; `background.rs` 5,078; `uds/hook.rs` 4,579; `server.rs` 4,455; `services/status.rs` 4,352; store `read.rs` 3,431.

**Goal**
1. Map over-limit modules to S1 gaps (G1–G17) and V2 stages; which splits are prerequisites (Stage 0)?
2. Decomposition plan + sequencing + rough effort for a solo operator; decide refactor-then-evolve vs new codebase (bears on D7, Option A/P6).
3. Reconcile with "freeze V1, security fixes only."
4. Limit policy: is 500 the right limit? Propose a better one (e.g. separate thresholds for src vs tests, counting rules), and how to HOLD it — enforcement mechanism (CI check / clippy / script, ratchet so files can't grow, baseline exemptions burn-down). ci.yml changes go via issue.

**Breadth**: code-only for Q1–Q3; code+ecosystem for Q4 (Rust tooling for line limits/ratchets — clippy, custom scripts, existing CI tools).
**Approach**: evaluation; measurement for the size inventory and effort basis (e.g. counting call sites and tests a split touches).
**Confidence**: directional overall; empirical for the line-count inventory and for any effort number used to choose refactor-then-evolve vs new codebase.
**Target outputs**: `ass-111-s8-findings.md`; then amend `ass-111-findings.md` (KC4, D7, Stage 0/1, decision list — add new decisions, don't make them).
**Constraints**:
- Hard: all campaign Hard constraints. The spike must not edit `.claude/rules/rust-workspace.md`, `ci.yml`, or any code; any limit or enforcement change is a recommendation only, and ci.yml changes go through an issue.
- Hypothesis (challengeable): the 500-line limit itself; S7's "freeze V1, security fixes only"; S7's single-repo default for D7.
**Dependencies**: S1–S7 findings. Unblocks the S7 amendment.
**Prior art**: `.claude/rules/rust-workspace.md`; S7 findings (KC4, D7, Stages 0–1, V1 disposition); S1 gap list G1–G17; S2's ~800 SQL-site count and architect condition C6 (no raw SQL outside the store crate); Unimatrix lessons on large-file refactors (e.g. #3328, the crt-027 compile-cycle blowup during service replacement).

## S9 — Audit trustworthiness (fire-and-forget audit writes) (add-on, human-approved 2026-11-27)

**Context**: V1 audit writes are fire-and-forget; acceptable for a personal repo, a weakness for V2 SaaS. Draft P2 claims "append-only and complete", which fire-and-forget cannot guarantee.

**Goal**
1. Inventory every audit write path (crates/unimatrix-server/src/infra/audit.rs and all callers, incl. background/tick, UDS, export/import): how each can lose, reorder, or misattribute rows (async spawn, swallowed errors, outside the state-change transaction, shutdown/OOM/container stop), and which state changes emit no audit row.
2. Options for trustworthy audit: same-transaction audit write, transactional outbox, synchronous durable write, per-store tamper-evident hash chain + verification; retention/export. Recommend one, with failure semantics (fail the operation vs degrade).
3. Measured per-write latency/throughput cost of the recommendation vs today; interaction with per-project stores, control-plane audit log (G11), vectors-in-SQLite (C1), Litestream RPO, multi-cell (C16–C28 in ass-111-s2-architect-review-2.md).
4. Amendments to P2, G9, G11, stages.

**Roles** (human direction, NOT researched): roles are already tenant-scoped by the OAuth design (no cross-tenant queries); audit logs are tenant-scoped; an auditor role is a tenant-scoped read-only variant that views audit logs. No interface exists, so the role is deferred — record it as a later roadmap item only.

**Breadth**: code-only for 1 and 3; code+ecosystem for 2. **Approach**: evaluation + measurement. **Confidence**: directional; empirical for the loss-path inventory and the per-write cost.
**Constraints** — Hard: campaign Hard constraints; no code changes; recommendations only; P2 append-only stands. Hypothesis: fire-and-forget is acceptable for any audit class.
**Prior art**: S1 (G9, G11, roles), S2 + both architect reviews (C1–C28), #5714, infra/audit.rs.
**Dependencies**: S1, S2, architect review 2. Feeds S7.

---

## Output files

`product/research/ass-111/ass-111-s{N}-findings.md` per spike; `ass-111-findings.md` for S7.
