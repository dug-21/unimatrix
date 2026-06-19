# Capability Map — Developer-Friendly Deployment (`personal-cloud`, goal #4946)

> **⚠️ MIGRATED — this file is an archived snapshot, not the source of truth.** The live capability map
> now lives in the Unimatrix `capability` corpus (entries **#5142–5163**, migrated 2026-06-19). Query it:
> `context_lookup category="capability" tags=["personal-cloud"]` or `context_graph` over the goal's
> incoming `Advances` edges. Update capabilities via the **`uni-capability` skill** — not this file. This
> snapshot is kept for human reading + git-history of the alpha; it may drift from the corpus.
>
> **What this is.** The decomposition of a goal into the concrete **capabilities** that must each
> *exist and behaviorally work* for the goal to be delivered. A capability is "delivered" only when a
> behavioral, real-artifact test proves its **Done when** — never when a feature merely *claims* it.
>
> **Why it's adjacent to the goal, not inside it.** The goal entry holds stable *intent*; this file holds
> the *decomposition + volatile status*. Keeping status out of the goal entry keeps the goal's
> correction chain about intent, not delivery bookkeeping. (Owner: uni-zero / goal curator.)
>
> **Graph-shaped on purpose.** Each capability is a NODE; `Advances` / `Prerequisite` / `Motivates` /
> `About` are EDGES (validated `RelationType`s, see `.claude/skills/uni-capability`); `delivered_by` /
> `proven_by` are FIELDS (their targets aren't Unimatrix nodes). Authored as nodes-and-edges so the
> migration to Unimatrix's typed graph is a transcription, not a redesign. Until then, git history is
> the capability-evolution record.

## Schema (also the migration spec)

```
Capability (Unimatrix entry, category "capability"):
  kind        functional | nfr   functional = an OUTCOME; nfr = a quality PROPERTY in business terms (cross-cutting)
  name        OUTCOME (functional) / quality PROPERTY (nfr) — never an implementation
  why         one sentence — the problem it solves
  done_when   1-2 BEHAVIORAL, runnable statements — the proof gate. nfr: tested ACROSS the governed surface.
  status      proven 🟢 | partial 🟡 | missing 🔴 | claimed ⚪ (asserted, no behavioral test)
  delivered_by  FIELD — GH ref(s), e.g. "#787"
  proven_by     FIELD — evidence ref, e.g. "live: arch-research store/get round-trip"
Edges (validated RelationType):
  Advances     capability -> goal        PPR-neutral
  Prerequisite capability -> capability  PPR-positive; functional DAG; the prerequisite is the SOURCE
  Motivates    research   -> capability  PPR-neutral (research stays out of retrieval until it graduates)
  About        nfr        -> functional  PPR-neutral; "this NFR governs that capability"
Classes: functional = "done" once proven. nfr = "maintained" — re-proven as the governed surface grows.
```

## Overview

**Legend:** 🟢 proven · 🟡 partial / in-flight · 🔴 not delivered · ⚪ claimed, unverified

| id | capability | status |
|----|------------|--------|
| **C0 ★** | Full intelligence-pipeline fidelity over HTTPS ≡ local | 🔴 |
| C1 | Zero-friction deploy yields a working, serving instance | 🟢 |
| C2 | Operator explicitly registers a project; attach errors if unregistered | 🟢 |
| C3 | Per-slug routing + isolated stores (DB / vector / hash-chain) | 🟢 |
| C4 | One isolation seam, local-UDS ≡ cloud | 🟢 |
| C5 | Per-slug analytics maintained | 🔴 |
| C6 | Per-slug configuration | 🔴 |
| C7 | Self-signed trust via fingerprint pinning | 🟢 |
| C8 | Remote reachability over HTTPS by configured host + bearer | 🟢 |
| C9 | Bundle attach, dumb client | 🟢 |
| C10 | Remote on-demand retrieval (`context_*`) over pinned HTTPS | 🟢 |
| C11 | Remote behavioral signals (observe) over pinned HTTPS | 🟢 |
| C12 | Credential safety (sole bearer, out-of-tree, never committable/logged) | 🟢 |
| C13 | Multi-client per project (N clients → one slug) | 🟡 |
| C14 | Multi-LLM parity (Claude / Codex / Gemini attach identically) | 🔴 |
| C15 | Operator can verify a remote client works (runbook) | 🔴 |
| C16 | Air-gap deploy | ⚪ |

### NFR capabilities (`kind: nfr` — cross-cutting quality promises, "maintained" not "done")

| id | nfr capability (business terms) | status | governs (`About`) |
|----|----------------------------------|--------|-------------------|
| N1 | Every served port is authenticated; no unauthenticated surface ships | 🟢 | C1, C8, C10, C11 |
| N2 | No secret reaches a committable or loggable surface | 🟢 | C9, C11, C12 |
| N3 | Writes are integrity-protected — never mis-routed across projects | 🟡 | C3, C5 |
| N4 | A healthy deployment emits no false-alarm signals operators must triage | 🟡 | C1 |
| N5 | The shipped artifact is always deployable as released | 🟡 | C1, C3, C5 |

## Capabilities

### C0 ★ — Full intelligence-pipeline fidelity over HTTPS ≡ local — 🔴
- **why:** the goal's marquee promise — a remote project must be a first-class Unimatrix, not a degraded one.
- **done_when:** for a remote slug, retrieval AND behavioral signals AND analytics/learning all function at parity with a local-UDS deployment of the same workload (measured, not asserted).
- **depends_on:** C5, C10, C11
- **status:** 🔴 — C10/C11 proven, **C5 broken** ⇒ rollup fails.

### C1 — Zero-friction deploy yields a working, *serving* instance — 🟢
- **why:** "one container, one command" is the goal's entry promise; a clean run must serve, not misroute.
- **done_when:** clean `docker run` of the GHCR image boots HTTP-on (non-root, healthcheck green, embedding model available — self-downloads on first use today); register slug → write over HTTPS → lands in the per-slug store, not the hash store.
- **depends_on:** —
- **delivered_by:** #786 (posture bake), #784 (healthcheck) · **proven_by:** `docker-http-posture-smoke.sh` (3/3)
- **status:** 🟢 — the model self-downloads on first use, so deploy works end-to-end. NOT a gap: #767 (bake the model at build) is an **efficiency** item, repositioned as a **prerequisite of C16 (air-gap)** — a no-network boot can't self-download. The standing-gate wiring of the smoke is an N5 (deployability) feature, not a C1 gap.

### C2 — Operator explicitly registers a project; attach errors if unregistered — 🟢
- **why:** stores must be operator-declared, never client-auto-created (integrity).
- **done_when:** `register <slug>` creates the store + stanza; `init --remote .../<unregistered>` errors.
- **delivered_by:** vnc-034, vnc-038 · **status:** 🟢

### C3 — Per-slug routing + isolated stores — 🟢
- **why:** N projects, one cloud, no cross-project data bleed.
- **done_when:** write to slug A is in A's DB/vector/hash-chain and absent from B; unregistered slug → 404.
- **delivered_by:** vnc-033, vnc-034 · **status:** 🟢

### C4 — One isolation seam, local-UDS ≡ cloud — 🟢
- **why:** the local install is the proving ground; no cloud-only isolation path.
- **done_when:** `resolve_store` resolves identically in local path-hash and cloud slug modes (one code path).
- **delivered_by:** vnc-034 ADR-003 · **status:** 🟢

### C5 — Per-slug analytics maintained — 🔴
- **why:** without per-project tick maintenance, a slug's learning (confidence, co-access, phase-blending) never runs — the self-improving half is dead per-slug.
- **done_when:** store to slug A → run a tick → A's confidence/co-access/phase caches reflect the write; B's do not; against a running multi-project server (not a unit stub).
- **depends_on:** C3
- **delivered_by:** — · **proven_by:** — · **status:** 🔴 (#787)

### C6 — Per-slug configuration — 🔴
- **why:** different projects are different domains and want their own categories/domain config.
- **done_when:** slug A resolves its own categories/domain config; slug B resolves different ones; one global default underneath; transport config stays global.
- **depends_on:** C5 (config tunes an engine that must run)
- **delivered_by:** — · **status:** 🔴 (#785)

### C7 — Self-signed trust via fingerprint pinning — 🟢
- **why:** OSS trust model is leaf-fingerprint pinning, not CA-trust.
- **done_when:** client connects iff the served leaf's `sha256:<hex>` matches the bundle `fp`; wrong pin → socket destroyed before the bearer is written.
- **delivered_by:** vnc-034/038/039 · **proven_by:** real-TLS pin suite + parity fixtures · **status:** 🟢

### C8 — Remote reachability over HTTPS by configured host + bearer — 🟢
- **why:** a client on another machine must reach the cloud by its real hostname without a 403.
- **done_when:** request to `https://<public-host>/v1/<slug>` with the bearer clears the host gate + auth and reaches MCP (not 403/401).
- **delivered_by:** #774/#778 (host allowlist), vnc-038 · **proven_by:** live curl + bridge against arch-research · **status:** 🟢

### C9 — Bundle attach, dumb client — 🟢
- **why:** the server is the sole authority on route shape; the client composes nothing (#766 class).
- **done_when:** client posts the v:2 bundle's `mcp_url`/`observe_url` byte-for-byte; composes no path, derives no slug.
- **delivered_by:** vnc-038 · **status:** 🟢

### C10 — Remote on-demand retrieval (`context_*`) over pinned HTTPS — 🟢
- **why:** the curation surface (search/lookup/get/store) must work remotely, not just locally.
- **done_when:** over the bridge, `initialize → tools/list → context_store/get/lookup` round-trips against the real cloud.
- **depends_on:** C7, C8, C9
- **delivered_by:** vnc-039 · **proven_by:** **live, arch-research** (store id 1 → get + lookup) · **status:** 🟢

### C11 — Remote behavioral signals (observe) over pinned HTTPS — 🟢
- **why:** the proactive/learning half must stream from remote clients, ingested per-slug.
- **done_when:** hook events POST to `observe_url` over pinned HTTPS and the server records them under the slug (not a UDS fallthrough).
- **depends_on:** C7, C8, C12
- **delivered_by:** vnc-039 (Scope B) · **proven_by:** **live** — server logged `PostToolUse`/`transcript_delta` under `http-` session · **status:** 🟢

### C12 — Credential safety — 🟢
- **why:** the bearer is the sole credential; it must not leak to a committable/loggable surface.
- **done_when:** token lives out-of-tree (mode 0600), never in `.mcp.json`/argv/logs/printSummary; `git add -A` stages no secret.
- **delivered_by:** vnc-039 (credstore) · **status:** 🟢

### C13 — Multi-client per project — 🟡
- **why:** N machines / checkouts / CLIs attach one project by its slug.
- **done_when:** two distinct clients attaching the same slug both read/write its store; attribution stays per-session.
- **depends_on:** C8
- **delivered_by:** vnc-034 (routing) · **status:** 🟡 (routing supports it; only Claude wired — see C14)

### C14 — Multi-LLM parity — 🔴
- **why:** the goal commits to Claude/Codex/Gemini connecting identically over HTTPS.
- **done_when:** the bridge is wired into Codex and Gemini config (not only `.mcp.json`) and all three drive `context_*` against one slug.
- **depends_on:** C10
- **delivered_by:** — · **status:** 🔴 (bridge is client-agnostic; auto-wiring is Claude-only)

### C15 — Operator can verify a remote client works (runbook) — 🔴
- **why:** an operator must be able to set up and *prove* a remote client without reverse-engineering.
- **done_when:** following the doc, an operator attaches a client and confirms MCP round-trip + observe landing.
- **delivered_by:** — · **status:** 🔴 (#768 stale)

### C16 — Air-gap deploy — ⚪
- **why:** secure/enterprise environments deploy without egress.
- **done_when:** pre-populate a volume, start with no network, register + store + retrieve succeed; backup = volume snapshot, restore = fresh container + volume → identical state.
- **depends_on:** C1, **#767 (model bake at build — a no-network boot can't self-download)**
- **delivered_by:** — · **proven_by:** — · **status:** ⚪ (in the goal; no behavioral test cited)

## NFR capabilities (detail)

### N1 — Every served port is authenticated; no unauthenticated surface ships — 🟢
- **why:** a cloud that ships an unauthenticated port is a breach on first boot.
- **done_when:** every served route requires the bearer (401 otherwise); a zero-project image serves only `/health` + uniform 404; TLS non-optional on the served port.
- **About:** C1, C8, C10, C11 · **proven_by:** #786 security review (auth+TLS mandatory, zero-project=404) · **status:** 🟢

### N2 — No secret reaches a committable or loggable surface — 🟢
- **why:** an operator must never leak the bearer via `git add` or logs.
- **done_when:** token out-of-tree (0600); absent from `.mcp.json`/argv/logs/printSummary/error messages; `git add -A` stages no secret.
- **About:** C9, C11, C12 · **proven_by:** vnc-039 credstore + fresh-context security reviews · **status:** 🟢

### N3 — Writes are integrity-protected — never mis-routed across projects — 🟡
- **why:** a mis-routed write corrupts the wrong project's hash chain, unrollbackably (the integrity basis of the whole isolation model).
- **done_when:** a write for slug A can only ever land in A's store — proven across the served surface, not one path.
- **About:** C3, C5 · **status:** 🟡 — holds *now* (the #774 / #783 mis-route violations are fixed), but **maintained**, not guaranteed: the regression guard (N5's standing gate) isn't wired, and C5 (#787) is still an open per-slug surface. Re-prove as surface grows.

### N4 — A healthy deployment emits no false-alarm signals operators must triage — 🟡
- **why:** ERROR-level false alarms erode trust in a self-run cloud (felt live during arch-research validation).
- **done_when:** a healthy daemon emits no ERROR-level lines for benign events (healthcheck probe, graceful shutdown, client disconnect).
- **About:** C1 · **delivered_by:** #784 (pre-initialize disconnect) · **status:** 🟡 — healthcheck noise fixed; the `Cancelled`-on-restart case may still emit ERROR (see #784 review) — surface not fully proven.

### N5 — The shipped artifact is always deployable as released — 🟡
- **why:** the first-run path of the *released image* was never gated; #774/#783 shipped broken because the artifact itself was never exercised.
- **done_when:** every release runs `docker build → boot → register → per-slug write` and fails the release if the write doesn't land in the per-slug store.
- **About:** C1, C3, C5 · **delivered_by:** `docker-http-posture-smoke.sh` exists (ran 3/3) · **status:** 🟡 — proven *once*, not *maintained*: the smoke is **not wired as a standing release gate** (the feature that flips this 🟢).

## Dependency DAG (next-to-build = next unblocked + unproven)

```
C1(deploy) ──┬─> C8(reach) ──┬─> C10(retrieve) ─┐
C7(pin),C9(bundle),C12(cred) ─┘  └─> C11(observe)┤
C3(stores) ──> C5(analytics) ──> C6(config)      ├─> C0 ★ (full fidelity)
C8 ──> C13(multi-client)         C10 ──> C14      │
C10,C11 ─────────────────────────────────────────┘
```

**Highest-leverage next build:** **C5** — it unblocks C6 *and* the marquee C0, and is open per-slug
surface for N3. The DAG computes the "tick before config" ordering. **Honest-unknown to retire:**
**C16** (⚪ — claimed, never tested).

**NFR maintenance (`About`-governed, re-prove as surface grows):** N3/N4/N5 are 🟡 not because the
property is absent but because the *maintenance/guard* is — N5's standing release gate is the single
feature that most raises confidence (it guards N3 and N5 and would have caught #774/#783). Efficiency
(#767) and prevention (the standing gate) are features *advancing* nfrs — not functional capabilities.

## Delivery plan — features to fully deliver `personal-cloud`

> Derived from the gaps above (the `uni-capability` "report what's left" view). 🟢 capabilities need no
> work. **Keystone: #787 (C5)** — the only open issue gating the marquee C0★, and it also feeds C6 and
> is open per-slug surface for N3. Sequence it first.

### Functional gaps

| Wave | Feature | Closes | State | Depends |
|------|---------|--------|-------|---------|
| **B** | #787 — per-slug analytics (tick Step A: per-slug rebuild + full service config) | **C5** | open | C3 |
| B | **NEW** — C0★ full-fidelity parity validation (remote ≡ local, measured) | C0★ | create | #787 |
| **C** | #785 — per-slug config overlay (carry the `(tenant,project)` enterprise-seam framing) | C6 | open | #787 |
| C | **NEW** — multi-LLM wiring (Codex / Gemini) + N-clients-one-slug test | C14, C13 | create | C10 |
| **D** | #768 — remote-client runbook + reconcile the `rmcp-initialize-capture` fixture | C15 | open | — |
| D | **NEW** — air-gap behavioral validation | C16 | create | #767 |

### NFR maintenance (features that *advance* an nfr — "maintained", not "done")

| Feature | Advances | State |
|---------|----------|-------|
| **NEW** — wire the docker smoke as a standing release gate | **N5** (guards N3) | create — highest confidence; would have caught #774/#783 |
| **NEW** — downgrade the `Cancelled`-on-restart log line | N4 | create (small; from the #784 review) |
| #767 — bake the embedding model at build time | C16 prerequisite / efficiency | open |

### Net-new issues to create (5)

1. C0★ — full-fidelity parity validation (remote ≡ local), post-#787
2. Multi-LLM client wiring (Codex/Gemini) + N-clients-one-slug behavioral test
3. Air-gap behavioral validation (depends #767)
4. Standing `docker build → boot → register → per-slug write` release gate (N5)
5. `Cancelled`-on-restart benign log downgrade (N4)

### Housekeeping

- Confirm **#786 / #784 merged** (assumed in C1 / N4 / N5 status).
- **Close #770** if vnc-038 (PR #772) fully subsumed it.
- Out of the core path per the goal: **#732** (auth metrics — deferred observability), **#682** (Rust hook
  retirement, soak-gated), **#578** (audit retention — enterprise-deferred).

### Critical path

```
#787 (C5 keystone) ──> C0★ validation ──────────> marquee promise 🟢
        └──> #785 (C6) , multi-LLM (C14)
N5 standing gate ── guards N3+N5; independent — do EARLY for confidence
#768 , air-gap(+#767) ── parallel, independent
```
