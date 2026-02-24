# ASS-009/05: Strategic Synthesis — Unimatrix Beyond Software Development

**Date**: 2026-02-24
**Type**: Strategic synthesis
**Inputs**: 01-core-capabilities.md, 02-market-opportunities.md, 03-architecture-decomposition.md, 04-competitive-landscape.md, PRODUCT-VISION.md

---

## Part 1: Executive Summary

**Thesis.** Yes, conditionally. Unimatrix's core can serve broader markets, but not by becoming a general-purpose knowledge management platform. The portable kernel -- embedded transactional storage, dual retrieval (semantic + deterministic), knowledge lifecycle with hash-chained correction history, and trust-attributed audit trails -- is genuinely domain-agnostic. Twelve capabilities were assessed; six rated HIGH portability, six rated MEDIUM (report 01). The architecture is already 90% ready for multi-domain deployment; the remaining 10% is configuration externalization estimated at 1-2 days of work (report 03, Option D). However, the commercial path is not "build a platform for everything." It is: nail the agent-knowledge infrastructure category first, then selectively extend into domains where the knowledge lifecycle differentiator creates a gap no incumbent fills. The window for this is narrowing -- Letta, Zep, and Mem0 are professionalizing fast (report 04), and Unimatrix's trust/integrity moat only holds if the product is production-deployed before competitors retrofit similar features.

**Three most compelling non-dev opportunities:**

1. **DevOps/SRE operational knowledge** (match: 9/10, GTM friction: low-medium). Incident runbooks are the canonical knowledge-lifecycle problem. No incumbent specializes in accumulating, curating, and retrieving operational knowledge with correction chains. SRE teams adopt tools bottom-up and value local-first reliability. MCP integrates naturally with AI-assisted incident response workflows.

2. **Product Management decision journals** (match: 8/10, GTM friction: low-medium). Product decision rationale is a knowledge management problem no tool addresses. Correction chains model "we changed our decision because new data showed Y" -- PMs lose this context within weeks. The competitive gap is real and specific.

3. **Scientific Research lab knowledge** (match: 8/10, GTM friction: medium). Lab protocols evolve constantly, and institutional knowledge leaves when postdocs do. ELNs capture experimental data but are poor at capturing accumulated wisdom. The local-first, hash-chain-integrity model appeals to pre-publication data sensitivity requirements. The ELN-AI market is $1.88B (2025) and growing.

**The single most important architectural insight.** From report 03: the architecture is already domain-agnostic at the storage layer. Every field on `EntryRecord` except one (`feature_cycle`, a free-form string with a dev-flavored name) is universally applicable. The domain-specific coupling lives entirely in four server-level constants: the initial category allowlist (8 dev categories), the server instructions text, the default agent bootstrap, and the content scanning patterns. Externalizing these four items into a config file is the entire unlock for multi-domain deployment. This is not a rewrite. It is not even a refactor. It is configuration extraction.

---

## Part 2: The Portable Kernel

### What constitutes the reusable core

Drawing from reports 01 and 03, the portable kernel is a four-layer stack:

**Layer 1: Storage Foundation (fully portable, zero domain coupling)**
- Embedded transactional knowledge store (redb, single-file, ACID, COW pages)
- Typed records with user-defined topic/category/tags/status metadata
- 5-axis secondary indexing with set-intersection query (`QueryFilter`)
- Content integrity chain (SHA-256 hash per record, previous_hash linking, version counters)
- Append-only audit trail with monotonic IDs and transactional consistency
- Automatic schema migration on open (idempotent scan-and-rewrite)

**Layer 2: Intelligence (mostly portable)**
- Local ONNX embedding generation (7 pre-configured 384-d models, no API dependency)
- HNSW approximate nearest neighbor search with filtered search support
- Combined semantic + deterministic retrieval
- Near-duplicate detection on write (0.92 cosine threshold)

**Layer 3: Access Control (portable with domain remapping)**
- Agent/actor registry with 4-tier hierarchical trust
- Capability-based access control (Read, Write, Search, Admin)
- Auto-enrollment for unknown actors (Restricted)
- Per-request audit logging with actor attribution
- Content scanning (PII + injection detection, pattern set is swappable)

**Layer 4: Protocol Interface (AI-ecosystem specific)**
- MCP server with 8 tools
- Behavioral driving via server instructions
- Compiled orientation briefings (role + task)
- Format-selectable responses (summary/markdown/json)

### What is configuration vs what is architecture

**Architecture (cannot change without code):**
- The `EntryRecord` schema (23 fields, append-only evolution)
- The `Status` enum (Active/Deprecated/Proposed -- adding states requires code)
- The 4-tier trust hierarchy (System > Privileged > Internal > Restricted)
- The 4 capabilities (Read, Write, Search, Admin)
- The correction chain model (supersedes/superseded_by)
- The combined write transaction pattern (entry + indexes + audit in one txn)
- redb as the storage backend; hnsw_rs as the vector index; ONNX as the inference engine

**Configuration (can change without code, or with trivial changes):**
- The initial category allowlist (8 strings in a constant)
- The server instructions text (1 constant)
- The default bootstrap agents (1 function)
- The content scanning pattern set (~31 regex patterns, additive)
- The embedding model selection (7 pre-configured, selectable)
- The near-duplicate threshold (1 float)
- The confidence decay parameters (constants)
- Topic, category, and tag values (free-form strings, no validation at storage layer)
- The `feature_cycle` field value (free-form string despite the dev-flavored name)

**The critical observation:** Every domain-specific element is in the "configuration" column. The architecture is already domain-agnostic. Report 03 confirms this: "the storage engine itself handles ANY domain without schema changes. The constraint is entirely in the server's policy layer, which is already designed to be extensible."

### Minimum changes to unlock multi-domain deployment

From report 03, Option D (Protocol-First), ranked as highest ROI:

1. **Move initial categories to config** (~0.5 days). Replace `const INITIAL_CATEGORIES` with a `ServerConfig` struct loaded from `~/.unimatrix/config.toml` or environment variables. A legal deployment loads `["precedent", "statute", "brief", "ruling", "regulation"]`. A scientific deployment loads `["protocol", "guideline", "finding", "hypothesis", "method"]`.

2. **Make server instructions configurable** (~0.5 days). Replace the `SERVER_INSTRUCTIONS` const with a config field. Different domains get different behavioral driving text.

3. **Make default agents configurable** (~0.5 days). Let the config specify which agents are bootstrapped and at what trust level.

4. **Optionally externalize scanning rules** (~0.5 days). Domain-specific PII patterns (e.g., medical record numbers for healthcare, case numbers for legal) are additive to the built-in patterns.

**Total estimated effort: 1-2 days.** This is the entire unlock. After this, the same binary serves any domain by swapping a config file.

---

## Part 3: Top 3 Opportunity Deep-Dives

### Opportunity 1: DevOps/SRE Operational Knowledge

**Why this domain, specifically.**

Report 02 scored this 9/10 on capability match -- the highest of any domain analyzed. The reason is structural: operational knowledge *is* knowledge lifecycle management. Runbooks get written after incidents, corrected after subsequent incidents reveal errors, and deprecated when infrastructure changes. This is exactly the store/correct/deprecate cycle with attribution that Unimatrix implements. No incident management tool (incident.io, PagerDuty, Rootly, FireHydrant) specializes in the *knowledge layer underneath* -- they handle coordination, not curation.

The MCP interface is a natural fit because SRE teams are already the earliest adopters of AI-assisted operations. Incident response AI agents need access to persistent operational knowledge -- "has anyone seen this error signature before?" is the canonical query. Report 04 confirms no competitor combines lifecycle management with semantic search in a self-contained, local-first package. Local-first is not just nice-to-have here; it is critical -- incident response tools should work even when cloud services are down.

**What the product looks like.**

- **Packaging**: Same `unimatrix-server` binary, shipped with an SRE-focused config file. Categories: `runbook`, `incident-finding`, `post-mortem`, `escalation-path`, `architecture-decision`, `monitoring-rule`. Server instructions orient AI agents toward operational knowledge patterns.
- **Deployment**: MCP server integrated with Claude Code, Copilot, or any MCP-compatible AI assistant. Runs locally on the SRE engineer's machine or in a shared dev container.
- **UX**: Invisible infrastructure. The AI assistant asks Unimatrix for context during incident response. Engineers review and correct knowledge entries via the AI chat interface (or future `mtx-002` Knowledge Explorer). On-call handoff briefings via `context_briefing` with role="on-call-engineer" and task="incoming shift for {service}".
- **Key workflow**: After each incident, the responding engineer (or an AI agent) stores findings. Correction chains capture "the old runbook was wrong because..." Over time, the system accumulates institutional operational knowledge that persists across team turnover.

**Competitive moat.**

Report 04's competitive positioning matrix shows Unimatrix is the only system combining: self-contained binary + local embeddings + knowledge lifecycle + correction chains + hash integrity + trust/attribution + content scanning + audit trail + MCP native + deterministic lookup. Incident management tools (incident.io, PagerDuty) focus on coordination. Wiki-based postmortem stores (Confluence) have no lifecycle, no semantic search, and no correction chains. The "operational knowledge engine" category does not exist yet. Unimatrix defines it.

Retrofitting lifecycle management into incident.io or PagerDuty would require fundamental data-model changes to those platforms. This is a structural moat, not a feature gap.

**What ships from the current roadmap vs what is new work.**

- **Ships from roadmap (M1-M2, complete)**: Storage engine, vector index, embedding pipeline, MCP server, all 8 tools, content scanning, audit trail, agent trust. This is the entire SRE knowledge engine backend.
- **Ships from roadmap (M4, planned)**: Usage tracking (crt-001), confidence evolution (crt-002), contradiction detection (crt-003). These directly serve SRE: "which runbooks haven't been validated recently?" and "these two runbooks for the same service contradict each other."
- **New work**: SRE config file with appropriate categories and server instructions (S effort, <1 day). SRE-specific content scanning patterns for infrastructure secrets (S effort). Documentation and marketing positioning (M effort).

**Go-to-market sketch.**

- **Who buys**: SRE team leads and platform engineering managers at mid-size companies (50-500 engineers). Teams with 5-50 on-call responders who already use AI assistants.
- **How they find it**: MCP server registry listings. SREcon talks and blog posts. "Operational knowledge engine" content marketing targeting "incident postmortem knowledge gets lost" pain point. Hacker News, SRE Slack communities, incident.io community.
- **What they pay**: Open-source core (MCP server + SRE config). Paid tier for team features: shared knowledge base, admin dashboard (mtx-001), knowledge explorer (mtx-002). Pricing: $15-25/seat/month for team features. Revenue per team: $75-1,250/month. Requires volume.
- **Risk**: Small per-team market. Revenue per customer is low. Needs high adoption volume to be meaningful revenue. But this is the "land" in a land-and-expand strategy.

---

### Opportunity 2: Product Management Decision Journals

**Why this domain, specifically.**

Report 02 scored this 8/10 on capability match with low-medium GTM friction. The insight is precise: product teams make dozens of decisions per week, and within a month, the rationale is lost. The correction chain model directly captures "we changed our decision on X because new data showed Y." No current tool specializes in product decision knowledge management. Productboard handles roadmapping. Dovetail handles user research synthesis. Notion and Confluence store decisions statically. None of them track decision evolution with attribution and verifiable history.

Report 04 confirms the gap: "No tool specializes in *product decision knowledge management* -- the accumulation, evolution, and retrieval of why decisions were made." PMs are technically literate enough to adopt MCP-integrated tools, and the AI-assisted PM workflow is growing fast.

**What the product looks like.**

- **Packaging**: Same binary, PM-focused config. Categories: `decision`, `rationale`, `customer-signal`, `competitive-intel`, `metric-insight`, `experiment-result`, `strategy-change`. Server instructions orient AI toward decision capture and retrieval patterns.
- **Deployment**: MCP server integrated with the PM's AI assistant (Claude, ChatGPT, etc.). Local to the PM's machine. Knowledge accumulates across product cycles.
- **UX**: "Product Decision Journal" positioning. After making a decision, the PM tells their AI assistant what was decided and why. The assistant stores it via `context_store` with appropriate categorization. When revisiting a decision area, the PM asks "what have we decided about pricing?" and gets a chronological, attributed history with correction chains showing how the thinking evolved. `context_briefing` with role="product-manager" and task="onboarding to feature area X" compiles everything a new PM needs to know.
- **Key differentiator**: The correction chain. When a PM changes a decision, the old rationale is not deleted -- it is deprecated with a link to the new decision and the reason for the change. This is the institutional memory that every product org claims to want but no tool provides.

**Competitive moat.**

The moat is the correction chain data model. Notion can add AI search. Confluence can add better categorization. But neither can add hash-chained correction histories with attribution without rebuilding their storage layer. The integrity guarantee (SHA-256 content hashes, previous_hash linking, version counters, audit trail) is architecturally baked in. A competitor would need years to retrofit this.

**What ships from the current roadmap vs what is new work.**

- **Ships from roadmap (M1-M2, complete)**: Everything needed for the core product decision journal.
- **Ships from roadmap (M5, planned)**: Retrospective pipeline (col-002) directly serves PM use case -- "what decision patterns correlate with successful launches?" Process proposals (col-003) could surface "you always revisit pricing decisions after beta -- consider locking pricing before beta starts."
- **New work**: PM config file (S effort). PM-oriented documentation and positioning (M effort). Decision record templates as starter knowledge entries (S effort).

**Go-to-market sketch.**

- **Who buys**: Product managers and product leaders at B2B SaaS companies. Individual PMs first, then team adoption.
- **How they find it**: Product Hunt launch. PM community channels (Lenny's Newsletter, Product-Led Alliance, Mind the Product). "Why your product decisions keep getting re-debated" content marketing. AI tool roundup articles targeting PMs.
- **What they pay**: Free for individual use. Paid for team features (shared knowledge base, admin dashboard). $10-20/seat/month. Revenue per team: $50-400/month.
- **Risk**: PMs are busy and notoriously hard to change workflows for. Adoption requires demonstrating immediate value in the first session. The "decision journal" concept needs to feel lighter than "knowledge management."

---

### Opportunity 3: Scientific Research Lab Knowledge

**Why this domain, specifically.**

Report 02 scored this 8/10 on capability match with medium-high revenue potential. The ELN-AI market reached $1.88B in 2025, projected to $3.6B by 2029. But the gap Unimatrix fills is not the ELN itself -- it is the accumulated wisdom that ELNs are terrible at capturing: "I tried X and it didn't work because of Y" knowledge that makes senior researchers effective and that vanishes when they leave.

The hash-chain integrity model has special relevance here. Research reproducibility is a crisis -- a provable chain of custody for protocol evolution (who changed what, when, and why) directly addresses it. Local-first deployment addresses pre-publication data sensitivity, proprietary methods, and export-controlled research. Trust levels map naturally to lab hierarchy (PI, senior researcher, postdoc, graduate student).

**What the product looks like.**

- **Packaging**: Same binary, lab-focused config. Categories: `protocol`, `method`, `finding`, `hypothesis`, `troubleshooting`, `reagent-note`, `equipment-setting`. Server instructions orient AI toward lab knowledge patterns: "Before starting a new experiment, search for related protocols and troubleshooting notes."
- **Deployment**: Runs on the lab's shared workstation or individual researcher machines. No cloud dependency -- critical for sensitive research data.
- **UX**: "Lab Knowledge Engine" positioning. Researchers interact via their AI assistant. "Has anyone in this lab worked with this cell line before?" triggers semantic search. Protocol corrections create chains: "Protocol v3 replaced v2 because reagent X was discontinued by the supplier." New lab members get oriented via `context_briefing` with role="graduate-student" and task="starting work on [project]".
- **Key workflow**: Senior researchers store protocol rationale and troubleshooting knowledge. When they leave, the knowledge persists with attribution and correction history. Junior researchers benefit from the accumulated wisdom without having to rediscover it through costly failed experiments.

**Competitive moat.**

ELN platforms (Benchling, LabArchives, SciNote) are full experiment recording systems. They do not compete on knowledge curation. The moat is positioning: Unimatrix is "the knowledge layer that runs alongside your ELN," not a replacement. The hash-chain integrity (useful for regulatory compliance in pharma/biotech), local-first deployment (useful for sensitive research), and correction chain model (useful for protocol evolution tracking) are structural differentiators that ELNs cannot easily add.

**What ships from the current roadmap vs what is new work.**

- **Ships from roadmap (M1-M2, complete)**: Core engine.
- **Ships from roadmap (M4, planned)**: Contradiction detection (crt-003) is high-value for labs -- "these two protocols for the same assay use different buffer concentrations."
- **New work**: Lab config file (S effort). Lab-oriented documentation (M effort). Validation that 384-d general-purpose embeddings handle scientific terminology adequately, with model recommendation if not (M effort -- may need to test biomedical-specific sentence transformers). Academic pricing/licensing (S effort).

**Go-to-market sketch.**

- **Who buys**: PIs and lab managers at academic labs and biotech startups. Also core facility staff who maintain shared protocols across multiple research groups.
- **How they find it**: Scientific computing communities (BioStars, SEQanswers). Preprint servers and academic conferences. "Lab knowledge management" content targeting the reproducibility crisis. Word-of-mouth in research communities.
- **What they pay**: Free for academic use (open-source core). Paid for institutional features (shared knowledge base, admin dashboard, export for compliance). $20-50/seat/month for institutional licenses. Biotech companies pay more. Revenue per lab: $100-500/month for academic, $500-2,500/month for biotech.
- **Risk**: Academic adoption is slow. Researchers resist new tools unless the pain is acute. The embedding model may not handle specialized scientific terminology well without a domain-specific model.

---

## Part 4: What Ships Together, What Ships Separately

### Core Engine (in every package)

The following constitutes the universal Unimatrix kernel, present in every deployment regardless of domain:

- `unimatrix-store`: Embedded transactional knowledge store with 5-axis indexing, content integrity chains, schema migration
- `unimatrix-vector`: HNSW vector index with bidirectional ID mapping and crash-safe persistence
- `unimatrix-embed`: Local ONNX embedding pipeline with model management
- `unimatrix-core`: Trait abstractions, domain types, async wrappers
- `unimatrix-server`: MCP server runtime, audit logging, agent registry, content scanning, response formatting

The core engine binary is identical across domains. It reads its "personality" from configuration.

### Domain Packs (domain-specific configuration)

A domain pack is a configuration bundle that transforms the generic engine into a domain-specific knowledge system. Each pack contains:

| Component | What It Contains |
|-----------|-----------------|
| `categories.toml` | Initial category allowlist for the domain |
| `instructions.md` | Server instructions text (behavioral driving for AI agents) |
| `agents.toml` | Default agent bootstrap (who gets what trust level) |
| `scanning.toml` | Additional content scanning patterns (domain-specific PII, etc.) |
| `starter-knowledge/` | Optional seed entries (conventions, reference material) |

**Planned domain packs:**

| Pack | Categories | Key Instruction Theme |
|------|-----------|----------------------|
| `dev` (default) | outcome, lesson-learned, decision, convention, pattern, procedure, duties, reference | "Before starting implementation, search for relevant conventions and past decisions." |
| `sre` | runbook, incident-finding, post-mortem, escalation-path, architecture-decision, monitoring-rule | "Before responding to an incident, search for similar past incidents and relevant runbooks." |
| `product` | decision, rationale, customer-signal, competitive-intel, metric-insight, experiment-result, strategy-change | "Before making a product decision, search for related prior decisions and their outcomes." |
| `research` | protocol, method, finding, hypothesis, troubleshooting, reagent-note, equipment-setting | "Before starting a new experiment, search for related protocols and troubleshooting notes." |

Domain packs are the sole new artifact for multi-domain deployment. They are documentation-weight, not code-weight.

### Standalone Components

**`unimatrix-embed` as a standalone crate.** Report 03 identifies this as the only crate with genuine independent value on crates.io. It is a clean ONNX embedding crate wrapping `ort` + `tokenizers` with 7 pre-configured models, mean pooling, L2 normalization, and HuggingFace Hub download management. Zero Unimatrix dependencies. The market for "easy local embeddings in Rust" is underserved. Rename to something generic (e.g., `onnx-embeddings` or `local-embed`) and publish. Effort: Low (1-2 days, primarily resolving the `ort` version pinning and `anndists` patch issues).

**`KnowledgeEngine` facade.** Report 03 recommends creating a `KnowledgeEngine` struct that wraps store + vector + embed and provides a clean synchronous Rust API without MCP. This serves the "embed Unimatrix in my Rust application" use case. Effort: Medium (3-5 days). This is the SDK that would let other tools use Unimatrix as an embedded knowledge backend.

Other crates (`unimatrix-store`, `unimatrix-vector`, `unimatrix-core`) do not have sufficient standalone value to justify independent publishing. `unimatrix-store` is too opinionated (23-field `EntryRecord`), `unimatrix-vector` is a thin wrapper over `hnsw_rs`, and `unimatrix-core` is a facade over the stack.

### The MCP Question

**Is MCP the universal interface, or do some domains need REST/SDK?**

MCP is the right interface for the primary use case: AI agents consuming structured knowledge. Report 02 confirms MCP has become the de facto protocol for AI tool integration (8M+ server downloads, 5,800+ servers, donated to Linux Foundation's Agentic AI Foundation). The 2026 projection is enterprise MCP adoption at scale.

However, three scenarios require non-MCP access:

1. **Embedded Rust library** (SDK use case). When another Rust application wants to use Unimatrix as a knowledge backend without running a separate MCP server process. Solution: `KnowledgeEngine` facade (report 03, Option C). No MCP, no tokio required. Synchronous API.

2. **REST/HTTP API** (enterprise integration). When enterprise systems need to query or populate Unimatrix from non-AI-agent contexts (CI/CD pipelines, web dashboards, integrations). Solution: The MCP server's internal plumbing is already transport-agnostic (report 03). Adding an HTTP transport alongside stdio is a future milestone, not an architecture change. The `mtx-*` features (Milestone 6) implicitly require this.

3. **CLI** (human operator). When a human wants to interact directly without an AI assistant. Solution: `nan-001` (CLI binary) is already on the roadmap in Milestone 9.

**Recommendation**: MCP is the primary interface. The `KnowledgeEngine` facade (for embedding) and HTTP transport (for dashboards and integrations) are the two necessary supplements. Do not build a REST API speculatively -- let dashboard requirements (M6) drive the HTTP transport.

### Proposed Package Structure

```
unimatrix/
├── crates/
│   ├── unimatrix-store/          # Embedded knowledge storage engine
│   ├── unimatrix-vector/         # HNSW vector index
│   ├── unimatrix-embed/          # Local ONNX embeddings (publishable standalone)
│   ├── unimatrix-core/           # Traits, types, async wrappers
│   ├── unimatrix-engine/         # NEW: KnowledgeEngine facade (no MCP)
│   └── unimatrix-server/         # MCP server binary + lib
│
├── packs/                        # NEW: Domain configuration packs
│   ├── dev/                      # Default: software development
│   │   ├── categories.toml
│   │   ├── instructions.md
│   │   ├── agents.toml
│   │   └── starter-knowledge/
│   ├── sre/                      # DevOps/SRE
│   ├── product/                  # Product management
│   └── research/                 # Scientific research
│
└── configs/                      # NEW: Example config files
    └── unimatrix.toml            # Config schema with comments
```

---

## Part 5: Roadmap Implications

### What changes are needed to accommodate multi-domain

**Milestone 2 (MCP Server) -- changes to vnc-002 or a new vnc-004:**

| Change | What | Effort |
|--------|------|--------|
| Externalize category allowlist | Replace `const INITIAL_CATEGORIES` in `categories.rs` with config-file loading. Fall back to dev defaults if no config file present. | S |
| Externalize server instructions | Replace `SERVER_INSTRUCTIONS` const in `server.rs` with config-file loading. | S |
| Externalize agent bootstrap | Move `bootstrap_defaults()` configuration to `agents.toml`. | S |
| Add config file infrastructure | `ServerConfig` struct, TOML parsing, `~/.unimatrix/config.toml` or per-project config. | S-M |

**Milestone 4 (Learning & Drift) -- no changes needed:**

The learning features (usage tracking, confidence evolution, contradiction detection, co-access boosting) are fully domain-agnostic. Usage patterns, confidence decay, and contradiction detection work identically for runbooks, product decisions, and lab protocols.

**Milestone 7 (Multi-Project & Identity) -- add domain pack support:**

| Change | What | Effort |
|--------|------|--------|
| Domain pack loading | `dsn-004` config should include domain pack selection. `unimatrix init --pack sre` loads the SRE domain pack. | S |

### What should be ADDED to the roadmap

| Addition | Where | Rationale | Effort |
|----------|-------|-----------|--------|
| `vnc-004`: Config externalization | After M2 (vnc-003), before M4 | The multi-domain unlock. Without this, domain packs cannot exist. | S (1-2 days) |
| `KnowledgeEngine` facade crate | New `nxs-005` or part of a new milestone | Enables "embed in my Rust app" use case. Required for any tool that wants to use Unimatrix as a backend library. | M (3-5 days) |
| Domain pack infrastructure | Part of `dsn-004` (Config & Export) | Config schema, pack loading, pack validation. The mechanism for domain packs to work. | S |
| `unimatrix-embed` standalone publish | Part of `nan-004` (Release Automation) | Publish the embedding crate to crates.io under a generic name. Community building, ecosystem contribution. | S (1-2 days) |

### What should be RESEQUENCED

**Move config externalization (vnc-004) to immediately after M2 completion.** The current roadmap goes M2 -> M4 (Learning & Drift). Insert a small vnc-004 between them. This is 1-2 days of work and unlocks multi-domain deployment before any learning features ship. The learning features then benefit from being domain-configurable from day one.

**Revised sequence:**
```
M1: Foundation (nxs) ................ COMPLETE
M2: MCP Server (vnc-001/002/003) ... COMPLETE
    vnc-004: Config externalization . NEW, 1-2 days
M4: Learning & Drift (crt) ......... unchanged
M5: Orchestration Engine (col) ..... unchanged
M3: Agent Integration (alc) ........ deferred (already deferred)
M6: Real-Time Interface (mtx) ...... unchanged
M7: Multi-Project (dsn) ............ add domain pack support to dsn-004
M8: Thin-Shell Migration (alc) ..... unchanged
M9: Build & Deploy (nan) ........... add unimatrix-embed publish to nan-004
```

The `KnowledgeEngine` facade can be built in parallel with M4 as an independent track.

### What should NOT change

These roadmap elements are already correct for multi-domain:

1. **M1 (Foundation) architecture**. The `EntryRecord` schema, `QueryFilter` model, correction chain model, and security fields are universally applicable. Report 01 confirms: "the EntryRecord schema is remarkably domain-agnostic."

2. **M2 (MCP Server) tool design**. The 8 tools (`context_search`, `context_lookup`, `context_store`, `context_get`, `context_correct`, `context_deprecate`, `context_status`, `context_briefing`) represent generic knowledge management operations. Their parameter shapes (`topic`, `category`, `tags`, `query`) are domain-neutral by design (ASS-007's "three hard design constraints").

3. **M4 (Learning & Drift) features**. Usage tracking, confidence evolution, contradiction detection, and co-access boosting are domain-agnostic intelligence features. They work for any domain's knowledge.

4. **M5 (Orchestration Engine) process intelligence**. The retrospective pipeline and process proposal workflow are domain-agnostic. "What knowledge patterns correlate with good outcomes?" is a universal question.

5. **M6 (Real-Time Interface)**. A dashboard for browsing knowledge, viewing correction chains, and managing process proposals serves any domain.

6. **M9 (Build & Deploy)**. Single-binary distribution, Docker packaging, and CLI are universal.

7. **The milestone dependency structure**. M4 depends on M2 (correct). M5 depends on M4 (correct). The dependency chain is domain-independent.

### Estimated effort for the "unlock" changes

| Change | T-Shirt Size | Days | Dependency |
|--------|-------------|------|------------|
| Config externalization (vnc-004) | **S** | 1-2 | None (M2 complete) |
| SRE domain pack | **S** | <1 | vnc-004 |
| Product domain pack | **S** | <1 | vnc-004 |
| Research domain pack | **S** | <1 | vnc-004 |
| `KnowledgeEngine` facade | **M** | 3-5 | None (independent track) |
| `unimatrix-embed` standalone publish | **S** | 1-2 | Resolve ort pinning, anndists patch |
| Documentation for multi-domain | **M** | 2-3 | vnc-004 + at least one domain pack |

**Total unlock cost: 4-6 days of focused work** (vnc-004 + first domain pack + documentation). The `KnowledgeEngine` facade and `unimatrix-embed` publish are parallel tracks that add value but are not blockers.

---

## Part 6: Risks and Anti-Patterns

### Domains to explicitly avoid

**Healthcare/Clinical.** Report 02 scored this 5/10 on capability match with "Very High" GTM friction. The regulatory burden (FDA oversight for clinical decision support, HIPAA compliance, clinical validation requirements) is prohibitive. The domain requires validated content, not just a knowledge engine. General-purpose 384-d embeddings cannot capture biomedical semantic relationships. EHR integration (Epic, Cerner, MEDITECH) requires specialized health IT expertise. Do not pursue unless a healthcare partner approaches with a specific integration path and regulatory sponsorship.

**Education.** Report 02 scored this 5/10. The use case is too thin -- institutional knowledge management is a real need but does not justify domain investment. Education requires learner-facing features (adaptive paths, assessments, progress tracking) that are entirely outside Unimatrix's design. The e-learning market ($365B+) sounds large but is dominated by full LMS platforms that Unimatrix cannot compete with.

**General Enterprise Knowledge Management (head-on).** Report 02 explicitly warns against competing with Glean ($4.6B valuation), Guru (acquired by Dialpad), Confluence, or SharePoint on general enterprise KM. The integration requirements (SSO/SAML, SharePoint, Slack, Teams, ServiceNow) and collaboration features (shared editing, review workflows, approval chains) would consume years of development. The only viable enterprise angle is the air-gapped/regulated niche -- defense contractors, classified environments, security-conscious engineering orgs -- where local-first, zero-cloud-dependency is a requirement rather than a limitation.

**Government/Policy (direct).** Very High GTM friction (12-36 month procurement cycles, FedRAMP requirements, government sales expertise needed). The air-gap advantage is real but unreachable without specific government certifications and sales channels.

### The "second system effect" risk

The primary risk of multi-domain strategy is losing focus on the core product before it is proven. Report 04 is blunt: "The process intelligence capability is planned, not shipped. Until it exists, the 10x story is aspirational." Unimatrix's strongest differentiators -- correction chains with hash integrity, trust-tiered access, and process intelligence -- only matter if the product is production-deployed and battle-tested.

The second system trap looks like this: instead of shipping M4 (Learning & Drift) and M5 (Orchestration Engine), the team spends cycles on domain packs, SDK facades, and crates.io publishing. The core product stagnates. Competitors (Letta, Zep, Mem0) ship better learning features. Unimatrix ends up as a mediocre knowledge store in four domains instead of the best knowledge engine in one.

**Mitigation**: The config externalization (vnc-004) is the ONLY multi-domain work that should happen before M5 is complete. Domain packs are documentation, not code. The `KnowledgeEngine` facade is genuinely useful for the core product (it cleans up the library API). Everything else waits.

### What happens if we try to be everything to everyone

The product becomes a "generic knowledge platform" that competes with Notion, Confluence, and ChromaDB simultaneously while being worse than all of them at their respective jobs. The positioning becomes "it stores knowledge" -- which is not a differentiator. The development roadmap fractures across domain-specific feature requests (legal citation formats, medical terminology support, LIMS integration, LMS compatibility) that each serve a tiny slice of the market.

Report 04 identifies the correct positioning: "Agent memory systems remember. Unimatrix ensures what agents remember is trustworthy, correctable, and auditable." This positioning works across domains but anchors on a specific capability advantage (trust + lifecycle + integrity), not on being a generic platform.

### Recommended guardrails for scope management

1. **No domain-specific code in the kernel.** If a feature requires `if domain == "medical"` branching, it does not belong in the core engine. Domain-specific behavior is configuration, never code.

2. **Domain packs are documentation-weight.** A domain pack is 4 config files and optional seed data. If creating a domain pack requires writing Rust code, the configuration abstraction is insufficient -- fix the abstraction, do not add domain code.

3. **One domain at a time.** Do not launch three domain packs simultaneously. Ship the SRE pack first (strongest capability match, lowest GTM friction). Learn from adoption. Then ship the next.

4. **M5 before any domain pack ships publicly.** Process intelligence (retrospective pipeline, process proposals) is the "10x story" that makes Unimatrix more than a knowledge store. Without it, Unimatrix competes on features; with it, Unimatrix competes on capabilities.

5. **Track domain packs in separate feature tracks.** Each domain pack gets its own `ass-*` research spike to validate the specific opportunity before investment. Do not assume the opportunity analysis in report 02 is sufficient for a go/no-go decision.

6. **Kill domains fast.** If a domain pack does not see organic adoption within 3 months of launch, deprecate it and redirect the marketing effort. Do not invest in domain-specific features to "fix" adoption.

---

## Part 7: Recommendation

Unimatrix should pursue a **staged multi-domain strategy anchored on the agent-knowledge infrastructure category**, with domain expansion as a deliberate Phase 2 activity after the core product proves its differentiation in production.

### Phase 1 (Now through M4): Prepare the foundation, stay focused

**What to change on the current roadmap:**
- Insert `vnc-004` (config externalization) as a 1-2 day task immediately. This is the entire architectural unlock. It costs almost nothing and preserves optionality for every future domain decision.
- Begin the `KnowledgeEngine` facade as a parallel workstream during M4 development. It improves the core API surface regardless of multi-domain ambitions.
- Do NOT create domain packs, publish standalone crates, or write multi-domain documentation yet.

**What to ship:**
- vnc-004 (config externalization)
- M4 features (usage tracking, confidence evolution, contradiction detection, co-access boosting)

**Decision criteria for proceeding to Phase 2:**
- M4 is complete and merged
- At least 3 non-author users have used Unimatrix for real projects (not demos)
- The learning features (confidence evolution, contradiction detection) work as designed in production use
- Config externalization (vnc-004) is complete

### Phase 2 (After M4, before or alongside M5): First non-dev domain

**What to do:**
- Ship the SRE domain pack as the first non-dev configuration. SRE was chosen because it has the highest capability match (9/10), lowest GTM friction among high-match domains, and the clearest competitive gap.
- Write "operational knowledge engine" positioning content. Blog posts targeting the "incident postmortem knowledge gets lost" pain point.
- List the SRE-configured Unimatrix on MCP server registries.
- Publish `unimatrix-embed` as a standalone crate under a generic name. This is community building and ecosystem contribution, not a product launch.

**What NOT to do:**
- Do not build SRE-specific features (alert integration, incident timeline, PagerDuty integration). Unimatrix is a knowledge engine, not an incident management tool.
- Do not launch the Product or Research packs yet. Validate with one domain first.

**Decision criteria for proceeding to Phase 3:**
- At least one SRE team is using the SRE domain pack in production
- M5 (Orchestration Engine) is in progress or complete
- Feedback from SRE adoption has been incorporated
- No domain-specific code was required (confirming the configuration abstraction works)

### Phase 3 (After M5): Formalize multi-domain as a strategy

**What to do:**
- Ship the Product Management and Scientific Research domain packs.
- Formalize the domain pack specification as a documented interface.
- Consider community-contributed domain packs with a review process.
- Evaluate whether `KnowledgeEngine` facade has generated demand for Unimatrix-as-embedded-library.
- Begin planning M6 (Real-Time Interface) with multi-domain dashboard needs in mind.

**What NOT to do:**
- Do not pursue Healthcare, Education, General Enterprise KM, or Government/Policy as product targets.
- Do not build domain-specific features for any domain. The value proposition is the generic kernel with domain-appropriate configuration.

**Decision criteria for scaling multi-domain further:**
- Process intelligence (M5) is working and demonstrably domain-agnostic
- At least two non-dev domain packs have organic users
- Revenue or adoption metrics justify further domain investment
- No evidence of "second system effect" (core product development velocity has not declined)

### The bottom line

Unimatrix's core is already a multi-domain knowledge engine wearing a software-development costume. The costume is four configuration constants. Removing it costs 1-2 days of work. The strategic question is not "can we serve other domains?" -- it is "when do we tell other domains we exist?" The answer: after M4 proves the learning features work, and after M5 proves the process intelligence vision is real. Until then, prepare the foundation (vnc-004), stay focused on the core roadmap, and let the architecture's inherent generality be an insurance policy rather than a marketing campaign.

---

## Sources

This synthesis draws evidence and findings from the following research reports, all produced as part of ASS-009:

- **01-core-capabilities.md**: Capability taxonomy with portability scores (6 HIGH, 6 MEDIUM), portable kernel layered architecture, EntryRecord domain-agnosticism assessment.
- **02-market-opportunities.md**: 10-domain opportunity scan with match scores, GTM friction ratings, competitive landscape per domain, market size data. AI-driven KM market: $7.71B (2025), 47.2% CAGR. MCP ecosystem: 8M+ downloads, 5,800+ servers. ELN-AI market: $1.88B (2025).
- **03-architecture-decomposition.md**: Dependency graph analysis, 5 packaging options assessed, EntryRecord field-by-field domain-agnosticism audit, Option D (Protocol-First) identified as highest ROI at 1-2 days effort.
- **04-competitive-landscape.md**: 6-category competitive analysis. Key competitors: Letta (HIGH threat), Zep (MEDIUM-HIGH), Mem0 (MEDIUM). Blue ocean assessment: auditable knowledge lifecycle + process intelligence + self-contained embedded binary + MCP = no direct competition. Competitive positioning matrix across 18 capability dimensions.
- **PRODUCT-VISION.md**: 9-milestone roadmap (M1-M2 complete, M4 next), milestone dependency graph, security cross-cutting concerns, phase-to-proposal mapping.
