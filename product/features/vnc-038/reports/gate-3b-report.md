# Gate 3b Report: vnc-038

> Gate: 3b (Code Review)
> Date: 2026-06-17
> Result: **PASS**
> Reviewer: vnc-038-gate-3b
> Commit reviewed: 2f79bfcd (waves f31273e4 / 9e6111da / 476df92f / 2f79bfcd) vs design-artifacts ce30aeda

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Pseudocode fidelity | PASS | All 12 components match their Stage-3a pseudocode; no undocumented departures. |
| 2. Architecture compliance | PASS | ADR-001..008 honored — dumb-client verbatim, v:2 bundle, slug-only resolver (Default deleted), per-slug observe on the funnel, atomic [[projects]] write, token-via-bundle-only, local direct binding untouched. |
| 3. Interface implementation | PASS | v:2 bundle byte-equal Rust↔JS over regenerated hex corpus; `ProjectKey::Slug`-only resolver; observe resolves per-request via `observe_ctx.resolver`; both JS sites post server-composed URLs verbatim. |
| 4. Test case alignment | PASS | R-01..R-15 covered at unit/structural level; the three Default-arm integration tests left for planned Stage-3c inversion (documented). |
| 5. Code quality | PASS | Builds green; no stubs/TODO/FIXME; no non-test `.unwrap()`; zero clippy warnings in touched files; router.rs 422 ≤500. |
| 6. Security | WARN | https-only URL validation, slug allowlist before FS use, TOML-injection-proof newtype, token redacted, MAX_RAW_LEN-first. `cargo audit` = 1 pre-existing transitive CVE (rsa via sqlx-mysql) + bincode unmaintained — neither introduced by vnc-038. |
| 7. Stewardship compliance | PASS | All four implementation agent reports carry `## Knowledge Stewardship` with Queried + Stored entries (or documented reason). |

**Verdict: PASS.** No blocking issues. Findings under checks 6 and the advisory section are pre-existing debt or planned-deferred Stage-3c work, not vnc-038 code defects.

## Detailed Findings

### Check 1 — Pseudocode fidelity
**Status:** PASS
All 12 component pseudocode files map to implemented code. Bundle codec (`client_bundle.rs`) implements the planned v:2 `Bundle{v,mcp_url,observe_url,token,fp}`, `BUNDLE_VERSION=2`, `compose_route_urls` as the sole encode-path grammar owner mirroring `parse_project_key`, and `decode_bundle` mirroring the JS guard ordering (length→scheme→base64url→JSON→strict schema). Route grammar (`seam.rs`) collapses `ProjectKey` to `Slug`-only with the documented single rule and loud errors. The `let _store` discard and parallel fixed-adapter dispatch are gone; `wraps_store` debug-assert preserved. No departures from pseudocode found.

### Check 2 — Architecture compliance (ADR-001..008)
**Status:** PASS
- **ADR-001 (dumb-client):** init.js stores `mcp_url`/`observe_url` verbatim ("NO endpointBase, NO /v1 append, NO slug branch", init.js:334); transport-http posts `observe_url` byte-for-byte (no `/observe` append).
- **ADR-002/008 (v:2 + token channel):** encoder composes both URLs; bundle stdout is blob-only, stderr omits token; golden fixture regenerated to v:2.
- **ADR-003 (per-slug observe on funnel):** `ObserveContext` holds `Arc<dyn StoreResolver>` (router.rs:69), `route_observe` resolves per-request (handlers.rs:66) via the same `parse_project_key` grammar; top-level `/observe` removed (router.rs:193 suffix-dispatch). C6-folded-into-C7 observe coverage confirmed present despite no separate C6 commit.
- **ADR-004 (delete Default):** `default_resolver.rs` removed (121 lines); `MultiProjectRouter` drops `default` field + default params; `resolve_store`/`adapter_for` are total over the single-variant `Slug` enum, returning `UnknownProject` for unregistered slugs.
- **ADR-005 (reserved slugs):** `RESERVED_SLUGS = ["v1","health","observe","tools"]` value retained, rationale re-documented; stale "default alias" message updated.
- **ADR-006 (local direct binding):** local UDS (main.rs ~:859) and STDIO (~:1158) open the path-hash store directly and thread `Arc<Store>` to handlers; never call `parse_project_key`, never construct the HTTP resolver, never a resolver key (AC-10 preserved).
- **ADR-007 (register writes [[projects]]):** atomic temp+fsync+rename, idempotent, additive; State A loud error, State B re-attach (no genesis clobber), store-first ordering; `ProjectSlug` newtype blocks TOML injection.

### Check 3 — Interface implementation
**Status:** PASS
v:2 bundle contract is byte-equal across the Rust encoder and JS decoder over the regenerated hex corpus (`bundle-golden.json`). Strict schema: exactly 5 keys, `v==2`, both URLs `https://`-only, token/fp grammar-validated, no token in error messages. settings.local.json `unimatrix.remote` moves `{url}` → `{mcp_url, observe_url}` consistently between writer (init.js) and reader (transport-http.js). Resolver and observe handler read the same per-entry map; both enter the one `resolve_store(parse_project_key(path))` funnel.

### Check 4 — Test case alignment
**Status:** PASS (with planned Stage-3c carry)
- Server lib: 4212 passed / 0 failed / 1 ignored.
- JS: 120 passed / 0 failed across 34 suites (bundle decoder, transport, init-remote, remote-client).
- Unit/structural coverage present for R-01..R-15 (closed-set invariant, guard ordering, strict-reject matrix, v:1 hard-cut reject, atomic write, re-attach no-clobber, reserved-set, loud-first-boot, local-binding guard, token redaction, line-count/dead_code cleanups).
- **Planned-deferred (NOT a 3b defect):** `tests/project_routing_integration.rs` and `tests/client_bundle_e2e.rs` are intentionally red — they reference removed `ProjectKey::Default` / `Bundle.base_url`. Confirmed the compile errors are exactly those (no field `base_url`, arg-count mismatch). Per the test plan + lesson #4452, Stage 3c inverts the three Default-arm tests to loud-error assertions and updates the e2e bundle test, landing the N=2 cross-store isolation proof green (R-02/R-09).

### Check 5 — Code quality
**Status:** PASS
- `cargo build -p unimatrix-server`: green.
- No `todo!()`/`unimplemented!()`/`TODO`/`FIXME` in added non-test code.
- No `.unwrap()` in non-test code (the two textual hits are doc comments asserting absence). Error paths use `RouteError`→`json_error_response` and `.map_err`/`?` with context.
- Zero clippy warnings in vnc-038-touched files (236 crate-wide warnings are all pre-existing elsewhere).
- File sizes: router.rs 422 (≤500, AC-12 met); client_bundle.rs 423; seam.rs 472; all touched files ≤500. `public_url.rs` retains no `#![allow(dead_code)]` / "until wiring lands" (AC-13 met).

### Check 6 — Security
**Status:** WARN (pre-existing dependency advisory only)
- Bundle decode trust boundary: `MAX_RAW_LEN` length cap runs FIRST; strict exact-5-key + `v==2` pin; `https://`-only validation on both URL fields (no downgrade/SSRF to attacker host); token never echoed in error reasons; zero-dependency JS decoder.
- Slug edge: `ProjectSlug::TryFrom` enforces `^[a-z0-9][a-z0-9-]{0,62}$` BEFORE any filesystem use — `..`, `/`, `%2f`, uppercase, whitespace cannot pass the charset; path traversal structurally impossible.
- Config write: slug is the charset-constrained newtype, so no TOML metacharacter survives into the stanza; atomic temp+fsync+rename.
- Token: `render_first_boot_notice` is token-free and path-free; bundle is the sole channel.
- No new dependencies added by this feature (Cargo.toml/package.json diffs add none).
- `cargo audit`: **1 vulnerability** — RUSTSEC-2023-0071 (`rsa 0.9.10` via `sqlx-mysql`, medium 5.9, "no fixed upgrade available"), and `bincode 1.3.3` unmaintained. **Both are pre-existing transitive dependencies, not introduced by vnc-038.** Tracked as inherited debt; does not block this gate.

### Check 7 — Knowledge stewardship compliance
**Status:** PASS
The four implementation (rust-dev/js-dev) agent reports each carry a `## Knowledge Stewardship` block with `Queried:` and `Stored:` entries:
- `agent-3-boot-wiring`: Queried briefing/#5083/#5082/#5087/#5090; Stored #5093 (Default-deletion forces per-request observe resolver).
- `agent-3-bundle-codec-rust`: Queried (documents context tools unavailable in session, proceeded per non-blocking rule, read ADRs from files); Stored "nothing novel via MCP path" with a documented reason + a swarm rustfmt note.
- `agent-3-bundle-decoder-js`: Queried briefing/#5081/#4961; Stored #5092 (dual-side atomic coupling, corpus-gated parity).
- `agent-3-token-redaction`: Queried #5088/#4960; Stored #5089 (token-redaction regression guard).

## Advisory Observations (non-blocking)

1. **Legacy `{remote,token}` non-bundle init/observe branch retained.** init.js derives `observe_url` only on the legacy `--remote` branch ("the bundle branch composes none", init.js:361); the closed-set invariant guards the BUNDLE path. Correctly scoped — legacy is not the #766 surface. Optional one-line retirement, not required.

2. **Implementation agent-report coverage.** Four dedicated implementation reports exist (boot-wiring, bundle-codec-rust, bundle-decoder-js, token-redaction) for 12 components delivered across 4 waves; seam/resolver, client-attach-js, hook-transport-js, register-cli, reserved-slugs, local-binding-guard, and wave1-cleanups have no standalone report. The existing reports all comply with stewardship; the missing standalone reports are a process/coverage observation, not a stewardship violation in any present report. Recommend the coordinator confirm those surfaces' stewardship was folded (or note at retro).

3. **Inherited debt:** `main.rs` (1939) and `infra/config.rs` (12074) exceed the 500-line guideline but were already that large before vnc-038 (1941 / 12052) — not introduced here. The 236 crate-wide clippy warnings and the `rsa`/`bincode` advisories are likewise pre-existing.

4. **Stage 3c scope (carry-forward, not a 3b defect):** invert the three Default-arm tests in `project_routing_integration.rs` to loud-error assertions and update `client_bundle_e2e.rs` to the v:2 `Bundle` shape — landing the N=2 cross-store isolation proof (R-02/R-09, C-11) green. These targets are intentionally red at 3b.

## Decision

**PASS.** The implementation faithfully realizes the validated pseudocode and the approved architecture/ADRs. Interfaces are implemented as specified (v:2 dual-side bundle, slug-only unified resolver, per-slug observe on the funnel, dumb-client verbatim post, register writes [[projects]], local direct binding preserved). Lib/bins compile; unit + JS suites green; no stubs, no non-test unwraps, no new vulnerabilities. The integration-test redness and the N=2 green proof are the documented, planned Stage-3c work. Cleared for Stage 3c.

**Report path:** `product/features/vnc-038/reports/gate-3b-report.md`
