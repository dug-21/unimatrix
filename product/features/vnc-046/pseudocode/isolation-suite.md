# Component: isolation-suite (behavioral INV-T/K/C)

**Source:** `tests/` — extend `project_routing_integration.rs`; reuse the #800 multi-slug HTTP fixture
**ADR:** ADR-004 · **AC:** AC-01…AC-07 · **Risks:** R-01, R-02, R-08, R-09, R-10, R-15, R-16

> The `uni-tester` (Stage 3a) owns the authoritative test plan. This file is the **pseudocode-level
> structure** the implementation follows: the bidirectional N≥2 shape, assembled-wiring rule, and the
> mandatory coverage-enumeration table. It is a hint set, not the test plan.

## Purpose

The durable, solution-independent guardrail: prove every isolation invariant through the public
`/v1/{slug}/...` interface, **bidirectionally at N≥2**, against assembled production wiring. Any future
rewire that breaks cloud isolation trips these.

## Non-Negotiable Suite Properties (AC-06)

- **N≥2:** register slugs A and B on ONE cloud instance (N=1 cannot distinguish a real funnel from a
  global-handle bypass — #5172/#4974).
- **Assembled production wiring only:** POST `transcript_delta` via `route_observe` → read via that
  slug's `McpAdapter`. **Never** construct a `SessionRegistry` and hand it into `dispatch_request`
  (R-02 — that structurally hides the split-brain). No `Arc::ptr_eq`, no field-overwrite in this crate
  (those are white-box, live in `boot-assertion.md`).
- **Bidirectional per data class:** for each invariant, TWO named cases — (A-writes → present-in-A AND
  absent-in-B) and (B-writes → present-in-B AND absent-in-A). Neither inferred from the other (R-01;
  a one-directional probe false-GREENs the reverse mis-route — lesson #5348).
- **Isolation half keys on synchronous observable state** (fold result / returned entries), not the
  absence of an async effect (#5427 caveat).

## Fixture (reuse #800 — do NOT fork; SR-08 / OPEN QUESTION 4)

```
FIXTURE two_slug_cloud():
    // Extend the #800 multi-slug HTTP fixture — confirm owner before building INV-C variants.
    register slug A and slug B on one instance (each with its own store + declared config)
    RETURN handle exposing per-slug: post_observe(slug, hook) and mcp_call(slug, tool, args)
```

## Test Module Structure (`project_routing_integration.rs`)

```
MODULE per_slug_isolation:
    // Helper: bidirectional driver so each case is explicit, not looped-away.
    FUNCTION assert_bidirectional(write_class, read_surface, present_check, absent_check):
        // A-driver
        post write_class to A via /v1/A/observe
        ASSERT present_check(read A)           // fidelity-in-own
        ASSERT absent_check(read B)            // isolation-in-other
        // B-driver (symmetric — the reverse mis-route guard)
        post write_class to B via /v1/B/observe
        ASSERT present_check(read B)
        ASSERT absent_check(read A)
```

### INV-T1 — transcript fidelity (AC-01, #930)  [fidelity-only, both slugs]
```
post transcript_delta to /v1/A/observe under cycle X
ASSERT cycle_review(A, X) folds it (non-empty candidates / transcript bytes)
repeat for B  // both slugs prove own-fold
```

### INV-T2 — transcript isolation under IDENTICAL cycle name (AC-02; R-10, R-15)
```
A and B both run the identical {phase}-{NNN} name (e.g. nxs-001)
A-driver: post to A; ASSERT cycle_review(A) folds A's; ASSERT cycle_review(B) folds/counts/DISTILLS none of A's
B-driver: symmetric
ASSERT both candidate COUNT and DISTILLATION INPUT exclude the other slug (not just returned bytes)
// R-15 persistence: after collision, ASSERT B's persisted store never contains A's transcript-derived entries
```

### INV-T3 — pending-entries isolation (AC-03)
```
drive pending-entries state for A via A's surface
ASSERT present at cycle_review(A), absent at cycle_review(B); swap roles
```

### INV-K1/K2 — knowledge read fidelity + isolation (AC-04; R-09, R-15)
```
write distinct knowledge as A and as B
A-driver: ASSERT A's observe-path briefing/search/compact returns A's AND returns NONE of B's
B-driver: symmetric
// persistence: ASSERT the durable store is not contaminated via distillation (R-15)
```

### INV-C1/C2 — config parity + isolation (AC-05; R-08)
```
register A and B with DIFFERENT declared config (via resolve_slug_config → build_project_server; NEVER seed)
signal_class_names → drive each slug a SIGNAL-BEARING delta matching its OWN declared class pattern;
   ASSERT cycle_review(A) signal_class_counts are NON-ZERO for A's classes (not just names) and B's
   reflect B's — a signal-bearing driver is required so an empty per-slug scanner (OQ-2) fails here
observation_registry → ASSERT status surface shows A's observation categories, not B's
retention_config → ASSERT A's purge/held-buffer behavior follows A's retention; B's follows B's
```

## Coverage Enumeration Table (MANDATORY — AC-06; absence is a gate failure, SR-05)

The suite ships this table as a comment/const so gaps are visible, not implied:

| Invariant | AC | Coverage | Bidirectional | Notes |
|---|---|---|---|---|
| INV-T1 | AC-01 | behavioral | both slugs (fidelity) | #930 core |
| INV-T2 | AC-02 | behavioral | yes | identical cycle name; count + distillation exclusion |
| INV-T3 | AC-03 | behavioral | yes | pending-entries |
| INV-K1/K2 | AC-04 | behavioral | yes | + persistence-level (R-15) |
| INV-C1/C2 (`signal_class_names`, `observation_registry`, `retention_config`) | AC-05 | behavioral | yes | public surfaces |
| INV-C (`store_config` byte-limit) | AC-05/AC-08 | **white-box only** | yes | documented AC-06 exception — wiring-pin unit (boot-assertion.md), R-04 |
| INV-C (`inference_config` briefing blend) | AC-05/AC-08 | **white-box only** | yes | documented AC-06 exception — wiring-pin unit, R-04 |

The two white-box-only rows MUST be named explicitly — never silently omitted.

## Meta-Test (R-01 negative control — recommended)

A throwaway harness variant that deliberately mis-wires B's route to resolve A's registry → assert the
bidirectional suite goes **RED**, confirming the reverse direction is actually exercised (guards against
a suite that only ever writes A).

## AC-07 Parity (HTTPS == UDS; R-16)

Drive the SAME input through HTTPS (`route_observe` → `McpAdapter`) and local UDS; compare
`cycle_review` fold field-for-field, normalizing/excluding wall-clock fields (`computed_at`) to avoid
flake (#5285). Confirm no change to UDS/stdio construction paths (NFR-4).

**Signal-bearing input required (OQ-2 anti-fake-green, R-16).** The parity input MUST be a
**signal-bearing** transcript against a slug that declares `[transcript_signals]` — i.e. a delta that
matches a declared class pattern so `signal_class_counts` are **non-zero** on the UDS leg. An
empty/no-match input yields `signal_class_counts == {}` on both legs and would pass even with an empty
per-slug HTTPS scanner (the OQ-2 defect) — a false green. Assert both legs agree **and** that the
matched-class count is > 0, so the comparison actually exercises the per-slug scanner wired in
`project-provisioner.md` P1. This is the parity half of the non-zero-count regression guard.

## Constraints

- External `tests/` crate; extend existing fixtures (cumulative infra). Respect #878 `--jobs 1` link
  discipline for the large server test binaries.
- No `Arc::ptr_eq` / field-overwrite in this crate (AC-06).

## Key Test Scenarios (summary)

All AC-01…AC-05 invariants: two named cases each (A-driver, B-driver), fidelity-in-own +
absence-in-other, via `route_observe` → `McpAdapter`, N=2. Plus AC-06 coverage table, AC-07 parity,
AC-09 field-absence compile/diff. AC-08/white-box pins live in `boot-assertion.md`.
