# ASS-100 — Edge-minted agent identity: can Unimatrix see, attribute, and (conditionally) enforce on it?

**Designation:** ass-100
**GitHub:** #954 · label `goal:integrity`, `research`
**Origin:** cross-crown handoff from Jurati ASS-009 (dug-21/jurati#11, branch `spike/ass-009-control-model-poc`)
**Session type:** research spike. Executes only after this SCOPE is complete (CLAUDE.md research rule).

---

## Framing — read this first (non-negotiable)

This is an **information spike**. It changes nothing.

- It does **NOT** amend ADR-008 (the human-locked "`agent_id` is audit-only, never an authorization input" ruling). The ceiling tier is framed as **"IF ADR-008 were amended, what becomes possible and how big is the change?"** — a sizing question, not a recommendation to act.
- It does **NOT** define the trust-establishment mechanism. Naming that precondition is in scope; *designing* it is a separate downstream step whose shape likely differs by transport (STDIO vs HTTP).
- Its output feeds a **later human decision**, alongside other Jurati input. No deliverable here triggers a change.

Keep the two tiers distinct throughout; a positive floor result does **not** imply the ceiling works.

- **Floor — attribution/observability.** Can the server *see and record* an edge-attached identity? What is the smallest change to make attribution real end-to-end?
- **Ceiling — authorization activation (conditional).** *If* config-enabled and ADR-008 were amended, can the server *resolve a principal from* the identity and *enforce real constraints* on it — activating the day-1 seams (`external_identity` → `require_cap`, live `TrustLevel`)? How big is the change, and is the day-1 architecture built to absorb it or is it hollow scaffolding?

---

## The question

**How big a change would it take on the Unimatrix server to make client-attached, edge-minted attested identity real end-to-end — for BOTH tiers (see/attribute, and conditionally enforce) — and is it even possible without a redesign?**

Per pathway (P1/P2/P3) **and** per tier, deliver a **feasibility verdict + change-size estimate** on this scale:
`no-change / handler-only / schema+handler / transport / architectural` — each pinned to the specific file/seam it lands on. That sizing is the primary deliverable; the numbered questions feed it.

---

## Why it matters (vision alignment)

- **`goal:integrity`** — the goal's success criteria rest on *attribution* (every entry attributed) and *capability checks at the service layer* (Principle 3). Edge-minted identity is the first real principal those seams could bite on.
- **The ass-094 line — "architect for enterprise, ship coarse for OSS."** Prior work deliberately built an inert enterprise identity seam (ADR-007 `external_identity`, ADR-008 Write-gate trust-elevation, the unused `TrustLevel` enum) and accepted the OSS gap as documented risk. The precondition those seams waited for — a trustworthy identity the server didn't have to mint itself — is exactly what an edge product (Jurati) now supplies. **This spike tests whether that seam is real or hollow.**
- **OSS default posture is preserved.** With the config gate OFF, behavior is unchanged (identity audit-only, per ADR-008). Any activation is proposed *behind* the flag — the spike does not propose changing the default.

---

## Grounding — the proven Jurati connection (what our test must bind to)

Jurati proved these to the **client wire** (floor); they could NOT see the server side (no introspection in their env). Jurati's own verdict was **controller-first (Mode B)** — so **P2 is the connection they would actually ship**; P1 works but is Claude-Code-coupled; P3 is the on-vision endgame gated solely on a Unimatrix-side change. Our test is based on *this* connection, not a hypothetical.

**Transport (all pathways):** fingerprint-pinned HTTPS. MCP JSON-RPC → `mcp_url = …/v1/{slug}`; observe frames → `observe_url = …/v1/{slug}/observe`. Live-wire probe is the **read-class `context_lookup` only** (no knowledge pollution).

| Pathway | Jurati result | Wire shape the server receives | Seam character |
|---|---|---|---|
| **P1 — hook-mutate** (Claude Code `PreToolUse` rewrites MCP payload) | WORKED to wire | `context_lookup` args: `{"topic":"…","_attested_identity":"ATTEST-P1-modeA"}` — LLM authored only `{"topic":"…"}` | TIGHTLY-COUPLED (Claude-Code-only `updatedInput`) |
| **P2 — controller-proxy** (external process stamps the wire) | WORKED to wire; server returned `ok:true` on the enriched payload | Identical field on the MCP JSON-RPC wire: worker emitted `{"topic":"arithmetic helpers"}` → proxy sent `{…,"_attested_identity":"ATTEST-P2-modeB"}` | CLEAN (worker-agnostic) — **Jurati's shipping choice** |
| **P3 — cooperative, cross-channel** (two-crowns) | forward-only; server correlation INCONCLUSIVE | Identity on the **observe** channel: `ContextSearch{role="ATTEST-P3-coop"}` → `observe_url`. Concurrent `context_lookup` carries **no** identity, **no** shared session id | Most on-vision; blocked **only** on Unimatrix-side channel correlation |

**Key facts Jurati established that sharpen our scope:**
- The server **accepted an unknown `_attested_identity` field without error** (`ok:true`) — strong external confirmation of the accept-and-drop hypothesis. Our floor probe verifies whether it is *retained/recoverable* server-side or discarded at deserialization.
- Shared tenant `projectHash 1d56af7c6c4906e3` was present in Mode B — a candidate join key for P3 correlation (tenant/project slug from the pinned cert).
- P3's forward is fragile even at the floor (conditional on transcript-tail extraction) — but that fragility is **Jurati's** to own; our question is purely the server-side correlation ceiling.

---

## Bounded questions

### Floor — attribution (unconditional)
1. **P1/P2 — unknown field.** When `context_lookup` arrives with an extra `_attested_identity` arg the schema doesn't define, does the server see it, and does it drop / reject / retain? Where in the pipeline does the JSON-RPC `arguments` object exist as raw `Value` **before** struct deserialization — i.e. is the field recoverable *without* a wire-schema change, or does capturing it require a params-struct field?
2. **P1/P2 — attribution.** Can the server attribute/record the call under the injected identity as-is, or does that need a schema/handler change? What is the minimal change, and should it feed the existing `agent_attribution` field or a new one? (The injected marker is conceptually a *third* thing — a per-call attested arg, not the `initialize` `clientInfo.name`.)
3. **P3 — correlation.** Can the server correlate an observe-channel `ContextSearch{role}` with a near-in-time `context_lookup` on the MCP channel from the same client/tenant, given no shared session id today? What join key would work (project slug from the pinned cert, tenant + timestamp window, or a correlation id the client must add — which pushes work back to Jurati)?
4. **Direction.** Which pathway is the server best positioned to support as the **canonical attested-identity ingress**, and what is the smallest server change to make attribution real end-to-end? (Weigh against Jurati's controller-first / P2 choice and the P3 two-crowns endgame.)

### Ceiling — authorization activation (conditional: "IF ADR-008 were amended")
5. **Enforcement activation.** If a received edge-minted identity is fed into the day-1 seam `build_context_with_external_identity(external_identity: Some(…))` — always `None` today — does `require_cap` then enforce on **that** principal instead of the spoofable self-declared `agent_id`? Trace end-to-end: does the resolved principal actually reach **all 15 `require_cap` chokepoints**, or does the self-declared `agent_id` still win somewhere? **Is `TrustLevel` (stored, never consulted) wired to become live at the gate, or is that hollow?**
6. **Config gating.** What is the shape of the `enabled` gate so the OSS default is unchanged when OFF (identity stays audit-only) and constraints activate when ON? One flag, or per-tenant/per-project scoping (per-slug config already ships)? Where does it read, and what is the blast radius of getting it wrong (fail-closed vs fail-open)?
7. **Trust-binding — the crux (NAME it, do not solve it).** For enforcement to be real, the server must authenticate that the identity assertion is **genuinely from the trusted edge authority, not from an attacker also POSTing `_attested_identity`.** A config flag alone does not establish this. What *would* bind the assertion's authenticity — mTLS client cert, a bearer/signed token (ADR-007's planned `agent_attribution` → JWT `sub`), the fingerprint-pinned channel itself? Does the day-1 bearer/JWT plan cover it, or is a new trust-establishment step required? **State this precondition explicitly and flag it as transport-dependent (STDIO vs HTTP) and downstream** — without it, config-gated enforcement is a spoofable gate wearing a security costume. Designing the mechanism is out of scope; naming the precondition and sizing its absence is in scope.
8. **Delegation vs. per-agent (size both, even-handedly).** Two models: (a) the server trusts the edge as a **delegating authority** and accepts the per-agent identities it vouches for; (b) full per-agent credentials terminate at the server. Which does the received-identity shape actually support, and which is the smaller/safer server change? Size both; do not assume the two-crowns path.

---

## Approach — server-side testing, not code-reading alone

- **Floor** — exercise the live instance with a read-class `context_lookup` carrying `_attested_identity` (P1/P2 shape) and an observe-channel `ContextSearch{role}` frame (P3 shape). Determine empirically what the server sees, retains, and can attribute. **Writes no knowledge.**
- **Ceiling** — validate the `require_cap` / `external_identity` / `TrustLevel` seam in **server-side test fixtures (unit/integration, throwaway store)**. Provoking a real `-32003 CapabilityDenied` needs a gated path, so it must **NOT** run against the live corpus. This honors "no knowledge pollution" while testing enforcement for real.

---

## Server-side starting points (hypotheses to test — verify, don't assume)

- **The two-field model already exists.** ADR-007: `agent_id` (agent-declared, spoofable, tool param) vs `agent_attribution` (transport-attested from `clientInfo.name` at MCP `initialize`, captured in `server.rs` `client_type_map`, used only as a write-only audit column). Decide whether P1/P2 should feed `agent_attribution` or a new field.
- **Q1 prior.** The codebase deliberately never sets `deny_unknown_fields` (explicit comments at `tools.rs`, `format.rs`); `LookupParams` is a typed struct (`crates/unimatrix-server/src/mcp/tools.rs`). Hypothesis: `_attested_identity` is silently discarded on deserialization → accept-and-drop (Jurati's `ok:true` is consistent). Find *where* raw `arguments` `Value` exists before the struct, or whether a catch-all `HashMap` could capture it.
- **The enforcement seam is pre-built and inert.** `build_context_with_external_identity(external_identity: Option<&ResolvedIdentity>)` (`server.rs`) is always `None` today — the reserved bearer/JWT identity-override point. Likely home for turning an attested marker into a real attributed principal.
- **The chokepoints exist.** 15 live `require_cap` sites return `-32003 CapabilityDenied`; the `Capability` enum (Read/Write/Search/Admin/SessionWrite) is enforced. What's missing is a trustworthy principal feeding them.
- **ADR-008 is the governing prior — its precondition is what changed.** The audit-only ruling was conditional on OSS having no credentialed transport to distinguish a real agent from an invented id. The updated lens supplies a real edge-minted identity. Re-examine the ruling's *implications* under the new precondition — as information, not as a ruling to overturn.
- **P3 correlation** likely turns on whether the observe and MCP channels share any server-visible key (tenant/project slug from the pinned cert? request timing?). If not, a client-added correlation id is the minimal join — which pushes work back to Jurati and must be called out as such.

---

## Deliverables

- **Per-pathway (P1/P2/P3) × per-tier (attribute / enforce): feasibility verdict + change-size estimate** (`no-change / handler-only / schema+handler / transport / architectural`), each pinned to the file/seam it lands on.
- Answers to **Q1–Q8**, grounded in **server-side testing**, not code-reading alone.
- **The day-1-seam verdict:** does `external_identity` → `require_cap` → live `TrustLevel` actually work when fed a real identity, or is it hollow? Concrete gap list if partial.
- **The trust-binding precondition (Q7)** stated explicitly: what must authenticate the edge identity before enforcement is real, whether the day-1 bearer/JWT plan covers it, and that its design is a transport-dependent downstream step.
- **Direction options with sizing** (Q4, Q8): the candidate canonical ingress and the delegation-vs-per-agent tradeoff — **options and sizes, not a recommendation to act.** Where Jurati's controller-first/P2 choice and the P3 two-crowns endgame bear on the direction, say so.

---

## Constraints & non-goals

- **Information only.** No deliverable amends ADR-008, defines the trust mechanism, or changes the OSS default. Feeds a later decision.
- **Live-wire probe stays read-class** — `context_lookup` only, writes no knowledge. Enforcement is validated in **test fixtures / throwaway store**, never against the live corpus.
- **HTTP transport only.** STDIO is a separate Jurati sequel — out of scope. (Note: the trust mechanism, when later designed, likely differs STDIO vs HTTP — flag, don't solve.)
- **OSS default posture unchanged** — gate OFF ⇒ identity audit-only. Activation is proposed strictly behind the flag.
- **Two tiers stay distinct** — a positive floor result does not imply the ceiling works.
- **Jurati is backdrop, not a dependency** — the client-side reference is done; Unimatrix owns the server-side verdict.

---

## Prior art / seams to verify

- ADR-007 — two-field model (`agent_id` vs `agent_attribution`); `external_identity` upgrade path (JWT `sub` under bearer auth).
- ADR-008 (vnc-045) — audit-only posture; the governing prior; its own text points at the Write-gate trust-elevation attach-point.
- `build_context_with_external_identity` (`server.rs`), the 15 `require_cap` sites, the inert `TrustLevel` enum, `deny_unknown_fields`-off deserialization.
- ass-094 lesson — "architect for enterprise, ship coarse for OSS."
- Jurati ASS-009 FINDINGS (dug-21/jurati, branch `spike/ass-009-control-model-poc`) — `FINDINGS.md`, `FINDINGS-MODE-A.md`, `FINDINGS-MODE-B.md`, `POC-HARNESS.md`. Controller-first (Mode B/P2) is Jurati's verdict; P3 two-crowns unlocks on server-side channel correlation.

---

*Originated: Jurati ASS-009 (dug-21/jurati#11). Read-only `context_lookup`, HTTP. Server-side verdict owned by Unimatrix.*
