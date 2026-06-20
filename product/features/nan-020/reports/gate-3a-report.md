# Gate 3a Report: nan-020

> Gate: 3a (Component Design Review)
> Date: 2026-06-20
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Architecture alignment | PASS | All 6 components map 1:1 to the ARCHITECTURE component table; ADR-001..005 honored in pseudocode + test plans. Codebase facts (smoke helpers/marker/store_size/cleanup, run_smoke_gate grep) verified accurate. |
| 2. Specification coverage | PASS | FR-1..FR-22, NFR-1..NFR-10, AC-01..AC-09 all have corresponding pseudocode + test coverage. No scope additions. OQ-A (`--slug` retirement) resolved per code. |
| 3. Risk coverage | PASS | All 16 risks mapped to test scenarios. R-01/R-03/R-07 Critical called out; R-07 negative control classified REQUIRED pre-merge (not PENDING). R-16 correctly recorded as accepted residual. |
| 4. Interface consistency | PASS | OVERVIEW shared contracts A–H (exit table, messages, blob format, isolated credstore path, terminal marker, byte-unchanged wrapper) consistent across all component files; verified against actual smoke script + lib. |
| 5. Knowledge stewardship | PASS | All 4 design-phase agents have `## Knowledge Stewardship` blocks; architect+risk-strategist have Stored: entries, pseudocode+spec+testplan have Queried: with reasons. |

## Detailed Findings

### 1. Architecture alignment
**Status**: PASS
**Evidence**:
- Six pseudocode components match ARCHITECTURE's "Component Breakdown" table exactly: `docker-http-posture-smoke` (Gates 5–7), `hermeticity-sandbox` (ADR-005 sandbox + negative control), `release-yml-setup-node` (ADR-002 amendment / NFR-10), `docs-client-setup` + `readme-bundle-example` (doc rewrites), `uni-docs-remit` (ADR-004).
- **ADR-001 (extend-in-place / exit-1 fold)** — pseudocode keeps exit numerics 0/1/3/4 unchanged, folds all 6 new failure modes into `fail()` exit 1 with the exact distinct messages from the ADR truth table. No new codes 5/6/7. Verified `release-gate-lib.sh::run_smoke_gate` is the target of the byte-unchanged diff-assertion.
- **ADR-002 (host/container split + pinned node)** — Gate 5 emit runs in a `--rm` throwaway container off the shipped image; Gate 6 consume runs the repo-checkout JS client on the host; `setup-node@v4` (node 24) pinned on both smoke jobs. Verified `package-npm` uses node 24 at release.yml:215–218 as the parity model.
- **ADR-005 (hermeticity as proof obligation, process boundary)** — `hermeticity-sandbox.md` sets HOME + `--project-dir` on the spawned child only ("The harness NEVER mutates its own HOME"), explicitly cites the Rust-2024/vnc-041 AC-02 ban on in-process HOME mutation, requires clean-on-entry, and specifies the REQUIRED negative control (poison stale cred + broken attach → Gate 7 still red). This matches the load-bearing obligation in the spawn prompt.
- **Codebase verification**: `fail()` (line 33, exact `[783-smoke] FAIL:` prefix), `store_size()` (54), `cleanup()`/`trap cleanup EXIT` (35/42), `exit 3` preflight (60), Gate-4 PASS (183), terminal marker (185), and `grep -qx '\[783-smoke\] ALL GATES PASSED.*'` in the lib — all cited correctly. Placement "between line ~183 and ~185" is accurate.

### 2. Specification coverage
**Status**: PASS
**Evidence**:
- Doc rewrite FRs (FR-1..FR-6): `docs-client-setup.md` removes 501/W2-7/curl-observe (6× 501, W2-7, three curl blocks confirmed present in current file), documents `init --bundle` + `/v1/{slug}/observe`, marks `--remote` legacy, adds verified-on footer.
- Doc-test FRs (FR-7..FR-16): Gates 5–7 in-test emission, per-slug store-grew assertion (reusing `store_size`), extend-in-place (no new script), `run_smoke_gate` wiring, append-only ordering, distinct messages, hard-fail skip paths, anchored marker.
- Hermeticity FR-22 / AC-09 / NFR-9: fully covered by `hermeticity-sandbox.md` + `hermeticity-negative-control.md`.
- uni-docs FRs (FR-17..FR-20) / AC-07: `uni-docs-remit.md` widens scope, states blast-radius + full-tree-audit non-goal, bounds the source-read relaxation, retains prompt-injection defense, fences Feature-2 machinery.
- NFR-10 (pinned node): `release-yml-setup-node.md` + test plan static greps.
- **No scope additions**: no drift-checker, no new CI job, no new NFR, no CLI/route change. OQ-A resolved to "no `--slug` on bundle path" per `init.js:353` — confirmed authoritative.

### 3. Risk coverage
**Status**: PASS
**Evidence**:
- Test-plan OVERVIEW §3 maps all 16 risks to owning component test(s) with pre-merge classification.
- **R-01 (Crit)**: every ADR-001 failure-mode row has a stub test asserting exit-1 + marker-suppressed; happy path is the ONLY green (`test_happy_path_is_only_green`); no-early-exit-0 test.
- **R-03 (Crit)**: truth-table invariance {0,1,3,4}, `set -e` RC-survival by execution, append-only Gate-4 precondition, single terminal marker, `run_smoke_gate` byte-unchanged diff.
- **R-07 (Crit)**: `hermeticity-negative-control.md` `test_hermeticity_negative_control_still_red` is the load-bearing REQUIRED pre-merge control (poison + break → STILL red), with discrimination proof (a non-isolated harness WOULD pass), positive twin, clean-on-entry, fresh-write delta, and ≥5-run non-flaky + discriminating store-grew. **Correctly classified REQUIRED pre-merge — PENDING called out as a gap** (matches the spawn-prompt obligation). The R-07 negative control is NOT classified PENDING — gap obligation satisfied.
- **R-16 (accepted residual)**: no `--remote` round-trip owed; sole mitigation (the "legacy" marker in both files) is testable by inspection. Recorded correctly.
- Test plan grounds the stub vehicle in the real existing `release-gate-logic-test.sh` truth-table convention (verified: `run_case`, `fixtures/stub-smoke.sh`, sourced shipped lib bytes, #5192) — cumulative extension, not parallel scaffolding.

### 4. Interface consistency
**Status**: PASS
**Evidence**:
- OVERVIEW shared contracts A–H are the single source of truth and every component references them:
  - **A. Exit-code truth table** 0/1/3/4 — consistent everywhere; verified against lib (exit 3/4 arms, marker grep).
  - **B. New-failure messages** — identical exact strings in pseudocode, test-plan R-02 assertions, and ADR-001.
  - **C. Bundle blob format** (`unimatrix-bundle:` prefix, stdout-only, stderr token-redacted) — matches README:587 verbatim.
  - **D. Invocation signatures** — emit `docker run --rm ... client-bundle <slug>`; consume `HOME=... node ... init --bundle "$BUNDLE" --project-dir ...` (no `--slug`); wrapper `run_smoke_gate IMAGE bash docker-http-posture-smoke.sh`.
  - **E. Isolated credstore path** `$SANDBOX/home/.unimatrix/<projectHash>/remote.json` (mktemp -d, clean-on-entry) — consistent ADR-005 ↔ pseudocode ↔ test.
  - **F. Terminal marker** single, last line, gates 5–7 print before it — consistent.
  - **G. Per-slug store growth** via `store_size` delta — reuses nan-019 helper.
  - **H. Canonical doc form** `init --bundle <blob>` (no `--slug`) + legacy `--remote <url> --token` — binding on BOTH doc components; `docs-client-setup.md` and `readme-bundle-example.md` agree.
- **README enumeration is more complete than the brief**: design grep found FOUR bundle-via-`--remote` occurrences (123, 130, 585, 587) vs the brief's OQ-B two; line 113 correctly kept as the legacy URL+token form. Verified against actual README — all four confirmed present at those lines, line 113 is genuinely the URL+token legacy mode. README four-occurrence convergence obligation satisfied.
- **Stub-drivability seam** (env-injectable client-bundle / init / observe) is explicitly designed into the pseudocode ("Delivery MUST factor the Gate 5–7 logic so the external commands ... are env-injectable") so the truth table + negative control are stub-drivable pre-merge — matches the existing `SMOKE_CMD...` indirection precedent in the lib.

### 5. Knowledge stewardship compliance
**Status**: PASS
**Evidence**:
- **architect** (active-storage): `Stored: #5249 ADR-001, #5250 ADR-002, #5251 ADR-003, #5252 ADR-004` via context_store; Queried: context_briefing. ✓
- **risk-strategist**: the RISK-TEST-STRATEGY.md Knowledge Stewardship block has `Queried:` (six lessons) and `Stored: nothing novel -- {reason}` with explicit justification (faithful reuse of #5180/#5183/#5189/#4977/#4903). ✓ (reason present → no WARN)
- **pseudocode** (read-only): `Queried:` #5180, #5192; deviations: none. ✓
- **spec**: `Queried:` context_briefing; read-only tier, no storage stated. ✓
- **tester** (testplan): `Queried:` + `Stored: nothing novel -- {reason}` with justification. ✓
- All blocks present; all "nothing novel" entries carry a reason → no WARN.

## Rework Required

None.

## Observations (non-blocking, for delivery awareness)

1. **README narrative `init --remote` mentions at lines 627 and 693** are NOT in the design's enumeration. Both are narrative prose (627: "Remote mode is configured via init --remote"; 693: cert-rotation "re-running init --remote on each client"). Line 693 parallels line 130 (cert-rotation, bundle context) which the design DID converge to `init --bundle`. Delivery/uni-docs should consider whether 693's "re-run `init --remote`" should also converge to `init --bundle` for consistency, since the bundle-rotation flow is the bundle path. Line 627 ("configured via `init --remote`") legitimately describes the legacy URL/token mode and may stay. This is narrative-prose consistency, not an executable-claim drift — the AC-02 grep (which targets example forms `init --remote (unimatrix-bundle:|<bundle>)`) is unaffected. Flagged so the convergence is exhaustive in prose too.

2. The Gate 7 hook-client invocation in `docker-http-posture-smoke.md` is left at a deliberately under-specified altitude (the exact hook-client entry path + minimal event JSON are marked as an implementation note for the dev). This is appropriate at pseudocode altitude, but Stage 3b must resolve the real hook-client entry (`packages/unimatrix/.../hook-client/`) and a valid minimal hook event. The store-delta assertion is correctly identified as the load-bearing signal (the client is fail-open, so its exit code is not trusted) — a sound design choice.
