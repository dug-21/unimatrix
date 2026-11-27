# FINDINGS: ASS-111 S5 — Engine boundary: session memory & harness definitions

**Spike**: ass-111 (S5)
**Date**: 2026-11-26
**Approach**: evaluation
**Confidence**: directional
**Agent**: researcher-ass-111-s5
**Unimatrix entries consulted**: #5690, #5689, #5682, #5684, #5732, #190, #5700, #5526, #5793, #5537, #4580 (read in full); #5796, #5557, #5705, #5628, #5544 (surfaced by briefing/search, summary only). No writes.

---

## Verdicts at a glance

| Item | Verdict (V2 initial) | One-line boundary rule |
|---|---|---|
| Transient workflow-session memory (new memory class) | **No-go** | If losing it mid-cycle would break a workflow, it is orchestration state and stays outside Unimatrix. |
| Cycle-scoped *provisional knowledge* (what most "session memory" content actually is) | **Go (small)** | Stored as normal hash-chained entries with `proposed` status + cycle attribution; disposition at cycle close. |
| Harness definitions stored as knowledge / in a shared scope | **No-go** | Unimatrix does not store, distribute, or serve protocols, agents, or skills. Harness-native channels do that. |
| Harness artifact *references* on knowledge (name@version / digest) | **Go (small)** | Living knowledge may cite a released harness artifact. Unimatrix never holds the artifact. |
| Tenant-facing configuration surface (categories, edges, domain packs, instructions) | **Go (required by SaaS)** | Config is versioned, validated, audited, admin-applied engine contract. It is not knowledge. |
| Workflow-aware AND non-workflow coexistence | **Go: additive-layer model** | Without declared workflow context, the core engine behaves exactly as it does today. Workflow features only add to that. |

---

## Findings

### Q: "Is there a real need for transient workflow-session memory (state shared across independent agents, retained until the workflow session ends)? What would it hold that knowledge entries and existing session context do not?"

**Answer**: No, not as a separate memory class. Candidate content falls into three buckets, and none of them needs a new transient store inside Unimatrix:

| Candidate content (observed in today's harness) | What it really is | Where it belongs |
|---|---|---|
| Task status, who is doing what, stalls, resume points, handoff messages, gate pass/fail driving the next step (`STALLS.md`, SendMessage, SM wave tracking) | **Coordination state.** Another agent's control flow depends on it. | Harness/coordinator: SendMessage, files on the branch, GH issues, the runtime's own session state. |
| In-flight decisions, provisional findings, open questions (`DECISIONS.md` rows, agent reports in `product/features/{id}/agents/`) | **Provisional knowledge.** It may turn into durable knowledge or be thrown away. | Unimatrix, as entries with `proposed` status attributed to the cycle (see "Go (small)" below). |
| What the cycle is doing now: goal, phase, which sessions belong to it, what was injected, transcript activity | **Workflow context.** It is observed, not authored. | Already in Unimatrix: `cycle_events`, `sessions`, `SessionState` (phase, goal, injection history, category histogram), transcript buffers and hold, `context_cycle_review`. |

**Evidence**:
- *Existing session context already spans independent agents within a cycle.* `context_cycle(start)` persists goal and phase to `cycle_events`. Sessions are attributed to a cycle (`SessionState.feature` + `feature_source` precedence, vnc-030). Transcript buffers are held per `feature_cycle` with a TTL (`transcript_hold`, crt-052, with `sweep_expired(ttl)` on the maintenance tick). `context_cycle_review` aggregates across all sessions in the cycle. In effect a "workflow session" already exists: the feature cycle. What it lacks is a slot for agent-*authored* free-form working state, and every example of that content is either coordination or provisional knowledge.
- *The real cross-session handoff today* is `DECISIONS.md`. Per `uni-primary-authority.md` it is written in design and read in a fresh delivery session. It is deliberately durable, readable by the human, and tracked in git, and its rows are decisions, which is knowledge. Making it transient would lose that property.
- *A provisional lifecycle slot already exists but nothing uses it.* `Status::Proposed = 2` exists in `unimatrix-store/src/schema.rs` and is parsed in `validation.rs`, but `StoreParams` (`mcp/tools.rs:235`) has no status field, so agents cannot write it.
- *Industry:* every comparable that ships transient session state is an **agent runtime that owns the session lifecycle**: Google ADK session `state` with `temp:`/session/`user:`/`app:` prefixes; the LangGraph checkpointer (thread-scoped) vs Store (namespaced, long-term); AgentCore short-term events (expiry 7–365 days) vs long-term records extracted by strategies; Letta shared memory blocks; Claude Managed Agents memory stores attached at session creation. Pure memory layers (Mem0) do **not** add a transient class. `run_id` is just another partition of the same store, cleared explicitly when the run closes. AgentCore's pattern, where raw short-term events are distilled into long-term memory, already matches Unimatrix's observations → cycle review → stored lessons path. Short-term working state is therefore a runtime concern, and adding it would move Unimatrix toward being the runtime.
- *ass-063 litmus (RQ-7):* "if a feature requires Unimatrix to initiate an action … it has crossed the line". A shared scratchpad passes that test literally, but it becomes a blackboard coordination substrate the moment agents rely on it to sequence work. That reliance is the actual crossing point, which is why the loss test below exists.

**Recommendation**:
1. **No-go** on a transient workflow-session memory class for V2 initial.
2. **Go (small)** on cycle-scoped provisional knowledge. Let `context_store` accept `status: "proposed"` with a required `feature_cycle`. Proposed entries are hash-chained and audited like any other entry (Principles 1 and 2 hold). They are excluded from retrieval and learning outside their own cycle. `context_cycle_review` (or `stop`) lists them for disposition: promote to `active` or deprecate. This covers `DECISIONS.md`-class content and keeps the full integrity trail.
3. Re-open trigger: a concrete content example that fails both buckets. The most likely source is S6's "coordination across project scopes", for example a cross-machine handoff between an OpenCode session and a GitHub Actions agent that do not share a working tree. If that happens, build only under the Q2 rules.

---

### Q: "If built, what constraints keep it on the knowledge-engine side of 'not an orchestration engine' (TTL, scope, no coordination semantics, never a learning signal)?"

**Answer**: These rules must all hold together. Breaking any one of them turns the feature into orchestration state or a leak path.

| # | Rule | Rationale / precedent |
|---|---|---|
| R1 | **Loss test.** If the store were silently wiped mid-cycle, every workflow still completes, just less informed. Unimatrix is never the authoritative holder of state some agent's control flow depends on. | Core boundary test. Makes the store best-effort by definition, so it is safe on in-memory, tenant-pinned workers (S2). |
| R2 | **Scope = (tenant, project, cycle).** A note can only be written into a declared cycle (no cycle, no store). Never cross-project. Never placed in a shared scope (S3). | Hard tenant isolation. The cycle is the existing workflow-session identity. |
| R3 | **Lifetime.** Purged at `context_cycle(stop)` plus a short grace period. A hard maximum TTL applies even if the cycle never stops, swept on the maintenance tick. Purges are audited as counts only. | Same pattern as `transcript_hold_ttl_secs` / `sweep_expired` / `TranscriptPurgeRecord`. |
| R4 | **No coordination semantics.** Append and read only. No compare-and-swap, locks, leases, claims, assignee fields, status enums, queues, blocking or long-poll reads, subscriptions or change notifications, and no ordering guarantee beyond the write timestamp. Unimatrix never interprets note content. | Each listed primitive is what makes a blackboard into a coordinator. |
| R5 | **Never a learning signal.** Not written to `query_log`, `co_access`, usage, confidence, `PhaseFreqTable`, graph enrichment, or NLI. Not embedded into the knowledge vector index. | Hard learning-isolation constraint. Prevents transient chatter from biasing ranking. |
| R6 | **Never delivered proactively.** Returned only by an explicit read call from a member of the same cycle. Never injected via hooks, briefing, or search. | Keeps proactive delivery a knowledge surface, not a message bus. |
| R7 | **Not knowledge.** No hash chain, confidence, edges, or export as knowledge. Promotion only happens by an agent explicitly calling `context_store`, which creates a new entry whose provenance cites the cycle. | The integrity chain is reserved for knowledge. The promotion path makes the boundary explicit. |
| R8 | **Security.** Content-scanned like `context_store` (Principle 8: agents paste tokens). Per-note and per-cycle byte caps. Write requires a capability check after identity resolution (Principle 3). Writes are audited with attribution, content excluded (Principle 2). Read-mode tokens (vnc-050) cannot write. | Campaign Hard constraints. |
| R9 | **Degradation.** If the store is unavailable, reads return empty with an explicit "unavailable" marker, never an error that blocks the caller. | Principle 5 applied by analogy. Follows from R1. |

**Evidence**: Precedents in the code are the transcript buffer contract ("response-transient", "never written onto `RetrospectiveReport`", counts-only purge records; `distill_handler.rs` header, `session.rs` `HeldBufferScan`), the `phase IS NOT NULL` learning filters (`query_log.rs:252,299,338`), and the vnc-050 capability ceiling (`cap_ceiling.rs`). On the industry side, ADK `temp:` (discarded after the invocation) and AgentCore event expiry show TTL-bound short-term memory working as a pattern, but in both cases inside a runtime.

**Recommendation**: Adopt R1–R9 as the binding constraint set if the feature is ever re-opened. Put R1 (loss test) and R4 (no coordination primitives) into the vision text as the general engine-boundary test, not only for this feature.

---

### Q: "Should harness definitions (protocols, agent, skill definitions) be stored in a shared scope to integrate more closely with a harness? Distinguish distribution/versioning of artifacts from retrieval-as-knowledge. What breaks in the static/dynamic mental model?"

**Answer**: No. The two concerns need different machinery:

| Concern | What it needs | Fit with the Unimatrix knowledge store |
|---|---|---|
| **Distribution/versioning of artifacts** | Atomic multi-file bundles, byte-exact fidelity, semver plus content digest, pinning and rollback, install into harness-specific paths, a trust and signing story | **Poor.** Entries are corrected and superseded one at a time, confidence-scored, ranked by usage, and returned as ranked snippets. A protocol is one atomic unit whose steps cannot be superseded individually (ass-063 out-of-scope discovery #7, FINDINGS-VISION §5). Confidence decay or reranking of a protocol is meaningless. |
| **Retrieval-as-knowledge** | Finding the relevant protocol step or agent at the right moment | **Low value.** ass-063 RQ-3 measured total savings of 2–6% of context. Harnesses already do progressive disclosure natively: Agent Skills loads metadata first and the body on demand, in Claude Code, Codex, and OpenCode. |
| **Knowledge *about* the harness** (lessons, procedures, conventions on how a protocol behaves) | Living, correctable, learned | **Already served today** (e.g. #4580 procedure, retro lessons). Keep. |

The distribution channel already exists in several forms, none of them Unimatrix. The npm package bundles `protocols/` and `skills/` at release time (#4580 step 7a/7b). `.collective/` is a digest-pinned baseline release surface whose `MANIFEST.yaml` carries per-file sha256 values and which ships a `SKILL.md`. Industry channels include Claude Code plugin marketplaces (bundling skills, agents, hooks, and MCP, with org-curated distribution on Team/Enterprise), the Agent Skills open standard (Agentic AI Foundation, adopted by OpenCode, Codex, Copilot, and others), OpenCode remote config via `.well-known/opencode` (organization defaults, lowest precedence), and Langfuse-style prompt registries (versions plus labels, client-side cache). The registries that do runtime fetch (Langfuse) are *prompt management products*, which would be a second product for a solo operator.

**What breaks in the static/dynamic mental model**:
1. **"Static" really means *released*, not *rarely changed*.** 13 of 59 commits since 2026-08-26 (about 22%) touched `.claude/{protocols,agents,skills}`, in retro-driven bursts. These files change by deliberate release, not by usage. The partition that holds is **released artifacts vs living knowledge**, not slow vs fast.
2. **"They live in your tooling" breaks under SaaS, multiple machines, and multiple harnesses.** Definitions are fetched from a distribution channel. The channel is the harness's (marketplace, skills, `.well-known`, npm, `.collective`), not Unimatrix's.
3. **The model is missing a link from living knowledge to released artifacts.** A lesson that indicts "delivery protocol Stage 3b" has no machine-readable pointer to *which version* of the protocol it applies to. Adding artifact references (`harness_artifact: uni-delivery-protocol@0.3.0` / digest) on entries, plus the harness release recorded at `context_cycle(start)`, lets Unimatrix *answer* "which lessons since protocol v3 target Stage 3b?" without hosting the protocol. That closes the protocol-improvement loop while staying inside the ass-063 "answers questions" boundary.
4. **The model leaves out a third layer: engine configuration** (categories, edge vocabulary, domain packs, server instructions). It is neither harness nor knowledge, and Unimatrix legitimately owns it (platform goal L1, #5689). See Q4.
5. **Shared scope was carrying two different meanings.** "Enterprise shared decisions" is knowledge and belongs in S3's shared scope. "Enterprise shared protocols" is artifact distribution and belongs in org plugin marketplaces or skill registries. Merging them would force S3 to carry artifact semantics (atomic bundles, pinning) it should not have.

**Recommendation**:
- **No-go**: do not store harness definitions as entries, in a shared scope, or in a Unimatrix-hosted artifact registry in V2 initial.
- **Go (small)**: add optional harness-artifact references to entries, and a `harness_release` field to `context_cycle(start)`, so living knowledge can target released artifacts by version.
- Keep storing knowledge *about* the harness (lessons, procedures) as today.
- Restate the mental model as three layers (draft text in Q5).

---

### Q: "Should onboarding/configuration (domain packs, categories, edges today hand-edited in TOML) be a tenant-facing configuration surface, and how does it relate to DA1/DA10/PL-2?"

**Answer**: Yes. SaaS forces this. Today config is loaded once at boot and cannot change until restart, from `config.toml` plus a per-slug overlay file (`main.rs:1328` `resolve_slug_config`; the seed comment in `config.rs:4641` says "on the next restart (no hot-reload)"). `ProjectConfigEntry` is deliberately slug-only. A tenant cannot edit server files, so the file-plus-restart model stops working the moment the server is hosted.

**Design: the Project Configuration Document (PCD)**

| # | Rule |
|---|---|
| C1 | **Config is not knowledge.** It is not stored as entries, not ranked, not learned, and not in a shared scope. It configures the *engine* (L1), never the *harness*. This is the line that separates the config go from the harness-definitions no-go. |
| C2 | **Versioned and audited.** Each applied PCD version is immutable and numbered. Every apply writes an audit entry with attribution and a diff hash (Principle 2). Rollback means re-applying a prior version. |
| C3 | **Validated and atomic.** An apply runs the same validator as startup (DA2: config is a validated trust boundary) and swaps the project's in-memory active config atomically. DA1 design-lock (1) is preserved but restated: in-memory active config is authoritative, **immutable until the next apply** instead of until restart. |
| C4 | **Contract.** The PCD carries a contract version, and each key carries a stability tier (PL-2 done-when 1–3). Confidence and inference knobs are tier "experimental". Categories, edges, and instructions are tier "stable". |
| C5 | **No secrets** (Principle 8). The schema has no credential-bearing keys, and the validator rejects secret-shaped values. Credentials are referenced by handle to a secret manager. |
| C6 | **Authority.** Only the tenant/project admin role (S1 role model) applies config. Agents get *read* access through DA1 discovery. Agent-originated taxonomy changes are at most *proposals* that an admin applies (see Escalations). |
| C7 | **Domain packs become versioned templates.** Unimatrix publishes `sdlc`, `research`, and `med-search` packs (categories + descriptions, edge vocabulary, server instructions, observation rules). A project instantiates one at onboarding (copy-on-apply). After that the project owns its PCD. This is the main lever for lower-friction onboarding (feeds S1 Q5). |
| C8 | **Removal safety.** Removing a category or edge type that existing entries use is rejected unless an explicit migration is supplied (DA7: never regress). Engine-intrinsic sets (built-in relation types with PPR semantics) cannot be configured away. |
| C9 | **Config-as-code stays.** TOML remains the import/export format, so a solo operator or a GitHub Actions job can push config from the repo through the API. The API is authoritative and the file is a serialization of it. |

**How the existing capabilities map onto it**:
- **DA1** (#5526, missing) is the *read* side. Agents discover the active categories and relation types at runtime. In SaaS this is per project, and the server instructions are rendered per project.
- **DA10** (#5793, #1011 open) is one *key* in the PCD: project-declared edge vocabulary plus allow-list enforcement at write time.
- **PL-2** (#5700, missing) is the *contract wrapper*: version plus stability tiers plus a regression test on breaking shape changes.
- **DA2 / DA7** provide the validator and the no-regression guard.
- The domain-agnostic goal's statement "one instance serves one domain" (#5732) becomes **"one project, one domain"** in V2.

**Recommendation**: **Go.** Make the PCD the V2 L1 surface: an API-applied, versioned, validated, audited per-project document with domain-pack templates. Sequence PL-2 first (contract version, which is also the claim floor), then DA1 (read), then DA10 (edges), then templates. Per-project hot-apply interacts with Principle 7 (per-tenant rebuild of the allowlist and hot-path caches). S2 owns that cost.

---

### Q: "Unimatrix stays workflow-aware AND serves non-workflow knowledge … How do the two coexist: which engine features (phase conditioning, proactive delivery, session signals) apply to non-workflow knowledge and clients, what degrades gracefully, and how is the vision restated so workflow-awareness remains an explicit, named differentiator — additive, never diluted?"

**Answer**: Use a two-layer model. The **core engine** is workflow-independent: store, integrity, graph, search, contradiction and quarantine, config. The **workflow layer** is activated only by *declared* workflow context. When no context is declared, behavior is exactly the core engine's. The code already mostly works this way. The workflow scoring terms are additive and sit outside the normalized six-weight sum (`search.rs:107–122, 237–239`). `current_phase = None` means the phase lock is never taken and the term is 0.0 (`search.rs:878–885`). The phase-conditioned learning tables filter `phase IS NOT NULL` (`query_log.rs`). Briefing already runs without phase (`index_briefing.rs:188`).

**Feature coexistence matrix**

| Engine feature | Needs | Workflow client (harness + cycle) | Non-workflow client: pipeline (Med Search ingest), chat app, public chat | Degradation when absent |
|---|---|---|---|---|
| Store / correct / deprecate / quarantine, hash chain, audit | — | full | full (read-mode tokens cannot write) | n/a, core |
| Search / lookup / get, PPR graph expansion, NLI contradiction | — | full | full | n/a, core |
| Phase-explicit term (`PhaseFreqTable`) | declared phase | active | inactive | term = 0.0, plain ranking |
| Phase histogram (session category counts) | session with stores | active | inactive unless the client stores in a session | 0.0 |
| Goal-cluster boost | cycle goal | active | inactive | no boost |
| Briefing | task string | goal-conditioned | works as a "task → ranked index" | loses only the goal boost |
| Hook injection, PreCompact restore | harness hooks + session | active | not available; explicit retrieval instead | explicit search |
| Session signals (rework, implicit helpfulness, co-access) | session | active | per client class (S4); public chat = none | no signal |
| Phase-conditioned learning | phase | feeds | excluded by `phase IS NOT NULL` | n/a |
| Work attribution (SL1), cycle review, retrospective | cycle | active | none, or opt-in (W4) | n/a |

**Coexistence rules**
- **W1 Declared, never inferred, for non-workflow classes.** The col-017 topic-signal majority vote that infers a feature must not run for chat or public client classes. Otherwise a public question could be attributed to a cycle.
- **W2 Additivity invariant.** With no workflow context, results equal core ranking. Extend PDN1 (#5537, "never worse than plain search", still unproven) into a *parity* assertion that applies to all non-workflow clients. Under deterministic mode (S4) the assertion becomes "the same query gives the same results".
- **W3 Workflow-derived signals learn only from workflow traffic.** Phase tables already comply. Signals that are not phase-filtered (co-access, usage, helpfulness, confidence) are *not* workflow-filtered today, so non-workflow traffic feeds them unless the client uses read mode. S4 owns per-class isolation. S5 states the boundary.
- **W4 "Workflow" is not SDLC-only.** Any client may declare a cycle. For example, a Med Search ingest run with phases `fetch → extract → reconcile → publish` gets attribution, phase-conditioned retrieval, and a retrospective. This is the differentiator growing into new domains, not being diluted.
- **W5 Non-workflow knowledge is first-class.** Entries without cycle attribution are not penalized in ranking, and retention and integrity are identical.

**Draft vision text** (for S7 and the human. Do **not** apply to PRODUCT-VISION.md.)

> **Vision (lead paragraph).** Unimatrix is a workflow-aware, self-learning knowledge engine. It retains, curates, and retrieves trustworthy knowledge for any client — agents in a delivery workflow, autonomous pipelines, chat applications. When a client declares its workflow context — the cycle, phase, and goal it is working in — Unimatrix uses that context to deliver the right knowledge before it is asked for. **Workflow-awareness is the differentiator, and it is additive:** knowledge retained and retrieved outside any workflow gets the full integrity and retrieval engine and never pays for, or is distorted by, workflow features it does not use.
>
> **Boundary.** Unimatrix is not an orchestration engine. It does not coordinate agents, schedule work, hold task state, or manage workflows. It answers questions; it never initiates actions, and no workflow depends on state only Unimatrix holds. It understands workflow context in order to surface knowledge, not to control the work.
>
> **Key mental model — three layers.**
> - *Harness definitions* — workflows and protocols, agents, skills — are **released artifacts**: versioned, deliberately changed, distributed by your harness's own channels (plugins, skills, packages). Unimatrix references them by version; it does not store, distribute, or run them.
> - *Living knowledge* — decisions, patterns, lessons, findings — **evolves with every cycle**. Unimatrix was designed to manage this layer: attributed, hash-chained, correctable, and continuously re-ranked by use.
> - *Engine configuration* — categories, relationship vocabulary, domain packs — is **each project's contract with the engine**: versioned, validated, and managed through Unimatrix.

Proposed principle additions (draft):
- **P-new-a. Workflow-awareness is additive.** Absent declared workflow context, every surface returns core-engine behavior. Workflow features add signal and never remove it.
- **P-new-b. Answers, never initiates.** Unimatrix never initiates an action and never holds state whose loss would break a workflow (the loss test).

**Recommendation**: Adopt the two-layer model with rules W1–W5. Make PDN1 parity the provable guard for "additive". Carry the draft text into S7 as the vision restatement. It names workflow-awareness in the first sentence, states additivity explicitly, and retains the orchestration boundary with a testable criterion.

---

## Escalations for human

1. **Session memory no-go (H6, first half).** The draft vision expects "completely transient … retained until workflow session has completed". This spike found no content that is neither coordination state nor provisional knowledge. Confirm the no-go for V2 initial, or supply a concrete example of the content you had in mind. One concrete example is enough to re-open under R1–R9.
2. **Harness definitions no-go (H6, second half).** Confirm that V2 does not host or distribute protocols, agents, or skills, and relies on harness-native channels plus `.collective`. This interacts with S6/H3: if OpenCode-first is chosen, *serving an OpenCode `.well-known/opencode` remote config* from the SaaS is a distribution decision that should be made explicitly in S7, not slipped in.
3. **Taxonomy change authority.** The draft says "Approved agent self discovery" and "Bring your own edge definitions". Recommended position: discovery is read-only for agents, and only the tenant admin applies config. Decide whether agents may *file proposals* (an extra surface) or not at all.
4. **Vision restatement.** The draft text and two new principles are a change to vision identity. Only the human approves edits to PRODUCT-VISION.md.

---

## Unanswered Questions

- **Is cross-project or cross-machine coordination (H3) a real need for shared working state?** S5 cannot settle it because S6 owns what OpenCode-first coordination concretely requires. It is the only identified re-open trigger for session memory.
- **What does per-project config hot-apply cost under N tenants?** This is blocked on S2 (hot-path, cache, and worker model). Rebuilding the allowlist, the server instructions, and the derived caches per apply is cheap for one project. Fleet cost was not measured.
- **How often are coordination failures caused by a missing shared state?** This needs measurement across `STALLS.md` and retros, which a directional spike does not do. The only evidence collected is one `STALLS.md` record type (SM ended turn while child running), which a shared store would not have fixed.

---

## Out-of-Scope Discoveries

- **`Status::Proposed` is dormant.** It exists in the schema and parsers, but there is no agent write path (`StoreParams` has no status field) and no promotion flow. This is a cheap enabler for cycle-scoped provisional knowledge and possibly for S3 shared-scope write review.
- **Vision/implementation drift on briefing.** The proactive-delivery goal (#5682) says briefing is a phase-boundary handoff, but `index_briefing.rs:188` explicitly carries no phase (col-031). Briefing is goal-conditioned only. The vision text or the goal entry should state this accurately.
- **Non-phase learning signals are not workflow-scoped.** Co-access, usage, helpfulness, and confidence are fed by any session traffic. Only phase tables filter. This matters for S4 learning isolation (public chat, chat apps).
- **Topic-vote feature inference (col-017) needs a client-class gate in V2.** Without one, non-workflow traffic can be attributed to a cycle (rule W1).
- **ass-063's "proceed-with-narrowed-scope" (workflow graph + 5 workflow tools) was never built in about 6 months.** There are no `workflow`/`step`/`gate` categories in `INITIAL_CATEGORIES`. The compliance pain was addressed harness-side instead (authority matrix, primary triage, STALLS backstop). This supports keeping workflow control in the harness.
- **`.collective/` is a working digest-pinned artifact distribution surface** for skills. It is precedent that harness artifact distribution with integrity already exists outside Unimatrix.
- **DA1 design-lock (1) ("config load-on-boot, immutable until restart") needs a V2 restatement** ("immutable until next apply"). It is binding text in a capability entry, so whoever owns the capability map should correct it.

---

## Recommendations Summary

- **Q1 (session memory need)**: No-go on a transient workflow-session memory class. Coordination state stays in the harness. Go (small) on cycle-scoped provisional knowledge via `proposed` status, `feature_cycle`, and disposition at cycle close.
- **Q2 (constraints if built)**: Bind any future build to R1–R9. The loss test and the ban on coordination primitives (no locks, claims, queues, notifications, or status enums) are the core. Scope is (tenant, project, cycle), purged at cycle stop plus a hard TTL, never a learning signal, never proactively delivered.
- **Q3 (harness definitions)**: No-go on storing, distributing, or serving protocols, agents, or skills. Distribution stays with harness-native channels and `.collective`. Go (small) on harness-artifact version references on entries and `harness_release` on cycle start. Restate the mental model as released artifacts / living knowledge / engine config.
- **Q4 (config surface)**: Go. Use an API-applied, versioned, validated, audited per-project configuration document with domain-pack templates, admin-only apply, and no secrets. Sequence PL-2, then DA1, then DA10, then templates. TOML stays as the import/export format.
- **Q5 (coexistence)**: Use the two-layer model: a workflow-independent core plus a workflow layer activated only by declared context. Apply rules W1–W5 and make PDN1 parity the proof of additivity. Use the draft vision text above, which names workflow-awareness first, states additivity, and keeps the orchestration boundary testable.

---

## Sources (industry)

- [Google ADK — State: The Session's Scratchpad](https://google.github.io/adk-docs/sessions/state) · [ADK Memory](https://google.github.io/adk-docs/sessions/memory)
- [LangGraph persistence (checkpointer vs store)](https://docs.langchain.com/oss/python/langgraph/persistence) · [LangGraph memory concepts](https://docs.langchain.com/oss/python/concepts/memory)
- [Amazon Bedrock AgentCore Memory types](https://docs.aws.amazon.com/bedrock-agentcore/latest/devguide/memory-types.html) · [AgentCore Memory CDK (event expiry)](https://docs.aws.amazon.com/cdk/api/v2/docs/aws-cdk-lib.aws_bedrockagentcore.Memory.html)
- [Letta shared memory blocks](https://docs.letta.com/guides/agents/multi-agent-shared-memory)
- [Mem0 entity-scoped memory](https://docs.mem0.ai/platform/features/entity-scoped-memory)
- [Claude Managed Agents memory](https://platform.claude.com/docs/en/managed-agents/memory)
- [Claude Code plugin marketplaces](https://code.claude.com/docs/en/plugin-marketplaces) · [Claude Code plugins reference](https://code.claude.com/docs/en/plugins-reference)
- [Anthropic — donating Agent Skills to the Agentic AI Foundation](https://www.anthropic.com/news/donating-agent-skills-to-the-agentic-ai-foundation)
- [OpenCode config (remote `.well-known/opencode`)](https://opencode.ai/docs/config/)
- [Langfuse prompt version control](https://langfuse.com/docs/prompt-management/features/prompt-version-control)
- [MCP specification — prompts](https://modelcontextprotocol.io/specification/2025-11-25/server/prompts)
