# ASS-044: Admin UI Architecture

**Date**: 2026-04-09
**Tier**: 2 (independent, parallel with Tier 1)
**Feeds**: W2-4 (admin console)

---

## Question

What technology should the Wave 2 admin console use, and what is the minimum viable feature scope for the enterprise tier?

---

## Why It Matters

The admin console is the primary human management surface for the enterprise tier. Technology choice directly affects: whether the single-binary model is preserved, the build pipeline complexity, future extensibility toward the Matrix phase knowledge explorer, and the operational model (separate service vs. embedded).

A wrong choice here means either breaking a non-negotiable (single binary) or building something that can't grow into the full Matrix phase product without a rewrite.

---

## What to Explore

### 1. Embedding Model Options
Evaluate four approaches against the single-binary non-negotiable:

**Option A — Static assets compiled into the binary** (`include_str!` / `rust-embed` crate): HTML, CSS, JS baked into the binary at compile time. Zero external dependencies. Single binary preserved. Works in air-gapped deployments. Constraints: requires build step for UI assets before `cargo build`; UI framework must produce static output; hot-reload in development requires rebuilding the binary or using a dev proxy.

**Option B — Static assets served from a mounted volume**: UI assets in `unimatrix-shared` volume, served by the daemon. Binary stays small. Air-gap requires the assets to be in the volume. Operationally more complex (assets and binary versions must be kept in sync). Single binary property: technically preserved, but requires a mounted directory to be functional.

**Option C — Separate UI service in docker-compose**: Nginx or similar serves the UI; daemon provides the API. Breaks single-binary model explicitly. Simplest build pipeline. Higher operational surface.

**Option D — Server-side rendering via Rust templates** (askama, tera, maud): Admin pages rendered server-side, no JS build step. Minimal JS for interactivity (HTMX, Alpine.js). Binary includes templates compiled in. Smallest possible build surface. Least rich interactivity.

Evaluate each for: single-binary compatibility, build pipeline complexity, dev ergonomics, air-gap support, future extensibility to Matrix phase.

### 2. Framework Selection
Given the technology choice from §1, evaluate frameworks:

- If compiled static assets or mounted: Svelte (small bundles, minimal runtime), Preact (React-compatible, tiny), HTMX + lightweight CSS.
- If server-side rendered: askama (compile-time checked templates), maud (Rust macro DSL), tera (runtime templates).
- Evaluate: bundle size (smaller = faster air-gap bundle), build toolchain complexity, TypeScript vs. plain JS, testing story.
- The Matrix phase knowledge explorer will likely need richer interactivity (graph visualization, real-time updates). Does the Wave 2 choice compose with that, or is it a throwaway?

### 3. Wave 2 Admin Console Scope
Define what is in and out for Wave 2:

**In scope (Wave 2 prerequisite)**:
- Agent enrollment (enroll new agent, set role class admin/operator)
- Agent management (list enrolled agents, revoke credentials, view last-active)
- Project/repo registration (register a new project, link to repo hash)
- Role binding management (assign operator to project, revoke assignment)
- Audit log viewer (paginated, filterable by agent/operation/date range)
- System health (schema version, daemon uptime, project count, entry counts per project)

**Explicitly deferred to Matrix phase**:
- Knowledge entry browser (search, view, edit entries)
- Graph visualization (entry relationship explorer)
- Feature drilldown (what entries were produced by feature X)
- Prompt debugger (replay retrieval with different parameters)
- Control manager (live config changes)

Validate this scope split against the security decisions in ASS-042: does the admin console need any additional surface to support the bootstrap flow or credential management?

### 4. API Surface
Does the admin console call the existing MCP tool API, or does it need a separate admin REST/HTTP API?

- **MCP tool API**: admin console uses the same 12+ MCP tools as Claude Code clients. Requires MCP client JS library (or HTTP+SSE client). No new API surface, but MCP tool response format may not be UI-friendly.
- **Separate admin REST API**: dedicated JSON REST endpoints for admin operations. Cleaner for UI consumption, standard auth patterns (cookie session or bearer token), but doubles the API surface to maintain.
- **Hybrid**: MCP tools for knowledge operations, REST for admin-specific operations (enrollment, binding management, audit log) that don't have MCP tool equivalents.

Evaluate: maintenance overhead, auth model consistency (does the admin console authenticate via OAuth client credentials or a separate session cookie?), and whether MCP tool responses are consumable by a UI without transformation.

**Router layer decision — forward compatibility for W3 REST/WebSocket**: ASS-041 designed the W2 HTTP transport as `TcpListener → TLS → Auth middleware → StreamableHttpService`. Adding REST or WebSocket routes in W3 (UI, knowledge explorer, real-time dashboard) requires inserting a router between the auth middleware and `StreamableHttpService`. If that insertion happens in W3, it touches existing transport infrastructure. If it happens in W2, W3 routes are purely additive.

Evaluate:
- Should W2 introduce an HTTP router (e.g. axum `Router`) as the top-level dispatch layer — even if the only W2 content-port route is MCP — so that W3 additions require no structural change?
- What path namespace should be established now? (e.g. `/` or `/mcp` for MCP SSE; `/api/v1/` reserved for future REST; `/ws` reserved for future WebSocket.)
- If the admin port (8444) serves a REST API in W2, should it use the same router type as the content port (8443) for infrastructure consistency — or is the admin port REST-only, making a separate router natural?
- Axum `Router` is tower-compatible: `StreamableHttpService` can be mounted as a fallback handler inside an axum `Router`. This is the implementation path to evaluate.

This question has a direct answer from whichever API surface option is chosen: if any REST endpoints exist on either port in W2 (admin console included), introduce the router now. If both ports are MCP-only in W2, the router can defer to W3.

### 5. Build Pipeline Integration
- Where does the UI source live? `crates/unimatrix-enterprise/ui/`, a top-level `ui/` directory, or separate repo?
- What does `cargo build` produce? Does it run the UI build step (via `build.rs`)? Or is the UI built separately and the binary build assumes compiled assets are present?
- Development workflow: how does a developer iterate on the UI without rebuilding the binary on every change?
- What does `cargo install` of the enterprise binary require as prerequisites? The OSS `cargo install unimatrix` path must remain unaffected.

---

## Output

1. **Technology recommendation** — chosen embedding approach with rationale; hard requirement: single binary compatibility
2. **Framework selection** — with evaluation against bundle size, build complexity, and Matrix phase extensibility
3. **Wave 2 scope** — confirmed feature list (in/out), validated against ASS-042 security model
4. **API surface recommendation** — MCP tools, REST, or hybrid; with auth flow for the admin console itself; and explicit router layer decision (introduce axum `Router` in W2 or defer to W3) with path namespace reservation
5. **Build pipeline integration approach** — source location, build step, dev workflow, install prerequisites

---

## Constraints

- **Single binary model must be preserved** (PRODUCT-VISION.md non-negotiable) — Option C is only acceptable if explicitly approved as an exception
- Admin console authenticates via the same OAuth flow as other clients — no back-channel or hardcoded credentials
- Air-gap deployable: UI assets must be bundleable without internet access at runtime
- Full knowledge explorer is explicitly out of scope — do not let scope creep into Matrix phase territory
