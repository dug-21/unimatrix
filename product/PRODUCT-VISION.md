# Unimatrix — Product Vision

---

## Vision

Unimatrix is a workflow-aware, self-learning knowledge engine built for agentic workflows. It makes knowledge curation a first-class activity in the workflow itself — not a side effect. Agents search, store, and correct knowledge entries as a normal part of doing work: decisions get attributed, lessons get captured, patterns get refined. Unimatrix makes that knowledge trustworthy, consistent, and — as it learns from actual usage — continuously more relevant.

Two surfaces, both driven by the same engine: agents retrieve knowledge on demand (search, lookup, get), and Unimatrix delivers it proactively — phase-conditioned injections and briefings that surface what matters before agents need to ask. The combination of explicit curation and self-improving delivery is what makes it distinct.

Unimatrix is not an orchestration engine. It does not coordinate agents, schedule work, or manage workflows. It is a knowledge engine that understands workflow context — your current phase, what your team has been doing, what comes next — and uses that understanding to surface relevant knowledge at exactly the right moment.

The key mental model: workflow definitions, agent definitions, and skill definitions are static — they live in your tooling and change infrequently. Architecture decisions, patterns, and lessons-learned are dynamic — they evolve with every feature, every delivery, every failure. Unimatrix was designed to manage the dynamic layer.

Built for agentic software delivery. Configurable for any workflow-centric domain.

---

## Story

Unimatrix began in agentic software delivery, where the problem was specific: AI agents forget, contradict each other, and confidently repeat mistakes. We built a knowledge engine where nothing is merely stored — everything is attributed, hash-chained for integrity, scored by real usage, and correctable with full provenance. Agents stopped relitigating decisions. Knowledge started improving with every delivery.

That foundation became a platform. A typed knowledge graph formalizes relationships — not just what agents retrieve together, but why: support, contradiction, supersession, dependency. A confidence system learns from actual usage rather than manual calibration. Any event source — hooks, webhooks, automated pipelines — feeds the learning layer without agent cooperation. Any knowledge-intensive domain runs on the same engine, configured not rebuilt.

The intelligence pipeline is the core. It is a session-conditioned, self-improving relevance function: given what the agent knows, what they have been doing, and where they are in their workflow, surface the right knowledge — before they ask for it. The graph, the confidence system, the observation pipeline, and the scoring function are all inputs. The function learns. Every session makes it better.

---

## Strategic Goals

Four strategic goals drive all roadmap decisions. Each is maintained as an enriched entry in Unimatrix — query `context_lookup(category="goal")` for current content.

| Goal | Tag | Summary |
|------|-----|---------|
| Self-learning intelligence | `self-learning` | Every deployment improves retrieval quality from actual usage — no manual tuning |
| Proactive knowledge delivery | `proactive-delivery` | Knowledge arrives before agents search for it — phase-conditioned, session-aware |
| Developer-friendly deployment | `personal-cloud` | One container, one bearer token, one command — full pipeline fidelity over HTTPS |
| Domain-agnostic platform | `domain-agnostic` | Any domain, configured not rebuilt — validated on SDLC and research workflows |

Query current goal content: `context_lookup(category="goal", tags=["goal", "{tag}"])`

Feature delivery is tracked via GitHub Issues with `goal:*` labels:
- `goal:self-learning` — intelligence pipeline, GNN, learning signals
- `goal:proactive-delivery` — injection, briefing, session context
- `goal:personal-cloud` — container, HTTPS, auth, multi-LLM
- `goal:domain-agnostic` — config externalization, domain packs, multi-retrieval

---

## Architectural Principles

Non-negotiable across all work:

1. **Hash chain integrity is immutable.** `content_hash` and `previous_hash` on every entry — never skipped, backdated, or made optional.

2. **Audit log is append-only and complete.** Every state change produces an AUDIT_LOG entry with full attribution. Enforced by DDL triggers.

3. **Capability checks at the service layer.** Whether the caller arrives via UDS, stdio, HTTP, or OAuth — capability checks happen after identity resolution. Transport auth is a precondition, not a substitute.

4. **Typed relationship graph.** Knowledge relationships are explicit, typed, and traversable. Agents declare edges at write time. Graph traversal surfaces what vector search alone cannot.

5. **Graceful degradation.** Every ML capability (NLI, GNN, GGUF) has a defined fallback. Absent or failed model = previous behavior, not broken behavior.

6. **Single binary, zero required infrastructure.** Container is optional. Daemon + UDS works without it.

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
