## ADR-004: Delete the Default — Unified Resolver, Single Project Is N=1 (RD-5, SR-04)

### Context

The cloud/container model has two ways to reach a store: the `/v1/tools/...` default alias → `ProjectKey::Default` (vnc-034 ADR-005 / #4949), and `/v1/{slug}/...` → `ProjectKey::Slug`. The default alias is the zero-step path everyone lands on, and it is the one path where a bundle can silently land on the wrong/default store (the integrity hole, SCOPE C5). It is also the source of #766: `/v1/observe` reaches `parse_project_key`, where `observe` is treated as a candidate slug and rejected.

The current grammar (`seam.rs:178-193`):
```text
/v1/tools/...      -> ProjectKey::Default     (default alias)
/v1/{slug}/...     -> ProjectKey::Slug(slug)
(anything else)    -> ProjectKey::Default     (backward-compat fallback)
```
The boot swap (`main.rs:1004`) selects `DefaultResolver` when `[[projects]]` is empty, else `MultiProjectRouter`. `MultiProjectRouter` itself still carries a `default: Option<ProjectEntry>` arm (`project_resolver.rs:86`) and `resolve_store`/`adapter_for` both branch on `ProjectKey::Default` (`project_resolver.rs:199-223`).

RD-5: hard cut. Delete `DefaultResolver`, the `/v1/tools → Default` arm, and the `_ => Default` fallback. A single-project deployment is just N=1 — one registered slug through the same resolver, no special-case path. SR-04 warns the blast radius touches the central route grammar and the reserved-slug set, and must not break the local-UDS path-hash identity (AC-10, handled in ADR-006).

### Decision

**There is one resolver and one route grammar. The default route, `DefaultResolver`, and every `ProjectKey::Default` served-project arm are deleted. Single-project = N=1 through the unified resolver.**

1. **`ProjectKey` loses `Default` for the served-project model.** `ProjectKey` becomes effectively `Slug(ProjectSlug)` for cloud/container routing. (The local-UDS single store is addressed by ADR-006's mechanism, NOT by reviving `Default`.)

2. **`parse_project_key` collapses to one rule:**
   ```text
   /v1/{slug}/...          -> ProjectKey::Slug(slug)   (slug allowlist-validated at the edge)
   /v1/{slug}/observe      -> ProjectKey::Slug(slug)   (observe is just another segment under the slug — ADR-003)
   (no valid slug segment)  -> RouteError (loud 404/400), NEVER a default
   ```
   The `(Some("v1"), Some("tools")) => Default` arm and the `_ => Ok(Default)` fallback are removed. A path with no registered slug fails loud — it does not silently resolve a default store (AC-01, AC-09).

3. **`MultiProjectRouter` is the sole resolver.** It is renamed/retained as the single `StoreResolver` impl. Its `default: Option<ProjectEntry>` field, the `ProjectKey::Default` arms in `resolve_store` and `adapter_for`, and `from_servers`' `default_store`/`default_server` parameters are removed. It is built purely from the `[[projects]]` slug set:
   ```text
   resolve_store(Slug(s)) -> slugs.get(s) or RouteError::UnknownProject
   adapter_for(Slug(s))   -> slugs.get(s).adapter  (SOLE dispatch route; no default arm to fall through to)
   ```
   `adapter_for` keeps NO trait default and NO fallback (the #4974 guard): an unregistered slug is a hard `UnknownProject`, never a silent default.

4. **Boot swap deleted.** `main.rs:1004`'s `if project_slugs.is_empty() { DefaultResolver } else { MultiProjectRouter }` branch collapses to: build the unified resolver from `project_slugs`. **Empty `[[projects]]` ⇒ nothing servable** — the resolver has zero slugs, every served request returns `UnknownProject`, and first boot logs the loud actionable "register a project to begin" (AC-09, RD-1). No store is auto-served.

5. **N=1 is not special.** One registered slug is one entry in `slugs`. The same code serves it as serves N entries. There is no "single-project" branch.

### Consequences

- **Easier:** Mis-routing to a default store is structurally impossible — there is no default store and no default arm. The integrity hole (C5) is closed by deletion, not by validation.
- **Easier:** One resolver, one route rule, one test surface — the central grammar is simpler than the three-arm version it replaces.
- **Easier:** `MultiProjectRouter`'s `adapter_for`/`resolve_store` lose their `Default` branches, removing the residual surface where a default could leak in.
- **Harder:** Hard cut with no fallback — a no-slug deployment that worked under vnc-034 stops working. Acceptable because there are no existing served users (RD-1, validated assumption); first boot fails loud, not silent.
- **Harder:** The reserved-slug set must be re-derived because `tools` is no longer a default-alias literal and `observe` now appears under a slug (ADR-005).
- **Harder:** Touches the same router/boot surface as #735 (SR-06) — leader sequences accordingly.

### Related

- ADR-003 (this feature): observe routes through the same unified resolver as a per-slug segment.
- ADR-005 (this feature): the reserved-slug re-derivation forced by deleting the `tools` default alias.
- ADR-006 (this feature): how local UDS addresses its single store WITHOUT reviving `ProjectKey::Default` (resolves the AC-10 ↔ RD-5 tension).
- vnc-034 ADR-005 (#4949): the default alias this deletes — deprecated in Unimatrix via `context_correct`.
- vnc-034 ADR-003 (#4950): the `resolve_store` seam this prunes to a single resolver.
- #4974: the ceremonial-funnel checklist — `adapter_for` keeps no default fallback.
