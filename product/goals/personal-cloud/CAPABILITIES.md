# Capability Map — Developer-Friendly Deployment (`personal-cloud`, goal #4946)

> **What this is.** The decomposition of a goal into the concrete **capabilities** that must each
> *exist and behaviorally work* for the goal to be delivered. A capability is "delivered" only when a
> behavioral, real-artifact test proves its **Done when** — never when a feature merely *claims* it.
>
> **Why it's adjacent to the goal, not inside it.** The goal entry holds stable *intent*; this file holds
> the *decomposition + volatile status*. Keeping status out of the goal entry keeps the goal's
> correction chain about intent, not delivery bookkeeping. (Owner: uni-zero / goal curator.)
>
> **Graph-shaped on purpose.** Each capability is a NODE; `depends_on` / `delivered_by` / `proven_by`
> are EDGES. This is authored as nodes-and-edges so the eventual migration to Unimatrix's typed graph
> is a transcription, not a redesign. Until then, git history is the capability-evolution record.

## Schema (also the migration spec)

```
Capability (node):
  id          C{n}            stable id
  name        string          OUTCOME a user/operator experiences (never an implementation)
  why         string          one sentence — the problem it solves
  done_when   string          1-2 BEHAVIORAL, runnable statements — the proof gate + definition of done
  status      enum             proven 🟢 | partial 🟡 | missing 🔴 | claimed ⚪ (asserted, no behavioral test)
Edges:
  depends_on  -> Capability    DAG; "next to build" = next unblocked + unproven node
  advances    -> Goal          this file ⇒ #4946
  delivered_by-> Feature/PR     which delivery work built it
  proven_by   -> Test/evidence  the behavioral artifact that cleared done_when (empty ⇒ not proven)
```

## Overview

**Legend:** 🟢 proven · 🟡 partial / in-flight · 🔴 not delivered · ⚪ claimed, unverified

| id | capability | status |
|----|------------|--------|
| **C0 ★** | Full intelligence-pipeline fidelity over HTTPS ≡ local | 🔴 |
| C1 | Zero-friction deploy yields a working, serving instance | 🟡 |
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

## Capabilities

### C0 ★ — Full intelligence-pipeline fidelity over HTTPS ≡ local — 🔴
- **why:** the goal's marquee promise — a remote project must be a first-class Unimatrix, not a degraded one.
- **done_when:** for a remote slug, retrieval AND behavioral signals AND analytics/learning all function at parity with a local-UDS deployment of the same workload (measured, not asserted).
- **depends_on:** C5, C10, C11
- **status:** 🔴 — C10/C11 proven, **C5 broken** ⇒ rollup fails.

### C1 — Zero-friction deploy yields a working, *serving* instance — 🟡
- **why:** "one container, one command" is the goal's entry promise; a clean run must serve, not misroute.
- **done_when:** clean `docker run` of the GHCR image boots HTTP-on (non-root, healthcheck green, ONNX models present); register slug → write over HTTPS → lands in the per-slug store, not the hash store.
- **depends_on:** —
- **delivered_by:** #786 (posture bake) · open: #767 (model populate), #769 (healthcheck noise)
- **proven_by:** `docker-http-posture-smoke.sh` (pending standing-gate wiring)
- **status:** 🟡

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
- **depends_on:** C1
- **delivered_by:** — · **proven_by:** — · **status:** ⚪ (in the goal; no behavioral test cited)

## Dependency DAG (next-to-build = next unblocked + unproven)

```
C1(deploy) ──┬─> C8(reach) ──┬─> C10(retrieve) ─┐
C7(pin),C9(bundle),C12(cred) ─┘  └─> C11(observe)┤
C3(stores) ──> C5(analytics) ──> C6(config)      ├─> C0 ★ (full fidelity)
C8 ──> C13(multi-client)         C10 ──> C14      │
C10,C11 ─────────────────────────────────────────┘
```

**Highest-leverage next build:** **C5** — it unblocks C6 *and* the marquee C0. The DAG computes the
"tick before config" ordering. **Honest-unknown to retire:** **C16** (⚪ — claimed, never tested).
