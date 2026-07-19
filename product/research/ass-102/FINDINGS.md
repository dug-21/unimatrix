# FINDINGS: Extensible-platform assessment + capability breakdown for `goal:platform`

**Spike**: ass-102
**Date**: 2026-07-18
**Approach**: investigation / assessment (read-only; no build)
**Confidence**: directional — a candidate objective + capability decomposition backed by code-verified seam facts (file:line); proposes the breakdown for uni-zero to author, does not validate by build and does not author capability nodes.
**Goal advanced**: `goal:platform` — Extensible platform surface (Unimatrix #5689)

> **Firewall (locked).** This spike *proposes* the capability breakdown. It does **not** author capability nodes into Unimatrix (uni-zero owns that), does not amend ADRs, does not build. All Unimatrix access was read-only. Section (c) is INPUT for a uni-zero authoring pass.

---

## Orientation — what `goal:platform` actually owns

`goal:platform` (#5689) is a **meta-surface** over three layers. It does **not** re-prove that domain config works — `goal:domain-agnostic` (#5683) already owns that and its claim-floor is **fully met** (DA2/3/4/6/7 all `delivery:proven`). Platform's job is the **delta**: naming, **presenting**, **versioning**, and giving **DevX** to the seams a solution extends, so "what Unimatrix IS (engine) vs what it BECOMES (a solution)" stays clean.

The load-bearing distinction throughout this assessment: **a seam existing in code != a seam presented as a versioned contract.** Nearly every L1/L2/L3 seam below exists and (for L1/L3) is even documented — but **none carries a version/stability marker**, the L2 policy seam is `pub(crate)` and exercised by nobody, and there is no published compatibility contract or DevX measure. That gap *is* the goal.

---

## Findings (the 5 "What to explore" items)

### Q: 1. Seam inventory + state — enumerate L1/L2/L3 extension points; presented-vs-implicit, versioned-vs-not, stability contract today.
**Answer**: 12 distinct seams inventoried across the three layers (full table in **deliverable (a)**). Headline state: **L1 is presented-but-unversioned** (categories, confidence weights, server instructions, observation domain packs — all TOML-externalized, documented in README + `config.toml`, but zero version markers; domain-pack registry is startup-frozen). **L2 is a real-but-internal seam** (`BearerValidator` trait exists, is `pub(crate)`, single hardwired impl, no config selection, no issuer verifier). **L3 is presented-and-additively-stable but not formally versioned** (15 MCP tools presented via `#[tool(description)]`; tolerant-reader/additive-only posture is real and code-documented but has no published contract and no breaking-change guard; `/v1/` is enforced-but-frozen, cosmetic as a version lever; SDK is semver-published and standalone but lockstep-pinned with no skew policy). The only **hard** version gates in the whole repo are the DB `schema_version` (31, forward-guarded migrations) and the export bundle `format_version` — neither of which is the *extension* contract.
**Evidence**: L1 — `KnowledgeConfig.categories` (`infra/config.rs:395`), `INITIAL_CATEGORIES` allowlist (`infra/categories/mod.rs:15`), confidence presets/weights (`confidence.rs:21-31`, `config.rs:463/3719`), server instructions (`server.rs:191`, `config.rs:428`, per-slug-overlayable `config.rs:4511`), `DomainPack`/`DomainPackRegistry` startup-frozen (`observe/src/domain/mod.rs:28,75,96`), `ProjectConfigEntry` slug-only (`config.rs:124`) + file-based per-slug overlay (`http_provision/slug_config.rs:56`) with `PER_SLUG_CONFIG_CLASSIFICATION` (`config.rs:4447`). L2 — `BearerValidator` trait `pub(crate)` (`http/auth.rs:70`), single `StaticTokenValidator` minting fixed `http-bearer`/`Restricted` (`auth.rs:107-137`), hardwired at `StaticTokenAuthLayer::new` (`auth.rs:151`). L3 — 15 `#[tool]` handlers (`mcp/tools.rs`), no `deny_unknown_fields` anywhere (documented `tools.rs:8026`, `engine/wire.rs:234/281`), `/v1/` grammar (`http/router.rs:201`, `router/seam.rs:243/440`), SDK `@dug-21/unimatrix` 0.11.3 (`packages/unimatrix/package.json`), DB `CURRENT_SCHEMA_VERSION=31` (`store/migration.rs:26`). **No config or wire surface carries a version/stability field** (confirmed across all three explorers).
**Recommendation**: Treat "**present a version + stability tier on the extension surfaces**" as the single highest-leverage platform move — it is the property every seam is missing and the one the goal names first. Do not conflate the DB `schema_version` (internal durability) with an *extension-contract* version (what a solution author builds against); they are different streams and only the latter is platform's concern.

### Q: 2. DevX baseline — what it takes today to (a) add a domain pack (L1), (b) add a policy/auth seam consumer (L2), (c) stand up a new domain solution (L3); sharpest edges.
**Answer**: (a) **L1 domain pack — config-only and documented, the healthiest path**, but with sharp edges: required fields fail-fast at boot (good), yet the surface is startup-frozen (no live reload), has a two-site default trap on `boosted/adaptive_categories` (serde-default `["lesson-learned"]` vs Rust `Default` `[]`), and README carries category-count drift (says both "5" and "7"). (b) **L2 policy consumer — not reachable without reaching into internals**: the seam (`BearerValidator`) is `pub(crate)`, has one hardwired impl, and offers no config selection point — a solution literally cannot supply a verifier without editing engine source. There is *no* issuer-verifying (relying-party) impl at all, and enforcement behind it is hollow (ass-100/101). (c) **L3 stand-up — the best-trodden path**: consume the 15 MCP tools over the bridge; presented and additively stable. But there is no *published* "here is the contract you're building against" doc — an author learns the tolerant-reader rules by reading Rust comments, and there is no guard that a future change won't break them. Full gap detail in **deliverable (b)**.
**Evidence**: L1 config-only add proven by DA2/DA3 (`#5531`/`#5627`, `delivery:proven`); domain-pack config `config.rs:147`, startup-frozen `observe/domain/mod.rs:75`; default trap `config.rs:370-376` vs `415-416`; README drift (explorer). L2 unreachable — `pub(crate) trait BearerValidator` (`auth.rs:70`), hardwired construction (`auth.rs:151`); enforcement hollow (ass-100 day-1-seam verdict, gaps 1–6). L3 tools presented (`tools.rs` `#[tool(description)]`), tolerant-reader documented only in comments (`tools.rs:8026`).
**Recommendation**: Rank the DevX fixes by the layer's *distance from presented*: L2 is furthest (make the seam `pub` + config-selectable + documented before anything else claims "solutions plug their own policy"); L1 is closest (fix doc drift + the default trap + publish a version tier); L3 needs a *published* compatibility contract + one guard test, not new mechanism.

### Q: 3. The identity seam as the reference L2 — is the ass-100/101 verifier generic (any issuer) or Jurati-shaped? What makes it a clean, presented L2 seam?
**Answer**: The **code seam is generic; the reference consumer (Jurati) is not the seam's shape.** `BearerValidator` (`auth.rs:70`) is issuer-agnostic by construction — `validate(token) -> ResolvedIdentity`, no consumer specifics. The ass-100/101 recommendation (pinned Ed25519 short-lived signed assertion, verified once at the auth layer, single-root delegation) is likewise a **generic relying-party pattern** — "trust one pinned issuer, accept the `sub`s it vouches for" — the same shape as OIDC/SPIFFE/SSH-CA, *not* a Jurati-specific protocol. Jurati is merely the first controller-proxy that would present such an assertion. **What is missing to make it a clean, presented L2 seam** is not genericity — it is presentation and safety: (1) the trait is `pub(crate)`, so no solution can supply an impl without editing the engine; (2) there is no config selection point — `StaticTokenAuthLayer::new` hardwires the one impl; (3) no issuer-verifying impl exists (only the deployment-token validator); (4) the default is **fail-open** (`resolve_or_enroll` auto-enrolls unknown ids), so a policy seam that "declines to Unimatrix" currently declines *toward permissive*, the wrong direction for a security seam; (5) it is undocumented as an extension point.
**Evidence**: ass-100 Q5 + day-1-seam verdict (seam "largely hollow," 6-gap list, fail-open by construction); ass-101 consensus (Anchor B pinned-Ed25519 generic single-root delegation; verify once at `auth.rs:198` -> `external_identity=Some`; same `enforce_external_identity` flag preserves OSS default OFF); `BearerValidator` trait shape + single `StaticTokenValidator` (`auth.rs:70,82,128-137`).
**Recommendation**: Promote the seam as **generic relying-party by name**: make `BearerValidator` (or a thin identity-verifier wrapper) a **public, config-selectable** extension point with a documented contract, one shipped generic issuer-verifier impl (Ed25519 assertion per ass-101), and a **fail-closed** default when the enforce flag is ON. Explicitly forbid encoding any Jurati specifics in the seam — the seam trusts *an issuer*, never *a named consumer*. **Do not** build the full enforcement gate rewrite or per-agent PKI now (see deliverable (d)).

### Q: 4. Real L3 evidence — which domains actually exercise the engine, and which seams are exercised vs assumed?
**Answer**: Only **SDLC (`uni-*`)** genuinely exercises the engine across the memory seam (heavy L3 read+write: briefing/search/get/store/correct/cycle/deprecate/quarantine/tag/edge/lookup) and is the **sole active L1 pack** (built-in `claude-code`, default). **Research (`uni-research-*`)** exercises L3 **read paths** (briefing/search/get, read-mostly) and is a *validated fit target* (ass-057) but rides the same roster and registers no pack. **Environmental/NDP (`ndp-*`)** is a complete 19-agent roster that touches **zero Unimatrix engine seams** — file-based ADRs, file-driven validation, its own product config; grep for `mcp__unimatrix__*`/`context_*` across all 19 `ndp-*` files returns nothing. **L2 (policy/identity) is exercised by nobody** — it is the purely *assumed* seam that ass-100/101/102 are probing; Jurati appears only in research spikes, never in a roster. **Consequence for the goal**: the exercised seams are **L1-config + L3-consume**; the L2 policy seam and any live/dynamic L1 reload are **assumed, not exercised**. NDP is also positive evidence that a "domain solution" can exist as pure agent-spine convention with *no* engine coupling — extension is a spectrum, not a single contract.
**Evidence**: roster explorer — `.claude/agents/uni/` (20 agents, MCP-heavy), `.claude/agents/ndp/` (19 agents, zero `context_*`), `product/research/ass-057/` (research fit validation, graph-first orthogonal to vector, assesses gaps not-yet-served), `config.toml:140-153` (only `claude-code` pack active, others commented). L2 inert per ass-100 (`external_identity` all 17 sites pass `None`).
**Recommendation**: Anchor the **claim-floor on SDLC + research** (the two domains that actually consume L1+L3). Treat L2 as a **named seam promoted only when a real consumer (Jurati) lands** — building it now is speculative. Do **not** treat NDP as evidence the engine seams work (it proves the opposite — a domain can bypass them entirely); NDP is out-of-scope for platform proof.

### Q: 5. Candidate capability decomposition — objectives + capabilities (functional + nfr), draft `done_when`, first-pass status; claim-floor vs north-star.
**Answer**: Proposed as **4 objectives / 9 candidate capabilities** in **deliverable (c)**, using the `uni-capability` format (kind, name, why, done_when, delivery status, archetype). First-pass status read against today's code: **1 proven** (standalone SDK), **4 partial**, **4 missing** — one of which (PL-1) is a **shared node reused from domain-agnostic** (DA1 runtime discovery, #5526). Claim-floor vs north-star split and the prove-core / seam-rest call are in **deliverables (c)** and **(d)**.
**Evidence**: synthesized from findings 1–4 above; status pinned to the seam facts cited throughout.
**Recommendation**: uni-zero authors these as `category:"capability"` nodes with `Advances`->#5689 edges; **reuse (do not duplicate) DA1 (#5526)** as a shared node advancing both `domain-agnostic` and `platform`. Author the L2 capabilities as **named seams at `delivery:missing`** without scheduling their architectural build — hold the line in deliverable (d).

---

## (a) Seam inventory + state table

Legend — **Presented**: documented/discoverable by a solution author · **Versioned**: carries a version/stability marker · **Stability contract**: the actual guarantee today.

### L1 — domain config

| # | Seam | Presented? | Versioned? | Stability contract today | Evidence |
|---|------|-----------|-----------|--------------------------|----------|
| L1-1 | Knowledge **categories** (open `Vec<String>` allowlist, not enum) | **Yes** (README, `config.toml:36`) | **No** | Operator-set at boot, frozen after; serde-default = 7 `INITIAL_CATEGORIES`; README count drift (5 vs 7) | `config.rs:395`, `categories/mod.rs:15` |
| L1-2 | **boosted/adaptive categories** | Partial (`config.toml:44`) | No | Two-site default trap (serde `["lesson-learned"]` vs Rust `Default` `[]`) | `config.rs:399,403,370-376` |
| L1-3 | **Confidence presets** (`enum Preset`) | Yes (README:306) | No | 4 preset weight tables **hardcoded in Rust**; Collaborative default | `config.rs:166,2564,3719` |
| L1-4 | **Custom confidence weights** (TOML) | Yes (`config.toml:176`) | No | Honored only when `preset="custom"`; must sum 0.92 +/-1e-9; no `Default` (anti-zero-init) | `config.rs:463,471,3652` |
| L1-5 | **Fusion / PPR weights** | Yes ("do not change unless directed") | No | Per-field serde defaults; internal tuning surface | `config.rs:596+`, `config.toml:228` |
| L1-6 | **Server instructions** (MCP `instructions`) | Yes (`config.toml:74`) | No | `Option<String>`->compiled default; per-slug-overlayable; injection-scanned; **verbatim, not templated** | `server.rs:191`, `config.rs:428,4511` |
| L1-7 | **Observation domain packs** (`DomainPack`) | **Yes** (most-presented surface — README:229, `config.toml:139`) | No | Config-registered, boot-validated, **startup-frozen** (no runtime re-registration); built-in `claude-code` hardcoded + always loaded | `observe/domain/mod.rs:28,45,75,96`; `config.rs:147` |
| L1-8 | **Per-slug config overlay** (`PerSlugOverlayable`/`GlobalLocked`) | Yes (README:277) | No | File-based overlay `{base}/{slug}/config.toml`; explicit overlayable/locked key registry; `ProjectConfigEntry` still **slug-only** (D2 body deferred) | `slug_config.rs:56`, `config.rs:124,4447` |

### L2 — policy/auth seams

| # | Seam | Presented? | Versioned? | Stability contract today | Evidence |
|---|------|-----------|-----------|--------------------------|----------|
| L2-1 | **`BearerValidator` trait** (the swappable-verifier seam) | **No** (`pub(crate)`) | No | Real trait seam, but internal-only; **single** hardwired impl; no config selection | `auth.rs:70,151` |
| L2-2 | **Identity verifier / relying-party** (issuer-verifying impl) | **No — does not exist** | No | Only `StaticTokenValidator` (deployment token -> fixed `http-bearer`/`Restricted`); no issuer/`sub` verification | `auth.rs:82,107-137` |
| L2-3 | **`external_identity` enforcement path** (`build_context_with_external_identity`) | No | No | **Hollow**: all 17 sites pass `None`; gate re-resolves caps by `agent_id`, ignores asserted caps; `TrustLevel` never read at gate; **fail-open** auto-enroll | ass-100 Q5/day-1-seam; `server.rs:526` |
| L2-4 | **`enforce_external_identity` gate flag** | No (proposed, not built) | No | Sized on `AgentsConfig` (OFF preserves OSS default); per-slug blocked on D2; fail-closed must be net-new | ass-100 Q6 / ass-101 Q6; `config.rs:434` |
| L2-5 | **Observation ingestion source** (`source_domain` on observe frame) | Partial (part of domain pack) | No | Domain-neutral ingestion proven (DA3); `source_domain` validated `^[a-z0-9_-]{1,64}$`, `unknown` reserved | `observe/domain/mod.rs:205`; DA3 `#5627` |

### L3 — solution contract (memory + capability substrate over MCP wire + SDK)

| # | Seam | Presented? | Versioned? | Stability contract today | Evidence |
|---|------|-----------|-----------|--------------------------|----------|
| L3-1 | **MCP tool wire** (15 `context_*` tools) | **Yes** (`tools/list` + `#[tool(description)]`) | **No** (no schema/tool version) | **Tolerant-reader / additive-only**: no `deny_unknown_fields`, unknown fields silently dropped; add = safe, remove/rename = breaking; contract documented **only in code comments**, no guard test | `mcp/tools.rs`; `tools.rs:8026`, `wire.rs:234/281` |
| L3-2 | **`/v1/` route grammar** | Yes (path) | **Cosmetic** | Enforced (non-`/v1` = hard error) but **frozen** — no `/v2`, no negotiation, not a version-evolution lever | `router.rs:201`, `seam.rs:243,440` |
| L3-3 | **Client SDK** `@dug-21/unimatrix` | **Yes** (npm, README) | **Yes (semver)** | Published 0.11.3, MIT/Apache, **standalone-installable**; CLI/edge (not importable lib); hook-client (observe) + mcp-bridge (MCP proxy); "dumb-client" invariant; **lockstep-pinned to server, no documented skew policy** | `packages/unimatrix/package.json`; `bin/unimatrix.js` |
| L3-4 | **Memory substrate** (`EntryRecord`, hash chain, `RelationType` edges) | Partial (schema in code) | **DB schema_version=31** (strong, internal) | Hash-chained tamper-evidence; additive `serde(default)` fields; edges enum-checked at engine boundary, free TEXT in DB; forward-guarded migrations | `store/schema.rs:49`, `engine/graph.rs:139`, `migration.rs:26` |
| L3-5 | **Capability-map substrate** (GOAL capabilities) | **No** (convention only) | No | `category:"capability"` string + `delivery:*` tag + edge types; contract owned by `SKILL.md`, enforced by **no compiler, no store validation** (contrast: AGENT `Capability` is a closed compiled enum) | `schema.rs:56` (category String), `schema.rs:265` (agent enum); `.claude/skills/uni-capability/SKILL.md` |

**One-line state of the union**: L1 = *presented, unversioned, startup-frozen*. L2 = *real seam, internal, single-impl, hollow, fail-open, exercised by nobody*. L3 = *presented + additively-stable + semver'd SDK, but no published contract, no skew policy, `/v1` frozen*.

---

## (b) DevX gap assessment — concrete friction to extend, per layer

**L1 — add a domain pack / configure a domain** (the healthiest path)
- **Sharpest edge**: startup-frozen registry (`observe/domain/mod.rs:75`) — no live reload; every domain change is a restart. Acceptable today (no domain needs live reload), but undocumented as a constraint.
- **Undocumented/trap steps**: the `boosted/adaptive_categories` two-site default trap (a `#[serde(default)]` fn returns `["lesson-learned"]`, the Rust `Default` returns `[]` — an author reading the struct sees the wrong default); README category-count drift (5 vs 7); preset->weight tables are Rust-hardcoded, so "custom scoring" silently requires `preset="custom"` + all six weights summing to 0.92.
- **Internals-reaching**: none for the config path — this is genuinely config-only (DA2/DA3 proven). The gap is *presentation polish + a version marker*, not mechanism.

**L2 — add a policy/auth seam consumer** (furthest from presented)
- **Sharpest edge (blocking)**: the seam is `pub(crate)` (`auth.rs:70`) and hardwired at construction (`auth.rs:151`) — **a solution cannot supply an identity policy without editing engine source and recompiling.** This is the single clearest violation of "solutions extend it, never reach into it."
- **Missing surface**: no issuer-verifying impl exists; no config selection; no documented contract; **fail-open default** means the seam declines toward permissive (`resolve_or_enroll` auto-enrolls) — the opposite of what a security seam should do.
- **Internals-reaching**: total — every step requires source access today.

**L3 — stand up a new domain solution** (best-trodden, but contract implicit)
- **Sharpest edge**: no *published* compatibility contract. The tolerant-reader/additive-only rules are real and correct but live in Rust comments (`tools.rs:8026`); an author has no document stating "add-only, never rely on unknown-field rejection," and **no test guards a future breaking change.**
- **Undocumented steps**: SDK<->server version skew is undefined — lockstep 0.11.3 with no statement of how much skew is tolerated; the "two hard co-evolution seams" the goal names (wire stability + SDK semver) are asserted, not guarded.
- **Internals-reaching**: low for consume (MCP tools + bridge are presented); the capability-map substrate (L3-5) is convention-only, but that is Unimatrix's internal tool, not a surface solutions must extend.

**DevX has no baseline measure today.** No time-to-first-extension, no seam-doc-completeness metric. NDP is the accidental datapoint: a full roster stood up *bypassing every engine seam* — which measures nothing about the seams and confirms DevX is currently unmeasured.

---

## (c) Candidate capability decomposition — INPUT for uni-zero to author

> Proposed only. uni-zero authors the outcome-phrased nodes (`category:"capability"`, `Advances`->#5689). Status = first-pass read against today's code. Prefix `PL-*`. Archetype: **T**=threshold (binary, terminal), **C**=curve (asymptotic). **CF**=claim-floor, **NS**=north-star.

### OBJ-1 — The L1 domain-config seam is a *presented, versioned* extension surface

| Cap | kind / arch | Draft name & `done_when` | First-pass status |
|-----|-------------|--------------------------|-------------------|
| **PL-1** (= DA1 #5526, shared) | functional / T · **CF** | *An agent/author in any repo discovers this deployment's active taxonomy at runtime.* `done_when`: a runtime call returns the active category set (with per-category when-to-use from config) + the compiled relation-type set (with engine-intrinsic when-to-use), served from in-memory config, correct on an empty corpus; the boot-rendered server instructions name the taxonomy and point to that call. | MISSING (inherit DA1 status; reuse node, add second `Advances` edge — do NOT duplicate) |
| **PL-2** | nfr / T · **CF** | *A solution author can rely on a declared stability tier for the config surface.* `done_when`: the L1 config surface (categories shape, domain-pack shape, confidence-knob set) declares an explicit version + a documented stability tier; a runtime call reports the surface version; a breaking change to the shape bumps it (guarded by a test). | MISSING — no config surface carries any version/stability marker (all three explorers) |

### OBJ-2 — The L2 policy/auth seam is a *generic, presented, fail-safe* relying-party seam

| Cap | kind / arch | Draft name & `done_when` | First-pass status |
|-----|-------------|--------------------------|-------------------|
| **PL-3** | functional / T · **CF** | *A solution supplies its own identity policy against a presented, issuer-agnostic verifier seam — by config, not by editing the engine.* `done_when`: an alternate identity verifier is selected by config and used at the auth layer without recompiling the engine; the seam is `pub`, documented, and issuer-generic (verifies an issuer's assertion/`sub`, encodes no named-consumer specifics). | PARTIAL — `BearerValidator` trait exists (`auth.rs:70`) but `pub(crate)`, single hardwired impl, no config selection, no issuer verifier |
| **PL-4** | nfr (security) / T · **CF** · *About*->PL-3 | *When a solution plugs an identity policy, absent/invalid identity fails closed and the OSS default never silently widens.* `done_when`: enforce flag OFF => bearer-only/audit-only, behavior byte-identical to today; enforce flag ON => absent/invalid/unpinned identity rejected before context build (never auto-enrolled); a test proves both. | MISSING — fail-open by construction (`resolve_or_enroll`), enforcement hollow, no trust-binding (ass-100 gaps 1–6 / ass-101 Q6) |

### OBJ-3 — The L3 solution contract (wire + SDK) is *versioned and co-evolution-safe*

| Cap | kind / arch | Draft name & `done_when` | First-pass status |
|-----|-------------|--------------------------|-------------------|
| **PL-5** | functional / T · **CF** | *A solution consumes the memory substrate over a documented MCP tool contract whose evolution rules are explicit and guarded.* `done_when`: the tool set + params are presented (already true) AND a published compatibility contract (tolerant-reader/additive-only) exists AND a regression test fails on a breaking wire change (removed/renamed field or tool). | PARTIAL — tools presented; tolerant-reader real but documented only in code comments; no published doc, no guard test |
| **PL-6** | nfr / T · **CF** | *Client SDK and server co-evolve under semver without breaking standalone installs.* `done_when`: SDK is semver-published + standalone (already true) AND a version-skew policy is documented and tested (server tolerates a client one minor behind, or reports the skew). | PARTIAL — published 0.11.3 + standalone (proven); lockstep-pinned, no documented/tested skew policy |
| **PL-7** | nfr / T · **CF** | *The extension SDK stays standalone-installable — a solo dev runs plain Unimatrix + a coding-agent CLI with no harness.* `done_when`: `npm i -g @dug-21/unimatrix` + server runs with no domain harness present; hook-client + mcp-bridge function standalone. | PROVEN — standalone-installable, optionalDependency native binaries, pure-Node edge (L3 explorer) |

### OBJ-4 — Extending along any seam is *documented, measured, internals-free* (DevX)

| Cap | kind / arch | Draft name & `done_when` | First-pass status |
|-----|-------------|--------------------------|-------------------|
| **PL-8** | nfr / **C** · **NS** · *About*->PL-1/3/5 | *A developer stands up a new domain solution against documented seams without reaching into engine internals.* `done_when` (curve, clears-current-bar): a solution author configures L1 + (optionally) plugs L2 + consumes L3 using only presented/documented surfaces — no `pub(crate)` reach, no source edit — measured on a real domain; bar rises as new seams are added. | PARTIAL — L1-config + L3-consume reachable (SDLC, research); L2 requires source edit today (blocks the claim) |
| **PL-9** | nfr (keystone) / **C** maintained · *Prerequisite*->PL-8 | *A trusted DevX ruler exists.* `done_when`: seam-doc-completeness (documented seams / total presented seams) and time-to-first-extension are defined and measured repeatably; every PL-8 claim is only as valid as this ruler today. | MISSING — no DevX measure exists |

**Decomposition read**: 1 proven (PL-7), 4 partial (PL-3/5/6/8), 4 missing (PL-1/2/4/9); PL-1 inherits DA1's missing status as a shared node. The **north-star** is PL-8 (DevX curve) gated on the PL-9 ruler; everything else is **claim-floor thresholds**.

**Deliberately NOT proposed** (over-build guard / hold the line): a general plugin-SDK capability; a validator-plugin *loader*; per-agent PKI / N-root credentials; live domain-pack reload; a domain marketplace; making the GOAL capability-map a product surface. None has a real consumer; each would be speculative generality (goal #5689 "Out of scope").

---

## (d) Prove-core-seam-rest recommendation

**Principle**: prove the contract on the domains that **actually exercise the engine** (SDLC + research — finding 4); **name** the rest as seams and promote them **only on evidence** of a real consumer. Hold the "**no speculative plugin SDK**" line explicitly.

### Claim-floor — PROVE now, on SDLC + research (the exercised seams)
These are provable today against real domains and constitute the honest "we have a presented, versioned, DevX-first contract" claim:
- **PL-1** (runtime taxonomy discovery) — both SDLC and research consume L3 and benefit immediately; already scoped as DA1.
- **PL-2** (versioned L1 config surface) — the single highest-leverage gap; presentable/versionable on the existing, proven config mechanism without new domain evidence.
- **PL-5 + PL-6** (documented + guarded wire contract; SDK skew policy) — the real consumers (SDLC heavy, research read) are exactly who the contract protects; provable now.
- **PL-7** (standalone SDK) — **already proven**; counts toward the floor today.

The floor is claimable when PL-1/2/5/6 flip `proven` (PL-7 already is) — i.e. the contract is *presented + versioned* on the seams SDLC and research use. DevX (PL-8) is **north-star**: do not gate the claim on the curve.

### Seam-only — PROMOTE on evidence, do NOT build speculatively
- **L2 identity/policy seam (PL-3, PL-4)** — exercised by **nobody** today (finding 4); Jurati is a research subject, not an in-repo consumer. **Do**: present the seam — make `BearerValidator` public + config-selectable + documented, ship one generic Ed25519 issuer-verifier (ass-101), default fail-closed-when-ON. **Do NOT**: build the architectural enforcement gate rewrite (ass-100 gaps 3–5), the transport trust-binding (gap 6), or per-agent PKI — promote those **only when Jurati (or an equivalent real consumer) lands** and exercises the seam. The seam is named at `delivery:missing`; its build is demand-pulled.
- **Live domain-pack reload / runtime re-registration** — startup-frozen today; no domain needs it. Leave frozen; document the constraint. Do not build a live loader speculatively.
- **Capability-map-as-product (L3-5), per-agent credentials, multi-domain-in-one-instance, marketplace** — out of scope for `goal:platform`; each is speculative generality with no consumer.

### The line, stated
`goal:platform` is **not** a general plugin SDK. It is: (1) the **three concrete seams real domains touch** — L1 config, L3 wire, L3 SDK — made *presented + versioned + DevX-measured*; plus (2) **one named-but-unbuilt L2 seam** (generic relying-party identity), *presented* now, *enforced* only on real-consumer evidence. Prove (1) on SDLC + research; seam (2) and promote on Jurati. Everything past that is over-build.

---

## Unanswered Questions

- **Where does the L1 surface-version belong — a new runtime call, `context_status`, or the server-instructions render?** PL-2 needs a home; DA1's design-locks lean toward extending `context_status` (already serves `category_lifecycle`). Resolvable in a uni-zero/design pass, not here. *(Not a blocker to authoring PL-2.)*
- **What skew tolerance is correct for PL-6 (client N minors behind server)?** Depends on the release cadence and the tolerant-reader guarantees; needs a policy decision, not a spike.
- **Exact per-slug scoping of any L2 enforce flag** remains blocked on the deferred D2 config-overlay body (`ProjectConfigEntry` slug-only, `config.rs:124`) — carried forward from ass-100/101, unchanged.

## Out-of-Scope Discoveries

- **NDP proves a "domain solution" can bypass every Unimatrix engine seam** (19 agents, zero `context_*`, file-based ADRs). This reframes "extension" as a spectrum, not one contract — and raises a strategic question the goal does not yet answer: *is a solution that consumes none of L1/L2/L3 still a "platform extension," or is it just a co-located roster?* One-line note; may warrant a vision clarification, not a spike. *Not pursued.*
- **README category-count drift (says both "5" and "7" categories)** and the **`boosted/adaptive_categories` two-site default trap** are concrete DevX defects surfaced during L1 assessment. File as GitHub issues (per repo convention — bugs are issues, not lessons); do not fold into a capability. *Not pursued.*
- **Two independent version streams already exist and work** (DB `schema_version=31`, export `format_version=1`) — proof the project *can* version a surface; the extension contract simply hasn't adopted the pattern. A design reference for PL-2, not a new finding. *Not pursued.*
- **The GOAL capability-map is the least-formal L3 surface** (convention-only, contract in `SKILL.md`, no compiler/store enforcement) — contrasted with the closed compiled AGENT `Capability` enum. If autonomous goal-driven delivery ever hardens, this convention may need a validated schema; today it is deliberately soft. *Flagged, not pursued.*

---

## Recommendations Summary

- **Seam state**: L1 = presented-but-unversioned + startup-frozen; L2 = real `BearerValidator` trait but `pub(crate)`, single-impl, hollow, fail-open, exercised by nobody; L3 = presented + additively-stable + semver'd standalone SDK, but no published contract, no skew policy, `/v1` frozen. **No extension surface carries a version marker** — that gap is the goal.
- **DevX**: L2 is furthest from presented (source-edit required to plug a policy) — fix first; L1 needs presentation polish + a version tier; L3 needs a *published* compatibility contract + one guard test, not new mechanism. No DevX baseline measure exists today.
- **Identity seam (reference L2)**: the code seam is **generic** (issuer-agnostic), the ass-100/101 anchor is a generic relying-party pattern — Jurati is the consumer, not the shape. Make it clean by presenting it (public + config-selectable + documented + fail-closed), not by re-shaping it.
- **Real L3 evidence**: only SDLC (heavy L3, sole L1 pack) and research (read-mostly L3, validated fit) exercise the engine; NDP touches zero seams; **L2 is assumed, exercised by nobody**. Anchor the claim-floor on SDLC + research.
- **Candidate decomposition (c)**: 4 objectives / 9 capabilities — PL-1..PL-9 (reuse DA1). 1 proven (PL-7 standalone SDK), 4 partial (PL-3/5/6/8), 4 missing. North-star = PL-8 DevX curve gated on the PL-9 ruler; the rest are claim-floor thresholds. uni-zero authors; reuse DA1 (#5526) as a shared node.
- **Prove-core-seam-rest (d)**: **Prove** PL-1/2/5/6 (+ already-proven PL-7) on SDLC + research = the claimable floor. **Seam** PL-3/4 (generic relying-party L2) — present now, enforce only on Jurati evidence. **Hold the line**: no general plugin SDK, no validator-loader, no per-agent PKI, no live pack reload, no marketplace — each is speculative generality with no consumer.
