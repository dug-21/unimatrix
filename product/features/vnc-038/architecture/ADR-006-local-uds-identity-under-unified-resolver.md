## ADR-006: Local STDIO/UDS Keeps Its Direct Path-Hash Store Binding — Not Routed Through the Unified Resolver (resolves AC-10 ↔ RD-5, SR-04)

### Context

This is the required reconciliation ADR. Two decided positions appear to collide:

- **RD-5 / ADR-004:** delete `DefaultResolver` and the `ProjectKey::Default` arm; single = N=1 through the unified resolver, **no special-case arm**. The unified resolver handles only `ProjectKey::Slug`.
- **AC-10 / SCOPE Non-Goals:** the local single-project STDIO/UDS install keeps its **path-hash identity** (vnc-034 ADR-004 #80) and must **NOT** be forced to take a manual slug. The cutover applies to the cloud/container served-project model only.

**An earlier revision of this ADR claimed the local path-hash is "self-registered as a key INSIDE the unified resolver." A code-cited impact analysis (GATE-2, 2026-06-17) proved that framing wrong and risky.** The local transports never touch the resolver at all:

- **Local STDIO** (`main.rs:1158`) and **local UDS** (`main.rs:859`) open the path-hash store (`~/.unimatrix/{hash}/unimatrix.db`) **directly at boot** and thread the resulting `Arc<Store>` **straight to their handlers**.
- They **NEVER** call `parse_project_key`, **never** construct the HTTP resolver (`DefaultResolver`/`MultiProjectRouter`), **never** reference `ProjectKey::Default`, and **never** touch a bundle.

Routing local through the unified resolver — as the prior ADR implied — would mean wiring local STDIO/UDS onto the HTTP per-request funnel that vnc-038 is reshaping. That introduces exactly the cross-store regression **AC-10 forbids**: a local store reached via the resolver's slug-keyed lookup is a code path that did not exist, cannot be exercised by local users today, and risks mis-binding the one local store. The reconciliation is therefore NOT "make local a resolver key" — it is "local never enters the resolver."

### Decision

**Local STDIO/UDS keeps its DIRECT path-hash store binding, untouched by the HTTP resolver. The unified resolver handles only `ProjectKey::Slug` (cloud/container HTTP). Local bypasses the resolver entirely.**

Mechanism:

1. **Direct binding, preserved verbatim.** Local STDIO (`main.rs:1158`) and local UDS (`main.rs:859`) continue to open `~/.unimatrix/{hash}/unimatrix.db` directly at boot and thread the `Arc<Store>` straight to their handlers. vnc-038 changes **nothing** on this path. There is no derive-and-self-register step, no resolver entry, no slug-map key for local.

2. **The resolver is cloud/container-HTTP only.** The unified resolver (ADR-004) is constructed and consulted **only** on the HTTP served-project surface, and resolves **only** `ProjectKey::Slug`. With the default deleted, the resolver has one code path (slug-keyed lookup) and no `Default` arm — and local never reaches it, so local imposes no requirement on the resolver's key type. RD-5's "no special-case arm" holds literally and trivially: the resolver carries no local case because local is not its concern.

3. **Two independent store-binding mechanisms, by transport.** Identity remains transport-derived (the vnc-034 C4 spirit), but via two **separate** mechanisms:
   - **Cloud/container HTTP:** slug from the URL path → `parse_project_key` → `ProjectKey::Slug` → unified resolver → `Arc<Store>`.
   - **Local STDIO/UDS:** path-hash derived from the install path at boot → direct `open(~/.unimatrix/{hash}/unimatrix.db)` → `Arc<Store>` threaded to the handler.

   These do not share the resolver. They are not two keys in one map; they are two distinct boot-time wiring paths. The local path is the pre-existing one, unchanged.

4. **No manual slug for local (AC-10 honored).** The operator never types a slug for the local install. The path-hash is derived automatically at boot exactly as today. `register <slug>` (ADR-007) is the cloud/container provisioning command only; local STDIO/UDS does not register anything and is not provisioned through `[[projects]]`.

5. **Delivery constraint (hard boundary).** Delivery MUST NOT route local STDIO/UDS through the unified resolver. The deletions in ADR-004 (`DefaultResolver`, the `/v1/tools → Default` arm, the `_ => Default` fallback, the `Default` resolver/dispatch arms) are **HTTP-cloud/container-only**. They must not reach into the local STDIO (`main.rs:1158`) or UDS (`main.rs:859`) boot paths. Touching those paths would introduce the cross-store regression AC-10 forbids and is a scope violation.

### Consequences

- **Easier:** Local is provably unaffected (RD-1 / AC-10): existing local stores keep working with NO migration and NO operator action, because the diff never reaches the local boot paths. The "local unaffected" guarantee is structural, not a test assertion.
- **Easier:** The resolver stays minimal — one slug-keyed lookup, one key type (`ProjectSlug`), no obligation to accommodate a derived path-hash key. RD-5 holds literally with the simplest possible resolver.
- **Easier:** The blast radius of vnc-038's HTTP rewrite is bounded to the HTTP surface; the impact analysis can assert "local untouched" by showing the deletions are confined to HTTP code.
- **Harder:** There are now two store-binding code paths (HTTP resolver vs direct local binding) that must each be reasoned about separately; an agent must not assume "all store binding goes through the resolver." This ADR is the explicit notice that it does not.
- **Harder:** Local does NOT get the resolver's single-funnel isolation proof for free — but it does not need it: local is single-store by construction (one socket/process, one directly-bound store), so there is no cross-project surface to prove against. The cloud isolation proof (ADR-003/004, N=2 guard) stands on its own.

### Related

- ADR-004 (this feature): the unified resolver and the deletion of `ProjectKey::Default`. This ADR scopes that deletion to HTTP-only and keeps local out of the resolver.
- vnc-034 ADR-004 (#4951): the path-hash store identity (A2) this ADR preserves verbatim for local.
- AC-09 / AC-10 / RD-1 / RD-5: the cloud-only-cutover, local-unaffected, and no-special-case decisions this ADR reconciles.
