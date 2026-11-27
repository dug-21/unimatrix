# Unimatrix V2 — Product Delivery Plan

> **Status: DRAFT.** Nothing references this document yet. **Human direction beyond ass-111 Rev 4 (kept deliberately):** two-level authority + OIDC user seam; Auditor role in Stage 2 (research had post-GA); generic inference seam; vision P12–P14; dependency-upgrade track. Source: ass-111 S7 synthesis (`product/research/ass-111/ass-111-findings.md`, Revision 3 + addendum), architect review 2 (`ass-111-s2-architect-review-2.md`, horizontal scale-out C16–C28), and ass-110. New, not from research: dependency-upgrade track (§3), two-level authority + Auditor roles, OIDC seam, generic inference seam. Trusted audit from ass-111 S9 (`ass-111-s9-findings.md`).

---

## 1. What V2 is

One hosted, multi-tenant Unimatrix service (S2 Option A): per-project SQLite + HNSW data plane, SQLite control plane, **project-placed, tenant-affine cells** behind a stateless router (architect review 2). Scales horizontally by adding standard cells, never by growing one box. Same binary and codebase, evolving on `main`.

**Authority model:** two levels — **Platform** (Platform Owner, Platform Auditor; operator admin plane) and **Tenant** (Owner, Writer, Reader, Auditor; plus service principals). Platform roles grant no access to tenant knowledge. User sign-in via OAuth 2.1 behind an **OIDC identity seam** (GitHub first).

**Trusted audit** (ass-111 S9): class A state changes audited in the same transaction, fail-closed; class B security events synchronous before response; class C access through a drained, loss-counted queue. Per-store audit hash chain, monotonic-insert trigger, anchored in the control plane. Measured faster than V1 (one commit instead of two). See §3a.

**Verdict:** CONDITIONAL GO. Delivered as three gated releases:

| Release | Who | Enter when |
|---|---|---|
| **α (dogfood)** | Human's own projects, one personal tenant | D1–D4 decided; Stages 1–2 complete (KC1, KC2 pass); Stage 3 built. Exits when all projects migrated and V1 retired |
| **β (second tenant)** | Med Search, own tenant | α exited; Stage 4 complete (fair-share + cold-start bounds measured); D8–D11 decided |
| **GA (external)** | Self-serve signup | Stage 7 passed — separate decision, taken after β |

**Branching:** `main` = V2. `release/v1` = V1 maintenance (any V1 update allowed) until α exit, then retired. `v*` tags become V1-only; V2 client uses `client-v*`; server deploys by image digest.

---

## 2. Decisions required (ordered by what they gate)

| # | Decision | Gates |
|---|---|---|
| D1 | V2 go/no-go + α→β→GA staging | Everything |
| D23 | CI, release and branching model (incl. D23b repo visibility) | Stage 0 CI port |
| D20 | Refactor-then-evolve (not rewrite) | Stage 0 refactor track |
| D2 + D22 | Local deployment disposition (H1) + V1 maintenance wording / porting rule | Stage 0, α |
| D3 | Durability / availability posture (RPO ≤60 s, no HA at launch) | α |
| D4 | Hosted authorization server — vendor; npm client as OAuth client | α |
| D21 | File-size limit policy + ratchet | Stage 0 W0 |
| D5–D7 | V2 vision/goals/principles; ADR reversals; repo/licence boundary (new crates) | Stage 2 |
| D8–D11 | Learning mode; retraction authority; tenant shapes; public-chat boundary | β |
| D12–D14, D17–D19 | Harness strategy; identity tiers; shared scope; learning defaults; taxonomy authority; engine boundary | Stages 4–6 |
| D15–D16 | Data governance (erasure/retention); launch region | GA |
| **DEP** *(new)* | Dependency-upgrade policy — scope, order, and whether `release/v1` stays pinned | Stage 0 dependency track |
| **D28** | Hosting platform — Hetzner VMs (recommended) vs Fly Machines (~3× compute); decided with D16 | First hosted deploy (Stage 3) |
| **D29** | Per-cell blast-radius cap (proposal ~100 tenants or one large tenant class); second production cell mandatory at GA, one cell acceptable through β | GA |
| **D30** | Inference worker timing — seam built in Stage 1 with local implementation; out-of-process worker only if arena cap fails or NLI-on tenant by β; network-remote on KC1 trigger | Stage 1 |
| **INF** *(new)* | Which model-backed capabilities (NLI, GGUF tasks, other) V2 actually ships — none are committed; the seam is | Post-β, via research spike |
| **AUTH-L** *(new)* | Two-level authority model (Platform / Tenant) and role set incl. Auditor at both levels | Stage 2 (control plane schema) |
| **OIDC** *(new)* | OIDC identity seam — GitHub as first IdP; AS choice (D4) must support it | Stage 3; feeds D4 |
| **D24** | Audit classes + failure semantics (A fail-closed, B deny-and-record-gap, C best-effort counted; no fire-and-forget) | Stage 1 G18 |
| **D25** | Access-audit completeness per tenant (best-effort counted default, per-tenant synchronous upgrade) | β; ties D11, D15 |
| **D26** | Anchor strength — control plane at Stage 2, tenant-visible heads in receipts/export by GA | Stage 2 / GA |
| **D27** | V1 export fidelity — fix export/import to carry all audit columns before the V1 cut (recommended) or declare the loss | Stage 0 step 4 (V1 cut) |
| D15 *(extended)* | Audit retention — life of project, no time-based GC, until GA | GA |

Step 0 (v0.12.0) needs no decision.

---

## 3. Stage 0 — Decide, de-risk, clear the blocking debt

**Ordered steps** (human direction, Revision 3 + addendum; dependency track inserted):

0. **Cut V1 v0.12.0** from `main` via existing `/uni-release` + `release.yml`, unchanged (87 commits since v0.11.3).
1. **Port CI to Actions on `main`, as-is** — Rust build/clippy/test, infra-002 guards, infra-001 suites tiered (smoke per PR, full on merge). Trigger on `main` + `release/**`. Non-required until green. Via GH issue.
2. **Green baseline** — fix/quarantine flaky tests (issue-linked waivers), triage 15 xfail, remove blind retry in `release.yml`, then make PR tier required.
3. **V1 security fixes** — `agent_id:"human"` → Admin; `context_cycle_review` missing `require_cap`. Bugfix sessions on `main`, released as v0.12.x.
   - Under D27 (a), the same v0.12.x patch makes V1 export/import carry every audit column (one small session [E]). Operational: restart the live daemon on a current binary.
4. **Cut V1 branch-point tag + `release/v1`** — first narrow V1 release workflow to V1-only tag pattern with channel-aware `latest` pointers. Tag fixes the V1 export format.
5. **Dependency upgrade track** *(new — see below)*.
6. **W0 size ratchet** — in-repo script + committed baseline, CI job.
7. **Module splits, CI-gated, behavior-neutral:**
   - W1a: move inline test modules >100 lines out of line (clears 48 of 85 violators).
   - W2: split six prerequisite modules in Stage 1 order — `background.rs` (+ `services/status.rs`, `services/nli_detection_tick.rs`); `store/migration.rs`, `store/db.rs`; `mcp/tools.rs`, `server.rs`; `uds/listener.rs` (`dispatch_request` → transport-neutral hook-dispatch).
   - W1b: split test files >1,000 lines (off critical path).

**De-risk in parallel:** Litestream checkpoint PoC (C12); AS evaluation with live claude.ai CIMD/DCR test; Stage 1–3 effort sizing (KC4).

**Eval:** local, α/synthetic data only. Run ass-098 odometer before W2 and after each W2 split; out-of-noise change blocks the next split.

### Dependency upgrade track (new)

**Why:** V2 needs current dependencies — MCP stateless revision (rmcp 3.x), security alerts (4 high cleared by #1024), and a pre-release ONNX runtime are all on the critical path. V1 stays on its pinned set on `release/v1`.

**Placement rationale:** after the V1 cut (so `release/v1` keeps the known-good set and upgrades never need backporting) and behind green required CI (so each upgrade is gated). Before the W2 splits where possible, so splits land on the final dependency surface.

**Known surface (from `Cargo.toml`):**

| Dependency | Current | Note |
|---|---|---|
| `rmcp` / `rmcp-macros` | `=1.7.0` | → 3.x (#1024, ass-107). Couples with G14 `client_type_map` fix (R17) — may need to land with Stage 3 rather than Stage 0 |
| `ort` / `ort-sys` | `=2.0.0-rc.9` | Pre-release pin; ONNX Runtime now from hash-pinned wheel (#1049). Affects R14 determinism |
| `sqlx` | 0.8 | Store layer; touches 413 store-crate SQL sites |
| `tokenizers` | 0.21 | Embedding pipeline |
| `hnsw_rs` | 0.3 + patched `anndists` | Local patch for edition 2024 — check upstream |
| `bincode` | 2 | Serialization of persisted state |
| `tokio-rustls` / `rustls-pemfile` | 0.26 / 2 | TLS; OAuth resource server depends on it |
| Node client (`packages/`) | — | npm deps for the V2 client line |

**Steps:**
1. Inventory — `cargo outdated` / `npm outdated` + open Dependabot alerts (20: 4 high, 8 moderate, 8 low).
2. Classify — patch/minor (batch) vs. major/breaking (one per delivery).
3. Order — security-driven first; then ML stack (`ort`, `tokenizers`) with eval before/after; then store (`sqlx`); `rmcp` 3.x per R17 placement.
4. Gate each — CI green + eval baseline unchanged beyond noise.

**Open:** exact scope ("most if not all"), whether rmcp moves to Stage 3, and whether any upgrade must reach V1 users (default: no backport).

### 3a. Trusted audit (ass-111 S9)

**Finding:** V1 audit is not trustworthy — written outside the state-change transaction, errors swallowed, several change and security-event kinds unaudited, triggers bypassable by a raw-DB actor, and a confirmed long-running silent outage in a real store. S7's "P2 holds" becomes **"holds only after Stage 1 G18"**. (Defect specifics stay out of public posts until Stage 1 lands.)

**Design:**

| Class | Covers | Write | On failure |
|---|---|---|---|
| **A — state change** | Every authoritative change in a knowledge store or control plane — client, background job or operator CLI | Same transaction via store-crate `*_with_audit` methods; rowid allocator (no counter) | Fail the operation |
| **B — security event** | Auth failures, denials, rejected attestations, rate/content rejects | Synchronous, own transaction, before response; floods aggregated | Deny anyway, log, record `gap` marker |
| **C — access** | Reads, search, briefing | Bounded queue, batched, drained at shutdown | Degrade; drops persisted as `audit_dropped` row |
| — | Derived/learned state | Not audited | — |

**Tamper evidence:** `prev_hash`/`row_hash` chain per store (genesis at project creation; V1 imports unchained under one digest row); monotonic-insert trigger; anchors `(project_id, epoch, event_id, row_hash)` into control-plane audit on interval and every drain/move; `verify` CLI + fleet job.

**Cost [M]:** same-txn p50 347 µs vs 493 µs today (+34% throughput single writer, +26% at 8 writers); chain +61 µs p50. Today's path lost audit rows of committed writes under write pressure and SIGKILL; same-txn lost none.

**Retention/export:** life of project, no time-based GC; export carries every audit column + chain head; removal only by whole-project purge (class A in control plane). Sealed archival post-GA.

**Extended gaps:** G9 adds `correlation_id`, `epoch`, ms timestamp, `system:<job>` and operator principals, full-column export/import. G10 every denial reaches platform log. G11 control-plane audit is class A + chained, holds operator CLI actions, platform denials, anchors, saga correlation rows. **New G18** — the audit write path itself.

**Exit Stage 0:** PoC result in, AS chosen, sizing done; v0.12.0 + v0.12.x shipped; PR tier green and required; `release/v1` exists; dependency track complete (or residual items bound to a later stage); W0 landed; six modules off the size baseline before Stage 1 touches them.

---

## 4. Stages 1–7

| Stage | Goal | Key work | Must prove | Gate |
|---|---|---|---|---|
| **1 — No-regret foundations** (today's product) | Retire cost + durability uncertainty | Vectors in SQLite with `model_id`+`dim`, HNSW rebuilt without inference (C1, C24) — entry, vector and audit row commit in one transaction; shutdown is never the persistence point (C27); tick fixes 1–2 (contradiction-scan gate, bound S1 enrichment), fix 3 (NLI new pairs only) only if NLI stays enabled anywhere; **generic inference seam** (C21, widened): task-typed contract (embed / score / generate-extract), model- and runtime-agnostic (ONNX, GGUF, other), workers advertise a catalog (model id, runtime, hardware), owned serializable requests, no store handle, no payload persistence — default implementation in-process, same container; no NLI- or GGUF-specific architecture; interactive/background inference lanes, explicit ORT threads from cgroup budget (C23); **G18** class-A same-transaction audit + rowid allocator + monotonic trigger + per-store chain, unaudited mutators covered, per-tick GC WARN removed; G9 extended columns; LI-2 hook-sink predicates; LI-3 drop audit-sourced learning; `ATTACH` ban CI gate | Post-fix tick cost [M]; inference-free rebuild + restore time; **audit census test** (every mutating path → exactly one in-txn row); SIGKILL + write-hog probes with 0 committed-without-audit; `verify` passes on a migrated store | **KC1** |
| **2 — Multi-tenant core** | Isolation by construction, N-wide shape | `ControlPlaneStore` (new crate) with **Platform / Tenant authority levels**; **Auditor role planned and built here** — Platform Auditor and Tenant Auditor in schema and authorization, read-only over their level's audit logs, no knowledge-content access unless also granted, enforced and covered by the isolation gate (viewer UI post-GA); control-plane audit class A + chained (G11); class-B writer with G10 platform denial log; class-C queue; chain-anchoring saga; `epoch` + `project_opened`/`project_drained` audit rows (C19); cross-plane ops write correlated class-A rows in both logs (C18); quota checked on data rows before the transaction (C26); `ProjectKey::Tenant`, `/v1/{tenant}/{project}`; dynamic resolver (`RouteError::Misdirected`); immutable ids; principals/tokens (G5, G7); legacy token off tenant plane (G8); reserve qualified-id wire format. **Scale-out day-one:** explicit router↔cell boundary via `RoutedRequest` (C17); no cross-plane transactions — idempotent sagas, cached serving during control-plane outage (C18); assignment-epoch fencing (C19); scheduler scoped to cell's assignment set, fleet jobs under single-runner lease (C20); field census classifies all state project / cell-stateless / forbidden-global; CI also exercises the inference seam out-of-process so it cannot silently become in-process-only | Bidirectional cross-tenant isolation gate **across a two-cell CI topology** (C16), incl. a project move between cells with knowledge chain **and audit chain** verified (no gap, no fork, head continuous across epoch) and no re-embed | **KC2** |
| **3 — Identity, onboarding, migration** | Reach α | OAuth 2.1 resource server (new crate) behind **OIDC identity seam** (GitHub IdP); Node client OAuth; `npx unimatrix init`; Tenant roles Owner/Writer/Reader/**Auditor** + service principals, Admin from Owner (G6); Platform Owner/Auditor on separate admin plane (SSO + hardware key); Auditor role (from Stage 2) assignable through onboarding; migration receipts carry V1 audit digest, V2 genesis head and declared V1 history gaps; import runs before the project's first open (T23); network-volume PoC first if D28 relies on reattachable volumes; client classes K1–K5; quotas keyed `(principal, project)`, wider limits eventually consistent (C26); loss-tolerant session state with client re-init test (C22); PCD core + DA1; migration importer; Litestream backup (replica path `project_id/epoch`); metrics; **V2 cell image** — role env, baked hash-pinned models, edge TLS + internal mTLS, readiness ≠ liveness (C27); continuous-deploy pipeline; first `client-v*` release | All projects migrated with receipts; α soak with no V1 fallback → **retire V1** | **α exit** |
| **4 — Second tenant** | Reach β | Residency (lazy load, LRU, memory budget); fair-share scheduler (with C23 lanes); retraction; contradiction policy; excludable inferred edges (#1013); `learning_mode`; DA10 + med-search template; K4/K5; GitHub OIDC workload exchange; fleet jobs under C20 (export/purge, backup verify + expiry, quarantine purge, usage rollup, model-epoch re-embed, rotation, audit verification); access-audit tier per D25 | Fair-share bound; cold-start bound; deterministic test; K5 writes zero learning rows | **β** |
| **5 — Agent identity T2** (parallel with 4) | Attested identity | Device-key registry; call-id cache keyed per project, persisted or drained before a move completes; Claude Code `PreToolUse` adapter; OpenCode v2 plugin pinned + CI fire-test; tier-gated ops (PL-4 amended) | Forgery probes P1–P7 over remote HTTPS; Codex at T1 | none |
| **6 — Intra-tenant shared spaces** | Explicit sharing | `kind=shared` stores; accepted grants; async owned-result `ReadView`; `FederatedSearch`; publish/fork provenance; shared store + consumers as one placement group (C25) | Parity, twin, three-case isolation tests | none |
| **7 — GA gate** | External tenants | HA per D3; erasure/retention (incl. audit retention) published; audit chain verify green fleet-wide; tenant-visible chain heads (D26); tenant export + purge; region; quotas/pricing; per-cell blast-radius cap (C28, D29); second production cell; security review (covers S9 inventory rows 21–30); cost re-measured on Med Search; tenant admin console (decide GA vs post-GA) | All requirements | **GA** |

**Cell sizing:** α/β 1 × 16 vCPU / 64 GB while NLI is in-cell; after Stage 1, standard cell = 8 vCPU / 32 GB, scale by adding cells. Hosting per E-A.

**Post-GA:** Auditor audit-viewing interface (tenant-scoped, read-only); sealed audit archival to object storage; SOC 2 readiness; enterprise IdP federation (SAML/SCIM) behind the OIDC seam; separate router processes; separate inference worker tier (own container, optionally GPU, scaled on background backlog — brings cross-cell fairness at the pool, worker autoscaling, model-catalog routing, tenant-data egress under D15, per-tenant inference cost); model-backed capabilities per INF (generative output lands as provenance-tagged machine-inferred, excluded in deterministic mode); automated rebalancer; T3 identity; NLI contradiction candidates; shared-space feedback; cross-tenant sharing; second region; Postgres control plane on HA trigger; quarantine purge lifecycle; chain-head anchoring (KI-CHAIN-XV-STRONG).

**Pre-split rule (Stages 1–6):** first delivery touching an over-limit module splits it as its first commit; ratchet blocks growth; new V2 subsystems are new crates.

---

## 5. What must be proven first

1. Post-fix tick cost [M] (KC1).
2. Vectors in SQLite, rebuilt without inference.
3. G18 same-transaction audit with census test, and G9 (Stage 1); then G6 with two-level roles (Stage 3) — all before a second tenant.
4. Bidirectional cross-tenant isolation gate on a two-cell topology, including a project move.
5. Fair-share and cold-start bounds.

---

## 6. Kill / re-plan criteria

- **KC1 cost** — one 16-core cell can't hold 900 s tick for 100 mixed projects after fixes 1–3 → stop before GA, re-scope background ML.
- **KC2 isolation** — gate can't cover hook-path sinks and audit-derived learning → no second tenant.
- **KC3 backup** — Litestream PoC fails and snapshot RPO worse than D3 → libSQL/Turso spike before β.
- **KC4 effort** — re-confirm GO after Stage 1–3 sizing; refactor rate re-derived from first two W1/W2 sessions.

---

## 7. Effort (known so far)

| Component | Estimate |
|---|---|
| CI port + green baseline | ≈2–5 sessions [E] |
| Refactor debt (total) | ≈28–31 split sessions [D] (range 15–45); ≈15–18 in Stage 0 |
| Before first tick fix | ≈6–10 sessions |
| Dependency upgrades | **Unsized** |
| Feature work Stages 1–3 | **Unsized** |
| V1 ports | Counted per port |
| Hosted AS | **Unpriced** |

---

## 8. Top risks

R1 cost estimate wrong · R3/R4 RPO 60 s, no HA · R5 isolation defect = cross-customer incident · R8 solo effort with two release lines · R9 legal/erasure undecided · R10 AS dependency · R17 rmcp 3.x + tenancy same window · R19 refactor regressions · R24 release-channel collisions · R26 client/server version skew · **new: dependency upgrades regress retrieval quality or determinism (mitigate: eval before/after ML-stack upgrades)** · **R31 scale-out rot** — process-global state ships in Stages 1–3 and the second cell becomes a re-architecture (mitigate: C16 two-cell CI + field census) · **R28 audit-coupled availability** — fail-closed class A turns an audit defect into a write outage (mitigate: rowid allocator, census test, alert on audit-insert errors; accepted by design) · **R29** V1 audit non-transactional and partly missing (closed for V2 by G18) · **R30** V1 audit history gaps are permanent (declared in receipts) · **R11 extended** — operator can rewrite a whole audit chain until anchors leave operator control (D26) · **new: inference seam shaped around one model type and needs widening later (mitigate: task-typed contract + out-of-process CI from day one).**

Full register: ass-111 findings, "Unresolved risks".
