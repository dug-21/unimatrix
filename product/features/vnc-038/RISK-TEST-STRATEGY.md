# Risk-Based Test Strategy: vnc-038

> Mandatory Project Identity at the Deployment Entrypoint (revises vnc-034). Risks specific to THIS architecture: dumb-client closed-set, atomic `v:2` dual-side bundle, per-slug observe on the funnel, delete-the-default blast radius, `register`→`[[projects]]` atomic write, and (revision pass) the folded-in #735 carry-items — first-boot token surface (ADR-008/AC-11) and the local-direct-binding guard (ADR-006 tightening / C-13). Grounded in vnc-034 history — the ceremonial-funnel lesson (#4974), parity-corpus mechanics (#4956), API-extension call-site gap (#2398), vacuous-pass gate-fix (#4452), and silent-fallthrough gate ordering (#4311).
>
> **Revision pass (vnc-038-agent-3-risk):** #735 folded into vnc-038 (no longer a sequencing collision — SR-06 resolved-by-fold-in, R-11 superseded). ADR-006 TIGHTENED: local STDIO/UDS keeps its DIRECT path-hash binding and is NOT a resolver key — never routed through the unified resolver. ADR-008 ADDED: first-boot token delivered ONLY via the `v:2` bundle, never stdout/logs. New risks: R-13 (local-regression guard / C-13), R-14 (token-to-stdout / AC-11), R-15 (low-severity #735 cleanup verification / AC-12/13). R-07 re-scoped to the new direct-binding model. R-01..R-12 otherwise intact.

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | **Missed client path-composition site** — a compose site survives outside the enumerated closed set (`init.js:303-308`, `transport-http.js:84`), or a future client adds one back, re-opening the #766 bug class. | High | Med | Critical |
| R-02 | **Ceremonial observe funnel** — observe resolves a per-request handle then dispatches through a boot-bound/parallel path; green at N=1, isolation unproven (direct #4974 repeat). | High | Med | Critical |
| R-03 | **`v:2` partial-rollout / decode-parity break** — Rust encoder and JS decoder drift; one side ships `v:2`, the other still on `v:1`; strict exact-key guard hard-rejects → total attach outage. | High | Med | Critical |
| R-04 | **Stale-bundle hard-cut breakage** — any client holding a `v:1` bundle (no backward compat) silently fails decode against a `v:2`-only server, with no actionable signal. | High | Med | Critical |
| R-05 | **Genesis-clobber on re-register** — non-idempotent `register <slug>` against an existing per-slug store genesis-clobbers it, destroying the unrollbackable hash chain (sacred). | High | Low | High |
| R-06 | **Partial/corrupt `[[projects]]` write** — interrupted register leaves a malformed config; daemon fails to boot or silently drops a routable slug. | High | Low | High |
| R-07 | **Delete-the-default over-reach breaks the MCP seam** — retiring `DefaultResolver` / `/v1/tools→Default` / `_=>Default` regresses the per-request MCP seam, or a `Default` consumer is left orphaned. (Local-UDS regression split out to R-13 under the ADR-006 tightening.) | High | Med | Critical |
| R-08 | **Reserved-slug drift** — new grammar (`/v1/{slug}/observe`, `tools` no longer the default alias) shifts what must be reserved; a missed update lets a registerable slug shadow a route segment. | Med | Med | High |
| R-09 | **Cross-pollination at N≥2** — a request bound to project B resolves project A's store (MCP or observe); catastrophic, unrollbackable corruption. | Critical | Low | Critical |
| R-10 | **Loud-first-boot regression** — empty `[[projects]]` resolves a silent default instead of failing loud, re-introducing the silent-default-store landing. | High | Low | High |
| R-11 | **~~#735 router/boot collision~~ — SUPERSEDED by fold-in.** #735 is folded into vnc-038 (no separate effort on the same surface); the sequencing/collision risk is dissolved. Retained for traceability only — see R-13/R-14/R-15 for the carry-item risks that replace it. | Med | Low | Low |
| R-12 | **Init-time Ping passes, runtime hook telemetry does not** (or vice-versa) — only one of the two observe entry points (AC-07/AC-08) is routed correctly; #766's wider blast radius left half-open. | High | Med | Critical |
| R-13 | **Local STDIO/UDS routed through the unified resolver** — delivery wires local (`main.rs:1158`/`:859`) onto the HTTP resolver / `parse_project_key` / a slug-map key instead of keeping its DIRECT path-hash binding, regressing AC-10 and re-introducing a cross-store path (ADR-006 tightening / C-13). This is the concrete GATE-2 guard. | High | Med | Critical |
| R-14 | **First-boot bearer token leaks to stdout/logs** — `http/token.rs:101` still prints the token (or a `tracing` path emits it), violating the cloud HTTPS posture; the `v:2` bundle is supposed to be the SOLE channel (ADR-008 / AC-11 / NFR-06). Failure mode: token persisted in aggregated container logs. | Med | Med | High |
| R-15 | **#735 cleanup verification (low-weight)** — `router.rs` not ≤500 lines after the rewrite (AC-12), or the stale `#![allow(dead_code)]` / "until wiring lands" comment survives in `public_url.rs` (AC-13). Mechanical verification items; track, do not over-weight. | Low | Low | Low |

Priority = Severity × Likelihood (Critical / High / Medium / Low).

## Risk-to-Scenario Mapping

### R-01: Missed client path-composition site (closed-set invariant)
**Severity**: High · **Likelihood**: Med · **Impact**: #766 bug class re-opens; client emits a path the server never authored → 404 or silent mis-target.

**Test Scenarios**:
1. Invariant/grep-assertion test: after the diff, the set of client-side path-composition sites is **empty** — assert no `+ "/v1"`, no slug append, no `/observe` append remain in `init.js` / `transport-http.js` (NFR-01).
2. Byte-for-byte verbatim-post test: given a decoded `v:2` bundle, assert the client POSTs to `mcp_url` and `observe_url` **exactly** as received (capture the outgoing request URL, assert string equality with the bundle field — no normalization, no trailing-slash mutation).
3. Regression guard: a test that fails if a future client re-introduces composition — assert the only source of the request URL is the validated bundle field, not `base_url + grammar`.

**Coverage Requirement**: Every site in the ADR-001 closed set (C-1/C-2/C-3) is deleted and asserted absent; the verbatim-post invariant holds for both MCP and observe.

### R-02: Ceremonial observe funnel (#4974 repeat)
**Severity**: High · **Likelihood**: Med · **Impact**: observe appears routed but a discarded handle (`let _store`) or parallel path serves it; isolation deferred, unproven.

**Test Scenarios** (apply the #4974 VERIFY-THE-FUNNEL checklist):
1. Grep/structure assertion: the observe handler holds the `Arc<dyn StoreResolver>` and resolves **per call**; assert no boot-bound `resolve_store(&ProjectKey::Default)` and no pre-resolved `store` field survive in `ObserveContext`.
2. Counting-resolver test at **N=2**: a recording resolver asserts each observe request consults the resolver **once** with the transport-derived `ProjectKey` — and that no parallel dispatch path can serve observe without going through it.
3. Sole-route assertion: assert no boot-bound or alternate observe adapter exists beside the funnel (the resolved handle is the SOLE route).

**Coverage Requirement**: Observe isolation proven at N=2, not N=1 (C-11). The resolved per-request handle is load-bearing — proven by a test that fails if a parallel/boot-bound path is reintroduced.

### R-03: `v:2` partial-rollout / decode-parity break
**Severity**: High · **Likelihood**: Med · **Impact**: strict exact-key guard hard-rejects mismatched schema → every attach fails; no graceful path.

**Test Scenarios**:
1. Round-trip parity (extended corpus, hex vectors, re-exported `pub(crate)` oracle fns per #4956): Rust-encode `v:2` → JS-decode → field equality, byte-for-byte.
2. Strict-reject matrix on **both** sides: missing key, extra key, wrong-type key, malformed (non-`https://`) URL, unknown major version — each rejects with `BundleError`/`ServerError`, no partial accept.
3. Guard-ordering test (NFR-08): `MAX_RAW_LEN` length cap runs FIRST on raw paste, before scheme/base64url/JSON/schema (preserve v:1 ordering for v:2).

**Coverage Requirement**: The corpus is the shared oracle for `v:2`; encode and decode move atomically; no single-side `v:2` passes its own round-trip against a `v:1` counterpart.

### R-04: Stale-bundle hard-cut breakage (`v:1` holder)
**Severity**: High · **Likelihood**: Med · **Impact**: a client with a `v:1` bundle (no backward compat) decodes against `v:2`-only logic → version-pin reject; user sees an opaque failure.

**Test Scenarios**:
1. JS decoder fed a well-formed `v:1` bundle asserts `obj.v !== 2` → `BundleError` with an **actionable** message (re-issue a `v:2` bundle), not a silent/opaque throw.
2. Rust server presented a `v:1`-shaped artifact at any ingest point rejects loud (no silent `v:1` compat arm survives).
3. Assert there is no `v:1` fallback decode path on either side (hard cut — RD-1/RD-5).

**Coverage Requirement**: A `v:1` bundle fails closed with a message that tells the operator to re-issue; no silent acceptance, no silent default.

### R-05: Genesis-clobber on re-register (hash chain sacred)
**Severity**: High · **Likelihood**: Low · **Impact**: catastrophic, unrollbackable — re-`register` overwrites an existing store's genesis, destroying A's chain.

**Test Scenarios**:
1. Re-attach test (State B precedent): `register <slug>` against an existing per-slug store **opens** it; assert the genesis block / chain head is unchanged (hash equality before/after).
2. Idempotency test: running `register <slug>` twice yields one `[[projects]]` entry and one untouched store (no duplicate stanza, no second genesis).
3. Negative: assert no code path calls genesis-creation when the per-slug data dir already exists.

**Coverage Requirement**: Re-register is provably re-attach (open), never genesis-clobber; chain-head hash invariant before == after.

### R-06: Partial/corrupt `[[projects]]` write
**Severity**: High · **Likelihood**: Low · **Impact**: malformed `config.toml` → daemon fails to boot or drops a routable slug.

**Test Scenarios**:
1. Atomicity test: simulate interruption mid-write (temp+fsync+rename); assert the on-disk `config.toml` is always the complete old OR complete new file, never partial.
2. Round-trip test: write `[[projects]]`, re-read at boot (`load_config_and_build_allowlist`), assert the slug is in `project_slugs`.
3. Existing-config preservation: register into a config that already has N stanzas — assert all N+1 are intact and well-formed (read-modify-write preserves prior entries).

**Coverage Requirement**: The write is atomic and additive; an interrupted register never yields a malformed config or loses an existing project's routing intent.

### R-07: Delete-the-default over-reach breaks the MCP seam
**Severity**: High · **Likelihood**: Med · **Impact**: removing `Default` arms regresses the per-request MCP seam (`/v1/{slug}/...`) or leaves a `Default` consumer orphaned.
> Note (revision): the local-UDS regression is split out to **R-13** under the ADR-006 tightening — local is no longer "path-hash-as-key under the unified resolver"; it bypasses the resolver entirely. R-07 now covers only the HTTP seam and the call-site audit.

**Test Scenarios**:
1. Seam test: assert `parse_project_key` no longer maps `/v1/tools/...` or the `_` arm to a servable `Default`; the MCP `/v1/{slug}/...` per-request seam still resolves correctly. The unified resolver resolves **only** `ProjectKey::Slug` (no `Default` arm).
2. Call-site audit (per #2398): enumerate every consumer of `ProjectKey::Default` / `DefaultResolver` / `MultiProjectRouter.default` / `adapter_for`'s Default arm before removal; assert each is reconciled, not orphaned. The deletions are **HTTP-only** and must not reach into the local STDIO (`main.rs:1158`) / UDS (`main.rs:859`) boot paths (cross-check with R-13).
3. Single-deployment = N=1 test: a one-registered-slug cloud deployment resolves through the same slug-keyed resolver with no special-case arm (RD-5).

**Coverage Requirement**: The cutover is bounded to the served-project HTTP model; the MCP seam is proven unbroken; no `Default` consumer is left dangling; the resolver has exactly one (slug-keyed) code path. Local preservation is covered by R-13.

### R-08: Reserved-slug drift under the new grammar
**Severity**: Med · **Likelihood**: Med · **Impact**: a registerable slug shadows a route segment (e.g., `observe`), enabling mis-routing at registration time.

**Test Scenarios**:
1. Registration-rejection table: attempt `register` against **every** reserved name (`v1`, `health`, `observe`, and any the new grammar introduces) → each rejected at the parse edge.
2. Grammar-coupling test: assert the reserved set is derived from / consistent with the actual route segments the new grammar uses (no segment routable AND registerable).
3. `tools` decision pin (OQ-3): a test that locks the chosen `tools` reservation state so a silent flip is caught.

**Coverage Requirement**: No registerable slug can shadow a live route segment; the reserved set is tested against the new grammar, not the old.

### R-09: Cross-pollination at N≥2 (the integrity hazard)
**Severity**: Critical · **Likelihood**: Low · **Impact**: B's request corrupts A's hash chain — unrollbackable.

**Test Scenarios**:
1. Two-store integration test (per #4974 step 5): register A and B; a write bound to B asserts A's store is untouched (A's write `is_err()`/absent from B and vice-versa) — for **both** MCP and observe.
2. Resolver-identity assertion: each request resolves to exactly the slug carried by the transport-derived key; tie resolve and dispatch to the same map (e.g., `wraps_store` debug-assert) so they cannot diverge.

**Coverage Requirement**: Isolation proven at N=2 for MCP and observe; the proof fails against any residual bypass (would pass only at N=1 — the #4974 trap).

### R-10: Loud-first-boot regression
**Severity**: High · **Likelihood**: Low · **Impact**: empty `[[projects]]` silently resolves a default store, re-introducing the silent-default hole.

**Test Scenarios**:
1. First-boot test: from empty state, assert no servable store exists and every request (MCP and observe) fails loud with the actionable "register a project to begin" substance.
2. Assert no adopt/derive/path-hash-migration code path runs on the served-project model (AC-09).

**Coverage Requirement**: Empty config = nothing servable + loud message; no silent default-store landing on any path.

### R-11: ~~#735 router/boot collision~~ — SUPERSEDED (fold-in)
**Severity**: Low (was Med) · **Likelihood**: Low (was High) · **Status**: SUPERSEDED.

#735 is folded into vnc-038 (SCOPE "Folded-in carry-items"; Dependencies "#735 — folded into vnc-038"; OQ-4 RESOLVED). There is no longer a separate effort editing the same `router.rs`/`main.rs` surface, so the sequencing/merge-collision risk is dissolved — not mitigated by coordination, but removed by construction. The three carry-items now land **inside** this feature's diff and are covered by **R-14** (CI-1 token-to-stdout → ADR-008/AC-11) and **R-15** (CI-2 router.rs ≤500 + CI-3 public_url.rs cleanup, AC-12/13). No coordination gate remains.

**Coverage Requirement**: none beyond the carry-item risks (R-14, R-15). Retained as a no-op for traceability so the prior SR-06→R-11 mapping resolves cleanly.

### R-12: Init-Ping vs runtime-hook observe asymmetry
**Severity**: High · **Likelihood**: Med · **Impact**: only one observe entry point routed; #766's wider blast radius half-open.

**Test Scenarios**:
1. AC-07 #766 repro: `init --bundle <v:2>` → init-time Ping posts to the bundle's `observe_url` over the real `/v1/{slug}/observe` route → **200** (was 404).
2. AC-08 hook transport: a runtime hook event posts to the same per-slug observe route → **200**, and resolves to the bundle's project store.
3. Assert BOTH paths use the bundle's `observe_url` verbatim — neither re-derives the route.

**Coverage Requirement**: Both observe entry points (init Ping AND every runtime hook) are proven reachable and correctly routed per-slug — neither is left to a separate, untested path.

### R-13: Local STDIO/UDS routed through the unified resolver (ADR-006 tightening / C-13 guard)
**Severity**: High · **Likelihood**: Med · **Impact**: delivery wires local onto the HTTP resolver instead of preserving its direct path-hash binding → AC-10 regression; a local store reachable via slug-keyed lookup is a NEW cross-store code path that cannot be exercised by local users today and risks mis-binding the one local store. This is the concrete GATE-2 confirmation guard.

**Test Scenarios** (the explicit C-13 guard):
1. **Direct-binding assertion (the load-bearing guard).** Assert local STDIO (`main.rs:1158`) and local UDS (`main.rs:859`) still open `~/.unimatrix/{hash}/unimatrix.db` **directly** at boot and thread the `Arc<Store>` straight to their handlers — with **no slug supplied**, behavior unchanged from ADR-004.
2. **Resolver-bypass assertion.** Assert the local boot paths never invoke `parse_project_key`, never construct the HTTP resolver (`DefaultResolver`/`MultiProjectRouter`), never reference `ProjectKey::Default`, and never touch a bundle. A structure/grep guard that FAILS if a future edit threads local through the resolver or adds a local resolver-map key.
3. **No-resolver-key assertion (ADR-006 tightening).** Assert local is NOT self-registered as a resolver key; the unified resolver's key space is `ProjectKey::Slug` only — there is no derived path-hash key in the slug map.
4. **HTTP-only-deletion cross-check (with R-07).** Assert the ADR-004 deletions (`DefaultResolver`, `/v1/tools→Default`, `_ => Default`) are confined to HTTP code and do not reach the local STDIO/UDS boot paths.

**Coverage Requirement**: Local is provably the pre-existing direct-binding path, untouched by the resolver — the "local unaffected" guarantee is structural (no migration, no operator action), proven by a guard that fails the instant local is routed through the resolver or made a resolver key.

### R-14: First-boot bearer token leaks to stdout/logs (ADR-008 / AC-11 / NFR-06)
**Severity**: Med · **Likelihood**: Med · **Impact**: the token reaches aggregated/persisted container logs → credential-exposure surface under the cloud HTTPS posture; the bundle is supposed to be the SOLE token channel. Failure mode = token substring present in first-boot stdout or captured `tracing` output.

**Test Scenarios**:
1. **First-boot token-surface test (the concrete verification).** On the HTTP/cloud first-boot path, capture stdout AND `tracing` log output and assert **no token substring** appears anywhere (`http/token.rs:101` redacted/gated).
2. **Sole-channel assertion.** Assert the emitted `v:2` bundle carries the token (the token IS delivered, just only via the bundle); assert no parallel "also print it" path survives.
3. **Local-surface non-regression (reconcile with ADR-006).** Assert the redaction is scoped to the HTTP/cloud first-boot context and does NOT remove/alter the local STDIO/UDS token affordance — if `token.rs:101` is shared, it is gated by deployment context, not unconditionally removed (a naive removal could regress local; cross-check with R-13/AC-10).

**Coverage Requirement**: The first-boot token never reaches stdout/logs on the cloud surface; the bundle is the sole channel; local token handling is functionally unchanged.

### R-15: #735 cleanup verification (low-weight)
**Severity**: Low · **Likelihood**: Low · **Impact**: a folded-in cleanup is missed — `router.rs` exceeds the 500-line guideline after the rewrite (AC-12), or stale `dead_code` cruft survives (AC-13). Mechanical; no behavioral blast radius.

**Test Scenarios**:
1. Line-count check: assert `crates/unimatrix-server/src/http/router.rs` is ≤500 lines post-rewrite (AC-12 / NFR-09). The extraction falls out of the route-grammar rewrite, not a separate effort.
2. Absence check: assert `crates/unimatrix-server/src/http/public_url.rs` retains no `#![allow(dead_code)]` and no "until wiring lands" comment (AC-13).

**Coverage Requirement**: Both mechanical items verified once. Note but do not over-weight — these are verification items, not architecture risks; they carry no isolation or integrity stake.

## Integration Risks

- **The two client→server arrows must carry server-authored URLs unmutated** (ADR-001). The highest-value integration test asserts the outgoing MCP and observe request URLs equal the bundle fields byte-for-byte (R-01).
- **Encode/decode boundary is the parity corpus** — it is the shared oracle. Any `v:2` field change must move Rust encoder + JS decoder + corpus in one diff (R-03); a single-side change is an integration break by construction.
- **`register` → boot read** is an async, restart-mediated integration: the write (`projects.rs`) and the read (`main.rs:1004`) are decoupled by a restart. Test the full write→restart→resolve loop, not just the write (R-06).
- **Observe folded onto the MCP funnel** is the riskiest interaction: it shares `resolve_store(parse_project_key(path))` with MCP but enters from a different handler. Prove both enter the one funnel (R-02, R-09).
- **Local STDIO/UDS is NOT an integration seam with the resolver** (ADR-006 tightening): local opens its path-hash store directly at boot and threads the `Arc<Store>` to its handlers, bypassing the resolver entirely (it is NOT a resolver key). The two store-binding mechanisms (HTTP slug-keyed resolver vs local direct binding) are independent boot-time wiring paths that do NOT share a map. The risk is delivery accidentally merging them — guarded explicitly by R-13. The diff must never reach the local boot paths (`main.rs:1158`/`:859`).
- **Token delivery is single-channel by construction** (ADR-008): the `v:2` bundle is the sole token channel for the cloud surface; there is no stdout/log fallback. The integration risk is a shared `token.rs:101` print site reachable on both surfaces — the redaction must be deployment-context-gated so local is unaffected (R-14 ∩ R-13).

## Edge Cases

- Empty `[[projects]]` at boot (R-10).
- Re-register an already-registered slug (R-05); register a reserved name (R-08); register into a non-empty config (R-06).
- `v:1` bundle presented to `v:2`-only client/server (R-04).
- Bundle at exactly `MAX_RAW_LEN` and one byte over (guard ordering, R-03).
- Malformed / non-`https://` URL in the bundle (R-03 strict-reject).
- N=2 with two slugs whose names are prefix-related (e.g., `proj`, `project`) — assert no path-prefix mis-resolution.
- Slug at the `^[a-z0-9][a-z0-9-]{0,62}$` boundary (max length, leading/trailing hyphen rejection).
- Observe POST to an unregistered slug → loud `UnknownProject`, not a default.
- Local STDIO/UDS boot with NO `[[projects]]` and NO slug → resolves its path-hash store directly, NOT a loud-first-boot failure (the loud-first-boot rule is cloud-only; local must not be caught by AC-09's empty-config failure) (R-13).
- `token.rs:101` print site shared between cloud first-boot and a local path → redaction gated by deployment context, not unconditionally removed (R-14 ∩ R-13).

## Security Risks

Untrusted external input enters at three surfaces; assess each:

- **Bundle decode (JS client trust boundary).** Untrusted input = the pasted bundle string. Damage from malformed input = decode crash, oversized-input DoS, or accepting a bundle that targets an attacker-chosen URL. Blast radius = the client posts telemetry/MCP to a hostile endpoint. Mitigations to test: `MAX_RAW_LEN` cap FIRST (DoS), strict exact-key + version pin (no field smuggling), `https://`-only URL validation (no downgrade/SSRF to attacker host), no partial accept. The bundle is base64url(JSON) — assert JSON parsing is bounded and the decoder stays zero-dependency (NFR-08).
- **Slug at the registration / route-parse edge.** Untrusted input = operator-supplied slug AND the slug segment in an inbound request path. Damage = path traversal into another project's data dir (`{base}/{slug}/`), or shadowing a route segment. Blast radius = cross-project store access / corruption. Mitigations to test: `ProjectSlug` regex validated **before any filesystem use** (no `..`, `/`, no reserved name); reject reserved slugs (R-08); the resolver only ever maps a validated, registered slug (R-09).
- **`config.toml` write (register).** Untrusted input = the slug written into TOML. Damage = TOML injection (newline/quote breaking the `[[projects]]` stanza) corrupting routing for all projects. Blast radius = daemon boot failure or mis-routing. Mitigations to test: the slug is regex-constrained pre-write (no TOML metacharacters survive the `ProjectSlug` newtype); atomic write (R-06).
- **First-boot token emission (credential-exposure surface, ADR-008/AC-11/NFR-06).** Untrusted observer = anyone with access to aggregated/persisted container logs. Damage = bearer-token capture → full access to the served projects. Blast radius = every project on the deployment. Mitigation to test: `http/token.rs:101` redacted/gated so the token never reaches stdout or `tracing` output; the `v:2` bundle is the SOLE delivery channel (R-14). The redaction must be deployment-context-scoped so it does not regress the local STDIO/UDS token path (R-13).
- **No new attack surface** (NFR-03/04): assert no new unauthenticated endpoint, no slug-listing surface, no secrets in any DB. Token/cert stay as files.

## Failure Modes

| Failure | Expected behavior |
|---------|-------------------|
| Empty `[[projects]]` at boot | Nothing servable; loud actionable "register a project to begin"; no silent default (R-10). |
| Request to unregistered slug | `RouteError::UnknownProject` — loud, never a default store. |
| `v:1` bundle at a `v:2`-only client | `BundleError` with a re-issue message; fail closed (R-04). |
| Malformed bundle (length/scheme/base64/JSON/schema) | `BundleError`, hard reject, length cap first; no partial accept. |
| Interrupted `register` write | Atomic temp+rename → old OR new complete file, never partial (R-06). |
| Re-register existing slug | Re-attach (open), never genesis-clobber; chain head unchanged (R-05). |
| Reserved name registration | Rejected at the parse edge (R-08). |
| Observe to a wrong/unregistered slug | Loud `UnknownProject`; never resolves another project's store (R-09). |
| First boot on the cloud surface | Token NOT printed to stdout/logs; delivered solely via the `v:2` bundle; redaction gated so local is unaffected (R-14). |
| Local STDIO/UDS boot | Opens `~/.unimatrix/{hash}/unimatrix.db` directly; never enters the resolver, never takes a slug, no bundle — unchanged from ADR-004 (R-13). |

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (dumb-client / server-composed bundle bet) | R-01, R-12 | ADR-001 makes route grammar server-only and the closed set explicit; R-01 asserts the closed set is empty + verbatim-post; R-12 proves both observe entry points route. |
| SR-02 (`v:2` parity break) | R-03, R-04 | ADR-002 treats `v:2` as one atomic dual-side + corpus change; R-03 round-trip + strict-reject matrix on both sides; R-04 covers the `v:1`-holder hard-cut. |
| SR-03 (ceremonial-funnel on observe) | R-02, R-09 | ADR-003 makes the resolved per-request handle the SOLE observe route (no boot-bound/parallel path); R-02/R-09 prove isolation at N=2 per #4974. |
| SR-04 (hard-cutover blast radius) | R-07, R-10, **R-13** | ADR-004 bounds the cutover to the served-project HTTP model; **ADR-006 TIGHTENED — local keeps its DIRECT path-hash binding and bypasses the resolver (NOT a resolver key)**, so the SR-04 local-UDS blast-radius concern is now guarded by the **R-13 local-direct-binding guard test** (the GATE-2 confirmation), not by reconciling local "under the resolver." R-07 proves the MCP seam unbroken; R-10 proves loud-first-boot (cloud-only). |
| SR-05 (reserved-slug coupling) | R-08 | ADR-005 re-derives the reserved set from the new grammar; R-08 tests registration against every reserved name + the `tools` decision (OQ-3). |
| SR-06 (#735 collision) | — (resolved-by-fold-in) | **RESOLVED by folding #735 into vnc-038** (SCOPE Dependencies; OQ-4 resolved). No longer a sequencing/coordination risk — there is no separate effort on the same `router.rs`/`main.rs` surface. The three carry-items land inside this diff and are covered by **R-14** (token-to-stdout → ADR-008/AC-11) and **R-15** (router.rs ≤500 / public_url.rs cleanup, AC-12/13). R-11 is superseded. |
| SR-07 (register→restart routing-intent write) | R-05, R-06 | ADR-007 specs atomic config write + re-attach-safe register; R-05 (no genesis-clobber) + R-06 (atomic, additive write) cover both halves. |
| (#735 CI-1, no original SR — folded in) | R-14 | ADR-008: first-boot token delivered ONLY via the `v:2` bundle, never stdout/logs (AC-11/NFR-06). R-14 asserts no token substring on stdout/logs + sole-channel + local non-regression. |
| (#735 CI-2/CI-3, no original SR — folded in) | R-15 | Mechanical cleanups falling out of the router rewrite: `router.rs` ≤500 lines (AC-12) and `public_url.rs` stale `dead_code` removed (AC-13). Low-weight verification. |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 8 (R-01, R-02, R-03, R-04, R-07, R-09, R-12, **R-13**) | ~22 scenarios — closed-set/verbatim-post, funnel-at-N=2, parity round-trip + reject matrix, `v:1` hard-cut, MCP-seam preservation, two-store isolation, observe dual-entry, **local-direct-binding/resolver-bypass guard (C-13)** |
| High | 5 (R-05, R-06, R-08, R-10, **R-14**) | ~13 scenarios — re-attach/no-clobber, atomic config write, reserved-set table, loud-first-boot, **first-boot token-surface (sole-channel + local non-regression)** |
| Medium | 0 | — |
| Low | 2 (**R-11** superseded, **R-15** mechanical) | 2 mechanical checks — `router.rs` ≤500 lines, `public_url.rs` stale-`dead_code` removed (AC-12/13) |

> Note: R-12, R-09, and R-13 listed under Critical alongside the others in the register; counts reflect 8 Critical-priority risks. N=2 proof (C-11) is mandatory for R-02 and R-09 — an N=1 green result is NOT accepted as proof (#4974). **R-13 is the load-bearing GATE-2 guard: it fails the instant delivery routes local through the unified resolver or makes local a resolver key (ADR-006 tightening / C-13).** R-11 is retained as superseded for SR-06 traceability only.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_search / context_get — surfaced #4974 (ceremonial-seam: prove the funnel at N=2, not N=1; VERIFY-THE-FUNNEL checklist), #4452 (gate-fix tests must exercise the previously-broken path, not a vacuous privileged pass), #4311 (silent-fallthrough normalization → gate-prerequisite test ordering), #2398 (API-extension call-site audit before removing/changing a widely-used signature — applied to `ProjectKey::Default` removal), and the #4956 parity-corpus mechanics (hex vectors, re-exported oracle fns). All applied to R-01/R-02/R-03/R-07/R-09.
- Queried (revision pass, vnc-038-agent-3-risk): re-read the UPDATED ADR-006 (#5085, tightened — local keeps DIRECT path-hash binding, NOT a resolver key), the NEW ADR-008 (token-via-bundle-only), and the updated SPECIFICATION (FR-15/16/17, AC-11/12/13, C-13). Applied to the new R-13 (local-direct-binding guard / GATE-2), R-14 (token-to-stdout), R-15 (mechanical #735 cleanups); re-scoped R-07 to the MCP seam; superseded R-11; resolved SR-06 by fold-in and re-pointed SR-04 to R-13.
- Stored: nothing novel to store — the governing patterns (ceremonial-funnel at N=2, parity-corpus atomicity, call-site audit, redact-secrets-from-logs) already exist; this revision applies them. The "tightened ADR re-pointed a guard from under-the-resolver to bypass-the-resolver" mechanic is a single-feature reconciliation, not yet a cross-feature (2+) pattern worth generalizing.
