## ADR-001: The Dumb-Client Invariant — Server Is the Sole Authority on Route Shape (spine, SR-01)

### Context

#766 happened because the client *composed* a path the server never authored: the no-slug init resolves `remote = base_url + "/v1"` (`init.js:307`) and the hook transport appends `/observe` (`transport-http.js:84`), yielding `/v1/observe` — a path that does not exist server-side (observe is split off at the top level before slug routing, `router.rs:188`). The bug is not the one route; it is the *bug class*: any client that applies route grammar can synthesize a path the server did not author.

vnc-034 ADR-001 (#4954) made the bundle cloud-wide and slug-free, which forced the client to append the slug and derive the observe path itself — i.e. it *required* client-side path composition. That is the structural hole this feature closes. SR-01 rates the bet High: the design pays off only if **every** client compose site is eliminated, not just the observe one.

The complete closed set of current client path-composition / identity-derivation sites:

| # | Site | What it composes | Source |
|---|------|------------------|--------|
| C-1 | slug append | `endpointBase + "/v1/" + slug` | `init.js:305` |
| C-2 | default-alias append | `endpointBase + "/v1"` (→ `/v1/tools/...`) | `init.js:307` |
| C-3 | observe path append | `u.pathname.replace(/\/+$/,"") + "/observe"` | `transport-http.js:84` |

C-1 and C-2 apply MCP route grammar at attach time; C-3 applies observe route grammar at every hook post. All three are the same defect — the client knowing route shape.

### Decision

**The server is the sole authority on route shape. The client composes no paths and derives no identity — it reads fully-formed endpoint URLs from the validated bundle and posts to them verbatim.**

Concretely, all three sites are designed out:

- **C-1 and C-2 are deleted.** `resolveRemoteTarget` no longer branches on `options.slug` and no longer appends `/v1` or `/v1/<slug>`. The decoded `v:2` bundle (ADR-002) already carries the finished MCP URL the server composed; the client stores it verbatim as `remote`. There is no `--slug` flag in the attach grammar anymore (the slug lives inside the server-composed URL, not as a client argument), and `assertSlugAllowlist` in the client is retired — slug validation is a server concern at register time (ADR-004 vnc-034 grammar, now enforced server-side only).
- **C-3 is deleted.** The hook transport no longer appends `/observe`. It reads the finished observe URL from client config (sourced verbatim from the bundle's `observe_url`, ADR-002) and POSTs to it. `transport-http.js` is given the full observe URL, not a base to which it appends a suffix.

**Invariant test (byte-for-byte, the load-bearing proof for SR-01):** a parity-style test feeds a known `v:2` bundle through `decodeBundle` → `init` and asserts that the URL the client posts MCP to, and the URL the hook transport posts observe to, are **byte-identical** to the `mcp_url` / `observe_url` strings the server placed in the bundle — no concatenation, no normalization, no suffix. The test fails if the client mutates the URL in any way. This is the structural guard that no compose site silently returns.

Route grammar is centralized server-side in `parse_project_key` (`seam.rs`, the single test surface) plus the URL *composition* helper that builds the bundle URLs (ADR-002). The client cannot produce a path the server did not author because the client never builds a path.

### Consequences

- **Easier:** #766's bug class is structurally impossible — there is no client code that applies route grammar, so no client path can diverge from server reality.
- **Easier:** Route grammar lives in exactly one place (server), one test surface; changing a route is a server-only change with no client re-release for path shape.
- **Easier:** Consistent with architectural principle #6 ("the client is an adapter, not infrastructure").
- **Harder:** The bundle must now carry two URLs (MCP and observe), making the `v:2` schema change mandatory and atomic (ADR-002) — the client cannot be a "dumb" poster until the server hands it finished URLs.
- **Harder:** The `--slug` attach flag and client-side slug allowlist are removed; operators no longer pass a slug at `init` (they receive a per-project bundle whose URL already encodes the slug). This is a deliberate devex shift, paired with the uniform `register` command (ADR-005).

### Related

- ADR-002 (this feature): the `v:2` bundle that carries the finished `mcp_url` and `observe_url` the client posts to verbatim.
- ADR-003 (this feature): the server-owned per-slug `/v1/{slug}/observe` route the `observe_url` points at.
- vnc-034 ADR-001 (#4954): the slug-free `v:1` bundle whose "append slug client-side" contract this ADR supersedes.
- vnc-034 ADR-005 (#4949): the `/v1/tools/...` default alias whose client-side composition (C-2) this ADR deletes.
