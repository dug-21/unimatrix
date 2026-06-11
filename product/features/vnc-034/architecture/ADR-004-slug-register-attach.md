## ADR-004: Slug Identity, Register/Attach Model, and Out-of-Band Discovery (C5, resolves OQ-B)

### Context

C5 is the slug + register/attach model. The slug is **operator-declared project identity, decoupled from any client's local path-hash** — the local 1:1 path-hash assumption (ADR-004 #80: "moving a project changes its hash") must NOT leak into cloud mode, where project identity is path-independent (A2). Two distinct operations exist: **register a project** (server-side, creates the store, never client-auto-created) and **attach a client** (`init --remote …/v1/<slug>`, no store creation).

OQ-B asks how a client learns which slug to attach to: does the server expose a slug list (an endpoint or `client-bundle --list`), or does the operator tell the client out-of-band? This decides whether any listing surface — and its auth posture — is added.

Two trust/integrity concerns shape this:
- **SR-09 (#4321):** the slug parser is a trust boundary accepting untrusted operator/client input. Slug path-injection (`../`, encoded separators, absolute paths) could escape `/data/.unimatrix/{slug}/`. A missing input-validation allowlist at a trust boundary is fix-before-merge, not cosmetic.
- **SR-06:** the 1-client:1-project boundary must be enforced at the transport, not by client config, or a misconfigured client can mis-target — and a mis-targeted write permanently corrupts another project's hash chain (catastrophic, unrollbackable). The basis is knowledge-integrity, not access control.

### Decision

**Slug grammar (allowlist, SR-09).** `ProjectSlug` is a newtype with `TryFrom<&str>` enforcing:

```
^[a-z0-9][a-z0-9-]{0,62}$
```

Lowercase ASCII letters/digits and hyphen; must start alphanumeric; 1–63 chars. Explicitly forbidden: `.`, `/`, `\`, `%`, whitespace, uppercase, and any path separator or encoding thereof. Validation happens **at the parse edge in `SlugRouter`, before any filesystem use**. Because `../`, encoded separators, and absolute paths cannot pass the allowlist, slug-derived paths **cannot escape** `/data/.unimatrix/{slug}/` — escape is unrepresentable, not merely rejected.

**Register vs attach.**
- **register** (server-side CLI, Wave 2): `unimatrix project register <slug>` validates the slug, creates `/data/.unimatrix/{slug}/` (own DB, vector index, hash chain, analytics — per-slug isolation), and adds it to `[[projects]]`. The store is **never client-auto-created**.
- **attach** (`init --remote <bundle> --slug <s>`): validates `s`, appends it to the base-url (`/v1/{slug}`), pins the cert, persists client config. **No store creation.** If `s` is unregistered, the *server* returns `RouteError::UnknownProject` at connect — the client never auto-creates.

**Cardinality.** N clients : 1 slug : 1 tenant, AND 1 client : 1 project (permanent OSS boundary). The 1:1 bound is **enforced at the transport** (SR-06): each client instance bakes exactly one slug into its endpoint at `init` and has no mechanism, in protocol or config, to address a second project. Cross-project fan-out by one client is unrepresentable. Multi-LLM (Claude Code, Codex CLI, Gemini CLI) is the N-clients-one-slug case — each CLI is a distinct client instance attaching the same slug; no per-LLM code path. Same-project multi-connection is allowed; per-client cross-project fan-out is not. Enterprise relaxes via a server-enforced `unimatrix_project` JWT claim that *rejects* a wrong-project write (C6 seam) — additive, never an OSS re-architecture.

**Slug decoupled from path-hash (A2).** The cloud slug is operator-declared and stable across container moves/remounts. ADR-004 (#80) path-hash identity is used ONLY by the local UDS install (the `DefaultResolver` store). The slug map is a separate, explicit registry — the path-hash "identity = path" assumption never enters cloud mode.

**OQ-B — discovery is out-of-band (no listing surface in OSS).** The server exposes **no** slug-listing endpoint and `client-bundle` does **not** list slugs. The operator, who registered the projects, tells the client the slug out-of-band (the same person runs both sides — solo developer / single tenant). Rationale: adding a listing surface means choosing between an unauthenticated endpoint (leaks the project topology to anyone who reaches the port — rejected) or an authenticated one (extra surface, extra code, for a single operator who already knows their own slugs). The smallest attack surface wins. An **authenticated** `client-bundle --list-slugs` (bearer-gated, server-side, never over the wire unauthenticated) is left as an additive Wave-2-or-later convenience if onboarding friction proves real — recorded here as deferred, not built.

### Consequences

- **Easier:** Slug path-injection is structurally impossible (allowlist at the edge) — SR-09 closed before merge.
- **Easier:** Mis-targeting is unrepresentable (transport-enforced 1:1) — knowledge-integrity protected by construction, not by client discipline (SR-06).
- **Easier:** No new unauthenticated network surface; minimal attack surface for the single-operator model (OQ-B).
- **Easier:** Cloud project identity survives container moves/remounts (slug, not path-hash) — A2 respected.
- **Harder:** The operator must communicate slugs out-of-band — a small UX cost, acceptable because the operator owns both ends. Mitigated by the deferred authenticated `--list-slugs`.
- **Harder:** True N:N client↔project is impossible in OSS by design — a deliberate, load-bearing product bet (A1) that a solo developer never needs one client across two projects.

### Related

- ADR-003 (C4 seam): `ProjectSlug` flows into `ProjectKey::Slug`; the resolver maps it to the per-slug store.
- ADR-005 (OQ-C): single-project clients use `ProjectKey::Default` (no slug) — Wave 2 slug attach is additive.
- C6: slug scopes data (integrity boundary), token authorizes (`BearerValidator`), cert secures transport — three separate concerns; the enterprise `unimatrix_project` claim binds slug→auth additively.
- ADR-004 #80 (vnc-001): the path-hash identity this ADR deliberately keeps out of cloud mode.
