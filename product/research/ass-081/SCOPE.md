# ass-081 — Is D6 isolation parity fixable over UDS, or an inherent measurability exception?

## Problem Statement

#845 (filed as a `bug`) reports that the nan-022 cross-transport parity matrix classifies
the **isolation (D6)** dimension as a stable `PARITY_FAIL` because the UDS leg reports
`slug_a_writes_visible_to_b = true` — a confirmed **harness measurement artifact**, not a
real cross-tenant leak. The HTTPS leg measures D6 correctly (real two-slug container);
the UDS leg's `daemon_server` fixture is single-slug (one daemon, one `--project-dir`, one
store), so a slug-B-hinted `context_search` reads slug A's own store.

The issue is internally inconsistent about its own disposition: it simultaneously calls
the gap **"directly analogous to the D5 PreCompact host-side documented gap"** (which
nan-022 treats as a human-signed *documented exception*, not a defect) **and** prescribes
a **"Fix": make the UDS isolation probe genuinely two-slug**. Those imply opposite
dispositions. Whether D6-over-UDS is a fixable test-infra defect or a structural
measurability exception is **undetermined**, and it bears directly on the C0 (#5304) flip
bar (SCOPE Open Q6: all six dimensions block, with a documented-exception escape valve for
a legitimately unreachable dimension).

This spike resolves that disposition before #845 is actioned as a bug.

## Goal

1. **Can the in-process UDS leg host a genuine second slug/store** such that a write to
   slug A is read back from slug B's *own* store — i.e. can the UDS leg symmetrically probe
   the same on-disk per-slug isolation property the HTTPS leg already proves? Specifically:
   does the in-process daemon used by `daemon_server` (`harness/conftest.py`) support
   multi-slug routing (a second registered slug, or a sibling per-slug store), or is it
   single-store *by construction* in the way the harness drives it?
2. **If feasible:** what is the smallest faithful change to the UDS fixture that makes the
   D6 probe genuinely two-slug (spawn/register a second slug vs. sibling per-slug store),
   and does it preserve the "extend infra-001, never fork / no production-code change"
   nan-022 constraints? Rank the candidate approaches.
3. **If not feasible (or only via production change or a non-faithful contortion):** is the
   correct disposition a **human-signed documented UDS-measurability exception** for D6
   (the D5 pattern) — i.e. measured-where-drivable (HTTPS) + documented UDS gap — and is
   that consistent with the per-slug routing architecture (#783 / vnc-034)?
4. **Disposition recommendation:** should #845 be kept as a `bug` (fixable, with the ranked
   fix path), or reclassified — closed as a bug and folded into nan-022 SCOPE as a D6/UDS
   documented exception alongside D5?

## Breadth

`code-only`. The answer lives in the nan-022/infra-001 harness (`harness/conftest.py`,
`harness/parity_legs*.py`), the per-slug store routing code (#783 / vnc-034), and how the
in-process UDS daemon is launched and given its project dir/slug. No ecosystem or
literature input required.

## Approach

`investigation` (code-anchored). A minimal local `proof-of-concept` probe is **permitted
and encouraged** to substantiate a "feasible" verdict — e.g. demonstrating a second
in-process slug/store reachable over the UDS MCP connection — but a full fixture
implementation is OUT of scope (that is the eventual fix, not the spike).

## Confidence required

`directional`. The deliverable is a disposition recommendation the human acts on, not a
shipped fixture. A "feasible" verdict should be backed by concrete code evidence (the
routing seam that makes it possible) rather than assertion; an "infeasible / exception"
verdict must show *why* the in-process UDS path is structurally single-store as driven.

## Target outputs

FINDINGS.md containing:
- A **go/no-go on UDS two-slug measurability** (Goal 1) with the specific code evidence.
- If go: **ranked fix approaches** (Goal 2) with the constraint-compliance check.
- If no-go: a **documented-exception recommendation** (Goal 3) with the architectural
  justification and the exact framing for the nan-022 SCOPE / C0 flip session.
- A **clear disposition for #845** (Goal 4): keep-as-bug (with fix path) OR
  reclassify-as-documented-exception.

## Constraints

**Hard** (fixed; changing requires rewriting shipped code or violating nan-022 charter):
- nan-022 is **test-only** — no production-code change. A fix that requires changing
  server/store routing code is NOT a test-infra fix and changes the disposition.
- **Extend infra-001 cumulatively** — no fork, no parallel scaffold (nan-022 AC-11).
- Per-slug store routing as shipped in #783 / vnc-034 is the ground truth; the UDS leg
  must measure the *same* on-disk isolation property the HTTPS leg measures.
- The spike is **read-only in Unimatrix** and writes **no production code** — PoC probes,
  if any, are throwaway and not committed as fixture changes.

**Hypothesis** (design positions held, challengeable by the researcher):
- That D6-over-UDS is "directly analogous to the D5 documented gap" (#845's claim). The
  D5 gap is host-side (cannot drive a live Claude-Code host); the D6 gap is store-routing
  in an in-process daemon. The researcher should test whether the analogy actually holds
  or whether D6 is materially *more* fixable than D5.
- That a second slug cannot be hosted in the in-process UDS fixture. #845 asserts the
  fixture *is* single-slug, but does not establish it *must* be — challenge this.

## Dependencies

- **Input / prior art:** #845, nan-022 SCOPE (`product/features/nan-022/SCOPE.md`,
  esp. the D6 row and Open Q6), and the merged nan-022 parity suite (#837 / commit
  `6a8282d3`). Consumes the infra-001 harness verbatim as the object of study.
- **Unblocks:** the #845 disposition decision, which in turn unblocks (or documents an
  exception for) the D6 dimension of the C0 (#5304) flip.

## Prior art

- **#845** — root cause confirmed: `feature=` is a query hint, not store routing; a
  slug-B-hinted search returns slug A's marker (`ids=[1]`) against the single-slug fixture.
- **nan-022 SCOPE** — D6 row ("per-slug store routing #783, vnc-034; already gated in
  posture smoke Gates 1–4"); Open Q6 (all six dimensions block C0, documented-exception
  escape valve); the D5 PreCompact host-side documented-gap precedent (Open Q3).
- **#783 / vnc-034** — per-slug db routing shipped; writes land in
  `/data/.unimatrix/<slug>/` not the path-hash dir. The HTTPS leg already exploits this for
  a real two-slug measurement.
- Harness entry points: `harness/conftest.py` (`daemon_server` fixture),
  `harness/parity_legs_capture.py` (`capture_isolation`), `harness/parity_legs.py`
  (`drive_uds_leg`).

## Tracking

GH Issue **#845** (this spike links to and resolves the disposition of that issue; the
issue stays open until the human acts on the recommendation).
