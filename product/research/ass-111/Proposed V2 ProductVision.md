# Unimatrix — Product Vision

---

## Vision
Global changes desired:  
- SaaS focus. Eliminate local deployment model.  Must retain guarantees of isolation.
    - This reduces some deployment complexity we currently have, and adds a necessary focus on scalability, as well as the potential other structural search impacts.
    - Raises questions of proper database platform and technology architecture
    - V1 -2 Migration
    - Easy client boarding
- Client expansion:  Enable implications of supporting autonomous agents (GH Actions), or potential chat application connection.
    - increase roles/security
- Harness Support:  We wanted to support more harnesses.  With this, IF an open harness like opencode can help us fulfill our dream of dramatically increased security, and coordination across multiple project scopes... I'm willing to forego harness ubiquity for deeper integration and functionality.  

Other targeted improvements
- Easier onboarding/configuration.  (Currently domain packs, kind of exist, but all config is edited manually in toml files)
- Add support for Shared, that 1 or more clients can access in addition to their own scope.  (Supports enterprise shared 'decisions')
- Approved agent self discovery of supported categories & edges for that repository. (currently only in config)
- Bring your own edge definitions.  
- Support for Oauth, in addition to existing connectivity forms.  Since SaaS, focus is on HTTPS transport


Unimatrix is a workflow-aware, self-learning knowledge engine built for agentic workflows. It makes knowledge curation a first-class activity in the workflow itself — not a side effect. Agents search, store, and correct knowledge entries as a normal part of doing work: decisions get attributed, lessons get captured, patterns get refined. Unimatrix makes that knowledge trustworthy, consistent, and — as it learns from actual usage — continuously more relevant.
 - Whats changes:  automated agentic connections, chat interactions from known applications(thru mcp)

Two surfaces, both driven by the same engine: agents retrieve knowledge on demand (search, lookup, get), and Unimatrix delivers it proactively — phase-conditioned injections and briefings that surface what matters before agents need to ask. The combination of explicit curation and self-improving delivery is what makes it distinct.
 - Whats changed: UnimatixV2 also enables autonomous automated agents (Github Actions or other task oriented) that may or may not be directly connected to the workflow, however can read and write information relevant to the project.  

Unimatrix is not an orchestration engine. It does not coordinate agents, schedule work, or manage workflows. It is a knowledge engine that understands workflow context — your current phase, what your team has been doing, what comes next — and uses that understanding to surface relevant knowledge at exactly the right moment.
- What may change: 
 - Is there a need to enable/expand "Workflow Session Memory" - more transient memory for better state management, or additional data across independent agents?  This class of data would not be stored with knowledge... completely transient is the expectation... retained until workflow session has completed.

The key mental model: workflow definitions, agent definitions, and skill definitions are static (the Harness) — they live in your tooling and change infrequently. Architecture decisions, patterns, and lessons-learned are dynamic — they evolve with every feature, every delivery, every failure. Unimatrix was designed to manage the dynamic layer.
- What's changes:  An evaluation to determine if a shared model that contains protocol definitions, agent definitions, skill definitions could be stored in a shared section that to work more closely with a harness

Built for agentic software delivery. Configurable for any workflow-centric domain.
 - Whats changes:  Expand horizons to more use cases.  an interesting one is med-search:
    - Med Search:  This is a medical research aggregation/consolidation and creates new hypothesis and publishes to a web page.  2 ideas come out of this:  
        - This is much more research, contradiction, retractions, etc happen and will be updated regularly... Optionally leveraging NLI utiliies/etc may become intersing.  They've also asked for an option to turn learning off... so same query gets same results??  
        - Enabling a chat interface on the web page so people could ask it questions via MCP.  But we don't want those questions to impact the same access patterns.

**Unimatrix is a platform.** A self-learning knowledge engine with a stable, presented extension contract. Domain solutions — a delivery harness, a research harness, a monitoring stack — are built *on* it, not *into* it: they configure its domain layer, plug their own policy (identity, cost, gates) into its seams, and consume its memory over MCP. The engine is the platform; each solution is an extension along clean, delineated lines. The developer experience of that extension surface is a first-class concern — the seams are documented, versioned, and presented as a product, not left implicit.

---

## Story

Unimatrix began in agentic software delivery, where the problem was specific: AI agents forget, contradict each other, and confidently repeat mistakes. We built a knowledge engine where nothing is merely stored — everything is attributed, hash-chained for integrity, scored by real usage, and correctable with full provenance. Agents stopped relitigating decisions. Knowledge started improving with every delivery.

That foundation became a platform. A typed knowledge graph formalizes relationships — not just what agents retrieve together, but why: support, contradiction, supersession, dependency. A confidence system learns from actual usage rather than manual calibration. Any event source — hooks, webhooks, automated pipelines — feeds the learning layer without agent cooperation. Any knowledge-intensive domain runs on the same engine, configured not rebuilt.

The intelligence pipeline is the core. It is a session-conditioned, self-improving relevance function: given what the agent knows, what they have been doing, and where they are in their workflow, surface the right knowledge — before they ask for it. The graph, the confidence system, the observation pipeline, and the scoring function are all inputs. The function learns. Every session makes it better.

---

## Strategic Goals

Seven strategic goals drive all roadmap decisions. Each is maintained as an enriched entry in Unimatrix — query `context_lookup(category="goal")` for current content.

| Goal | Tag | Summary |
|------|-----|---------|
| Self-learning intelligence | `self-learning` | Every deployment improves retrieval quality from actual usage — no manual tuning |
| Knowledge integrity | `integrity` | Every entry attributed, tamper-evident, and current — accurate data, never stale, corrupted, or contradictory |
| Proactive knowledge delivery | `proactive-delivery` | Knowledge arrives before agents search for it — phase-conditioned, session-aware |
| Developer-friendly deployment | `personal-cloud` | One container, one bearer token, one command — full pipeline fidelity over HTTPS |
| Domain-agnostic platform | `domain-agnostic` | Any domain, configured not rebuilt — validated on SDLC and research workflows |
| Extensible platform surface | `platform` | The extension contract (domain config, policy/auth seams, memory+capability substrate over MCP/SDK) is presented, versioned, and DevX-first — solutions extend it, never reach into it |
| Trusted agent identity *(emerging)* | `trusted-identity` | Capability enforcement bound to a non-forgeable, harness-attested agent identity — real controls, not self-asserted `agent_id` |

Query current goal content: `context_lookup(category="goal", tags=["goal", "{tag}"])`

Feature delivery is tracked via GitHub Issues with `goal:*` labels:
- `goal:self-learning` — intelligence pipeline, GNN, learning signals
- `goal:integrity` — hash chain, append-only audit, read-currency, contradiction management, poison-prevention
- `goal:proactive-delivery` — injection, briefing, session context
- `goal:personal-cloud` — container, HTTPS, auth, multi-LLM
- `goal:domain-agnostic` — config externalization, domain packs, multi-retrieval
- `goal:platform` — extension contract, policy/auth seams, plugin DevX, presented+versioned seams
- `goal:trusted-identity` — harness-attested identity, PL-4 fail-closed enforcement, per-agent-type capability policy *(emerging — done-when pending discovery)*

---

## Architectural Principles

Non-negotiable across all work:

1. **Hash chain integrity is immutable.** `content_hash` and `previous_hash` on every entry — never skipped, backdated, or made optional.

2. **Audit log is append-only and complete.** Every state change produces an AUDIT_LOG entry with full attribution. Enforced by DDL triggers.

3. **Capability checks at the service layer.** Whether the caller arrives via UDS, stdio, HTTP, or OAuth — capability checks happen after identity resolution. Transport auth is a precondition, not a substitute.

4. **Typed relationship graph.** Knowledge relationships are explicit, typed, and traversable. Agents declare edges at write time. Graph traversal surfaces what vector search alone cannot.

5. **Graceful degradation.** Every ML capability (NLI, GNN, GGUF) has a defined fallback. Absent or failed model = previous behavior, not broken behavior.

6. **Single binary server, zero required infrastructure.** The client is an adapter — JS/TS for hooks, the binary for MCP — not infrastructure. Container is optional. Daemon + UDS works without it.

7. **In-memory hot path.** All analytics-derived search data cached in `Arc<RwLock<_>>`, rebuilt by tick. Never read from the database at query time.

8. **No secrets in any database.** OAuth client secrets, API keys, TLS private keys never stored in knowledge.db or analytics.db.

---

## Domain Validation

| Domain | Status | Evidence |
|--------|--------|----------|
| Agentic software delivery | Validated | Waves 0–1B, ASS-040 Groups 1–10 |
| Autonomous research | Validated | ASS-057 — graph-first retrieval orthogonal to vector search |
| Environmental monitoring | Designed for | Config externalization, domain pack registration |
| SRE, legal, compliance | Designed for | Same engine — no domain-specific code required |

---

## Roadmap

Feature delivery is tracked via GitHub Issues with `goal:*` labels. Query any goal's current state:

```bash
gh issue list --label "goal:personal-cloud" --state all --json number,title,state
```

Research spike scopes and findings live in `product/research/ass-NNN/`. Historical roadmap detail is in `product/research/ass-040/ROADMAP.md` (intelligence pipeline, reference only).
