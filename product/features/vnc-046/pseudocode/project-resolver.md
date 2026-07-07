# Component: project-resolver (`MultiProjectRouter` / `ProjectEntry`)

**Source:** `crates/unimatrix-server/src/http/router/project_resolver.rs`
**ADR:** ADR-001 (funnel), ADR-002 (construction it consumes) · **Risks:** R-03, R-06

## Purpose

Hold the per-slug handles the funnel hands back, and implement the 3 new `StoreResolver` methods.
Convergence is **by construction**: `from_server` `Arc::clone`s registry/pending/services off the
assembled `server` **before** `server` moves into `McpAdapter::new`, so the entry's handles and the
server's fields are clones of one `Arc` — `resolver.registry_for(&slug)` returns the same instance
the slug's `UnimatrixServer` reads.

## `ProjectEntry` — 3 new fields (`project_resolver.rs:50`)

```
pub(crate) struct ProjectEntry {
    store: Arc<Store>,                                         // existing
    adapter: McpAdapter,                                       // existing
    // NEW (ADR-001) — Arc clones off `server` before it moves into the adapter:
    session_registry: Arc<SessionRegistry>,
    pending_entries_analysis: Arc<Mutex<PendingEntriesAnalysis>>,
    services: ServiceLayer,
}
```
Debug impl: leave `finish_non_exhaustive()` as-is (new handle fields are not printed).

## `ProjectEntry::from_server` — clone BEFORE move (ordering is load-bearing)

Current body (`project_resolver.rs:82-91`) constructs `McpAdapter::new(server, ...)`, consuming
`server`. Insert the clones **before** that line.

```
FUNCTION from_server(store, server, max_body_bytes, allowed_origins, allowed_hosts):
    // (2) CLONE-BEFORE-MOVE — must precede McpAdapter::new which consumes `server`.
    session_registry         = Arc::clone(server.session_registry)          // pub field
    pending_entries_analysis = Arc::clone(server.pending_entries_analysis)  // pub field
    services                 = server.service_layer()                       // accessor → ServiceLayer clone
    // (existing) construct the adapter — consumes `server`.
    adapter = McpAdapter::new(server, max_body_bytes, allowed_origins, allowed_hosts)
    RETURN ProjectEntry { store, adapter, session_registry, pending_entries_analysis, services }
```

Notes:
- `server.session_registry` / `server.pending_entries_analysis` are `pub` (`server.rs:226,229`).
- `service_layer()` accessor already used at `main.rs:1234`; returns a `ServiceLayer` (Clone of Arcs).
- **Do not** re-mint any handle here (R-03/R-06): a fresh `SessionRegistry::new()` would break
  convergence and defeat the whole feature. Clone off `server` only.
- The `store` param is already the handle `server` dispatches against (existing OQ-PR-4 invariant);
  the three new handles are likewise the server's own — no new agreement obligation beyond cloning.

## `MultiProjectRouter` — impl the 3 methods (`project_resolver.rs:197`)

Add to `impl StoreResolver for MultiProjectRouter`, mirroring `resolve_store`/`adapter_for`:

```
fn registry_for(self, key):
    MATCH key: ProjectKey::Slug(s):
        MATCH self.slugs.get(s):
            Some(entry) => Ok(Arc::clone(entry.session_registry))
            None        => Err(RouteError::UnknownProject)

fn pending_for(self, key):
    MATCH key: ProjectKey::Slug(s):
        MATCH self.slugs.get(s):
            Some(entry) => Ok(Arc::clone(entry.pending_entries_analysis))
            None        => Err(RouteError::UnknownProject)

fn services_for(self, key):
    MATCH key: ProjectKey::Slug(s):
        MATCH self.slugs.get(s):
            Some(entry) => Ok(entry.services.clone())
            None        => Err(RouteError::UnknownProject)
```
Total over `ProjectKey`, map lookup + clone only — no `.unwrap()`, no panic, no I/O (matches the
existing `resolve_store` discipline, `project_resolver.rs:204`).

## `from_servers` — unchanged signature

`from_servers` still takes `Vec<ProjectServerInput>` and builds entries via `from_server`. No new
params: the per-slug handles ride inside each `input.server` and are cloned in `from_server`. Duplicate
slug check unchanged.

## Data Flow

- **In:** assembled per-slug `UnimatrixServer` (from `build_project_server`, now carrying wired
  registry/hold/pending/config — see `project-provisioner.md`).
- **Out:** `ProjectEntry` holding Arc clones of the slug's registry/pending/services; the 3 resolver
  methods hand those back per request.

## Error Handling

Only `UnknownProject` (unregistered slug). Reached only after `resolve_store` — see the R-14 error
boundary in `resolution-funnel.md` / `observe-handler.md`.

## Key Test Scenarios (hints)

- `Arc::ptr_eq(router.registry_for(&Slug(a))?, a_server.session_registry)` == true; and for pending.
- `router.services_for(&Slug(a))?` resolves the slug's layer (store handle matches `resolve_store`).
- N=2: `registry_for(&Slug(a))` and `registry_for(&Slug(b))` are **distinct** `Arc`s (`!ptr_eq`).
- Unregistered slug → `Err(UnknownProject)` on all three.
- Clone-before-move regression: a variant that clones after `McpAdapter::new` fails to compile
  (server moved) — the ordering is enforced by the borrow checker, a free guard.
