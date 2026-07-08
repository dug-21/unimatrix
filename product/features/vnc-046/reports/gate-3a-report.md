# Gate 3a Report: vnc-046

> Gate: 3a (Component Design Review)
> Date: 2026-07-07
> Result: PASS (re-validation iteration 1 — was REWORKABLE FAIL)
> Validator agent: vnc-046-gate-3a-v2 (re-validation); vnc-046-gate-3a (original)

---

## RE-VALIDATION (iteration 1) — RESULT: PASS

The single gate-blocking item (OQ-2 — per-slug signature scanner omitted) is **substantively
closed**. All other checks were PASS/WARN in the original run and are unaffected. Three WARN-level
residuals remain (plumbing tidy, test-plan strengthening, OVERVIEW doc lag) — none block Stage 3b.

### OQ-2 closure — verified

| Artifact | Change | Verdict |
|----------|--------|---------|
| **ADR-002** | Per-slug "signature scanner (fallible)" block added; P1 renamed registry+hold+scanner **TRIPLE**; per-slug-vs-global verdict = **PER-SLUG** (compiled from `r.transcript_signals`, not the daemon's shared `Arc`); names↔counts split-brain rationale sound | CLOSED |
| **project-provisioner.md** | P1 builds cap+hold+scanner triple; scanner compiled from `r.transcript_signals.enabled_patterns()`, fallible via `.map_err(→ServerError::Config)?`, chained into the registry **before** `server.session_registry` is set → travels with the registry the `main.rs:1229` tick loop clones (F1/SR-03 preserved) | CLOSED |
| **boot-assertion.md** | `IsolationProbe` carries `has_hold` + `signal_class_names`; P3 sentinel (l.78-79) aborts boot if a slug declares signals but `transcript_signal_class_names` is empty — consistent with ADR-003's ratified probe | CONSISTENT |
| **OVERVIEW.md** | Shared Types now lists `.with_signature_scanner()`; OQ-2 marked RESOLVED | CLOSED |

**Fallible / paired-before-tick-clone / counts-aligned-with-names**: all confirmed. The pseudocode's
`map_err(|e| ServerError::Config(...))?` is the correct handling (there is no `From<ScannerError>`);
it is more precise than ADR-002's illustrative snippet, which uses a bare `?` (see WARN-1).

### Do the added tests catch the hollow-counts / AC-07 failure mode?

- **Empty-scanner defect — NETTED in the authoritative plan.** `test-plan/project-provisioner.md`
  INV-C Edge Cases (l.49-51) require a declared-class slug to **not** false-pass on the "declared
  classes but zero counts" #930 symptom, and to distinguish it from a legitimately-empty
  `[transcript_signals]`. A per-slug empty scanner (or a wrong-global scanner counting different
  classes than the per-slug names) therefore fails INV-C1/C2. This is the behavioral net.
- **Pseudocode hint is strong** (`pseudocode/isolation-suite.md` l.88-92, 123-129;
  `pseudocode/project-provisioner.md` l.147-159): non-zero counts required, signal-bearing input on
  AC-07 parity, matched-class count > 0 asserted. Good — but see WARN-2: this strengthening did
  **not** propagate to the authoritative `test-plan/` files.

### OQ-1 / OQ-3 consistency across ADR-002 / ADR-003 — confirmed

- **OQ-1 (IsolationProbe)**: ADR-003 ratifies the `&ProjectServerInput → &IsolationProbe` param
  refinement with probe `{ slug, session_registry, pending, services, has_hold, signal_class_names }`;
  ADR-002 defers convergence-pinning to ADR-003. `boot-assertion.md` matches. Consistent.
- **OQ-3 (`categories`)**: ADR-002 (l.108-110) and ADR-003 (l.75-84) both classify `categories`
  **PER-SLUG** config-snapshot and both flag NFR-5's "global allowlist" prose as stale. Consistent
  with each other and with shipped code (`slug_categories`, `main.rs:1183`).

### Residual WARNs (non-blocking — Stage 3b/3c must-confirm)

| # | Finding | Owner | Fix |
|---|---------|-------|-----|
| WARN-1 | Scanner source plumbing: `build_project_server`'s real signature (verified `http_provision.rs:136`) receives **no `r`/resolved config** — everything from `r` is threaded as explicit params (ADR-002's own "params-at-end, missing field = compile error" discipline). Yet the P1 scanner block references `r.transcript_signals.enabled_patterns()` **inside the function body**, where `r` is out of scope; only the derived `signal_class_names` (names) is threaded, not the patterns/scanner. ADR-002's snippet likewise mixes `retention_config` (param) with ambient `r.transcript_signals`, and uses a bare `?` vs the pseudocode's correct `map_err`. | uni-pseudocode / uni-architect | Thread the scanner as the daemon does: compile at the **call site** (`main.rs:1204`, where `r` exists — mirror how `slug_signal_class_names` is derived) and pass `signature_scanner: &Arc<SignatureScanner>` as a 4th param-at-end; OR thread `transcript_signals`. Fix ADR-002 snippet to match (params-at-end; `map_err`, not bare `?`). Non-blocking: intent + mechanism + call-site pattern are unambiguous, and a wrong 3b resolution (empty or global scanner) is caught by INV-C1. |
| WARN-2 | AC-07 parity test in the **authoritative** plan (`test-plan/isolation-suite.md` l.59-65, `test_https_equals_uds_observe_fidelity`) was **not** strengthened — no signal-bearing-input requirement, no count>0 assertion. As written it can false-green with an empty per-slug HTTPS scanner (both legs `{}` → agree), so R-16 signal-parity is vacuous for signals. The stronger guard lives only in the pseudocode **hint** (which explicitly states the tester owns the authoritative plan). | uni-tester (Stage 3c) | Add the signal-bearing input + matched-class count>0 assertion to the authoritative AC-07 parity test, matching `pseudocode/isolation-suite.md` l.123-129. Non-blocking: INV-C1 already nets the empty-scanner defect. |
| WARN-3 | `pseudocode/OVERVIEW.md` is stale vs the now-ratified ADRs: census table still routes `categories` to "see Open Question 3"; OQ-1 still lists a smaller `IsolationProbe` and "Flag for architect sign-off" (ADR-003 has since ratified it). Documentation lag, not behavioral drift. | uni-pseudocode | Reconcile OVERVIEW OQ-1/OQ-3 + census `categories` row to ADR-002/ADR-003 (both now decided). |

### No regression / scope drift
No new params beyond the ratified 3 (`store_config`, `retention_config`, `signal_class_names`); the
scanner is derived, not a new public surface. Census + Shared Types updated. Other components
(resolution-funnel, project-resolver, observe-context, observe-handler) unchanged and unaffected.

**Re-validation gate result: PASS.** The blocking OQ-2 defect is closed (scanner wired per-slug,
fallible, paired before the tick clone, decision recorded, class-names sentinel at boot, empty-scanner
defect netted by INV-C1). WARN-1/2/3 are Stage-3b/3c must-confirms, not gate blockers.

---

## ORIGINAL RUN (iteration 0) — REWORKABLE FAIL — retained below for provenance

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Architecture alignment | WARN | 7 components map 1:1 to ARCHITECTURE Component Breakdown; interfaces match. One parity gap: ADR-002 construction list omits the per-slug signature scanner the daemon builds (`main.rs:820/852`) — see OQ-2. |
| 2. Specification coverage | FAIL | FR-1…FR-13 covered, no scope additions. **FR-9 (`signal_class_counts`) and AC-07 (HTTPS==UDS parity) are NOT genuinely satisfiable** as designed: per-slug registry is built with an empty `SignatureScanner` → class counts stay zero → parity divergence with UDS (OQ-2). |
| 3. Risk coverage | PASS | R-01…R-16 all mapped to component test plans (OVERVIEW risk→test table). Bidirectional N≥2 suite (R-01/SR-06) + negative-control meta-test; assembled-wiring-only (R-02) via grep-gate + N≥2. |
| 4. Interface consistency | PASS | Shared types in OVERVIEW match per-component usage. `ObserveContext.adapt_service` (deleted, vestigial) correctly distinguished from `UnimatrixServer.adapt_service` (census PER-SLUG) — no contradiction. |
| 5. Knowledge stewardship | PASS | All design-phase agent reports carry `## Knowledge Stewardship`; active-storage (architect, risk-strategist) have Stored/Declined; read-only (pseudocode, spec) have Queried. Tester block is embedded in `test-plan/OVERVIEW.md` (WARN: no separate agents/ report file). |
| #800 first-class harness build | PASS | `test-plan/OVERVIEW.md` "Integration Harness Plan" specifies multi-slug HTTP boot, per-slug config, per-slug MCP client, new `test_project_isolation.py` (8 named bidirectional tests), marker-keyed read-as-barrier, smoke markers. |

**Checks: 3 PASS / 5 core checks (2 WARN, 1 FAIL).** One narrow REWORKABLE item (OQ-2).

## Detailed Findings

### Check 1 — Architecture alignment
**Status**: WARN
**Evidence**: The 7 pseudocode/test-plan components (resolution-funnel, project-resolver, observe-context, observe-handler, project-provisioner, boot-assertion, isolation-suite) map 1:1 to the ARCHITECTURE Component Breakdown and to IMPLEMENTATION-BRIEF Component Map. New interfaces (`registry_for`/`pending_for`/`services_for`, extended `ProjectEntry`, reshaped `ObserveContext`, `build_project_server` +3 params, `assert_per_slug_isolation`) match the Integration Surface signatures exactly.
**Issue**: ADR-002 claims "full per-slug construction parity with the daemon path (`main.rs:830-994`)" but its param list and the `project-provisioner.md` P1 block build the per-slug `SessionRegistry` with only `.with_transcript_cap().with_transcript_hold()` — the daemon also wires `.with_signature_scanner(...)` (`main.rs:852`, compiled from `config.transcript_signals`). This is a real parity gap, surfaced by the design as OQ-2 but left unresolved (see Rework).

### Check 2 — Specification coverage
**Status**: FAIL (reworkable)
**Evidence**: Every FR has corresponding pseudocode; NFR-1 (O(1) hot path), NFR-2 (release-hard boot assertion, not `debug_assert`), NFR-3 (no wire change), NFR-7 (no unwrap, ≤500 lines) all addressed. No unrequested features.
**Issue**: FR-9 requires `signal_class_counts_json` to "reflect the slug's declared signal classes." Source (`mcp/activity_fold_handler.rs:104-135`) shows the JSON's counts come from `snap.class_counts`, which are produced by the `SessionRegistry`'s `signature_scanner` at delta-ingest time (`infra/session.rs:279,350`). `SessionRegistry` defaults to `SignatureScanner::empty()` (`session.rs:305`). With no per-slug scanner wired, a slug that declares `[transcript_signals]` gets its class **names** in the JSON but all-**zero** counts — and diverges from the UDS path (which has the scanner), breaking AC-07 parity for any signal-bearing transcript. `transcript_signals` is available per-slug (`r.transcript_signals`; the provisioner already reads `r.transcript_signals.enabled_class_names()` for `transcript_signal_class_names`), so the fix is mechanical.

### Check 3 — Risk coverage
**Status**: PASS
**Evidence**: `test-plan/OVERVIEW.md` Risk→Test table maps all R-01…R-16 to a component plan + vehicle. The two Critical constraints are enforced structurally: (R-01/SR-06) every INV has two distinct named cases (`_a_driver`/`_b_driver`), plus `test_negative_control_reverse_misroute_trips_red`; (R-02) assembled-wiring-only via `route_observe` → `McpAdapter`, N≥2, plus an AC-06 grep-gate against `Arc::ptr_eq`/hand-passed `dispatch_request` in the behavioral crate. R-03 census false-pass is backstopped behaviorally (#5427). R-04 `store_config`/`inference_config` white-box exception is enumerated, never omitted. R-15 persistence-level assertions present.

### Check 4 — Interface consistency
**Status**: PASS
**Evidence**: `pseudocode/OVERVIEW.md` Shared Types + New/Changed Types are consistent with each component file and with ARCHITECTURE Integration Surface. `dispatch_request` signature change (drop `_vector_store`/`_adapt_service`) is consistent between `observe-handler.md` and OVERVIEW. Potential contradiction checked and cleared: `ObserveContext.adapt_service` (deleted vestigial, `observe-context.md`) is a different field from `UnimatrixServer.adapt_service` (census PER-SLUG independent, ADR-006) — the census operates over `UnimatrixServer`, the deletion over `ObserveContext`. No conflict.

### Check 5 — Knowledge stewardship
**Status**: PASS (1 WARN)
**Evidence**: architect — Queried + `Stored: #5630–#5634 ADR-001…005` + Declined (no bug-lesson). risk-strategist — Queried + `Stored: nothing novel` with reason. pseudocode (read-only) — Queried + `Stored: nothing — read-only tier` with reason. spec — Queried + Stored with reason. vision-guardian — Queried + Stored with reason.
**WARN**: The Stage-3a tester produced the test plans but there is no `agents/…-tester-report.md`; its stewardship block lives inside `test-plan/OVERVIEW.md` (Queried `context_briefing`/`context_search`; Stored: deferred harness-pattern until built). Obligation met in-artifact; note the missing separate report file for Gate 3c bookkeeping.

## Adjudication of the 5 Stage 3a Open Questions

### OQ-1 — Boot-assertion vs `from_servers` move ordering (IsolationProbe refinement)
**Verdict: RESOLVED acceptably; consistent with ADR-003's guarantee (housekeeping follow-up).**
ADR-003 literally says assert "after `build_project_server`, before `from_servers` moves the inputs" with signature `assert_per_slug_isolation(input: &ProjectServerInput, resolver, config)`. That is impossible as written — the `Arc::ptr_eq` convergence check needs the resolver, which does not exist until `from_servers` has consumed the inputs. `boot-assertion.md` resolves this soundly: capture a per-slug `IsolationProbe { slug, session_registry, pending, services, has_hold, signal_class_names }` (Arc clones) in the existing pre-move tick loop (`main.rs:1229`), build the router, then `assert_per_slug_isolation(probe, &*router, &config)?` per slug. This preserves every ADR-003 guarantee (real runtime `Result`, `ptr_eq` convergence, `has_transcript_hold` pairing, P3 sentinels, boot-abort) and only refines the param type. Flagged for architect sign-off, correctly.
**Non-blocking follow-up (Stage 3b):** architect should ratify the `&ProjectServerInput → &IsolationProbe` param refinement into ADR-003 via `context_correct` on the stored ADR so the record matches the built code.

### OQ-2 — Per-slug signature scanner param
**Verdict: UNCLOSED gap → REWORKABLE.** (The one gate-blocking item.)
Source confirms the per-slug registry needs a signature scanner: `signal_class_counts` values are produced by `SessionRegistry.signature_scanner` during delta apply (`session.rs:350`), which defaults to `SignatureScanner::empty()`. `project-provisioner.md`'s P1 block wires only cap+hold and comments the scanner as an open question ("do not invent a param"), which would steer Stage 3b to ship an empty scanner. Consequence: zero class counts for any slug that declared `[transcript_signals]` (hollow FR-9) and an AC-07 HTTPS≠UDS parity divergence on signal-bearing transcripts. The fix is confirmed and mechanical — `r.transcript_signals` is already resolved per-slug (the provisioner reads `r.transcript_signals.enabled_class_names()` for `transcript_signal_class_names`). See Rework.

### OQ-3 — `categories` classification (NFR-5 global vs code per-slug `slug_categories`)
**Verdict: RESOLVED acceptably — census classifies to match the CODE.**
Code truth confirmed: `main.rs:1183` builds a per-slug `slug_categories` from `r.knowledge.categories` and threads it into `build_project_server` (`http_provision.rs:153,257,267`) — categories IS per-slug/config-driven today (crt-031/vnc-040), contradicting NFR-5's "global operator allowlist" prose. `boot-assertion.md` census and `OVERVIEW.md` field-census table both explicitly direct the author to classify `categories` "consistent with the code (per-slug config-driven `slug_categories` today), not NFR-5's prose." That is the correct call for a census (it must reflect reality). It is set at ctor from the threaded param, so it needs no handle-convergence boot check (config-snapshot class, like P3).
**Non-blocking note:** NFR-5's characterization of `categories` as global is stale relative to shipped code. The census author should add a one-line rationale so a reviewer does not read the per-slug classification as an NFR-5 violation; the human may want NFR-5 corrected during retro.

### OQ-4 — Vestigial-field deletion blast radius (~100 `dispatch_request` call sites)
**Verdict: RESOLVED acceptably — planned as one atomic pass in the right component.**
Confirmed 107 `dispatch_request` references across `http/router.rs`, `handlers.rs`, `http/router/tests.rs`, `uds/listener.rs`, and two listener test modules; the two vestigial params are `_vector_store` (`listener.rs:777`) and `_adapt_service` (`listener.rs:779`). `observe-handler.md` owns the `dispatch_request` signature change, documents the ~100-site sweep, states it "cannot be half-done" (compile is the safety net), and forbids placeholder handles. IMPLEMENTATION-BRIEF OQ-4 assigns this to the observe-context/observe-handler wave — consistent. Sound.

### OQ-5 — #800 cert/bearer reuse + `inference_config` boot sentinel
**Verdict: RESOLVED acceptably — planned, not deferred as a gap.**
Cert/bearer reuse: `test-plan/OVERVIEW.md` Integration Harness item 1 requires reading the provisioned bearer token + served leaf cert from the data dir, "reuse the existing cert/token plumbing the parity legs + `cert_provisioner` already use — do not invent a new TLS path." `inference_config` boot sentinel: `boot-assertion.md` acknowledges `store_config`/`inference_config` "lack a clean sentinel → covered by guard 2 + wiring-pin unit (documented AC-06 exception)," and `project-provisioner.md` #5 (`test_inference_config_wiring_pin_bidirectional`) provides the bidirectional wiring-pin. This is the ratified AC-06 white-box-exception path (OQ-3 in SPEC, R-04). Appropriately handled.

## Rework Required (REWORKABLE FAIL)

| Issue | Which Agent | What to Fix |
|-------|-------------|-------------|
| OQ-2: per-slug `SessionRegistry` built with empty `SignatureScanner` → zero `signal_class_counts` (hollow FR-9) + AC-07 HTTPS≠UDS parity divergence | uni-architect | Add the per-slug signature scanner to ADR-002's construction-parity list (mirror `main.rs:820/852`): the per-slug registry must be built `.with_signature_scanner(...)` compiled from the slug's `r.transcript_signals.enabled_patterns()`. Update the stored ADR via `context_correct`. Decide + record whether signatures are per-slug (compile from `r.transcript_signals`) or operator-global (share the daemon's `Arc<SignatureScanner>`) — either way a scanner MUST be wired; "no scanner" is wrong. |
| OQ-2 (same): project-provisioner P1 block omits the scanner | uni-pseudocode | In `pseudocode/project-provisioner.md`, add scanner construction to the P1 registry build: `.with_signature_scanner(Arc::new(SignatureScanner::compile(r.transcript_signals.enabled_patterns())?))` (fallible → `?`; `build_project_server` already returns `Result`). Remove the "do not invent a param" hold now that the resolution is ratified. Add a `project-provisioner.md` test expectation asserting a slug with declared signals yields non-zero `signal_class_counts` for a signal-bearing delta, and add a note to the AC-07 parity test (isolation-suite) to drive a signal-bearing transcript so parity actually exercises the scanner (guard against R-16 fake-green). |

**Non-blocking follow-ups (do in Stage 3b, do not gate on them):**
- OQ-1: architect ratify `&IsolationProbe` param into ADR-003 (`context_correct`).
- OQ-3: census author add a one-line rationale that `categories` is per-slug per code (NFR-5 prose stale).
- Bookkeeping: tester should emit a separate `agents/…-tester-report.md` for Gate 3c (stewardship currently only in `test-plan/OVERVIEW.md`).

## Scope Concerns
None. The gap is a mechanical construction-parity omission with a confirmed, small fix — no scope, technology, or architecture blocker. P1/P2/P3 all land at the one seam as designed.
