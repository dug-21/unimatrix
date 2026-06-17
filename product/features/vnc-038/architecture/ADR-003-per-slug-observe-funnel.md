## ADR-003: Per-Slug Observe on the Per-Request Funnel — Sole Route, No Boot-Bound Fallback (SR-03)

### Context

Today `/observe` is a top-level route split off in `PathRouter` *before* slug routing (`router.rs:188`), and its store handle is bound ONCE at boot: `main.rs:1048` calls `resolve_store(&ProjectKey::Default)` and threads that single `Arc<Store>` into `ObserveContext` (#5079, #4963). Observe is therefore structurally single-store — every project's telemetry would land in the one boot-bound store. That is exactly why `/v1/observe` 404s (#766): the client posts observe to a slug-shaped path that the top-level split never sees, and `parse_project_key` then rejects `observe` as a candidate slug (`seam.rs:187`).

RD-3 resolves this: observe becomes a **server-owned per-slug route on the same per-request funnel as MCP**. The client posts where the bundle's `observe_url` tells it and never decides where observe lives.

SR-03 is the dominant risk here. vnc-034 shipped the per-request funnel *ceremonially* once (#4974): the resolved handle went into `let _store` (discarded) while a parallel fixed adapter served the request, so every N=1 test was green while the funnel carried no value. Re-routing observe risks repeating the trap — a boot-bound `ObserveContext` left in place "for compatibility" beside the new per-slug route would be exactly that bypass, invisible at N=1.

### Decision

**Observe is a per-slug route `/v1/{slug}/observe` resolved through the SAME `StoreResolver::resolve_store` funnel as MCP. The resolved per-request handle is the SOLE observe store route — the boot-bound `resolve_store(Default)` binding and the `ObserveContext`-holds-a-single-store construction are DELETED, not supplemented.**

Concretely:

1. **Route grammar.** `/v1/{slug}/observe` is parsed by the unified route grammar (ADR-004), yielding `ProjectKey::Slug(slug)` (or the local-UDS identity, ADR-006). The top-level pre-slug split of `/observe` in `PathRouter` is removed for the served-project model; `/observe` is no longer a top-level route. (`/health` stays top-level — it is store-independent.)
2. **Per-request resolution.** The observe handler resolves its store *per request* via `resolver.resolve_store(&key)` — the identical funnel MCP uses. There is no `ObserveContext` carrying a pre-resolved `Arc<Store>`; the handler is constructed with the `Arc<dyn StoreResolver>` (the same one `SlugRouter` holds) and resolves on each call.
3. **No parallel path.** The boot-time `resolve_store(Default)` call at `main.rs:1048` is removed. There is no fixed/default observe store a request can reach without going through the resolver. As in vnc-034 Wave 2's `adapter_for`, the resolved handle must be the *only* way observe reaches a store — no trait default, no `Option` fallback that silently returns the boot store.

**N=2 isolation proof (the SR-03 / #4974 guard — required, not N=1).** The acceptance proof registers TWO projects and asserts: an observe POST to `/v1/{slug-A}/observe` writes telemetry into project A's store and NOTHING into project B's store, and vice-versa. A residual boot-bound or default observe path passes at N=1 (one store, indistinguishable) and ONLY fails at N=2 — so the proof MUST be written at N=2. The #4974 checklist applies verbatim: grep the observe handler for a discarded/boot-bound handle; confirm no parallel observe dispatch exists beside the funnel.

### Consequences

- **Easier:** Init-time validation Ping (AC-07) and runtime hook telemetry (AC-08) resolve through one funnel — #766 closed by construction for both, not by rerouting one path.
- **Easier:** Observe inherits the MCP funnel's isolation guarantee for free — mis-routing telemetry to the wrong project is unrepresentable because identity is transport-derived (the slug in the URL), never in the observe payload.
- **Easier:** One funnel, one isolation proof — no separate "observe is single-store by construction" carve-out to reason about.
- **Harder:** Observe now pays a per-request `resolve_store` lookup instead of a boot-time bind. Bounded by the single-operator N and the same per-slug hot-cache the MCP path already uses.
- **Harder:** Removing the top-level `/observe` split is a route-grammar change on the same surface as #735 (SR-06); sequencing with #735 is a leader concern.

### Related

- ADR-001 (this feature): the client posts to the finished `observe_url` from the bundle — it never composes the observe path (the C-3 site this enables).
- ADR-002 (this feature): the `observe_url` field the bundle carries.
- ADR-004 (this feature): the unified resolver and route grammar `/v1/{slug}/observe` parses through.
- vnc-034 ADR-003 (#4950): the `resolve_store` single funnel this ADR extends to observe.
- #4974: the ceremonial-funnel trap whose checklist this ADR's N=2 proof discharges.
- #5079: the boot-bound `resolve_store(Default)` observe binding this ADR removes.
