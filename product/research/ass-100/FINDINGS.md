# FINDINGS: Edge-minted agent identity — can Unimatrix see, attribute, and (conditionally) enforce on it?

**Spike**: ass-100
**Date**: 2026-07-17
**Approach**: server-side testing (investigation + empirical probe against real structs / throwaway store)
**Confidence**: empirical (floor + ceiling claims backed by executed code, not code-reading alone)
**GitHub**: #954 · cross-crown handoff from Jurati ASS-009

---

## Framing recap (non-negotiable)

Information spike. Nothing here amends ADR-008, defines the trust mechanism, or changes the OSS default. The ceiling tier is framed strictly as *"IF ADR-008 were amended, how big is the change?"* — a sizing question. Two tiers kept distinct: a positive floor result does **not** imply the ceiling works. It does not.

**Empirical method.** A throwaway integration test (`crates/unimatrix-server/tests/ass100_throwaway_probe.rs`, since deleted — writes no knowledge, throwaway store) exercised the REAL `LookupParams` struct and the REAL `AgentRegistry`/`require_capability` gate. Raw results are quoted inline. The live daemon (`unimatrix serve --daemon`, pid running) confirms the server is the same build under test.

---

## PRIMARY DELIVERABLE — the sizing matrix

Per pathway × per tier. Scale: `no-change / handler-only / schema+handler / transport / architectural`. Each verdict pinned to the seam it lands on.

| Pathway | Tier: ATTRIBUTE (floor, unconditional) | Tier: ENFORCE (ceiling, IF ADR-008 amended) |
|---|---|---|
| **P1 — hook-mutate** (Claude-Code `updatedInput`) | **schema+handler** — add an attested field (or `#[serde(flatten)]` catch-all) to `LookupParams` @ `tools.rs:182`; thread through `build_context_with_external_identity` @ `server.rs:526`; write to a NEW audit column. Field is empirically **dropped at deserialization today** (see Q1). | **architectural** — attribution change + gate-resolution rewrite (Q5 gaps 1–3) + trust-binding (Q7, **transport**-dependent) + config gate (Q6). Client-coupling (Claude-Code-only) is Jurati's problem, not the server's. |
| **P2 — controller-proxy** (Jurati's shipping choice) | **schema+handler** — identical seam to P1 (same wire field on MCP `arguments`); worker-agnostic, so the cleanest ingress. Pinned to `tools.rs` params structs + `server.rs` context builder + new audit/attribution field. | **architectural** — same as P1. The trust root is singular (one edge/controller), which makes the *delegation* enforcement model (Q8a) the smaller sub-path, but "real enforcement" still crosses the architectural bar (gate semantics + Q7). |
| **P3 — cooperative, cross-channel** (two-crowns) | **handler-only** to *see/retain* (identity rides the observe frame's `role`, already a first-class persisted observation field) — but **architectural** to *attribute it to the MCP call*, because cross-channel correlation does not exist server-side (Q3). | **architectural** — enforce requires correlation FIRST (a join key that isn't there today) THEN the same gate rewrite. Blocked on a Unimatrix-side channel-correlation capability; deterministic join needs a Jurati-added correlation id. |

**Headline:** the floor (attribution) is a **bounded, schema+handler change** for P1/P2. The ceiling (enforcement) is **architectural for every pathway** and additionally gated on a **transport-level trust-binding step that does not exist** (Q7). The day-1 `external_identity → require_cap → live TrustLevel` seam is **partially hollow** (see day-1-seam verdict below).

---

## Findings

### Q: Q1 — P1/P2 unknown field: does the server see it, drop/reject/retain? Is it recoverable without a wire-schema change?
**Answer**: The server **accepts and silently drops** it. Deserializing the exact Jurati P2 wire `{"topic":"arithmetic helpers","_attested_identity":"ATTEST-P2-modeB"}` into the real `LookupParams` **succeeds with no error** (matches Jurati's `ok:true`), captures `topic`, and **retains the marker nowhere**. It is **not recoverable without a struct change**.
**Evidence**: Executed probe output — `Q1_RESULT accept=true topic_seen=true attested_retained=false round_trip={"topic":"arithmetic helpers","category":null,...,"agent_id":null}`. `LookupParams` (`tools.rs:182`) is a typed `#[derive(Deserialize)]` struct with **no `deny_unknown_fields`** (confirmed — the codebase deliberately omits it, comments at `format.rs:904`, `config.rs:12285`) and **no `serde(flatten)` catch-all**. Serde therefore discards unrecognised keys during deserialization. The handler signature is `context_lookup(Parameters(params): Parameters<LookupParams>, request_context)` (`tools.rs:871`): rmcp consumes the raw JSON-RPC `arguments` object in the router and hands the handler only the *typed* struct — **no raw `Value` survives to the handler**, and `request_context` carries HTTP parts/headers, not the tool `arguments`. So there is no in-handler recovery point.
**Recommendation**: Capturing the marker **requires a params-struct change** — either (a) a named `Option<String>` field (dirties the advertised JsonSchema with `_attested_identity`) or (b) a `#[serde(flatten)] extra: HashMap<String,Value>` catch-all (keeps the wire schema clean, captures any attested key). Use (b) if you want ingress without publishing the field name; use (a) if you want the field first-class and validated. Either is a **schema+handler** change; "handler-only" is impossible because nothing reaches the handler to capture.

### Q: Q2 — P1/P2 attribution: can the server attribute under the injected identity as-is, or does that need a change? New field or existing `agent_attribution`?
**Answer**: **Not as-is** (Q1: it's dropped). Minimal change is **schema+handler**, and it should feed a **NEW field, not `agent_attribution`**.
**Evidence**: ADR-007 (#4361) fixes `agent_attribution` as *transport-attested from `clientInfo.name` at MCP `initialize`* — a **per-connection** value captured in `client_type_map` (`server.rs:288`, populated at initialize `server.rs:1290`) and surfaced into audit as `agent_attribution: ctx.client_type` (`tools.rs:947`). The injected marker is a **per-call** assertion from a **different source** (the edge, mid-stream), i.e. the "third thing" the SCOPE names — neither the self-declared `agent_id` (spoofable tool param) nor the connection's `clientInfo.name`. Overloading `agent_attribution` would conflate two distinct attestation provenances and break ADR-007's non-conflation principle (SR-08).
**Recommendation**: Add a dedicated per-call attested field (e.g. `attested_identity`) to the params struct(s) and a matching **new audit column**, populated from the captured marker in `build_context_with_external_identity`. Keep `agent_id` (audit-only self-declared) and `agent_attribution` (per-connection transport-attested) untouched. Size: **schema+handler**, pinned to `tools.rs` params + `server.rs:526` + `AuditEvent`.

### Q: Q3 — P3 correlation: can the server correlate an observe-channel `ContextSearch{role}` with a near-in-time MCP `context_lookup` from the same client/tenant, given no shared session id? What join key works?
**Answer**: **Coarse correlation is possible; deterministic per-call correlation is not** — not without a Jurati-added id.
**Evidence**: Both channels funnel per-slug on the SAME router: MCP `/v1/{slug}` and observe `POST /v1/{slug}/observe` (`http/router.rs:6-12,201`), and the observe handler resolves the per-slug `session_registry` (`router.rs:80-83`). So the **slug** — derived from the URL on a fingerprint-pinned channel — is a genuine server-visible shared key at **tenant/project granularity** (consistent with Jurati's shared `projectHash 1d56af7c6c4906e3`). Observation records DO carry a `session_id` (`unimatrix-observe/types.rs`), but per the SCOPE the concurrent `context_lookup` carries **no** identity and **no** shared session id — Jurati sets none. So the only server-visible join today is **slug + timestamp window**, which cannot bind a *specific* observe frame to a *specific* MCP call (ambiguous under concurrency).
**Recommendation**: For coarse tenant-level attribution, slug (already present) suffices — **no client change**. For deterministic per-call correlation, the minimal join is a **client-added correlation id emitted on BOTH channels** — which **pushes work back to Jurati** and must be called out as such. Do not rely on timestamp-window heuristics for an authorization decision (race-prone). Server-side: coarse = **handler-only**; deterministic = **architectural** (new cross-channel correlation surface) and dependent on the Jurati id.

### Q: Q4 — Direction: which pathway is the server best positioned to support as canonical attested-identity ingress, and the smallest change to make attribution real end-to-end?
**Answer** (OPTION, not a recommendation to act): **P2 (controller-proxy)** is the server's best-positioned canonical ingress.
**Evidence**: P1 and P2 are **identical on the server wire** (both attach `_attested_identity` to the MCP `arguments` object); P1 additionally couples to Claude-Code's `updatedInput`, which P2 avoids (worker-agnostic). P2 is also Jurati's own controller-first (Mode B) shipping verdict, so the client reference is real, not hypothetical. P3 is the most on-vision endgame but is blocked on server-side correlation that doesn't exist (Q3) and would push a correlation-id requirement onto Jurati.
**Recommendation**: Treat **P2 as the canonical ingress**. Smallest change to make attribution real end-to-end: (1) capture the per-call marker via a params-struct field/catch-all (Q1), (2) thread it through `build_context_with_external_identity`, (3) persist it to a **new** attributed column distinct from `agent_id`/`agent_attribution` (Q2). Total floor size: **schema+handler**. Keep P3 as the declared endgame but note its correlation dependency; keep P1 as a Claude-Code-only convenience that needs no *extra* server work beyond P2's.

---

### Q: Q5 — Enforcement activation: if a received identity is fed into `build_context_with_external_identity(external_identity: Some(…))`, does `require_cap` enforce on THAT principal instead of the spoofable `agent_id`? Does the resolved principal reach all 15 chokepoints? Is `TrustLevel` wired to become live, or hollow?
**Answer**: **Largely hollow.** Feeding `Some(ext)` changes *which agent_id string* flows into context, but the gate's authority still comes from the **registry record for that agent_id**, not from the asserted identity — and **`TrustLevel` is never consulted at the gate.**
**Evidence**:
- All **15 `require_cap` chokepoints** call `require_cap(&ctx.agent_id, cap)` (`tools.rs:756,890,1003,…,4330`), and `ctx.agent_id` is set from `identity.agent_id` where `identity = external_identity` when `Some` (`server.rs:541,599`). So the principal string *does* reach every chokepoint. **But all 17 `build_context_with_external_identity` call sites pass `None`** (verified: `None,` at each site) — there is **no ingress today**.
- The gate does **not** consume `ResolvedIdentity.capabilities`. `require_cap → registry.require_capability(agent_id) → has_capability → store.agent_get(agent_id).capabilities.contains(cap)` (`registry.rs:81-99`). It **re-resolves capabilities from the registry by agent_id string**; the asserted `ext.capabilities` is decorative at the gate.
- **`TrustLevel` is empirically inert at the gate.** `require_capability` reads `.capabilities` only; `trust_level` is never read. Probe output: `Q5_TRUST_INERT` — a lowest-trust `Restricted` auto-enrollee passes Read/Write identically to any higher-trust agent with the same caps. Wiring "live TrustLevel at the gate" is **net-new logic, not activation of dormant logic** (consistent with ADR-008 #5609: `TrustLevel` stored but never consulted by `registry.rs:92`).
- **Fail-OPEN for unknown identities.** `resolve_or_enroll` auto-enrolls any novel `agent_id` as `Restricted` with default caps. Probe: `Q5_STRICT novel_id=ATTEST-edge-minted-worker-42 caps=[Read, Search] Read_ok=true Write_ok=false`; `Q5_PERMISSIVE … caps=[Read, Write, Search] Write_ok=true`. So an un-provisioned edge identity fed to the gate is **auto-granted** Read/Search (and Write under the shipped permissive default `AgentsConfig`), never denied.
**Recommendation**: Do not treat `external_identity=Some` as sufficient for enforcement. Real enforcement on an edge identity requires **changing the gate's resolution contract** so the asserted identity's authority (or a pre-provisioned mapping) is honored AND unknown/unverified identities **fail closed** — not the current auto-enroll fail-open. That is an **architectural** change to the authorization core, over and above ADR-008's amendment.

### Q: Q6 — Config gating: shape of the `enabled` gate so OSS default is unchanged OFF and constraints activate ON? One flag or per-slug? Where does it read? Blast radius?
**Answer**: A single flag on **`AgentsConfig`** activates it globally now; **per-slug scoping is desirable but depends on the deferred per-project config overlay landing first.** The dominant risk is fail-open vs fail-closed.
**Evidence**: `AgentsConfig` (`config.rs:434`, `#[serde(default)]`, holds `default_trust` + `session_capabilities`) is the natural home — adding `enforce_external_identity: bool` (default `false`) is backward-compatible and keeps the OSS default identity-audit-only. Per-slug config exists structurally (`UnimatrixConfig.projects: Vec<ProjectConfigEntry>`, `config.rs:113`) but `ProjectConfigEntry` is **`slug`-ONLY today** — "per-project config-overlay is split to a follow-up (D2)" (`config.rs:124-131`). So per-slug/per-tenant scoping of the flag is blocked on D2.
**Recommendation**: Ship the flag on `AgentsConfig` as a **schema+handler** change (read at registry construction / per-request `build_context`). Scope per-slug **only after** the D2 config overlay lands. **Blast radius:** getting the default wrong is the primary hazard — because the gate today fail-OPENs via `resolve_or_enroll` (Q5), a naive "enabled=true" that still routes unknown asserted ids through auto-enroll would **auto-grant** capabilities. The flag MUST be paired with a fail-CLOSED path for absent/unverified identities. The flag alone is cheap; the *enforcement behavior behind it* is architectural.

### Q: Q7 — Trust-binding (NAME it, do not solve it): what authenticates that the assertion is genuinely from the trusted edge, not an attacker also POSTing `_attested_identity`?
**Answer (the crux, stated explicitly)**: **Nothing in the day-1 stack authenticates the assertion's origin.** A config flag does not establish authenticity. Without a binding step, config-gated enforcement is a **spoofable gate wearing a security costume** — any client on the same channel can POST `_attested_identity` and is indistinguishable from the real edge.
**Evidence**: HTTP transport uses **fingerprint-pinned TLS**, but that pins the **server** cert so the **client** can trust the **server** — it does **not** authenticate the **client** to the server (no mTLS client-cert path today). Client auth today is a **static bearer token** (ADR-006 #4670, `credential_type="none"`/`static_token`), which authenticates the **deployment/connection**, not **per-agent** identities within it. ADR-007's planned upgrade — `external_identity ← JWT sub` under bearer auth — is the *designed* home for a signed per-agent assertion, **but that JWT-verification path is not built**: `external_identity` is always `None` and no signature verifier is wired.
**Precondition (named, not designed)**: Before enforcement is real, the server must cryptographically bind the assertion to the trusted edge authority. Candidates: **mTLS client cert** (authenticates the edge as a peer), a **signed/bearer token whose `sub` is the per-agent identity** (ADR-007's JWT path, verified against the edge's key), or a scoped credential over the pinned channel. **The day-1 bearer/JWT plan covers the *delegation* case IF the verifier is built; a new trust-establishment step is required** — it does not exist today.
**Flags (required by SCOPE)**: This precondition is **transport-dependent** — HTTP → mTLS or bearer/JWT verification; STDIO (out of scope here) likely differs (process/OS-level trust). It is **downstream** — designing the mechanism is a separate step. **Sizing the absence: transport** (a new authentication/verification stage in the request path), and it **gates the entire ceiling** — no enforcement pathway is real until it lands.

### Q: Q8 — Delegation vs per-agent (size both, even-handedly): which does the received shape support, which is smaller/safer?
**Answer**: The received `_attested_identity` shape is a **delegation artifact by construction** → model (a) is what it supports and is the **smaller/safer** server change; model (b) is **architectural** and larger.
**Evidence**:
- **(a) Delegation — server trusts the edge, accepts the per-agent identities it vouches for.** `_attested_identity` is an **edge-minted assertion carrying no independent cryptographic proof of the agent** — it is exactly a vouching artifact. It fits the existing single-principal trust model (static bearer token authenticates one edge, ADR-006) plus ADR-007's `sub`-in-JWT. The server authenticates **one** principal (the edge) and treats the marker as attributed subject within that boundary. Size: **schema+handler** for attribution; enforcement still needs the Q5 gate fix + Q7 verifier, but the trust root is **singular and partially present**. Reinforced by Jurati's controller-first/P2 choice — the controller is the natural single trust boundary.
- **(b) Per-agent credentials terminate at the server.** Each agent presents its own verifiable credential. Requires per-agent key management, an issuance/verification lifecycle, and per-agent registry provisioning (which also closes the Q5 auto-enroll fail-open, but for every agent). Size: **architectural** — a new credential subsystem. More granular/defensible for high-assurance, but shifts the trust model away from the two-crowns delegation Jurati is built around.
**Recommendation** (options + sizes, not an action): If this is ever activated, **(a) delegation** is the smaller, safer, shape-aligned server change and matches Jurati's controller-first posture. **(b) per-agent** is the higher-assurance ceiling but is an architectural build and should not be assumed. Present both to the human decision; do not pre-commit.

---

## The day-1-seam verdict

**Does `external_identity → require_cap → live TrustLevel` actually work when fed a real identity? — NO, it is partially hollow scaffolding.**

- `external_identity` is **real plumbing** — a genuine, threaded override parameter on `build_context_with_external_identity` — but it is **inert** (every one of 17 call sites passes `None`) and, more importantly, **under-powered**: flipping it to `Some` changes the agent_id/trust *carried in context*, yet the gate never consults the asserted authority.
- `require_cap` is **agent_id-string-and-registry-driven end to end**: it re-resolves capabilities from the registry record, ignoring `ResolvedIdentity.capabilities`. Feeding a real identity does not make the gate enforce on *that* identity's asserted grants.
- **`live TrustLevel` is hollow** — there is no code path where `trust_level` influences the gate verdict. Making it live is net-new logic.

**Concrete gap list (all six must close for the ceiling to be real):**
1. **No ingress** — 17 call sites hardcode `external_identity=None`.
2. **Marker not captured** — `_attested_identity` dropped at deserialization (Q1).
3. **Gate ignores asserted caps** — `require_cap` re-resolves by agent_id, discards `ext.capabilities` (Q5).
4. **Fail-OPEN for unknown ids** — `resolve_or_enroll` auto-grants default caps to any novel identity (Q5).
5. **TrustLevel unread** — never consulted at the gate; "live TrustLevel" is net-new (Q5).
6. **No trust-binding** — nothing authenticates the assertion's origin (Q7, transport-level, gates everything).

Gaps 1–2 are floor-adjacent (**schema+handler**). Gaps 3–5 are the **architectural** gate-resolution rewrite. Gap 6 is the **transport** precondition that dominates all of them.

---

## Unanswered Questions

- **Live-wire injection through the sanctioned MCP boundary was not exercised** — the MCP tool schema I call through does not let me attach an arbitrary `_attested_identity` field, so the accept-and-drop behavior was proven at the **deserialization boundary** (the real `LookupParams`, the exact wire payload) rather than by a raw HTTP POST to the running daemon. The deserialization boundary is where the drop provably occurs (rmcp hands the handler only the typed struct), so the conclusion is unaffected; a raw-socket reproduction against `/v1/{slug}` would add belt-and-suspenders confirmation but not change the verdict. Reason: sanctioned-client constraint, not a gap in the finding.
- **Exact per-slug flag wiring depends on the deferred D2 config overlay** (Q6) — its shape can only be finalized once `ProjectConfigEntry` gains a config-overlay body. Reason: blocked on separate planned work.

---

## Out-of-Scope Discoveries

- **Auto-enroll fail-open is a standing OSS posture, not new to this spike.** `resolve_or_enroll` granting default caps (permissive: Read/Write/Search) to any unseen `agent_id` is the shipped OSS default. It is consistent with ADR-008's "identity is audit-only" ruling, but it means **any future identity-based gate must explicitly opt into fail-closed** — the platform default works against enforcement. Worth a one-line note in whatever downstream design touches the gate; may warrant its own spike if enforcement is ever pursued. *Not pursued here.*
- **`agent_attribution` (clientInfo.name) is per-connection, not per-call** — a latent conflation trap for any future consumer that tries to attribute individual calls from it. ADR-007 already forbids conflation; flagging because the edge-identity work sits adjacent and could tempt an overload. *Not pursued here.*

---

## Recommendations Summary

- **Q1**: `_attested_identity` is accepted-and-dropped (no `deny_unknown_fields`, no catch-all, no raw `Value` in-handler) — capture needs a params-struct field or `serde(flatten)` map; **schema+handler**, not handler-only.
- **Q2**: Attribute via a **new per-call field + new audit column**, not `agent_attribution` (which is per-connection `clientInfo.name`); **schema+handler**.
- **Q3**: Server-visible join today is **slug + timestamp window** (coarse, non-deterministic); deterministic per-call correlation needs a **Jurati-added correlation id on both channels** — pushes work to the client.
- **Q4**: **P2 (controller-proxy)** is the best-positioned canonical ingress (worker-agnostic, identical server wire to P1, Jurati's shipping choice); smallest floor change is **schema+handler**.
- **Q5**: Day-1 enforcement seam is **largely hollow** — `require_cap` re-resolves by agent_id (ignores asserted caps), unknown ids **fail-open**, `TrustLevel` is **never consulted**; real enforcement is **architectural**.
- **Q6**: One `enforce_external_identity` flag on `AgentsConfig` (**schema+handler**) preserves the OFF default; per-slug scoping waits on the deferred D2 overlay; **must** pair with a fail-closed path or the fail-open gate auto-grants.
- **Q7**: **No trust-binding exists** — fingerprint-pinning authenticates the server-to-client, not the client's per-agent assertions; a new **transport-level** verification step (mTLS or JWT-`sub`) is required and gates the entire ceiling; transport-dependent, downstream — named, not designed.
- **Q8**: The received shape is a **delegation artifact** — model (a) trust-the-edge is smaller/safer/shape-aligned (**schema+handler** + gate fix); model (b) per-agent-credentials-terminate is **architectural**; present both, assume neither.
- **Sizing headline**: Floor (attribute) = **schema+handler** for P1/P2, correlation-blocked for P3. Ceiling (enforce) = **architectural** for all pathways, plus a **transport** trust-binding precondition that does not exist today.
