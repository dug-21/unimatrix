# FINDINGS: Feasibility & Landscape — A Human-Facing UI for Personal Cloud

**Spike**: ass-083
**Date**: 2026-06-22
**Approach**: investigation + evaluation
**Confidence**: directional (a ranked feasibility map + recommended phasing — NOT a design; no PoC)

---

## TL;DR feasibility map

| Capability | Tier | Determining constraint |
|---|---|---|
| **Packaging — embed static assets in the single binary, served over HTTPS+bearer** (W1) | **easy** | Precedent exists for in-binary content (`client_bundle`); HTTP stack is already an extensible tower `PathRouter`. The only new dep is a static-file embed crate + one new route arm. Principle 6 fully preserved. |
| **Project enumeration — list known projects for a switcher** (W2a) | **easy–moderate** | No HTTP/MCP "list projects" surface exists today; enumeration is a CLI-only sync subcommand (`unimatrix project list`). Needs one minimal new read endpoint (or read the `[[projects]]` config). |
| **Project switching — authenticate to the selected project** (W2b) | **easy** | There is **one** global bearer token that already reaches **every** slug; the slug is selected by URL path, not by credential. A switcher just changes the path. **No operator-level identity, no N tokens, no per-switch re-auth needed.** |
| **Read-only graph visualization** (W3) | **moderate** | `context_graph` (subgraph/neighbors/path) + vnc-037 ranked edges back a graph view as-is for small graphs, but the **server hard-caps `max_nodes` at 200** (`subgraph` mode) and `depth` at 10. Beyond a few hundred nodes needs server-side filtering/clustering — net-new query surface. |
| **Real-time lens — live "what's happening" view** (W4) | **moderate–hard** | Rich event *data* exists (transcript buffer, activity counters, observations, cycle events) but **there is zero publish/subscribe substrate** — no broadcast/watch/mpsc, no SSE/websocket endpoint. Everything is request-response or fire-and-forget. A *polled* lens is moderate; a *true push* lens is net-new streaming infrastructure. |
| **Region highlighting (client view-state)** | **easy** | Pure client-side view state; touches no server surface. Persisted "saved views" would be a new minor write surface (moderate) but is not required for an MVP. |
| **Node editing via UI** (W5) | **hard (mechanically clean, identity-incomplete)** | The write *path* is already integrity-safe: `context_correct` deprecates-original + appends-correction in one audited transaction, append-only by construction (P1/P2 intact). The blocker is **attribution**: every HTTP-bearer caller resolves to the single literal identity `agent_id: "http-bearer"`. A human edit is today indistinguishable from any agent or any other human. Distinct human attribution **requires new identity machinery** — this is the sole deep-dive spike. |
| **Cross-project aggregation / merged graph** | **dream — out of scope by decision** | Explicitly excluded by the decided one-project-at-a-time constraint. Would require cross-project RBAC, the enterprise boundary deliberately left as a door (vnc-034). Not needed for the switcher. |

---

## Findings

### Q: Can the UI ship as static assets embedded in the single binary, served over the existing HTTPS+bearer surface, with zero new infra (Principle 6)? (W1)

**Answer**: Yes. This is the strongest-fit option and preserves "one container, one bearer, one command" intact.

**Evidence**:
- The HTTP stack is a tower `Service` tree: `StaticTokenAuth` (auth layer) → `PathRouter` (path dispatch) → `SlugRouter`/`MultiProjectRouter` → rmcp `McpAdapter` (`crates/unimatrix-server/src/http/router.rs`, `http/auth.rs`). `PathRouter::call` already dispatches by `(method, path)` with explicit arms for `GET /health`, `POST /v1/{slug}/observe`, and an MCP catch-all (`router.rs:187-207`). Adding `GET /ui/*` and `GET /v1/{slug}/...` read arms is a localized extension of this match — no architectural change.
- The binary already embeds and composes non-trivial content in-process: `client_bundle.rs` encodes the connection bundle; the cert/token are provisioned to the data volume on first boot (`http_provision.rs`, `http/token.rs`). In-binary asset embedding is therefore on-pattern.
- **No static-asset serving exists today** — no `rust-embed` / `include_dir` / `include_str!` static-file route in the server. So embedded-static is net-new but small: one embed crate + one route arm + MIME mapping.
- The same `StaticTokenAuth` layer wraps the whole tree, so UI asset/data requests authenticate over the *same* HTTPS port + bearer with no parallel auth path. `/health` is the only auth-bypassed route (`auth.rs:34`); UI routes would sit behind auth like everything else.

**Cost comparison (the easy↔hard axis)**:
- **Embedded-static (RECOMMENDED)**: assets baked into the binary via an embed crate, served by a new `GET /ui/*` arm on `PathRouter`. Cost: one dependency, one route, MIME handling. **Zero new infra, zero new deploy, no second port.** Distroless-image-safe (no filesystem layout assumptions). Fully preserves Principle 6.
- **Separate SPA deploy**: a standalone frontend build served by another process/CDN. Cost: a second deployable, CORS config, a second origin to authenticate, separate release cadence. This is a **direct Principle-6 violation** ("the client is an adapter, not infrastructure") — reject unless a future requirement forces it.
- **No-build server-rendered**: server emits HTML directly (e.g. templated strings). Cost: lowest dependency footprint, but poor fit for an interactive graph/lens and pushes view logic into Rust. Viable for a *minimal* read dashboard, awkward for the graph view.

**Recommendation**: Ship the UI as **embedded static assets served by a new authenticated `GET /ui/*` arm on `PathRouter`, behind the existing `StaticTokenAuth` over the same HTTPS port**. Build the SPA at compile time and embed it; do not introduce a separate frontend deployable. This is the only option that holds "one container, one bearer, one command."

---

### Q: How does the UI enumerate the set of known/registered projects to populate a switcher? (W2a)

**Answer**: There is **no remote (HTTP/MCP) surface that lists registered slugs today.** Enumeration exists only as the operator CLI subcommand `unimatrix project list`. A switcher needs one minimal new "list projects" read endpoint.

**Evidence**:
- `unimatrix project list` is a **pre-tokio synchronous CLI subcommand** (`crates/unimatrix-server/src/projects.rs`). `ProjectCommand::List → registry.list_and_print()` (`projects.rs:162`) scans the `[[projects]]` routing stanzas and prints each slug plus a local `store_open` status (`projects.rs:383-416`). It is operator-only and not reachable over the wire.
- The MCP tool surface (enumerated in `crates/unimatrix-server/src/mcp/tools.rs`) is: `context_search, context_lookup, context_store, context_get, context_correct, context_deprecate, context_status, context_briefing, context_quarantine, context_enroll, context_cycle_review, context_edge, context_cycle, context_graph`. **None lists projects.** This is by design — slug isolation (C3) means a per-slug MCP session has no business listing other slugs.
- The registered set lives in the `[[projects]]` config the server reads at boot (`main.rs:1101-1124` builds the `MultiProjectRouter` from `project_slugs`).

**Recommendation**: Add a **minimal, operator-scoped "list projects" read** that returns the registered slugs (the same data `list_and_print` produces). Two viable shapes, both small: (a) a top-level authenticated route like `GET /projects` on `PathRouter` (parallels `/health`, sits behind the same bearer, returns the slug list + `store_open` status); or (b) the UI server reads the `[[projects]]` config directly since it is co-located in the same binary. Prefer (a) — it keeps the server the sole authority on route/registry shape and avoids the UI parsing config. This is the only net-new surface W2 requires.

---

### Q: When the operator switches to project X, where does X's credential come from? (W2b)

**Answer**: It comes from **nowhere new — the same single token already authorizes every slug.** Switching is purely a change of the URL slug. **No operator-level identity is required, and no model that does the switcher needs one.**

**Evidence**:
- In cloud `serve` mode the server loads **one** bearer token from `{data_dir}/token` (the shared/parent data dir, not per-slug) and builds **one** `StaticTokenAuthLayer` (`main.rs:1053`, `1271`). `load_or_generate_token` operates on `paths.data_dir` (the parent), not on any slug dir (`http/token.rs:59`).
- `StaticTokenValidator` holds exactly one 32-byte token and resolves *every* valid bearer to the same `ResolvedIdentity` regardless of slug (`http/auth.rs:107-138`). The slug is **not** part of authentication — it is parsed from the URL path (`/v1/{slug}/...`) and resolved to a per-slug store by `MultiProjectRouter` *after* auth (`router.rs:199-205`).
- This is confirmed as an explicit vision invariant (goal #4946): *"Token is the sole authorization credential; agent_id and project slug are scoping/audit metadata, not security boundaries (OSS single-tenant)... in OSS single-tenant it already reaches every slug; the slug scopes data, not access."* Per-slug isolation is in **data** (own DB / vector / hash-chain) and **routing** (path), never in the credential.

**Feasible enumerate+switch models, ranked by disturbance to the per-project bearer model (least → most)**:
1. **Single token + path-selected slug (RECOMMENDED — ZERO disturbance).** The UI holds the one cloud bearer; the switcher changes the URL slug. Matches exactly how the system works today. No operator-level identity. Requires only the W2a "list projects" read. **This is the destination for the one-project-at-a-time constraint.**
2. **N per-slug bearers held by the UI (HIGH disturbance, NOT NEEDED).** Would require introducing per-slug tokens (which do not exist today — there is one token) and the UI juggling a credential per slug. Pure overhead with no security gain in single-tenant; reject.
3. **Per-switch re-auth (MODERATE disturbance, NOT NEEDED).** Re-presenting a credential on each switch buys nothing because the one token already reaches all slugs. Reject.
4. **Operator-level identity above the per-project bearer (NOT REQUIRED for the switcher).** No switcher model needs this. An operator/human *identity* becomes relevant only for **editing attribution** (W5) and for the future **cross-project RBAC** enterprise boundary — neither of which the read-only switcher requires.

**Does the switcher unavoidably need an operator-level identity?** **No.** Under the decided one-project-at-a-time constraint, the switcher is a UI affordance over the single existing token + path routing. Cross-project RBAC/aggregation remains out of scope and is *not* forced by the switcher.

---

### Q: Can `context_graph` / vnc-037 edge surfaces back a graph view as-is, or is new read query surface needed? What viz library class and scale ceiling fits? (W3)

**Answer**: They back a read-only graph view **as-is for small graphs (low hundreds of nodes)**. Beyond that the server's own caps force net-new server-side filtering/clustering query surface.

**Evidence (backend fit)**:
- `context_graph` already exposes the modes a graph view needs: `subgraph` (BFS from seeds, returns `nodes` + typed `edges`), `neighbors` (depth-1..10 typed edges), `path` (shortest path), plus `chain`/`current`/`inverse`/`filter` (`crates/unimatrix-server/src/mcp/graph_read.rs:239-360`). The `SubgraphResponse` returns exactly `{nodes, edges, truncated, seed_ids, depth_reached}` — directly renderable.
- vnc-037 ranked typed edges on `context_get` (`include_edges` default-on for agent callers) gives a per-node neighbor expansion for click-to-expand interactions without a separate query.
- These are read-only and require only `Capability::Read` (checked in `tools.rs` before `handle_graph`).

**Evidence (scale ceiling — the hard limit)**:
- `subgraph` mode hard-caps at **`max_nodes` in [1,200], default 200**, and **`max_depth` in [1,10], default 3** (`crates/unimatrix-server/src/mcp/graph_read_subgraph.rs:38-39, 87-107`). Requests above 200 nodes are *rejected*, and BFS truncates at the cap (`truncated: true`).
- So the **as-is scale ceiling is ~200 nodes per query.** A view of low hundreds of nodes works today by composing seed-anchored subgraph + neighbor-expand calls. There is **no whole-graph fetch** and no server-side clustering/aggregation primitive.

**Scale verdict**:
- **100s of nodes**: feasible **as-is** via seeded subgraph + neighbor expansion. No new query surface.
- **1,000s of nodes**: needs **net-new server-side surface** — either a raised/paged node cap with cursoring, or server-side clustering/community-detection + level-of-detail, so the client never receives the full set.
- **10k+ nodes**: requires server-side clustering + semantic filtering + progressive disclosure as a first-class capability. This is a dream-tier extension, not an MVP concern.

**Viz library class (not a pick — per scope)**: a **force-directed / layered graph-rendering library class** (canvas/WebGL-backed for the >1k regime; SVG/DOM-backed acceptable for the <200 MVP regime). The typed, evolving, supersession-aware graph maps cleanly onto a directed-typed-edge renderer. Specific library selection is its own follow-on tech-selection spike.

**Recommendation**: Build the read-only graph MVP **on the existing `context_graph` subgraph/neighbors/vnc-037 surface, scoped to the ~200-node ceiling** (seed-anchored, click-to-expand). Do **not** build new graph query surface for the MVP. Treat ">200-node server-side filtering/clustering" and "graph-viz library selection" as separate follow-on spikes triggered only when a real graph exceeds the few-hundred-node regime.

---

### Q: What event sources already exist and could feed a live per-project activity view over the same HTTPS surface? What transport? (W4)

**Answer**: Rich event *data* exists, but **no live publish/subscribe substrate and no streaming endpoint exist today.** A useful **polled** lens is cheap; a **true push** lens is net-new streaming infrastructure. The cheapest first useful view is a poll-based activity summary, not SSE/websocket.

**Evidence (event data that exists)**:
- **Session transcript buffer (vnc-025)**: per-session, in-memory, bounded (4 MiB ring), accumulated live from deltas. Read-only via synchronous `snapshot()` / `contiguous_tail()` while holding a mutex — **no tail/subscribe API** (`crates/unimatrix-server/src/infra/session_transcript.rs`).
- **Activity counters (crt-054)**: an in-memory fold over transcript deltas — `bytes_total`, `delta_count`, per-class counts — exposed via `activity_snapshot()` (request-response) (`infra/transcript_activity.rs`). This is the single cheapest "what's happening" signal already computed.
- **Observations table**: hook events (`POST /v1/{slug}/observe`) written directly to SQLite, queryable via the service layer — **on-demand query, not an event stream** (`services/observation.rs`, observe ingestion path).
- **Cycle events / aggregates**: `cycle_events` table grows as `cycle_start`/`phase_end`/`cycle_stop` arrive (fire-and-forget spawn); `context_cycle_review` computes phase/rework/reuse aggregates **on demand**, never emits live events (`crates/unimatrix-observe/src/cycle_aggregates.rs`, `mcp/tools.rs` cycle_review).
- **Event queue** (`crates/unimatrix-engine/src/event_queue.rs`) is a **disk failover buffer** (JSONL, 7-day TTL), batch-replayed — not a live feed.

**Evidence (the substrate gap)**:
- **Zero `tokio::sync::broadcast` / `watch` / `mpsc` in the server** for activity. All writes are insert-then-unlock or `tokio::spawn` fire-and-forget; nothing publishes to in-memory consumers.
- The HTTP stack is rmcp `StreamableHttpService` (MCP-protocol-specific streaming) plus `/health` and `/observe`. **No SSE / websocket / streaming response body exists** for arbitrary feeds. rmcp's streaming is MCP-resource-shaped and not repurposable for an activity feed. A push lens therefore needs a **brand-new streaming route** (`GET /v1/{slug}/activity/stream`) **plus** an internal broadcast channel fed by the observation/cycle writers **plus** a fan-out subscription manager — all net-new.

**Transport recommendation**: When a live feed is built, **SSE over the existing HTTPS+bearer is the cheaper, better fit than websocket** — it is one-directional (server->UI, which is exactly the lens), rides plain HTTP responses (no protocol upgrade), authenticates with the same bearer, and composes with the existing tower stack as one more `PathRouter` arm. Websocket adds bidirectional complexity the lens does not need.

**Cheapest path to a useful live view (the MVP slice)**: a **polled** activity panel — the UI calls a per-slug read (e.g. an endpoint exposing `activity_snapshot()` counters + recent observations/cycle phase) on a short interval. It shows: current cycle/phase, active sessions, delta/byte throughput, recent observation/tool activity, recent corrections. This reuses existing on-demand reads with **no new streaming substrate** and delivers most of the lens's value. Upgrade to SSE push only if polling proves insufficient.

**Recommendation**: Phase-1 lens = **polled activity summary** over existing snapshot/query reads (moderate, no new substrate). Defer the **SSE push lens** (broadcast channel + streaming route + fan-out) to a later phase; it is the hard part and is not required for the first useful view.

---

### Q: How does a human UI edit route through `context_correct` (provenance), AUDIT_LOG attribution (P2), and hash-chain integrity (P1) without breaking them? Who is the attributed identity? Is editing phase-2+ or a non-starter? (W5)

**Answer**: The write *mechanism* is already integrity-safe and a human edit can route through `context_correct` cleanly **today** — P1 and P2 are not threatened. The unsolved problem is **attribution**: every HTTP-bearer caller (human or agent) resolves to the single literal identity `agent_id: "http-bearer"`, so human edits are indistinguishable. Distinct human attribution **requires new identity machinery.** Editing is therefore a **phase-2+ capability gated on an identity spike** — *not* a non-starter and *not* an integrity threat.

**Evidence (mechanism is integrity-safe — P1/P2 intact)**:
- `context_correct` does **not** mutate in place. `StoreService::correct` (`crates/unimatrix-server/src/services/store_correct.rs:17-123`) runs rate + content validation, then delegates to `store.correct_entry()` which **in one transaction** marks the original `Deprecated`, sets its `superseded_by`, and **appends a new entry** with `supersedes -> original_id`. Append-only by construction — the correction chain is a forward FK, and there is no path that edits an existing entry's content after insert.
- The audit log is **append-only** (`crates/unimatrix-server/src/infra/audit.rs` — INSERT-only `log_audit_event`, monotonic `event_id`). A correction emits an audit event recording `operation: "context_correct"`, `target_ids: [original, new]`, `capability_used: "write"`, `credential_type: "static_token"`.
- Requires only `Capability::Write` — already granted to the HTTP-bearer identity. So a human UI edit is *mechanically* a normal correction.

**Evidence (attribution is the blocker)**:
- The HTTP bearer path resolves **all** callers to a fixed identity: `ResolvedIdentity { agent_id: "http-bearer", trust_level: Restricted, capabilities: [Read, Write, Search, SessionWrite] }` (`crates/unimatrix-server/src/http/auth.rs:128-137`).
- That `agent_id` flows verbatim into the entry's `created_by` and into the audit `agent_id` (`server.rs` `build_context_with_external_identity` -> `AuditContext.caller_id`; `mcp/tools.rs` `created_by: ctx.agent_id`). So a human edit is stored as `created_by: "http-bearer"` — **indistinguishable from any agent or any other human** sharing the one cloud token. The audit trail is opaque as to *which human*.
- The capability model (`infra/registry.rs`) has a `"human"` bootstrap agent (Privileged, full caps) but it is **never used on HTTP flows** — there is no human-vs-agent distinction on the bearer path.
- The seams to fix this already exist: the `BearerValidator` trait is explicitly documented as the "bridge to JWT/OAuth" (`auth.rs:5-6, 66-76`), and the audit schema carries an `agent_attribution` (transport-attested, non-spoofable) field that is empty for bearer today. So adding human identity is *extensible*, not a re-architecture (consistent with vnc-034 "enterprise extends, never re-architects").

**Region highlighting**: client-only view state — no server surface, **easy**. Persisted "saved views" would be a small net-new write surface (moderate), but is not required for an MVP.

**Recommendation**: **Defer node editing to phase 2+, gated on a dedicated identity/attribution spike.** Editing is safe to *mechanically* implement (it rides `context_correct`), but must NOT ship until human edits are *attributable to a distinct human identity* — otherwise the knowledge base's core promise (every mutation attributed, P1/P2) is silently weakened for exactly the surface most likely to be used by a person. The follow-on spike should evaluate: (a) JWT/OAuth via the `BearerValidator` trait populating a real human `agent_id` + the non-spoofable `agent_attribution` field; vs (b) session-scoped human enrollment via the existing session registry; vs (c) an `X-Human-Editor` header (rejected — spoofable, no stronger than the shared token).

---

## Recommended MVP + phasing order

The scope hypothesis (switcher + read-only lens -> read-only graph -> highlighting -> editing) **largely holds**, with one refinement: the **read-only lens should start polled, not pushed**, and the cheapest-first slice is the switcher + a polled status panel.

**Cheapest first useful slice (MVP)**:
> An embedded static UI, served by a new authenticated `GET /ui/*` arm over the existing HTTPS+bearer, that (1) lists registered projects via one new `GET /projects` read and lets the operator pick one (path-selected slug, single existing token), and (2) shows a **polled** per-project status panel built from existing snapshot/query reads (activity counters, current cycle/phase, recent observations). **No new graph surface, no streaming substrate, no editing, no new identity.**

**Phasing order**:
1. **Phase 1 (MVP — easy)**: Embedded UI shell (W1) + project enumeration endpoint + switcher (W2) + **polled** read-only activity lens (W4 cheap path). All low-risk, all on existing surfaces. Cheapest first useful slice.
2. **Phase 2 (moderate)**: Read-only graph view (W3) on the existing `context_graph` subgraph/neighbors/vnc-037 surface, scoped to the ~200-node ceiling, plus client-only region highlighting.
3. **Phase 3 (moderate–hard)**: SSE push lens (broadcast channel + streaming route) — only if polling proves insufficient; and >200-node server-side graph filtering/clustering — only when a real graph exceeds the few-hundred-node regime.
4. **Phase 4 (hard — gated)**: Node editing via `context_correct`, **blocked on the identity/attribution spike**. Do not ship before distinct human attribution exists.

---

## Vision-alignment call

- **On-vision (build it)**: A **read/observability UI — the switcher, the lens, the read-only graph — is a natural third surface** that exposes what the engine already knows and is doing. It does not make Unimatrix an orchestration engine; it adds no control-plane. Served as embedded static assets over the one HTTPS+bearer, it fully honors Principle 6 ("one container, one bearer, one command") and Principle 3 (UI is just another caller behind the same auth -> capability path). This is squarely the "lens" the vision anticipates.
- **Scope-creep risk (guard)**: A **separate SPA deploy** would violate Principle 6 (a second deployable is infrastructure, not an adapter) — reject it. A **push (SSE/websocket) lens** is *more* infra than the lens needs at MVP — start polled.
- **Integrity-compromise risk (gate)**: **Node editing without distinct human attribution** is the real hazard. The write path is integrity-safe, but shipping editing while every human collapses to `"http-bearer"` quietly degrades the "every mutation attributed" promise (P1/P2) on the most human-touched surface. Gate editing behind the identity spike.
- **Out-of-vision-for-now**: **Cross-project aggregation / merged graph** is the enterprise RBAC boundary deliberately left as a door (vnc-034). The decided one-project-at-a-time constraint correctly keeps the UI on the OSS side of that line; do not let the switcher drift into aggregation.

---

## Unanswered Questions

- **Which identity mechanism for human edit attribution?** (JWT/OAuth via `BearerValidator` vs session-scoped human enrollment vs other) — out of scope for this feasibility spike by design; it *scopes* the question and hands it to a dedicated follow-on spike (see below). Reason: requires a security/identity design decision, not a feasibility verdict.
- **Exact >200-node graph strategy** (raised cap + cursoring vs server-side clustering/community detection) — requires a real large-graph dataset to measure against; reason: needs empirical scale data this directional spike did not collect, and a graph-viz tech-selection spike.
- **Whether polling is sufficient for the lens or SSE push is required** — depends on real usage/latency expectations; reason: a UX/performance question best answered after the polled MVP is in hand.

---

## Out-of-Scope Discoveries

- **`previous_hash` appears to be schema-present but never populated.** A sub-investigation reported that `EntryRecord.previous_hash` (schema field) is written as an empty string on both new entries and corrections; the operative correction-chain link is the `supersedes` FK plus per-entry `content_hash`. This does **not** affect any editing verdict here (append-only integrity holds via `supersedes` + deprecate-then-append), but it warrants a separate verification: confirm whether `previous_hash` is intended to carry a genuine prior-hash link (a true hash chain) or is a vestigial/forward-compat field. If the former, it is an integrity gap to file as a GitHub issue. Flagged for verification, not pursued here.
- **No per-slug bearer tokens exist despite "per-project bearer" framing.** There is one global cloud token; per-slug isolation is data+routing only. This is consistent with vision goal #4946 but is a common misread worth stating plainly in any UI design doc.
- **The `"human"` bootstrap agent (Privileged, full caps) is defined but unused on HTTP flows** (`infra/registry.rs`). It is a latent seam a human-identity feature could build on. Noted as a carry-forward for the identity spike.

---

## Follow-on spikes identified

1. **Human edit identity / attribution & provenance (REQUIRED before any editing ships).** Draft scope question: *"How should a human editing knowledge through the UI be attributed to a distinct, audit-meaningful identity (populating `created_by` + the non-spoofable `agent_attribution`) without breaking the single-token Principle-6 model or the P1/P2 integrity guarantees — JWT/OAuth via the `BearerValidator` trait, session-scoped human enrollment, or another mechanism?"* This is the W6-anticipated sole deep-dive candidate.
2. **Graph-viz tech selection & >200-node scale strategy.** Scope: pick a library class and validate a server-side filtering/clustering approach for the 1k–10k node regime against a real graph, defining the new read query surface (paged cap / clustering / level-of-detail) the >200 regime needs.
3. **(Conditional) SSE push-lens substrate.** Only if the polled lens proves insufficient: design the internal broadcast channel + streaming route + fan-out subscription manager. Lower priority than 1 and 2.
4. **(Deferred, enterprise) Cross-project identity/RBAC.** Explicitly *not* triggered by the read-only switcher; remains the vnc-034 enterprise boundary. Recorded so it is not re-derived.

---

## Recommendations Summary

- **W1 Packaging**: Ship the UI as **embedded static assets on a new authenticated `GET /ui/*` arm of `PathRouter`, behind the existing bearer over the one HTTPS port** — easy, zero new infra, preserves Principle 6. Reject a separate SPA deploy.
- **W2a Enumeration**: Add a **minimal authenticated `GET /projects` read** returning registered slugs (there is no remote list-projects surface today; it is CLI-only).
- **W2b Switching**: Use the **single existing global token + path-selected slug** — zero disturbance to the credential model; **no operator-level identity required.** Reject N-tokens and per-switch re-auth.
- **W3 Graph**: Build read-only graph on the **existing `context_graph` subgraph/neighbors/vnc-037 surface, scoped to the ~200-node server cap**; defer >200-node clustering/filtering and library selection to a follow-on spike.
- **W4 Lens**: Start with a **polled activity panel over existing snapshot/query reads** (no new substrate); if push is later needed, use **SSE over the existing HTTPS+bearer**, not websocket. A true push lens is net-new streaming infra.
- **W5 Editing**: **Defer to phase 2+, gated on an identity/attribution spike.** The write path (`context_correct`, append-only, audited) is integrity-safe today, but every human collapses to `agent_id: "http-bearer"` — distinct human attribution requires new identity machinery. Region highlighting is client-only (easy).
- **W6 MVP + phasing**: MVP = **embedded UI + project switcher + polled read-only lens** (cheapest first useful slice). Then read-only graph + highlighting -> (conditional) SSE push + large-graph filtering -> editing (gated). Read-only is on-vision; editing-without-identity and a separate SPA deploy are the two things to refuse.

---

## Extension: Editable workflow-authoring graph (W7–W10)

**Date**: 2026-06-22 · **Approach**: investigation + evaluation · **Confidence**: directional · **Breadth**: code+ecosystem (read-only in Unimatrix)

**The thesis under test**: the W5/W6 verdict deferred ALL node-editing to a gated phase 4 because it assumed the editing target is a hash-chained Unimatrix knowledge entry with unsolved human attribution. The killer use case has a *different* editing target — the git-tracked flat `.md` files that define our workflow chain (`.claude/protocols`, `.claude/agents`, `.claude/skills`, `.claude/rules`, the CLAUDE.md routing table). The question: does that different target change the verdict?

**Headline reconciliation (stated up front, detailed under W9/W10):**
- The W5 *identity blocker* (every human collapses to `agent_id: "http-bearer"`) is **real for both targets and is NOT dissolved by git** — because the server has **zero git integration today** (no `git2`/`gix` dep, no `Command::new("git")` anywhere: confirmed). Git authorship is only "free" provenance if a git identity is actually configured and invoked; the server has neither. So "git supplies the attribution W5 found missing" is **half-true**: git *can* carry attribution, but wiring it is net-new work, and a server-side commit would commit as whatever single git identity the deployment configures — re-creating the exact "one shared identity" problem on the git layer.
- **BUT** the filesystem target unlocks a write-back model the knowledge target cannot use: **emit-a-diff / open-a-PR instead of server-writes-files**. On that model the human's *own* git identity signs the merge, attribution is solved by the existing PR workflow, and the server never needs write access to the repo at all. **This does let a useful slice of workflow-node editing jump ahead of the phase-4 identity gate** — see W9/W10. The slice that jumps ahead is *propose-edit-as-diff*, not *server-commits-directly*.

---

### Q (W7): Can "a graph of editable linked entities" be defined independent of the backing store, behind one provider contract, so the same UI serves read-only knowledge nodes and editable workflow-artifact nodes?

**Answer**: Yes. The existing `context_graph` wire shape is already store-agnostic enough to be the common denominator, and a thin provider trait cleanly abstracts knowledge-DB vs filesystem. The single real adaptation is **node identity**: knowledge nodes are addressed by `u64`, filesystem nodes need a string URI. Standardize on an opaque string node-id and both fit.

**Evidence**:
- The current graph response (`crates/unimatrix-server/src/mcp/graph_read.rs:135-188`) is: `EdgeRecord { source_id, target_id, relation_type: String, direction, depth, metadata: Option<Value> }` and `SubgraphResponse { nodes, edges, truncated, seed_ids, depth_reached }`. `relation_type` is already a free-form string (handles arbitrary edge taxonomies); `metadata` is an already-present open extension slot. The shape needs no change to carry workflow edges.
- The only store-specific assumption is `source_id`/`target_id: u64` (a DB row id). Workflow nodes have no integer id; their natural id is a path/URI (`agent:uni-rust-dev`, `skill:uni-git`, `rule:rust-workspace`). The generic contract must therefore use an **opaque `node_id: String`** (a URI: `{kind}:{name}` or the repo-relative path). Knowledge provider maps `node_id = "kb:{u64}"`; filesystem provider maps `node_id = "agent:uni-rust-dev"`.
- Read-only vs editable is already a per-node property in the system's grain: the capability model (`Capability::Read` vs `Capability::Write`, checked in `mcp/tools.rs` before each handler) maps directly onto a per-node `writable: bool` the provider advertises.

**The generic node/edge/provider model (directional)**:
```
Node    { id: String (URI), kind: String, title: String,
          source_ref: String (DB id | repo-relative path),
          writable: bool, body: Option<String> /* lazy, via read() */ }
Edge    { source: String, target: String, relation_type: String,
          confidence: enum { explicit, derived, heuristic }, /* W8 needs this */
          metadata: Option<Value> }

trait GraphNodeProvider {
    fn enumerate(&self, seeds, depth, cap) -> Vec<Node>      // BFS/scan over the store
    fn resolve(&self, node_ids) -> Vec<Edge>                 // typed refs between entities
    fn read(&self, node_id) -> NodeBody                      // full body of one node
    fn write(&self, node_id, new_body, provenance) -> WriteResult  // editable nodes only
}
```
- `KnowledgeProvider` implements `enumerate`/`resolve`/`read` over `context_graph` + `context_get`, and `write` over `context_correct` (the W5 path — stays read-only-for-now, gated on identity).
- `WorkflowFsProvider` implements `enumerate`/`resolve` by parsing `.claude/` (W8), `read` by reading the file, `write` by one of the W9 write-back targets.
- `provenance` is the discriminator that carries either a knowledge identity (W5 gate) or a git/PR author (W9) — the contract is identical; only what fills `provenance` differs per provider.

**Recommendation**: Define the UI against this single `GraphNodeProvider` contract with **opaque string node-ids and a per-node `writable` flag**, not against `context_graph` directly. This is the reusable form: it lets the read-only knowledge graph (today) and the editable workflow graph (the killer use case) share one renderer, one selection model, one edge-typing convention, and one edit affordance that is simply disabled where `writable == false`. Add a `confidence` field to `Edge` (explicit/derived/heuristic) — W8 proves it is required to honestly render the workflow graph.

---

### Q (W8): How are the workflow→agent→skill→rule links represented in `.claude/` today — explicit or implicit? Is link extraction deterministic or heuristic? What is the real edge-type taxonomy, and how maintainable is it?

**Answer**: References are **almost entirely implicit** — prose name-mentions, `/slash-command` mentions, and backticked path strings — with **only one typed/structured surface** (agent YAML frontmatter, which carries `name`/`type`/`capabilities` but **no outbound reference keys**). So a typed graph is **derivable but partly heuristic**: path-based and `/slash` edges are deterministic; bare-name edges are heuristic and brittle. The derivation is genuinely useful precisely because it immediately exposes drift — I found multiple **dangling edges** during this spike.

**Evidence — how each edge type is actually expressed (real counts from the repo)**:

| Edge type | How expressed today | Deterministic? | Example / count |
|---|---|---|---|
| **CLAUDE.md → protocol** | Backticked path in the routing table | **Deterministic** (path string) | `.claude/protocols/uni/uni-design-protocol.md` ×4 in the table |
| **protocol → agent** | Mostly **bare name mention** in prose; SM-related agents also by path | **Heuristic** (name); deterministic where pathed | `uni-validator` ×23, `uni-tester` ×20, `uni-rust-dev` ×17 mentions across protocols |
| **agent → skill** | `/slash-command` mention in prose | **Semi-deterministic** (`/uni-` token, but a token ≠ a declared dep) | `/uni-knowledge-search` ×17, `/uni-store-pattern` ×15, `/uni-store-adr` ×10 across agents |
| **agent → rule** | Backticked path **and** bare-name mention | **Deterministic** (path) | `.claude/rules/rust-workspace.md` referenced in rust-dev, tester, validator |
| **agent → protocol** | Backticked path | **Deterministic** | scrum-master → all 3 protocols by path |
| **skill → skill** | `/slash-command` mention | Semi-deterministic | `/uni-retro` ×16, `/uni-store-procedure` ×8 across skills |
| **agent → agent (spawn)** | Bare name in coordinator prose | **Heuristic** | scrum-master names ~16 specialist agents |
| **capabilities (agent frontmatter)** | YAML list (`capabilities:`) | **Deterministic but semantic, not a reference** | `rust_development`, `test_plan_design` — describe, don't link |

- **The one structured surface is thin**: agent frontmatter (`AGENT-CREATION-GUIDE.md:46-54`) requires `name`, `type`, `scope`, `description`, `capabilities` — **there is no `uses:` / `skills:` / `rules:` outbound-reference key.** Skills are even less consistent: 15/16 `SKILL.md` files have YAML frontmatter; **`uni-git/SKILL.md` has none** (a parser keying off frontmatter would silently drop that node).
- **The de-facto centralized edge table already exists in prose**: `.claude/protocols/uni/uni-agent-routing.md` is a hand-maintained roster + routing table (agent-preference table, coordinator-routing table, full agent roster). It is the closest thing to a declared graph — but it is documentation, not a parsed manifest, so it drifts.

**Fidelity / maintainability — the honest read (this is the load-bearing W8 finding)**:
The graph is derivable but the heuristic edges are brittle, and I found **live drift** during derivation — which is exactly the use case's value, and also its caveat:
- **Dangling edge: `/uni-record-outcome`** is referenced in **6 places** (CLAUDE.md:71, AGENT-CREATION-GUIDE.md:119, uni-init/SKILL.md ×2, uni-review-pr/SKILL.md, uni-agent-routing.md:188) but **no `uni-record-outcome` skill exists** (`.claude/skills/` has no such directory). A name-mention edge resolves to nothing.
- **Dangling edge: `.claude/agents/uni/coordinator (you).md`** is referenced by path in `uni-agent-routing.md:47` but **does not exist**.
- **Type confusion: `/uni-design-protocol`** is written in `/slash` form in CLAUDE.md:6-9 as if it were a skill, but it is a *protocol* file — a `/slash`-keyed extractor would mint a wrong-typed edge.
- **The rename-fragility the scope asked about is real**: because agent→skill edges are bare `/uni-foo` mentions, renaming a skill silently orphans every mentioning agent with no error — there is no referential integrity. The graph would show the break (good), but nothing *prevents* it (the maintainability cost).

**Derivation approach (directional)**: a deterministic two-tier extractor — (1) **explicit tier**: parse YAML frontmatter (node identity + capabilities) and backticked `.claude/**/*.md` path strings → `confidence: explicit`; (2) **derived tier**: regex `/uni-[a-z-]+` against the known skill/agent name set → `confidence: derived`; everything else (free prose name match) → `confidence: heuristic`. Resolve each edge target against the actual file set so **dangling references render as broken edges** rather than vanishing. The `confidence` field (W7) is what makes this honest in the UI.

**Recommendation**: Build the workflow graph from a **deterministic path + `/slash` + frontmatter extractor over `.claude/`**, tagging every edge `explicit | derived | heuristic`, and **resolve targets against the real file set so dangling edges are shown, not dropped**. Do **not** claim a fully deterministic graph — the dominant edge class (name/`/slash` mentions) is referential-integrity-free. Treat "surface dangling/broken workflow links" as a **first-class feature**, not a bug: the three live dangling edges found in this spike are direct proof the graph view earns its keep. Optionally (follow-on) add an authoring convention — outbound-reference frontmatter keys (`uses_skills:`, `uses_rules:`) — to upgrade derived edges to explicit and gain rename-safety.

---

### Q (W9): Does git's commit authorship dissolve the W5 attribution blocker for filesystem-backed nodes? What is the safe write-back model — direct `.md` edit + git, promote to Unimatrix entities, or hybrid? Server-writes-files vs emit-a-diff/PR? Can editing this node class ship before the human-identity spike?

**Answer**: Git does **not** automatically dissolve the blocker, because **the server has no git integration at all** and is **not even repo-aware** — but the filesystem target enables a write-back model (emit-a-diff / PR) that **routes around** the W5 identity gate entirely. The recommendation is the **PR/diff model**, and **yes — a propose-edit (diff/PR) slice of workflow-node editing can ship before the human-identity spike**, because attribution is supplied by the human's own git identity at merge time, not by the server.

**Evidence — git is absent and the server is not repo-aware (decisive)**:
- **Zero git integration**: no `git2`/`gix`/`gitoxide` in any `Cargo.toml`; no `Command::new("git")` anywhere in `crates/*/src` (both greps returned NONE). So "git commit authorship supplies the attribution" presumes machinery that **does not exist** and would be net-new.
- **The server is `data_dir`-centric and has no concept of the repo working tree.** Every server write resolves under `paths.data_dir` (`main.rs` — token, `config.toml`, TLS, vector index all under `{data_dir}` / per-slug `{base}/{slug}/`). The string `.claude` appears in `crates/` only in one unrelated worktree comment (`unimatrix-engine/src/project.rs:378`). In the personal-cloud deployment the server runs against a **data volume**, and the `.claude/` repo source is **not guaranteed to be co-located** with it. A server "edit the `.md`" feature must first introduce a repo-root concept the architecture deliberately does not have.
- **Identity is still fixed**: HTTP bearer resolves every caller to `ResolvedIdentity { agent_id: "http-bearer", ... }` (`http/auth.rs:128-137`). If the server *did* commit, it would commit as one configured git identity for all callers — the **same shared-identity weakness W5 named, relocated to the git author field**. Git does not fix attribution unless a *per-human* identity reaches the commit, which is the W5 spike's job regardless.

**Evidence — the write infrastructure that DOES exist (so direct-write is mechanically feasible if repo-awareness were added)**:
- Atomic write pattern is established and reusable: token (`http/token.rs`) and TLS key (`http/cert_provisioner.rs`) both use `O_CREAT|O_EXCL` election + temp-file + atomic `fs::rename` at mode 0600/0644; vector index dumps via persist-on-shutdown. So safe atomic single-file write is on-pattern.
- Path bounding today relies on the **validated `ProjectSlug` newtype** (single join site `per_slug_data_dir`, `projects.rs:123-125`) — *not* on `canonicalize`-based traversal guards (the only `canonicalize` traversal check is for the snapshot `--out` path, `snapshot.rs:80-96`). Writing arbitrary `.claude/**` paths has **no analogous typed guard** — it would need a net-new repo-root canonicalization/allowlist guard to prevent `../` escape and symlink traversal.
- Validation-before-write precedent exists (`services/store_ops.rs`, `store_correct.rs`: rate check → content validation → atomic insert+audit). A `.md` writer would need its own validator (YAML frontmatter parse + schema check) before commit to satisfy the "don't corrupt frontmatter" hazard.
- **Concurrency hazard is unaddressed for repo files**: existing writes use `O_EXCL` election or shutdown-sequencing, not read-modify-write coordination. Editing a `.md` through the UI is inherently read-modify-write; concurrent edits (UI + a human's local editor + an agent mid-session) have **no lock today**. The PR model sidesteps this (git merge detects conflicts); direct-write would need optimistic concurrency (e.g. expected-hash precondition).

**The three write-back targets, compared**:

| Target | Attribution | Integrity hazards | Verdict |
|---|---|---|---|
| **(a) Direct `.md` edit + server commit** | Needs net-new git wiring; commits as one configured identity → re-creates the shared-identity problem | Repo-root concept absent; no path-traversal guard for arbitrary paths; frontmatter-corruption risk; **no concurrent-edit lock**; server must have repo write access + git in the container | **Reject for now** — most infra to build, re-imports the W5 identity problem on the git layer, and demands the server be repo-aware (it isn't). |
| **(b) Promote workflow defs to Unimatrix entities (`context_correct`)** | Inherits the **W5 identity gate** exactly | Hash-chain machinery; and it **moves the source of truth off git**, losing native diff/PR/rollback and the whole point (the artifacts must stay the files Claude Code loads) | **Reject** — fully blocked on the same phase-4 gate, and architecturally wrong (the `.claude/` files must remain the loaded source of truth). |
| **(c) Emit-a-diff / open-a-PR (server proposes, human's git merges)** | **Solved by the existing PR workflow** — the human merges under their own GitHub/git identity; server never authors a commit | Server emits a unified diff or opens a PR via the operator's token; **no server repo-write, no traversal guard needed, no concurrent-edit corruption** (git is the merge arbiter); validation (frontmatter parse) runs before emitting the diff | **RECOMMENDED** — least infra, native provenance, sidesteps repo-awareness and the identity gate. |
| **(c′) Hybrid: index/graph in Unimatrix, source stays files** | n/a for editing (read side) | Adds a sync/index pipeline | Useful **only** as a future read-scale optimization (mirror the W8-derived graph into Unimatrix edges for fast traversal); not needed for editing. Note as carry-forward. |

**Does editing this class jump ahead of phase 4?** **Yes — the propose-edit (diff/PR) slice does.** Reasoning: the W5 gate exists because a *server-authored knowledge mutation* must be attributable to a distinct human, and on the bearer path it is not. The PR model **never has the server author the change** — it produces a proposal the human accepts under their own identity through the normal git/GitHub review surface, which already carries non-spoofable authorship. So this slice is *not gated on the human-identity spike*. What remains gated on phase 4 is **server-side direct commit** (target a) and **knowledge-entity editing** (target b / the original W5 path) — those still need distinct human attribution before they ship.

**Recommendation**: Adopt the **emit-a-diff / open-a-PR write-back model (c)** for the editable workflow graph. The server reads `.claude/**` (read-only), the UI presents an edit, and the server returns a **validated unified diff** (or opens a PR via the operator's existing GitHub token) — it does **not** write repo files or author commits. This (1) makes attribution the human's real git/GitHub identity (W5 blocker bypassed for this node class), (2) requires **no repo-write, no path-traversal guard, no concurrent-edit lock** (git arbitrates), (3) keeps the source of truth in the files Claude Code loads, and (4) preserves Principle 6 (no new infra — reuse the operator git token; the server stays an adapter). Gate **direct server commit** behind both the human-identity spike *and* a repo-awareness/path-guard design; do not build it for the MVP.

---

### Q (W10): Where does "view + edit the workflow graph" land on the feasibility map, what is its MVP and phasing, and does it reorder the original W6 phasing that deferred all editing to phase 4?

**Answer**: **View** the workflow graph is **moderate** (a derivation + the same read-only renderer W3 already justifies). **Edit** via the PR/diff model is **moderate** (no new identity, no repo-write) — *not* the **hard/gated** tier the original map assigned to all editing. So **W10 does partially reorder W6**: it splits "editing" into two classes and lets **workflow-artifact editing (propose-as-diff) move out of phase 4 into an earlier phase**, while **knowledge-entity editing and server-direct-commit stay in the gated phase 4.**

**Re-tiered feasibility (this use case, against the original map)**:

| Capability (this use case) | Tier | Determining constraint |
|---|---|---|
| **Derive the workflow graph from `.claude/`** (W8) | **moderate** | Path + `/slash` + frontmatter extractor; deterministic for explicit edges, heuristic for name-mentions; must render dangling edges. |
| **View the workflow graph (read-only)** | **easy–moderate** | Reuses the W3 renderer + W7 provider contract; the workflow graph is tiny (~50 nodes: 5 protocols + 21 agents + 16 skills + 1 rule + CLAUDE.md) — **far below the 200-node ceiling**, so no scale problem at all. |
| **Edit a workflow node → propose as diff/PR** (W9c) | **moderate** | No new identity (human's git identity at merge), no server repo-write, no concurrency lock, no traversal guard. Net-new work = the diff/PR-emit surface + frontmatter validator. |
| **Edit a workflow node → server writes file + commits** (W9a) | **hard — gated** | Needs repo-awareness, path-traversal guard, concurrency control, git wiring, AND distinct git identity (= phase-4 identity spike). |
| **Edit a knowledge node** (original W5) | **hard — gated (unchanged)** | Still `context_correct` + `http-bearer` collapse; phase-4 identity spike. |

**MVP for the killer use case (cheapest first useful slice)**:
> The embedded UI (W1) renders the **workflow graph** from a new read-only `GET /workflow-graph` endpoint that runs the W8 extractor over `.claude/` and returns `{nodes, edges}` in the W7 shape (with `confidence` + dangling-edge markers). The human navigates protocol→agent→skill→rule visually and **sees broken links**. Editing is **view-first**; the first *edit* increment is **"propose change as a diff"** (W9c) — the UI posts an edited node body, the server validates frontmatter and returns a unified diff for the human to apply/commit through their normal git flow. **No server repo-write, no new identity, no streaming, no knowledge-entity editing.**

**Does it reorder W6?** **Yes, partially — and this is the explicit reconciliation:**
- The original W6 phasing put **all editing in phase 4, gated on the identity spike.** That remains correct **for knowledge-entity editing and for server-direct-commit.**
- This use case introduces a **new node class (filesystem/git-tracked) whose edit-as-diff path is NOT gated on identity** (W9). Therefore the revised phasing **pulls workflow-graph view + propose-edit-as-diff forward**, ahead of phase 4:
  - **Phase 1 (unchanged)**: embedded UI + switcher + polled lens.
  - **Phase 2 (revised)**: read-only graph **+ workflow-graph view** (W8 extractor; the workflow graph is the smallest, most self-contained graph and a strong first graph target) + client highlighting.
  - **Phase 3 (NEW, pulled ahead of the old phase 4)**: **workflow-node editing via propose-as-diff/PR** (W9c). Unblocked by the identity spike. This is the "step-change in authoring ergonomics" the human named.
  - **Phase 4 (unchanged, still gated)**: knowledge-entity editing via `context_correct` **and** server-direct-commit of workflow files — both blocked on the human-identity spike (+ repo-awareness/path-guard design for direct-commit).

**Recommendation**: Re-tier the killer use case as **view = moderate, edit-as-diff = moderate (ungated)**, and **revise W6 to pull workflow-graph view + propose-edit-as-diff into phases 2–3, ahead of the phase-4 identity gate.** Ship the workflow graph as the **first** graph (it is small, self-contained, and the diff/PR edit model needs no new identity), proving the W7 provider contract and the graph renderer on a low-risk, high-value target before tackling the larger, identity-gated knowledge graph. Keep knowledge editing and any server-direct-commit firmly in gated phase 4.

---

### Additions to existing lists (W7–W10)

**New Unanswered Questions**:
- **Should an authoring-convention be introduced to make workflow edges explicit?** (outbound-reference frontmatter keys `uses_skills:`/`uses_rules:` to upgrade derived/heuristic edges to explicit and gain rename-safety) — a convention/design decision, not a feasibility verdict; deferred to a design session.
- **Diff-only vs server-opens-PR for the W9c model** — both avoid the identity gate, but "server opens a PR via the operator's GitHub token" needs a decision on whether the server should hold/use that token (a lighter cousin of the repo-write question). Needs a small design decision, not a spike.
- **Does the personal-cloud deployment even co-locate the `.claude/` repo source with the server's data volume?** If not, the read-only extractor itself needs a configured repo path. Deployment-topology question; reason: depends on how operators actually run the cloud image, not determinable from code alone.

**New Out-of-Scope Discoveries**:
- **Three live dangling references in `.claude/` today** — (1) `/uni-record-outcome` referenced 6× but no such skill exists; (2) `.claude/agents/uni/coordinator (you).md` referenced by path in `uni-agent-routing.md:47` but absent; (3) `/uni-design-protocol` written in `/slash` (skill) form in CLAUDE.md though it is a protocol. These are workflow-graph integrity gaps, not code defects — fixable by a human edit (and exactly what the proposed graph view would surface). Flagged, not pursued.
- **Skill frontmatter is inconsistent** — `uni-git/SKILL.md` lacks the YAML frontmatter the other 15 skills have; any frontmatter-keyed tooling would silently drop it. Minor authoring-hygiene carry-forward.
- **Two parallel protocol families coexist** — legacy non-uni protocols (`agent-routing.md`, `implementation-protocol.md`, `planning-protocol.md`, `swarm-protocol.md` at `.claude/protocols/` root) alongside the active `uni/` family. A naive extractor would graph both; the derivation must scope to `uni/` (or the graph must distinguish active vs legacy). Carry-forward for the extractor design.
- **`uni-agent-routing.md` is a hand-maintained de-facto edge manifest** — it already encodes the roster + routing as prose tables. A future "explicit edges" convention could promote it (or generate it) from parsed frontmatter, making it the single source of truth instead of drift-prone documentation. Reusable-pattern carry-forward.

**New Follow-on spikes** (appended to the existing list):
5. **Workflow-graph extractor design & edge-confidence model.** Scope: design the deterministic `.claude/` parser (frontmatter + path + `/slash` extraction), the `explicit|derived|heuristic` confidence model, dangling-edge resolution, and whether to introduce outbound-reference frontmatter keys for rename-safety. Includes scoping to the active `uni/` family vs legacy protocols.
6. **Diff/PR write-back surface for filesystem nodes.** Scope: design the `GET /workflow-graph` read + the propose-edit-as-diff endpoint (frontmatter validation before emit; diff-only vs server-opens-PR via operator token; optimistic concurrency if direct-write is ever added). Explicitly the *ungated* editing path — does not wait on follow-on #1 (human identity).
   - Note: the original follow-on #1 (human edit identity/attribution) remains the gate for **knowledge-entity editing and server-direct-commit only** — it no longer blocks *all* editing.
