# Scope Risk Assessment: vnc-039

Mode: scope-risk. Inputs: SCOPE.md, PRODUCT-VISION.md, ass-080/FINDINGS.md. Historical grounding: #4970, #4965, #5105, #5098 (cert-pin / bridge / harness lineage).

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | The ~260-LoC correctness surface (SSE `text/event-stream` parsing + `Mcp-Session-Id` capture/replay) is hand-rolled with no SDK leverage; getting it subtly wrong yields plausible-but-broken JSON-RPC round-trips. | High | High | Architect: carve SSE-parse and session-replay into separately-testable units with their own fixtures; make the documented hybrid flip-bar (OQ-1) a concrete, pre-agreed delivery checkpoint, not a vibe. |
| SR-02 | Cert-pin reuse on the new bridge is trust-boundary code. vnc-034 F1 (#4970/#4965) shipped DEAD pin code through three green gates — caught only by fresh-context review. A bridge that flushes the bearer before the pin matches is a silent token-leak regression. | High | Med | Architect/spec: make a live self-signed handshake test (good-pin connects; wrong-pin rejected AND token never reaches wire) an acceptance criterion, and route the bridge to fresh-context security review even if gates are green. |
| SR-03 | Zero-dep DIY is a deliberate bet (ass-080). If SSE/session correctness proves harder than estimated, the only fallback (hybrid) imports a 91-pkg/25MB tree AND still needs the custom-fetch + Response adapter — an expensive, late pivot. | Med | Med | Architect: honor the 30-min SDK re-check ass-080 flagged (a slimmer client-only package may have shipped); set the flip-bar threshold before delivery so the pivot decision is data-driven, not panic-driven. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | #774 blocks Scope A live end-to-end validation; the bridge is stub-validated only. A stub that diverges from the real cloud (rmcp) wire behavior produces false-green confidence — exactly the false-green class vnc-034 hit. | High | Med | Spec: pin the stub's contract to observed real-server behavior (entry #5098 harness already speaks HTTPS+pin); record the #774 sequencing dependency as an explicit "not-validated-live" caveat on every Scope-A AC. Land Scope B first (no #774 dep). |
| SR-05 | Two scopes (A bridge, B creds relocation) fold two issues (#775, #776) plus resolve #773 and touch #768. Coupling delivery risk: Scope A's #774 block could stall Scope B if not kept independent. | Med | Med | Architect: keep Scope B a standalone deliverable (AC-11) that ships without a reachable cloud; do not let the bridge's blocked validation gate the creds-relocation merge. |
| SR-06 | Bundle-only cloud-MCP boundary: legacy `--remote` deliberately unsupported (#773 deprecated). Risk is a silent skip instead of a loud message, leaving users with a dead `context_*` surface and no signal. | Med | Low | Spec: AC-10's "loud, deterministic" unsupported message is the mitigation — make its exact wording and exit behavior a testable AC, not prose. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | Pre-existing `unimatrix.remote` schema mismatch: writer emits `{mcp_url, observe_url, token, fingerprint}`; hook client reads `{url, token, timeouts}` and never reads `fingerprint`. Scope B's relocation rewrites this exact load/store pair for TWO consumers (bridge + hook/observe) against ONE store. | High | High | Architect: land ONE coherent store schema; the hook client must newly read `observe_url`+`fingerprint` (not `url`). Fixing the latent unpinned/UDS-fallback bug is in Scope B's blast radius — do not faithfully port the mismatch forward. |
| SR-08 | Slug (server-authoritative, from bundle) vs `projectHash` (client-derived, hook client's `~/.unimatrix/<projectHash>/`) keying ambiguity (OQ-6). If the two consumers index the store by different keys, one silently fails to resolve its credential. | High | Med | Architect: pick ONE key both consumers agree on before any code; if slug, the hook client must learn to derive/obtain slug; document the key choice as a constraint, not an open question, before spec lock. |
| SR-09 | `.mcp.json` write must be idempotent + merge-preserving (mirror `writeMcpJson`) AND honor `--dry-run`. Clobbering co-resident MCP servers or duplicating entries on re-`init` is a regression surface. | Med | Med | Spec: reuse the local `writeMcpJson` idempotency contract verbatim; add a re-`init` idempotency test alongside the existing fixtures (cumulative, per Constraints). |

## Assumptions

- **A1 (SCOPE Non-Goals, Constraints):** The `v:2` bundle and `.../v1/{slug}` route are server-ready and frozen — this feature is client-only. If a server-side wire change is needed (beyond #774), Scope A's estimate is invalid. SCOPE asserts this was validated this session (raw `initialize` POST → 200); treat as load-bearing.
- **A2 (Proposed Approach → Scope A, ass-080):** The ~450 LoC budget holds. The whole BUILD verdict and flip-bar rest on this; a 2x overrun is the documented trigger to revisit (SR-03).
- **A3 (Constraints, Background Research):** `cert-pin.js` + `transport-http.js:150-176` flush-after-pin pattern ports to the bridge at ~0 net LoC. If the bridge's connection lifecycle (Streamable-HTTP keep-alive, session replay) diverges from the single-shot observe POST, the "free" TLS assumption weakens (SR-02).
- **A4 (Constraints):** Cleartext-at-rest is accepted; the only hardened risk is cleartext-in-repo. If a reviewer re-litigates at-rest encryption, it is out of scope by decision (#775/#776).

## Design Recommendations

1. **SR-02 + SR-04 + SR-01 — trust-boundary + false-green is the concentrated risk.** Make a real self-signed-handshake test and a real-wire-behavior stub acceptance criteria, and route the bridge to fresh-context security review. This is the exact failure vnc-034 shipped (#4970).
2. **SR-07 + SR-08 — the store is the integration hazard.** Decide one coherent schema and one key (slug vs projectHash) before spec lock; reconcile the latent mismatch rather than porting it. Two consumers, one store, one key — or one silently breaks.
3. **SR-05 + SR-03 — sequence to de-risk the #774 block.** Land Scope B first (independent, no cloud needed); set the hybrid flip-bar as a concrete delivery checkpoint with a pre-agreed threshold.

## Knowledge Stewardship
- Queried: context_search for cert-pin/trust-boundary lessons, schema-mismatch lessons, risk patterns -- found #4970/#4965 (vnc-034 cert-pin DEAD-CODE false-green, directly elevates SR-02/SR-04 likelihood), #5105 (bridge pattern confirms approach), #5098 (HTTPS+pin harness constraint).
- Stored: nothing novel to store -- a cross-feature risk pattern (trust-boundary code needs live-boundary tests, not shape assertions) already exists as lesson #4970; no 2nd-feature pattern beyond it yet to warrant a new pattern entry.
