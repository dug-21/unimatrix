# ass-084 — Production per-slug observe route-path structure, and can the Python harness emulate two-route isolation?

## Problem Statement

There are **four** transport surfaces, not two: the MCP path over {local-UDS, HTTPS} and
the observe/hook path over {local-UDS, HTTPS}. Three are vetted within the existing
infra-001 harness. The **fourth — the HTTPS observe path's cross-tenant isolation — is
untested**, and the gap is silent: if the server ever mis-routed absorbed observe data
across slugs, no test would catch it; it would surface only as anomalous agent behaviour
in production.

This is **not** a parity-with-local question (the local observe path is single-route by
the ADR-006 compile-time guard — there is no second local tenant to be "at parity" with).
It is a standalone **server-behaviour** question: the server absorbs observe data on a
per-slug HTTPS route **by design**, and must keep what it absorbs isolated to that slug's
own store (DB / vector / hash-chain). To test that, the harness must be able to push to
**two separate routes, as production does** — which it currently cannot (one-client /
single-route by deliberate design).

ass-081 (the D6-isolation spike, `product/research/ass-081/`) answered the narrow
mechanical question — "can a second store be reached over UDS?" → yes, spawn a second
daemon — but treated it as a small cumulative fix and under-weighted the design intent.
This spike replaces that framing with the right one: **understand the production route-path
structure first, then determine whether the harness can faithfully emulate two of those
routes.** If it can, the answer is to update the harness with the proper fixture/setup so
the isolation property is measured, not assumed.

## Goal

1. **Document the production per-slug observe route-path structure.** How does a registered
   slug become an HTTPS observe route (path-hash / funnel structure)? What is the server's
   route → identity → per-slug-store resolution seam — the shared, transport-agnostic
   dispatch (`dispatch_request`, knowledge #4691) where absorbed data is routed to a store?
   Define concretely what **"two routes in production"** looks like end to end: two
   registrations, two route paths, the identity each carries, and the two stores each must
   land in (and never cross into).

2. **Analyse the infra-001 Python harness against that structure.** Can it drive **two
   registered slugs over two distinct HTTPS observe routes** and probe each per-slug store
   independently, faithfully mirroring production? Identify the precise gap between its
   current single-route / one-client-emulates-local design and the two-route production
   topology — what specifically is single-route by construction vs. by current wiring.

3. **If faithfully emulable:** specify the **smallest faithful harness change** — the
   fixture/setup needed (two-slug registration, two-route drive, per-store assertions) —
   within the "extend infra-001 cumulatively, test-only, no production-code change" charter.
   Rank candidate approaches. This is the recommendation the human acts on.

4. **If not faithfully emulable** (only via production change, or via a contortion that no
   longer mirrors production routing): name the blocker precisely, state the alternative,
   and assess whether a human-signed documented exception is then warranted — and what the
   UDS-side guarantee (the ADR-006 single-route compile-time guard) contributes as the
   non-silent proof on the local leg.

## Breadth

`code-only`. The answer lives in (a) the server's per-slug observe routing / store
resolution (the shared `dispatch_request` seam, per-slug store routing from #783 / vnc-034,
the HTTPS `/observe` funnel) and (b) the infra-001 harness (`harness/conftest.py`,
`harness/parity_legs*.py`, `harness/parity_legs_capture.py`, the HTTPS leg scripts). No
ecosystem or literature input.

## Approach

`investigation` (code-anchored). A throwaway local proof-of-concept is **encouraged** to
substantiate a "feasible" verdict — e.g. registering a second slug and demonstrating two
distinct observe routes resolving to two distinct stores through the shipped server — but a
full fixture implementation is OUT of scope (that is the eventual delivery, not the spike).

## Confidence required

`directional`. The deliverable is a feasibility verdict + recommended fixture/setup the
human acts on, not a shipped fixture. "Feasible" must be backed by the concrete routing-seam
evidence that makes two faithful routes reachable from the harness; "infeasible" must show
*why* the harness cannot mirror two production routes without changing shipped code or
abandoning production-fidelity.

## Target outputs

FINDINGS.md containing:
- **The production route-path structure** (Goal 1): registration → route path → identity →
  per-slug store, with the exact resolution seam named, and a concrete picture of two
  production routes.
- A **go/no-go on faithful two-route harness emulation** (Goal 2) with the specific gap and
  code evidence.
- If go: **ranked fixture/setup changes** (Goal 3) with the charter-compliance check — the
  recommendation being "update the harness to accommodate two-route isolation testing."
- If no-go: the **blocker + alternative** (Goal 4), including whether a documented exception
  is warranted and what the UDS single-route guard proves on the local leg.

## Constraints

**Hard** (changing these means changing shipped code or abandoning production-fidelity):
- **Test-only.** A change that requires modifying server / store-routing code is NOT a
  harness fix and shifts the disposition.
- **Extend infra-001 cumulatively** — no fork, no parallel scaffold (nan-022 AC-11).
- The harness must drive the **same** route topology production uses — two real registered
  slugs over two real HTTPS observe routes, each landing in its own per-slug store. A probe
  that does not mirror production routing (e.g. the ass-081 `feature=`-hint read, which is a
  query hint, not store routing) does not count as measuring isolation.
- Read-only in Unimatrix; any PoC is throwaway and not committed as a fixture change.

**Hypothesis** (positions to test, not assume):
- That the harness is single-route **by construction** vs. merely by current wiring —
  ass-081 asserted single-slug but did not establish it is *necessarily* so. Challenge it.
- That two faithful HTTPS observe routes are reachable from the harness given the shipped
  multi-slug container (the HTTPS leg already runs a real multi-slug container; the open
  question is driving and probing *two* of them, not one).

## Dependencies

- **Input / prior art:** ass-081 FINDINGS (`product/research/ass-081/`) and #845 (the D6
  false-RED); nan-022 SCOPE (`product/features/nan-022/SCOPE.md`, D6 row + Open Q6); the
  merged nan-022 parity suite (#837). Consumes the infra-001 harness as the object of study.
- **Architecture ground truth:** per-slug store routing (#783 / vnc-034); the
  transport-agnostic `dispatch_request` seam (knowledge #4691); the ADR-006 local
  single-route compile-time guard.
- **Capability context:** the isolation security property is **N3 (#5161 — writes never
  mis-routed across projects)**, currently `partial`; this spike unblocks a faithful N3
  behavioural test. It is **not** a C0 (#5304) cross-transport parity dimension (no local
  two-route analog).
- **Unblocks:** the #845 disposition; the decision to build (or not) the two-route HTTPS
  observe isolation test under N3; and the honest framing of D6 for the C0 flip session.

## Prior art

- **ass-081** — established a second store is reachable over UDS by spawning a second
  daemon, but framed isolation as near-exception and did not model the production route
  topology. This spike sharpens that into a route-structure + harness-fidelity question.
- **#845** — root cause: `feature=` is a query hint, not store routing; the single-slug
  fixture cannot symmetrically probe cross-slug visibility.
- **#783 / vnc-034** — per-slug stores land in `/data/.unimatrix/<slug>/`, not the path-hash
  dir; the HTTPS leg already exploits this for a one-sided isolation check.
- **knowledge #4691** — `dispatch_request` is transport-agnostic; per-slug store resolution
  is parameterised by the request identity — the exact seam isolation hinges on.

## Tracking

GH research issue (ass-084). Cross-references #845 (whose disposition it informs) and the
N3 isolation test it unblocks. Single spike → execute via `uni-spike-researcher` once this
scope is confirmed complete.
