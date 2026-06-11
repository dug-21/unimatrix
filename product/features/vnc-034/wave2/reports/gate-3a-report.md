# Gate 3a Report: vnc-034 Wave 2 (#727)

> Gate: 3a (Component Design Review)
> Date: 2026-06-11 (rework iteration 1: 2026-06-11)
> Result: PASS
> Scope: Wave 2 pseudocode + test plans (`wave2/`) vs shared design (ARCHITECTURE, ADR-003/004/005, SPECIFICATION, RISK-TEST-STRATEGY) and the locked delivery decisions D1–D6 + funnel record.

## Rework Iteration 1 Outcome (2026-06-11) — PASS

The single REWORKABLE FAIL blocker (Check 5, Knowledge Stewardship) is **CLOSED**.
- `product/features/vnc-034/wave2/agents/vnc-034-wave-2-agent-1-pseudocode-report.md` now exists and carries a complete `## Knowledge Stewardship` block: `Queried:` entries (`context_briefing` + `context_search` surfacing #4963, #4958, ADRs #4949/#4950/#4951, each with how it was applied) and a `Stored / Deviations:` line ("nothing novel stored" with a stated reason — extends the merged Wave-1 seam per ADR-003, reuses the merged D1 allowlist; infra-001 transport-mismatch pattern correctly handed to tester/leader). Reason present after "nothing novel" ⇒ no WARN.
- Re-check confirmed the four pseudocode artifacts (`OVERVIEW.md`, `project-router.md`, `project-registry-cli.md`, `projects-config.md`) were NOT modified during rework (mtimes 18:00–18:06 precede the back-filled report at 18:13; no diff to design content). All 11 previously-PASSING checks (D1–D6, funnel elimination, seam contract, integration plan, D2/D3 negatives) remain satisfied.

**Final result: PASS (12/12 checks; 0 warnings).** No design rework to artifacts was required; this was a back-fill of the missing stewardship report only. Carry-forward items to Stage 3b (below) are unchanged and remain advisory, not blockers.

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Architecture alignment (ADR-003/004/005, StoreResolver seam) | PASS | Drop-in resolver swap at one call site; no interface re-cut of `ProjectKey`/`ProjectSlug`/`RouteError`. `adapter_for` trait extension flagged and justified. |
| 2. Specification coverage (FR-C1..C7, FR-X1..X5) | PASS | Every Wave-2 FR mapped to a component; no unrequested scope (D2/D3 splits honored). |
| 3. Risk coverage (RISK-TEST-STRATEGY) | PASS | R-01/03/04/06/10/12/13 all mapped to named tests; SR-09/R-03 covered with discriminators + no-escape assertion. |
| 4. Interface consistency | PASS | Shared types defined once in OVERVIEW; `per_slug_data_dir`, `RESERVED_SLUGS`, `ProjectSlug` single-sourced; no contradictions across files. |
| 5. Knowledge stewardship compliance | PASS (rework 1) | Pseudocode agent report back-filled with a complete `## Knowledge Stewardship` block (Queried + Stored/"nothing novel"+reason). Both test-plan agent reports also compliant. |
| D1 — exact slug regex `^[a-z0-9][a-z0-9-]{0,62}$`, reuse merged `TryFrom` | PASS | Reuses merged `ProjectSlug::TryFrom` (verified seam.rs:71–104); discriminators present. |
| D4 — delete=de-register; `--purge` loud; re-attach preserves chain | PASS | Two-state register + re-attach test; OQ-CLI-7 raised on `Store::open` non-destructiveness. |
| D5 — reserved-slug refusal, separate from charset | PASS | `tools` shadowing proven charset-valid-yet-rejected; single `RESERVED_SLUGS`. |
| D6 — register idempotence two-state, not collapsed | PASS | Distinct-message test; State A/B/C branch explicit. |
| D3 — list-field only, no per-slug network health | PASS | Negative tests `test_no_per_slug_health_endpoint` + `test_list_exposes_no_network_health`. |
| D2 — no config-overlay surface | PASS | `test_no_per_project_config_overlay_merge` negative; `ProjectConfigEntry` is slug-only. |
| FUNNEL — discard path eliminated, `adapter_for` sole dispatch | PASS | `let _store` + `self.project_router.route_mcp` removal specified; two-store no-bypass integration test. |
| Integration plan (real two-store edge + infra-001 smoke) | PASS | `tests/project_routing_integration.rs` for `/v1/{slug}/`; infra-001 repurposed as backward-compat gate; smoke mandatory. |

**Original result: REWORKABLE FAIL** — the design was technically sound and honored every locked decision; the single blocker was a process/stewardship-compliance gap (Check 5). **Rework iteration 1 result: PASS** — the pseudocode agent report was back-filled with the required `## Knowledge Stewardship` block; no design rework to the artifacts was needed.

---

## Detailed Findings

### Check 1 — Architecture alignment
**Status**: PASS
**Evidence**:
- `project-router.md` keeps the seam contract: `ProjectRouter` is a drop-in `StoreResolver` injected at the single `SlugRouter::new`/main.rs ~L898 call site; `ProjectKey`, `ProjectSlug`, `RouteError`, `parse_project_key`, and `SlugRouter` are explicitly untouched — matches ADR-003 "one trait-impl swap, no interface re-cut" and the merged seam.rs.
- `resolve_store` no-fallthrough (`Slug(unknown) → UnknownProject`, never default, never another slug) mirrors `default_resolver.rs:68–73` exactly — ADR-003 invariant + R-01 sc.3.
- The `adapter_for` second trait method is a genuine seam extension, not present in the merged `StoreResolver` (verified seam.rs:112–117 has only `resolve_store`). The pseudocode flags this honestly (OQ-PR-1/3/8/9), justifies it against ADR-003 "per-slug routing lives INSIDE the seam method, not a new edge," and forbids a default impl so resolution and dispatch read the same map. This is a sound, in-bounds elaboration of the seam, and the `DefaultResolver` touch to hold its adapter is correctly identified as additive (AC-CT-C4 preserved). No signature drift on `resolve_store`.

### Check 2 — Specification coverage
**Status**: PASS
**Evidence**: FR-C1 (slug→store via resolver), FR-C2 (`[[projects]]`), FR-C3 (per-slug DB/vector/hash-chain/analytics under `/data/.unimatrix/{slug}/`), FR-C4 (register/list/delete), FR-C5 (allowlist at edge), FR-C6 (absent ⇒ unchanged), FR-C7 (N:1 shared store) each map to a component file. FR-X1/X3/X5 (single funnel, sole-write handle, served-through-not-around) are the spine of `project-router.md`. No scope additions: D2 (overlay) and D3 (network health) are explicitly excluded with negative tests, and OQ-PR-6/7 correctly defer per-slug-vs-shared subsystem and background-tick decisions rather than inventing them.

### Check 3 — Risk coverage
**Status**: PASS
**Evidence**:
- **R-01 (Critical)**: swap-at-callsite, `PathRouter::new` takes `Arc<dyn StoreResolver>` (structural per #4963), unknown-slug→UnknownProject, routing-inside-seam, and the no-residual-fixed-adapter funnel test — all four R-01 scenarios covered.
- **R-03 (fix-before-merge)**: security table T-SEC-01..20 with the no-filesystem-escape assertion (`test_no_accepted_slug_escapes_data_dir`) and parse-edge-before-join assertion. Full traversal/encoding corpus (`../`, `%2f`, `%2e`, `\`, absolute, uppercase, empty) present.
- R-04, R-06, R-10, R-12, R-13 each routed to named tests. Edge cases from the strategy (concurrent same-slug, out-of-band-removed dir, interleaved Default/Slug) are in the plan.

### Check 4 — Interface consistency
**Status**: PASS
**Evidence**: OVERVIEW.md defines shared types once; `per_slug_data_dir(base, &ProjectSlug)` is the single slug→path translation, consumed by both `project-router` (build_project_entry) and `project-registry-cli`. `RESERVED_SLUGS` is defined once in `projects-config.md` and imported by the register CLI (no second list). `ProjectSlug` is reused from the merged seam by config, CLI, and router — no second validator. Data-flow diagrams across the four files are coherent. Referenced paths verified: `infra/config.rs` exists; `projects.rs` is correctly new.

### Check 5 — Knowledge stewardship compliance
**Status**: PASS (closed in rework iteration 1)
**Evidence (original run — FAIL)**: at first pass `wave2/agents/` held only the two **test-plan** agent reports (`vnc-034-wave-2-agent-2-testplan-report.md`, `vnc-034-wave-2-agent-2b-testplan-refine-report.md`), both compliant. There was no pseudocode (design) agent report and no embedded stewardship block in the four pseudocode files — missing block ⇒ REWORKABLE FAIL.
**Evidence (rework 1 — PASS)**: `wave2/agents/vnc-034-wave-2-agent-1-pseudocode-report.md` now exists and carries a complete `## Knowledge Stewardship` block:
- `Queried:` — `context_briefing` + `context_search`, surfacing and applying #4963 (build-but-unwireable seam → single-call-site swap), #4958 (`Arc::ptr_eq` for store identity), ADR #4949/ADR-005 (default alias → D5 reserved-slug rationale), ADR #4950/ADR-003 (seam invariant → `adapter_for`-inside-seam), ADR #4951/ADR-004 (allowlist/no-listing → D1 reuse + D3 posture). Satisfies the read-only design-agent `Queried:` requirement.
- `Stored / Deviations:` — "nothing novel stored" with a stated reason (design extends the merged Wave-1 seam per ADR-003 and reuses the merged D1 allowlist; no new generalizable pattern at pseudocode stage) and correct hand-off of the infra-001 transport-mismatch pattern to the tester/leader. Reason present ⇒ no WARN.
- Pseudocode artifacts confirmed unchanged during rework (mtimes 18:00–18:06 precede the 18:13 report; no design diff), so the other 11 checks remain valid.

Note (advisory, not a gate failure): the agent-2 test-plan report records that a `context_store` call was rejected (`Agent 'anonymous' lacks Write capability`). The agent correctly documented this and the leader/SM should store the infra-001-transport-mismatch testing pattern post-merge. The stewardship block is still present and valid, so agent-2 PASSES the check.

---

## Locked-Decision Findings (gate-enforced; the issue drifted and does NOT govern)

### D1 — slug allowlist EXACTLY `^[a-z0-9][a-z0-9-]{0,62}$`
**Status**: PASS
- Pseudocode REUSES the merged `ProjectSlug::TryFrom` (seam.rs:71–104, verified: 1..=63 chars, leading alnum, lowercase alnum + hyphen, no underscore) — it does **not** re-implement or widen. `projects-config.md` and `project-registry-cli.md` both delegate to it; the config layer is "structural only."
- The drifted issue-#727 value `^[a-z0-9][a-z0-9_-]{0,63}$` is explicitly named and rejected in OVERVIEW, both component files, and the test plan.
- Discriminators present and correctly polarized: **T-SEC-15 `my_project` → REJECT** (underscore), **T-SEC-16 64-char → REJECT** (over bound), **T-SEC-17 63-char → ACCEPT** (exact bound), plus `1alpha` ACCEPT (leading digit). Full corpus: `../`, `%2f`, `%2e`, `\`, absolute, uppercase, empty, whitespace, dot — all REJECT. No-filesystem-escape assertion present (`test_no_accepted_slug_escapes_data_dir`).

### D4 — delete = de-register-only; `--purge` destroys loudly; re-register RE-ATTACHES
**Status**: PASS
- `delete` (default) preserves the on-disk data dir + hash chain (de-register only); `--purge` is the sole destroy path and requires `--confirm <slug>` matching the slug exactly — bare `--purge` is refused. Non-interactive (no TTY) confirmation shape is correct for the distroless container.
- Re-attach integrity is the highest-value test: `test_deregister_reregister_reattaches_to_preserved_chain` asserts entries survive AND the hash-chain head is unchanged (chain continued, not reset to genesis); `test_purge_then_register_is_fresh_store` is the contrast guard.
- OQ-CLI-7 correctly raised: the re-attach guarantee depends on `Store::open` being non-destructive on an existing DB, or genesis being gated on `data_exists`. The pseudocode (State B vs State C) addresses OQ-CLI-7 by branching on `data_exists` and explicitly forbids funneling both through a truncating path. **Stage 3b MUST verify** `Store::open` semantics or implement the explicit `data_exists` genesis gate.

### D5 — reserved-slug refusal at register, separate from charset
**Status**: PASS
- Single `RESERVED_SLUGS = ["v1","health","observe","tools"]` in `projects-config.md`, imported by the register CLI. The check is explicitly separate from the D1 charset allowlist.
- `tools` shadowing is the critical case and is proven charset-valid-yet-rejected: `test_register_reserved_is_separate_from_charset` / `test_reserved_check_is_separate_from_charset` assert `ProjectSlug::try_from("tools")` is `Ok` while register/config reject it (T-RSV-01). Exact-match-only guard (`toolsx`, `v1-prod`, `healthcheck` ACCEPT) prevents an over-broad prefix check.

### D6 — register idempotence is two-state, not collapsed
**Status**: PASS
- State A (data + routing) → loud error; State B (data, de-registered) → re-attach (not an error); State C (no data) → fresh create. `test_register_two_states_distinct_messages` asserts the two existing-slug outcomes are not collapsed into one generic message.

### D3 — list-field only; D2 — no overlay
**Status**: PASS
- D3: `store_open` derived from local filesystem state only; `test_no_per_slug_health_endpoint` + `test_list_exposes_no_network_health` assert no per-slug network/HTTP surface, preserving AC-W1-S6 and the ADR-004/OQ-B no-listing posture. No `--list-slugs` network endpoint.
- D2: `ProjectConfigEntry` is slug-only; `test_no_per_project_config_overlay_merge` is a negative structural assertion.

### FUNNEL — Wave-1 discard + fixed-adapter fallback ELIMINATED
**Status**: PASS
- Verified the Wave-1 bypass exists today: seam.rs:283 `let _store: Arc<Store> = ...` (discarded) then seam.rs:299 `self.project_router.route_mcp(request).await` (fixed-adapter dispatch). The pseudocode targets BOTH for removal: the resolved `store` is USED, and `adapter_for(&key)` becomes the SOLE dispatch route (Default included), with no `None`-means-fixed-fallback escape hatch.
- `DefaultResolver` behavior remains byte-identical for the Default path (AC-CT-C4 / AC-W2-R2): `/v1/tools/...` dispatches through `adapter_for(&Default)` over the same store/adapter; the change is structural, not observable.
- No-bypass integration test present: `test_dispatch_through_adapter_for_no_fixed_bypass` with ≥2 slugs + Default, each serviced only by its own adapter/store; cross-checked by the A-write-invisible-to-B isolation tests (a residual fixed-adapter would fail these — and could not under N=1, which is why Wave 1 missed it).

### Integration harness
**Status**: PASS
- Real two-store `crates/unimatrix-server/tests/project_routing_integration.rs` drives the `/v1/{slug}/` edge (AC-W2-R1/R3/R5/R6), reusing merged seam wiring (`PathRouter::new(resolver, ...)`) and existing `tests/` conventions — cumulative, no isolated scaffolding.
- infra-001 correctly repurposed as the single-project backward-compat gate (it spawns `serve --stdio`, cannot reach the slug edge); `pytest -m smoke` is the mandatory minimum. No new infra-001 slug tests (correctly out of scope; follow-up issue noted if HTTP-transport harness is later wanted).

---

## Rework Required (original REWORKABLE FAIL — now CLOSED)

| Issue | Which Agent | What to Fix | Status |
|-------|-------------|-------------|--------|
| No design-phase pseudocode agent report with `## Knowledge Stewardship` block | uni-pseudocode (the agent that wrote `wave2/pseudocode/*.md`) | Emit `wave2/agents/{pseudocode-agent-id}-report.md` with a `## Knowledge Stewardship` block (`Queried:` + `Stored:`/"nothing novel"+reason). No change to pseudocode artifacts. | ✅ CLOSED (rework 1) — `vnc-034-wave-2-agent-1-pseudocode-report.md` back-filled and verified |

## Carry-forward to Stage 3b (not blockers — design correctly flagged these)

- **OQ-CLI-7 (load-bearing, D4 integrity):** confirm `Store::open` is non-destructive on an existing DB (attaches to the surviving hash chain, no genesis/truncate), OR implement the explicit `data_exists`-gated genesis branch. The re-attach guarantee depends on it.
- **OQ-PR-2/OQ-PR-8/OQ-PR-9:** the implementer must name the Wave-2 resolver type so it does not shadow the existing `ProjectRouter<ReqBody>`; thread the default `McpAdapter` into `DefaultResolver`; and ensure the HTTP `ProjectRouter<ReqBody>` is NOT reachable as a per-request MCP dispatch fallback once `adapter_for` is the sole route.
- **OQ-CFG-1:** confirm `infra/config.rs` importing `ProjectSlug` from `crate::http` does not create a dependency cycle; if it does, extract `ProjectSlug` to a leaf module — do NOT duplicate the regex.
- **File-size:** `crates/unimatrix-server/src/http/router.rs` is already 562 lines (pre-existing Wave-1 state). Wave-2 logic is correctly planned for a NEW `project_resolver.rs` submodule; Stage 3b must keep every touched/new file ≤ 500 lines.
