# Gate 3a — Component Design Review — vnc-038

**Feature:** vnc-038 — Mandatory Project Identity at the Deployment Entrypoint
**Gate:** 3a (Component Design Review)
**Verdict:** **PASS**
**Reviewer:** vnc-038-gate-3a
**Date:** 2026-06-17

Artifacts validated: `pseudocode/` (OVERVIEW + 12 component files), `test-plan/` (OVERVIEW + 12 component files).
Source documents: ARCHITECTURE.md, SPECIFICATION.md, RISK-TEST-STRATEGY.md, ADR-001..008 (#5080–5088).

---

## Summary

All 12 pseudocode components align with the approved architecture and implement the specification (FR-01..FR-17, NFR, AC-01..AC-13). All 12 test plans address the risk strategy (R-01..R-15), including the mandatory N=2 isolation proof for R-02/R-09 (#4974 ceremonial-funnel precedent). Component interfaces are consistent with the architecture contracts. Every gate-flagged design position is honored in both pseudocode and test plans. Two design claims that the artifacts hinge on were verified against live code and confirmed.

No SCOPE-FAIL, no REWORKABLE-FAIL issues. Findings below are advisory observations only.

---

## Validation Against Gate Criteria

### 1. Component ↔ Architecture alignment (ADRs #5080–5088) — PASS

| Component | ADR | Alignment |
|-----------|-----|-----------|
| 1 Bundle codec (Rust) | ADR-002/008 | `Bundle{v,mcp_url,observe_url,token,fp}`, `BUNDLE_VERSION=2`, `validate_schema` exact-5-keys + https + token/fp grammar. Matches Integration Surface "Planned v:2" exactly. |
| 2 Bundle decoder (JS) | ADR-002/001 | `EXPECTED_KEYS` updated to 5 keys; `obj.v !== 2`; https validation both URL fields; guard ordering length→scheme→base64url→JSON→strict-schema preserved. Matches surface. |
| 3 Client attach (JS) | ADR-001 | C-1 (`init.js:305` slug-append) + C-2 (`:307` default-append) deleted; stores both URLs verbatim; `--slug` retired on bundle path. |
| 4 Hook transport (JS) | ADR-001 | C-3 (`transport-http.js:84` `/observe` append) deleted; posts `observe_url` verbatim. |
| 5 Route grammar + resolver | ADR-004 | `ProjectKey` collapses to `Slug`-only; `parse_project_key` one rule, loud otherwise; `MultiProjectRouter` drops `default` field + params + `Default` arms; `DefaultResolver` deleted. |
| 6 Observe route + handler | ADR-003 | top-level `/observe` removed; `/v1/{slug}/observe` per-request via the same `resolve_store` funnel; `ObserveContext` holds `Arc<dyn StoreResolver>`, no pre-resolved store. |
| 7 Boot wiring | ADR-003/004/006 | unified resolver from `project_slugs` only; empty ⇒ nothing servable + loud; boot-bound `resolve_store(Default)` deleted; local paths untouched. |
| 8 register CLI | ADR-007 | atomic temp+fsync+rename `[[projects]]` write; State A/B/C preserved; re-attach never genesis; store-before-stanza ordering. |
| 9 Reserved slugs | ADR-005 | value `["v1","health","observe","tools"]` retained, derivation re-documented per literal; stale `validate_slug` "default-project alias" message flagged for update. |
| 10 First-boot token | ADR-008 | assert-and-guard (see verified claim below). |
| 11 Local binding guard | ADR-006 | negative/guard-only component; no production change; structural assertions. |
| 12 Wave-1 cleanups | — | `router.rs` ≤500 as outcome of Component 6 extraction; `public_url.rs:19` dead_code allow removed. |

No invented interface names — every signature traces to a cited `file:line` in the Integration Surface or to an ADR.

### 2. Pseudocode ↔ Specification (FR / AC) — PASS

- FR-01/AC-09 (loud first boot): boot-wiring Component 7.A — empty `[[projects]]` ⇒ empty-map resolver, every `/v1/...` → 404, actionable "register a project to begin". Gated to cloud/HTTP only.
- FR-02..05/AC-02/03/04 (uniform atomic register): register-cli Component 8 — single command, atomic write, idempotent, re-attach-safe, distroless std::fs only.
- FR-06/07/08/AC-05 (server-composed URLs; strict dual-side; client verbatim): Components 1+2+3+4 — `compose_route_urls` sole owner; both sides strict-validate; all 3 compose sites deleted; verbatim store + post.
- FR-09/10/AC-06 (per-slug observe on the funnel; single-store resolution): Component 6 — sole route, per-request resolve, N=2.
- FR-11/12/AC-07/08 (init-Ping + runtime-hook 200 over the real route): Components 3+4+6 — both target bundle `observe_url` verbatim.
- FR-13 (reserved-slug re-derivation): Component 9.
- FR-14/AC-10 (local unchanged, direct path-hash binding): Components 7.C + 11.
- FR-15/AC-11/NFR-06 (token not to stdout/logs): Component 10.
- FR-16/AC-12 (`router.rs` ≤500), FR-17/AC-13 (`public_url.rs` dead_code): Component 12.

NFR coverage is explicit (NFR-01 empty-compose-set invariant; NFR-02/07 corpus reuse; NFR-08 guard ordering + zero-dep; NFR-09 Rust hygiene incl. `.map_err` context, no `.unwrap()` in non-tooling).

### 3. Test plans ↔ Risk strategy (R-01..R-15; N=2 for R-02/R-09) — PASS

- R-01 (missed compose site): empty-set grep/AST invariant + byte-for-byte verbatim-post asserted in client-attach-js, hook-transport-js, bundle-decoder-js.
- R-02 (ceremonial observe funnel): observe-route — structure guard (no pre-resolved store, no boot-bound `resolve_store(Default)`, no discarded `let _store`) **+ N=2 counting-resolver**, plus infra-001 gap #3. **N=2, not N=1.**
- R-03/R-04 (parity / v:1 hard-cut): bundle-codec-rust + bundle-decoder-js — round-trip via regenerated `bundle-golden.json`, strict-reject matrix mirrored both sides, guard ordering, v:1 actionable reject with no fallback arm either side.
- R-05/R-06 (genesis-clobber / atomic write): register-cli — chain-head hash before==after, idempotent single stanza, atomicity + preservation + TOML-injection guard.
- R-07 (delete-default over-reach): route-grammar-resolver — call-site audit, seam preserved, resolver Slug-only.
- R-08 (reserved-slug drift): reserved-slugs — rejection table, grammar-coupling, `tools` lock.
- R-09 (cross-pollination): route-grammar-resolver + observe-route — **N=2** B-write-never-reaches-A for MCP AND observe, resolve==dispatch same map, prefix-collision edge (`proj`/`project`).
- R-10 (loud-first-boot): boot-wiring + route-grammar-resolver.
- R-12 (init-Ping vs hook asymmetry): observe-route + client-attach-js + hook-transport-js — both verbatim `observe_url`, both 200.
- R-13 (local routed through resolver): local-binding-guard — G1 direct-binding, G2 resolver-bypass grep guard, G3 no-resolver-key, G4 HTTP-only-deletion cross-check. The load-bearing GATE-2 guard.
- R-14 (token leak): token-redaction — no token substring in stdout/`tracing`, sole-channel, deployment-gated local non-regression.
- R-15 (#735 cleanup): wave1-cleanups — line-count + grep absence checks.

Cumulative-infra rule honored: parity corpus EXTENDS `tests/bundle_codec.rs` + `tests/fixtures/c1c2-parity/bundle-golden.json` (regenerated from the Rust oracle, never hand-written JS); N=2 routing reuses `tests/project_routing_integration.rs`; the three Default-arm tests are INVERTED, not deleted.

### 4. Interface consistency with architecture contracts — PASS

- **Rust↔JS v:2 bundle contract:** OVERVIEW pins canonical key order `{v,mcp_url,observe_url,token,fp}` (serde declaration order), exact-5-key set, https URLs, locked guard ordering, and the one-diff atomicity rule. Components 1 and 2 implement it identically; both test plans assert byte-equality over the shared hex corpus.
- **Unified resolver:** single `resolve_store(parse_project_key(path))` funnel, `ProjectKey::Slug` only, read by both `SlugRouter` (MCP) and the observe handler (one map, `wraps_store` debug-assert preserved).
- **Per-slug observe on the funnel:** observe enters from a distinct handler but the SAME funnel; sole route; N=2 isolation proof.
- **settings.local.json migration:** `unimatrix.remote` moves `{url}` → `{mcp_url, observe_url}` consistently across OVERVIEW, Component 3 (writer), Component 4 (reader).

---

## Gate-Flagged Design Positions — All Honored

| Position | Result | Evidence |
|----------|--------|----------|
| Dumb-client invariant: all client compose sites designed out; verbatim post asserted (AC-05) | ✓ | C-1/C-2/C-3 deleted in Components 3/4; empty-set grep/AST invariant + byte-for-byte post in the three JS test plans. |
| Per-slug observe + MCP isolation at N=2 not N=1 (AC-06); the three Default-arm tests INVERTED not deleted (#4452) | ✓ | observe-route + route-grammar-resolver require N=2 counting-resolver; test-plan OVERVIEW + route-grammar-resolver name all three tests to invert to loud-error. **Verified the three tests exist** at `project_routing_integration.rs:384/412/648`. |
| Local STDIO/UDS keeps DIRECT path-hash binding, NOT routed through the resolver (AC-10/C-13); AC-09 loud-first-boot gated to cloud/HTTP so local boot isn't caught | ✓ | Components 7.C + 11 (guard-only); boot-wiring + local-binding-guard edge cases explicitly exempt local from the empty-config failure. |
| v:2 bundle parity EXTENDS the existing corpus, not new scaffolding | ✓ | bundle-codec-rust/-decoder-js regenerate `bundle-golden.json` from the Rust oracle; re-export `pub(crate)` oracle fns per #4956. |
| CI-1 token: live code already redacted — Component 10 is an assertion/guard, not a removal | ✓ | **Verified:** `token.rs` `render_first_boot_notice()` is a token-free stderr notice (doc: "MUST NOT contain the token hex, the token-file path, or any secret"); `load_existing_token` logs path only. Component 10 specifies assert+guard, no removal. |

---

## Advisory Observations (non-blocking)

1. **Stale ADR Unimatrix IDs in the architect report.** `agents/vnc-038-agent-1-architect.md` lists ADR-006 = #5085 and stops at ADR-007; the gate-authoritative mapping (and the pseudocode/risk artifacts) use ADR-006 = #5087, ADR-007 = #5086, ADR-008 = #5088. The pseudocode and test plans are internally consistent with the gate-authoritative IDs and the ADR file titles all match. This is a knowledge-store labeling drift in an upstream report, not a design-artifact defect. Recommend the architect reconcile the Unimatrix entries at retro.

2. **OQ-A (MCP consumer of `mcp_url`).** Component 3 stores `mcp_url` verbatim regardless of whether the Node client or Claude Code's MCP layer issues MCP requests. Dumb-client holds either way. Correctly flagged as an impl-time confirmation, not a gap. No action for delivery beyond confirming the consumer.

3. **Empty-projects resolver choice (Component 7.A).** The pseudocode recommends building an empty-slug-map `MultiProjectRouter` (so `/health` works and every served request fails loud through the same funnel) over a no-resolver branch. Sound — it avoids a special no-projects code path. Delivery should adopt the recommended choice; either satisfies AC-09.

4. **Legacy `{remote,token}` init branch (Component 3).** Kept working with a single local observe derivation on the legacy branch only; the closed-set invariant test guards the BUNDLE path. Correctly scoped — legacy is not the #766 surface. Flagged for optional one-line retirement, not assumed.

---

## Decision

**PASS.** Pseudocode and test plans are complete, traceable, and faithful to the architecture, specification, and risk strategy. Both code-cited design claims (token already redacted; the three Default-arm tests exist to invert) are verified. No TODO/placeholder/stub content; all gaps are explicitly flagged as open questions with impl-time resolutions. Cleared for Stage 3b.

**Report path:** `product/features/vnc-038/reports/gate-3a-report.md`
**Issues:** none blocking; 4 advisory observations above.
