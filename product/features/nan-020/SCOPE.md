# nan-020 — Product Documentation Currency: Doc-Test Enforcement for Executable Claims

## Problem Statement

Unimatrix product documentation drifts silently out of sync with shipped behavior, and nothing in the workflow catches it. An operator following the docs today is hard-stopped: the documented attach path errors, and the documented hook model teaches an obsolete pre-bundle architecture.

The structural cause is a missing currency mechanism. The static layer (docs/, CLI surface) changes infrequently, so drift accrues unnoticed across many feature cycles until an operator hits it. Careful human review does not catch this class — the proof case is a documented command that *looks* correct line-by-line but fails only when run.

Two gaps compound:
1. **No detection** of executable-claim drift — no test exercises the commands the docs tell operators to run.
2. **No author** for `docs/`. The uni-docs agent definition is scoped "README.md only" and explicitly forbidden from touching anything else, so `docs/` (e.g. `client-setup.md`) has no owner in the delivery workflow. Drift there is nobody's job to fix.

## The Proof Case — GH #768

`docs/client-setup.md` is stale end-to-end and is the concrete wound this feature heals:

- **Obsolete model taught throughout.** The whole document teaches the pre-bundle attach model: hand-rolled `curl` shell hooks POSTing to a bare `POST /observe`, plus manual `claude mcp add ... -H` wiring. Reality: `/observe` shipped (vnc-022) and is now a per-slug route `POST /v1/{slug}/observe` (vnc-038); `init` wires the JS HTTP hook client automatically (no hand-rolled curl, no local binary required for the bundle path).
- **6 stale "Returns 501 until W2-7" claims** across the three client sections (the curl hook bodies and their follow-up "Note" callouts). W2-7 shipped; `/observe` does not return 501.
- **The README attach example mixes two valid modes** (see Approach / decision E below).

Defect attribution (scoping):
- README's other defects (2 of 3) already self-healed by intervening work — no action.
- `README:62` ONNX claim is **owned by #767** — cross-reference only, not fixed here.

## Goals

1. Establish a durable, minimal mechanism that catches **executable-claim** drift in product documentation automatically, before release.
2. Fix the #768 proof case: rewrite `docs/client-setup.md` to the current bundle/observe model and document both attach modes correctly in README + client-setup.
3. Close the `docs/` ownership gap by widening the uni-docs agent's authoring remit from README.md to all of `docs/` (scoped by blast radius, not full-tree audit).
4. Extend NFR **N5**'s governed surface from "deployable-as-released" to "usable-as-documented" — the doc-test is N5's regression guard, extended from the shipped image to the docs.

## Non-Goals

- **The `.claude/` automation-currency pattern (Feature 2).** A SEPARATE fast-follow feature. nan-020 does NOT design it. Its locked shape is recorded under "Follow-up: Feature 2" so it inherits the decision; nothing in nan-020 implements it. The single exception is the targeted uni-docs definition edit (Goal 3), which rides here because it is the authorship half of fixing #768 — flagged as the one `.claude/` touch in nan-020.
- **Generate-from-contract (option (c)).** Explicitly killed. Auto-generating docs from a contract is gold-plating for a small, slow-changing attach surface and cuts against the product principle that the static layer changes infrequently.
- **Fixing `README:62`'s ONNX claim** — owned by #767. Cross-reference only.
- **Auditing all of `docs/` every cycle.** uni-docs updates the surfaces a change *touches*, by blast radius — not a full-tree documentation audit.
- **A new bespoke CI job** for the doc-test. It MUST fold in as a sibling of the existing nan-019 release smoke (constraint below), not stand alone.
- **Minting a new NFR.** N5 is extended, not duplicated — avoid capability inflation.
- **Doc-testing the legacy `--remote <url> --token <tok>` mode (CONSCIOUSLY ACCEPTED GAP).** The doc-test exercises ONLY the canonical `--bundle` attach chain. `--remote` is DOCUMENTED — AC-02 still requires both modes documented correctly, and the docs MUST MARK `--remote` as legacy — but it is deliberately NOT doc-tested. **Human rationale (verbatim intent):** bundle is the only important/canonical mode; `--remote` is legacy, effectively unused, and will not be invested in; therefore verifying it earns nothing worth the boot/attach cost. This is a knowingly-accepted gap — the "seed of the next #768" risk, accepted with eyes open: `--remote` docs CAN drift undetected because nothing runs them. Goal 4 / N5's "usable-as-documented" framing therefore covers ONLY the canonical bundle chain — it MUST NOT be read as covering both modes.

## Background Research

### Existing mechanism to reuse (nan-019 release smoke)
The doc-test rides the proven nan-019 infrastructure at `product/test/infra-001/scripts/`:

- `docker-http-posture-smoke.sh` — builds/reuses the production image, boots it, registers a slug, and does a cert-pinned bearer POST to `https://localhost:PORT/v1/{slug}/observe`, asserting `204` and that the write lands in the per-slug store. **Critically: it `exit 3`s with a clear SKIP reason when Docker is absent** (lines 56-61) — it never false-greens itself.
- `release-gate-lib.sh::run_smoke_gate` — the gate wrapper that **turns `exit 3` into a HARD failure** (`::error::smoke SKIPPED (exit 3) ... HARD failure (SR-01)`, line 52) and asserts an anchored terminal run-marker so an early `exit 0` cannot pass. This is the exact hard-fail-on-no-Docker discipline nan-020 must adopt.

The doc-test's `--bundle` attach + `/v1/{slug}/observe` round-trip is a near-superset of what the existing smoke already does — the canonical attach path can be exercised in the same throwaway-container pattern.

### The two attach modes (resolved in code)
`packages/unimatrix/bin/unimatrix.js` (lines 17-25) plumbs TWO distinct, both-valid init modes:
- `--remote <url> --token <tok>` — legacy F3 direct attach.
- `--bundle <blob>` — vnc-034 dumb-client bundle attach. Bundle mode takes no `--slug`; the bundle blob encodes the slug (init.js:353) — verified during design (former OQ-A).

Neither flag is "wrong." The broken README example `init --remote unimatrix-bundle:<blob>` feeds a bundle blob to `--remote` and omits `--token`, so init errors "both required." The doc-test exercises the canonical `--bundle` path (vnc-038 dumb-client attach).

### The ownership gap (uni-docs)
`.claude/agents/uni/uni-docs.md` states scope as "**README.md only**" and "you do NOT modify `.claude/` files, protocol files, agent definitions, or per-feature documentation." Nothing in the workflow authors `docs/`. Widening this remit is the authorship half of the fix.

### Prior currency precedent
nan-005 established uni-docs and its delivery-protocol trigger (ADRs #1254-#1257). nan-020 extends that lineage from README to all of `docs/` and adds automated detection for executable claims.

## Proposed Approach

Split the documentation surface by **testability**:

- **Executable claims** — any line telling an operator to RUN a command — MUST be doc-tested. Detection of drift in these is owned by the doc-test (an executable round-trip), not by human judgment.
- **Narrative prose** — e.g. "what remote mode is" — gets manual rewrite plus a "verified on vX" stamp. No generation.

Authorship and detection are distinct and both required (not either/or):
- **uni-docs OWNS authorship** of `docs/` (widened remit) — it writes and maintains the prose and the executable examples.
- **The doc-test OWNS detection** of executable-claim drift — it fails CI when a documented command stops working. Detection ≠ authorship; do not conflate.

**Rationale to record (verbatim intent, decision E):** the truth wasn't "one flag is wrong" — it was "two modes exist and the example mixed them," which no amount of careful reading surfaces; only running it does. This is the strongest evidence for verification-over-judgment and the reason the doc-test exists.

### In Scope
1. **Rewrite `docs/client-setup.md`** to the current model: `init` + `--bundle` (vnc-034) wiring the JS HTTP hook client; telemetry via `POST /v1/{slug}/observe` (vnc-038); remove all 6 "Returns 501 until W2-7" claims and the hand-rolled-curl pre-bundle attach instructions.
2. **Document BOTH attach modes correctly** in README and client-setup: `--remote <url> --token <tok>` (legacy F3) AND `--bundle <blob>` (vnc-034, no `--slug`). Fix the broken `init --remote unimatrix-bundle:<blob>` example.
3. **Add a doc-test as a sibling of the nan-019 release smoke** (same `product/test/infra-001/scripts/` location, throwaway-container pattern, gated through `run_smoke_gate` or an equivalent that inherits its exit-code discipline). It exercises the canonical `--bundle` attach + a `/v1/{slug}/observe` round-trip end-to-end. **When Docker is absent it HARD-FAILs** — never silent-green.
4. **Widen the uni-docs agent definition** (`.claude/agents/uni/uni-docs.md`) to author/maintain all of `docs/`, scoped by blast radius (surfaces a change touches), not full-tree audit. This is the one `.claude/` edit in nan-020.

### Locked Design Decisions (resolved at scope lock)

- **D-1 (bundle source for the doc-test) — LOCKED: in-test emission.** The doc-test emits the connection bundle in-test by running the shipped CLI's bundle-emit command, `unimatrix client-bundle <slug>` (verified against the shipped CLI: a `ClientBundle { slug }` clap subcommand in `crates/unimatrix-server/src/main.rs`, a sync pre-tokio subcommand; the README "Serving projects" flow documents this exact form — confirm name at design if it has moved), run against the booted container, then feeds the emitted blob to `init --bundle`. **Rationale:** the doc-test then covers the bundle-emit command an operator actually runs, exercising the whole documented attach path rather than a pre-staged shortcut. A staged fixture would re-introduce the docs-vs-reality gap this feature exists to close. (Resolves former OQ-1.)
- **D-2 (script topology) — LOCKED: EXTEND `docker-http-posture-smoke.sh` in place; do NOT add a sibling script.** The `--bundle` round-trip is a near-superset of the existing per-slug-observe gate, so a separate script would duplicate boot/register/cert-pin setup at pure cost. Reuse-in-place is the smaller, cumulative-infra choice — consistent with the project test rule and satisfying C-1. **Caveat (design MUST honor):** design first checks the boot config does not genuinely diverge (e.g. a different image build); split to a sibling ONLY if it does. (Resolves former OQ-2; reflected in C-1.)
- **D-3 ("verified on vX" stamp) — LOCKED: a single footer line per doc-file, a pure authoring convention under uni-docs; NOT machine-checked.** **Rationale:** executable claims are already guarded by the doc-test (the load-bearing detection); auto-checking a prose stamp's freshness would be the gold-plating C-3 forbids. The stamp's value is human-signal, not a gate. (Resolves former OQ-3; reflected in C-3.)

## Acceptance Criteria

- **AC-01:** `docs/client-setup.md` contains zero occurrences of "501", "W2-7", or hand-rolled `curl ... /observe` hook scripts; it documents the `init`-wired hook client and the per-slug `/v1/{slug}/observe` route.
- **AC-02:** Both attach modes are documented correctly — `--remote <url> --token <tok>` (explicitly MARKED as legacy) and `--bundle <blob>` (the canonical mode; no `--slug`; the bundle blob encodes the slug, init.js:353 — verified during design, former OQ-A) — in README and client-setup; the previously-broken `init --remote unimatrix-bundle:<blob>` example is corrected to the canonical `--bundle` form. (Only the `--bundle` chain is doc-tested per AC-03 / C-7; `--remote` is documented-not-tested by conscious design.)
- **AC-03:** A doc-test exercises the canonical `--bundle` attach producing a successful `POST /v1/{slug}/observe` round-trip (write accepted, lands in the per-slug store) against a freshly built/booted image.
- **AC-04:** The doc-test is a SIBLING of the nan-019 release smoke — it lives under `product/test/infra-001/scripts/`, reuses the throwaway-container pattern, and is wired into the same release gate path. It is NOT a new bespoke CI job.
- **AC-05:** When Docker is unavailable, the doc-test causes the gate to HARD-FAIL (no silent skip / false-green) — verified by the same exit-3-is-fatal discipline as `run_smoke_gate` (SR-01 lesson #4796/#4970).
- **AC-06:** The doc-test asserts an anchored terminal run-marker so an early `exit 0` cannot pass the gate.
- **AC-07:** `.claude/agents/uni/uni-docs.md` remit is widened from "README.md only" to authoring/maintaining all of `docs/`, scoped by blast radius, with the full-tree-audit non-goal stated in the definition.
- **AC-08:** N5's governed surface is described as extended to "usable-as-documented" with the doc-test named as its docs-layer regression guard; no new NFR is minted. (Capability map update performed by the human after SCOPE locks.)

## Constraints

- **C-1 (load-bearing):** The doc-test MUST fold in with the existing nan-019 release smoke (`docker-http-posture-smoke.sh` / `release-gate-lib.sh` / throwaway-container pattern) — NOT a new bespoke CI job. **LOCKED form (D-2): EXTEND `docker-http-posture-smoke.sh` in place** (the `--bundle` round-trip is a near-superset of the per-slug-observe gate; a sibling would duplicate boot/register/cert-pin setup). Split to a sibling ONLY if design finds the boot config genuinely diverges (e.g. a different image build).
- **C-2 (load-bearing):** When Docker is absent the doc-test MUST hard-fail, never silent-green. A silently-skipped doc-test rebuilds the exact blind spot it exists to close (the #4796/#4970 false-green lesson).
- **C-3:** No generate-from-contract. Manual rewrite for prose; doc-test for executable claims. Minimal durable mechanism, no gold-plating. **LOCKED (D-3): the "verified on vX" stamp is a single footer line per doc-file, a pure authoring convention under uni-docs — NOT machine-checked.** Auto-checking the prose stamp's freshness would be exactly the gold-plating this constraint forbids; executable claims are already guarded by the doc-test.
- **C-4:** uni-docs authorship is blast-radius scoped, not full-`docs/` audit per cycle.
- **C-5:** Exactly one `.claude/` edit is in scope (the uni-docs remit widen). All other `.claude/` currency work is Feature 2.
- **C-6:** Extend NFR N5; do not mint a new NFR (avoid capability inflation).
- **C-7 (accepted-gap boundary):** The doc-test covers ONLY the canonical `--bundle` attach chain. The legacy `--remote <url> --token <tok>` mode is documented (AC-02) and MUST be MARKED legacy, but is deliberately NOT doc-tested. **Human rationale (verbatim intent):** bundle is the only important/canonical mode; `--remote` is legacy, effectively unused, and will not be invested in. This is a CONSCIOUSLY ACCEPTED gap — the "seed of the next #768" risk, accepted with eyes open: `--remote` docs can drift undetected. N5's "usable-as-documented" guard (Goal 4) is therefore scoped to the bundle chain alone and MUST NOT be read as covering `--remote`.

## Dependencies

- **nan-019** — release smoke infrastructure (`docker-http-posture-smoke.sh`, `release-gate-lib.sh`, exit-code/marker discipline). The doc-test is a sibling of this.
- **vnc-038** — per-slug routes `POST /v1/{slug}/observe`; the dumb-client attach the doc-test exercises.
- **vnc-034** — `--bundle` bundle attach mode (no `--slug`; the bundle blob encodes the slug).
- **vnc-022** — original `/observe` ship (context for the proof case).
- **#767** — owns the `README:62` ONNX claim (cross-reference; out of scope here).

## Capability Framing

- **Delivers capability C15 (runbook):** operators can attach a client by following the docs and have it work, verified by the doc-test.
- **Extends NFR N5:** from "deployable-as-released" to "usable-as-documented." The doc-test is N5's regression guard extended from the shipped image to the docs. EXTEND N5 — do NOT mint a new NFR.
- **Capability correction (scope reviewer):** N5 has already advanced past its corpus `done_when` via the now-closed #788. nan-020 therefore EXTENDS N5's governed surface (deployable-as-released → usable-as-documented) but does NOT change N5's status. Keep the "extend, don't mint" framing — no new capability is minted and N5's status is unchanged.
- The human updates the capability map once this SCOPE locks.

## Follow-up: Feature 2 (Out of Scope — recorded so it inherits the locked shape)

`.claude/` automation-currency pattern. NOT designed here. Locked shape for Feature 2 to inherit:
- **Discipline primary, NO standalone drift-checker.** Owner = delivery leader at Phase 4.
- **Catch-at-authoring:** the PR that changes an agent/skill/protocol contract updates its `.claude/` definition in the same PR.
- **Light retro backstop.**
- **The gate fires ONLY when the diff touches a `.claude/` referent**, not every PR.
- Rationale for the split: different surface / owner / urgency / altitude. Product-doc is a bleeding user-facing wound (operators hard-stopped today — C15); `.claude/` currency is an internal process change that benefits from reusing the doc-test pattern Feature 1 proves out. Bundling = one large mixed-altitude feature; don't.

## Open Questions

None — all resolved at scope lock. (Former OQ-1/OQ-2/OQ-3 are now locked decisions D-1/D-2/D-3 above, reflected in C-1 and C-3.)

## Tracking
https://github.com/dug-21/unimatrix/issues/768
