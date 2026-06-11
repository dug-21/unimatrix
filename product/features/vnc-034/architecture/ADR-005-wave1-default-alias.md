## ADR-005: Wave-1 Single-Project Addressing — `/v1/tools/...` Default Alias, Not a Mandatory Default Slug (resolves OQ-C)

### Context

OQ-C: does Wave 1 serve at the `/v1/tools/...` default-project alias, or require a default slug (`/v1/{default}/tools/...`) from day one? This is the highest-leverage route-shape decision and must be resolved **before** locking the C4 route grammar (ADR-003), because:

- **SR-05:** the choice determines whether Wave 2 is purely additive or forces a client-breaking re-init. Wave-1 clients are real installs (operator + N LLM clients onboarded over pinned TLS). If Wave 1 addresses at a slug that Wave 2 changes the meaning of, every Wave-1 client must re-`init` — a client-breaking migration.
- The existing code already serves MCP at `/v1/tools/...` with a pass-through router (router.rs); the local UDS install has no slug at all.
- The C4 invariant requires the **same seam** for local UDS (slug-free, path-hash), cloud single-project, and cloud multi-slug. A mandatory default slug would put a slug on the cloud single-project path that the local path has no analog for — straining the "one seam" guarantee.

### Decision

Wave 1 serves at **`/v1/tools/...`** — the slug-free default-project alias. It maps to `ProjectKey::Default` (ADR-003). There is **no** mandatory default slug.

Route grammar (locked):
```
/v1/tools/...           → ProjectKey::Default     (Wave 1: local UDS + cloud single-project)
/v1/{slug}/tools/...    → ProjectKey::Slug(slug)  (Wave 2: cloud multi-project — ADDITIVE)
```

In Wave 1, `/v1/{slug}/...` parses to `ProjectKey::Slug` but the `DefaultResolver` returns `RouteError::UnknownProject` for it — the route *shape* exists, only the resolver is inert until Wave 2. When Wave 2 swaps `DefaultResolver` → `ProjectRouter`, slug routes light up **without touching the `/v1/tools/...` alias**: slug-free requests keep resolving to `ProjectKey::Default` (the optional default store). Wave-1 clients, all addressing `/v1/tools/...`, are unaffected.

This is the additive alias R-b recommends. It also keeps the local UDS install (which has no slug) on the identical `/v1/tools/...` → `ProjectKey::Default` path, so the local and cloud single-project routes are byte-identical (reinforcing ADR-003's "one seam" / SR-08).

### Consequences

- **Easier:** Wave 2 is purely additive — no Wave-1 client re-init, no migration (SR-05 neutralized). Cert rotation (re-bundle + re-`init`) remains the *only* documented client re-init event.
- **Easier:** The local UDS install and the cloud single-project install share the exact `/v1/tools/...` route → `ProjectKey::Default`, so local parity is exercised on the same path (SR-08 / ADR-003).
- **Easier:** `[[projects]]`-absent ⇒ `/v1/tools/...` unchanged is the natural backward-compat behavior (Goals §14), not a special case.
- **Harder:** A cloud that later adds slugs has a "default" project addressable two ways (`/v1/tools/...` and never as a slug). This is intentional — the default is the alias, not a named slug. Operators who want every project named simply never use the default alias in multi-project mode. Documented as the migration story: a single-project cloud's data stays reachable at the alias; promoting it to a named slug is an explicit (optional) operator action, not forced.

### Related

- ADR-003 (C4 seam): this ADR fixes the route shape the seam parses; `ProjectKey::Default` is the alias target.
- ADR-004 (C5): slugs are additive on top of this alias; attach appends `/v1/{slug}`.
- Goals §14 (single-project backward-compat): `[[projects]]` absent ⇒ `/v1/tools/...`, zero change — this ADR is that guarantee.
