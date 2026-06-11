## ADR-003: The `resolve_store` Isolation Seam — One Trait, Two Resolvers, Single Funnel (C4)

### Context

C4 is the URL/route structure plus the store-resolution seam — THE isolation seam. It carries the cross-project knowledge-integrity guarantee (C5) and is **shared with the local-UDS install** (not a cloud-only path). It must satisfy one isolation invariant identically in all three modes: local UDS single-project (path-hash, ADR-004 #80), cloud single-project alias, and cloud multi-slug.

Two historical traps converge here:
- **SR-07 (deferred-seam trap, #4869):** the seam is built minimal in Wave 1 and populated in Wave 2. Deferred seams break when later-wave routing is bolted on *outside* the earlier-wave method, or when a source-assertion/revert gate false-positives on the new edge.
- **SR-08 / A2 / A4:** if the cloud slug path and the local path-hash path are two separate code paths, the "shared seam = proving ground" guarantee is lost and local parity (a non-negotiable constraint) silently regresses. The ADR-004 path-hash assumption ("moving a project changes its hash") must NOT leak into cloud, where the slug is operator-declared and path-independent (C5).

The existing code routes every MCP request through a pass-through `ProjectRouter` to a single `default_server: McpAdapter` (router.rs:297–364). There is exactly one place where a request becomes a store handle today.

The C4 invariant the design MUST hold in all modes:
1. Project identity comes from the **transport, never the request payload** — URL slug (cloud) or daemon path-hash (local). The agent has no field to name another project; mis-targeting is unrepresentable.
2. The resolved `Arc<Store>` is the **sole write capability**, threaded from the routing edge.
3. **Single funnel, no bypass** — every read/write resolves through this one seam.

### Decision

Introduce one seam interface, owned by the caller (the routing layer), with the resolver injected:

```rust
/// Transport-derived project identity. Constructible ONLY from the transport
/// (URL path or daemon path-hash) — never from a request payload.
pub enum ProjectKey {
    Default,             // slug-free: local path-hash store, or cloud single-project alias
    Slug(ProjectSlug),   // cloud multi-project (Wave 2)
}

pub trait StoreResolver: Send + Sync + 'static {
    /// THE single funnel. Every read/write in the process resolves here.
    fn resolve_store(&self, key: &ProjectKey) -> Result<Arc<Store>, RouteError>;
}
```

Two resolvers behind the one trait:

- **Wave 1 — `DefaultResolver { store: Arc<Store> }`**: returns `store` for `ProjectKey::Default`; returns `RouteError::UnknownProject` for any `Slug` (so `/v1/{slug}/...` parses but is inert until Wave 2 — additive, no client re-init). The local UDS daemon constructs `DefaultResolver { store: <path-hash store from ADR-004> }`; the cloud single-project install constructs `DefaultResolver { store: <the one project store> }`. **Same resolver, same code** — local parity is exercised by the identical seam the cloud uses.

- **Wave 2 — `ProjectRouter` implements the same trait**: `resolve_store(Slug(s))` looks `s` up in the `[[projects]]` slug map → per-slug `Arc<Store>`; `resolve_store(Default)` returns the optional default. It is a **drop-in swap at the same call site** — no new edge.

A new `SlugRouter` layer sits between `PathRouter` and the `McpAdapter`:
1. Parse the path: `/v1/tools/...` → `ProjectKey::Default`; `/v1/{slug}/tools/...` → `ProjectKey::Slug(ProjectSlug::try_from(slug)?)` (allowlist-validated, C5/ADR-004).
2. Call `self.resolver.resolve_store(&key)` → `Arc<Store>`.
3. Thread that store into the per-project `McpAdapter` for dispatch.

**Per-slug hot-path routing lives INSIDE the seam method** (Principle #7 per-slug caches rebuilt per project by tick), not in a separate edge (SR-07). The seam is the single funnel; the source-assertion/revert gate sees one method, not a scattered later-wave addition.

**In-process multi-store, NOT process-per-project** (C4): safe Rust (no `unsafe`, no `.unwrap()` in non-test) plus invariant #2 (the resolved `Arc<Store>` is the only write handle threaded from the edge) provide isolation without an OS process boundary. Process-per-project would tax the single-binary goal and the hot path at N× model memory for a boundary safe Rust already provides.

### Consequences

- **Easier:** Wave 2 is a single trait-impl swap (`DefaultResolver` → `ProjectRouter`) at one call site — no interface re-cut, no client migration (SR-07 neutralized).
- **Easier:** Local and cloud share the exact seam, so the common local install is the proving ground for cloud isolation (SR-08 / A4 satisfied); a local-install regression test in the Wave-1 acceptance set guards parity.
- **Easier:** The single funnel earns proof-grade treatment in one place; mis-targeting is unrepresentable because identity is transport-derived (invariant 1, SR-06).
- **Easier:** Enterprise extends the same seam (slug → JWT claim on the resolver) — additive (SR-04).
- **Harder:** Wave 1 must build the full seam (trait + `SlugRouter` + `DefaultResolver`) even though only the Default path is exercised — slightly more Wave-1 work than a bare single-store handler. This is the deliberate cost that buys A4 (the seam is genuinely exercised, not bypassed).
- **Harder:** Per-slug hot caches (Principle #7) must be keyed and tick-rebuilt per project in Wave 2 — more memory and tick work as N grows. Bounded by the single-operator N.

### Related

- ADR-004 (#80): the local path-hash that feeds `DefaultResolver` for the local install — this ADR does NOT change it; it wraps it behind the trait.
- C5 / ADR-004 (this feature): `ProjectSlug` allowlist; register/attach decoupled from path-hash.
- ADR-005 (OQ-C): the `/v1/tools/...` alias that maps to `ProjectKey::Default` and makes Wave 2 additive — must be locked before this seam's route shape.
- C6: the resolver is the data-scope seam; auth (`BearerValidator`) and transport (cert) are separate concerns.
