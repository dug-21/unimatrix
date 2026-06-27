# Scope Risk Assessment: infra-004

Mode: scope-risk. Run before architecture + specification. Product/scope-level risks only.
Historical evidence: Unimatrix #5267 (never-green-on-a-tag), #5184/ADR-004 (dispatch vs tag tag-resolution).

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | The #767-derived warmup bound (AC-01) is calibrated for the embed-readiness round trip (`context_store→context_search`), not the isolation gate's distinct readiness profile (model loaded AND **both** per-slug stores live + registered). If #767's window under-covers, cold runs flap INFRA → silently-vacuous enforcement. | High | Med | Architect: validate #767's window bounds the isolation gate's specific readiness; budget headroom/margin over the bare #767 number, not the number itself. |
| SR-02 | Cold first-boot HuggingFace download (C1/N5, AC-04) is an **external, variable dependency** — runner bandwidth / HF availability on release day governs whether the bound holds. A throttled HF makes enforcement vacuous on the very release it guards. | High | Med | Architect: confirm the #767 window was measured under a real **cold download** (not warm cache); decide whether download-time variance is in-bound or needs a pre-pull/pin distinct from "model failed". |
| SR-03 | `sqlite3` provisioning (AC-10) is coupled to in-flight #849; existing `smoke-*` lanes provision only `node`. Drift or ordering between infra-004 and #849 → preflight INFRA (C1) on a **blocking** lane → vacuous pass. | Med | Med | Architect: decide self-contained `sqlite3` provisioning vs hard dependency on #849 landing first; avoid an ordering trap that strands the feature. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | The blocking flip (Step 4) makes a **previously-non-existent** lane release-blocking. The tri-state maps RED→block / INFRA→pass, but **job-harness failures outside the script's exit code** (runner outage, GHCR login expiry, image-pull 404, checkout fail, the sqlite3 setup step itself) fail the `needs:` edge and block **all** releases regardless of verdict. Scope frames only the script's tri-state. | High | Med | Architect: ensure only the script's exit-2 maps to non-blocking; classify/contain harness-step failures explicitly or the lane becomes a release-wide outage vector. |
| SR-05 | **Never-green-on-a-tag (#5267).** The exit-2/INFRA branch + blocking `needs:` edge first execute on a **real tag** only post-merge. D-2's in-feature run exercises the **dispatch** path (`:latest-<arch>`), NOT the **tag-push** path (`:v<version>-<arch>`) — divergent tag resolution per ADR-004 (#5184). Release-only gates fail in sequence on first real tag. | High | Med | Architect/spec: treat AC-11 as proving warmup+verdict, **not** tag-push resolution; budget a post-merge tag round; make the lane diagnostic-capture-first so round 1 yields a diagnosis, not a guess. |
| SR-06 | The D-2 "byte-identical to `main`" claim holds only if build inputs are identical: no Cargo.lock/Dockerfile/build-context drift, deterministic build, **and `main` has not advanced past the branch point**. If `main` moves while the branch is open, the branch build is not current production code → AC-11 proves a stale image. | Med | Med | Spec: require AC-11 run against a branch freshly rebased on `main` (or assert branch-point == `main` HEAD at run time); confirm build reproducibility makes "byte-identical" true, not assumed. |
| SR-07 | "Loud enough that someone notices" (mitigation (c) / AC-13) is a **human-vigilance process control**, not an enforced one. A chronically-INFRA gate stays non-failing green-adjacent across N releases with no automated escalation — exactly the silently-vacuous outcome, just visible-if-watched. | Med | Med | Architect/spec: decide whether chronic-INFRA needs a stronger surface (tracked count / escalation threshold) or is explicitly accepted as documented human-vigilance risk. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-08 | The exit-2 branch (D-1) lands in the **shared** `release-gate-lib.sh` sourced by all four existing **blocking** smoke lanes + the stub test. An additive change that alters the `*)→return 1` catch-all could silently shift sibling-lane behavior. | High | Low | Architect: confirm no existing lane emits exit 2 today (branch is purely additive); stub test must cover the full tri/quad-state truth table to catch sibling regressions. |
| SR-09 | Dispatch-from-feature-branch (D-2 (a)) may lack GHCR write / cannot push `:latest-<arch>` for a non-default branch depending on runner+token config (D-2 already flags this). If so, Step 3 is stranded and forces the feature-resplitting fallback. | Med | Med | Architect: verify GHCR write + `:latest-<arch>` push from the feature branch **early**, before building Step 3 on D-2 (a); keep the two-step fallback specified. |

## Assumptions

- **#767's window is a valid bound for this gate** (SCOPE Goal 1 / AC-01). If #767 measured warm or measured only embed-readiness (not per-slug store liveness), the bound is invalid → SR-01/SR-02.
- **Feature-branch build == `main` production bytes** (SCOPE Proposed Approach Step 3 / D-2). Depends on deterministic build + no `main` drift → SR-06.
- **Only the script's tri-state exit governs blocking** (SCOPE "Blocking Semantics"). Silent on harness-step failures → SR-04.
- **A human will notice the `::warning::`** (SCOPE central risk mitigation (c) / AC-13). Unenforced → SR-07.
- **#849 lands compatibly** (SCOPE Dependencies / AC-10). Coordination, not a guarantee → SR-03.

## Design Recommendations

1. **Front-load the never-green-on-a-tag tax (SR-05, #5267).** AC-11's dispatch run does not exercise tag-push resolution; design the lane diagnostic-capture-first and explicitly budget a post-merge tag round rather than treating round-2 surprises as regressions.
2. **Scope the blocking blast radius beyond the tri-state (SR-04).** Architecture must specify how non-script (harness/setup) failures behave once the lane is in `create-container-manifest.needs:` — they are the un-modeled release-wide block vector.
3. **Validate the warmup bound's provenance, not just its presence (SR-01, SR-02).** The bound must cover this gate's readiness under a real cold download with margin; otherwise blocking is achieved but verification is silently vacuous — the feature's own central risk.
4. **Keep the shared-lib change additive (SR-08)** and prove the full exit-code truth table via the stub seam before the flip.
