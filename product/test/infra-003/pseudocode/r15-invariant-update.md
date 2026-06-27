# R-15 — New-smoke-script invariant update (#815, in-PR lockstep)

> Source: IMPLEMENTATION-BRIEF "Files to Create/Modify" + Delivery Action 2,
> RISK R-15. NOT a C1–C7 runtime component — a **delivery action** that ships in
> the **same PR** as the new script, cross-linked on #815. Documented here as
> pseudocode so the implementation agent makes the exact edit and the validator
> can check it.

## Purpose

ADR-001 adds a new top-level smoke script
`product/test/infra-001/scripts/multi-tenant-isolation-smoke.sh`. An existing
invariant test enforces a **closed allow-list** of smoke scripts and goes RED when
an unaccounted `*smoke*.sh` appears (open via #815 since #810 added a second
script). The new script matches the `*smoke*.sh` glob, so it WILL trip the
invariant unless registered in lockstep. Register it **while keeping the guard's
teeth** against future unaccounted scripts.

## Affected file (MODIFY — not a crates/ change)

`product/test/infra-001/scripts/release-gate-bundle-static-test.sh`

The invariant is `test_no_new_smoke_script()` (lines ~200-226). It builds the
on-disk set via `ls "$SCRIPT_DIR"/*smoke*.sh | grep -v 'stub-smoke.sh'` and asserts
it **equals** the `KNOWN_SMOKE_SCRIPTS` allow-list exactly (no unknown additions,
no missing knowns). Current declaration (lines ~196-199):

```
KNOWN_SMOKE_SCRIPTS=(
  docker-http-posture-smoke.sh   # #783 — the HTTP-posture smoke (Gates 1–8)
  docker-embed-readiness-smoke.sh # #767 — first-boot embedding-readiness smoke
)
```

## The edit (pseudocode — exact, single array entry)

```
EDIT KNOWN_SMOKE_SCRIPTS array: append one line registering the new script with a
rationale comment carrying the issue number (mirror the existing two entries' form):

  KNOWN_SMOKE_SCRIPTS=(
    docker-http-posture-smoke.sh   # #783 — the HTTP-posture smoke (Gates 1–8)
    docker-embed-readiness-smoke.sh # #767 — first-boot embedding-readiness smoke
    multi-tenant-isolation-smoke.sh # #853 — infra-003 bidirectional multi-tenant isolation gate
  )
```

No change to `test_no_new_smoke_script()` logic — only the allow-list data. The
assertion mechanics (unknown-set must be empty AND missing-set must be empty) are
preserved, so:
- the new script is now a **known** entry → no FORK-smell RED;
- a future unaccounted `*smoke*.sh` STILL trips (the guard keeps its teeth);
- if `multi-tenant-isolation-smoke.sh` is ever removed but left in the list, the
  `missing` branch still goes RED (no silent removal).

## Delivery choreography (Delivery Action 2 — NOT executed by this script)

1. The same delivery PR that adds `multi-tenant-isolation-smoke.sh` also makes the
   one-line `KNOWN_SMOKE_SCRIPTS` edit above (in-PR lockstep; not a follow-up).
2. Post an issue comment on **#815** cross-linking the PR and recording that the
   invariant was extended (and #815's intent closed) in this change.
3. Confirm the new script honors the verify-by-name / exit-code contract the
   invariant family enforces (#5180) — terminal `ALL GATES PASSED` only on GREEN;
   SKIP=3, RED=1, INFRA distinct, never exit 0 on a non-pass (see C7).

> Companion delivery action (R-16, not a code edit): post a durable comment on
> **#788** requiring N5 to adopt this gate into the recurring lane, and word the
> capability evidence as "advances, does not close N3" (NFR-04). Tracked here for
> completeness; it is a GitHub linkage, not a file change.

## Error Handling

| Condition | Outcome |
|-----------|---------|
| script added but allow-list NOT updated | invariant RED (FORK smell) — the exact trap R-15 prevents |
| allow-list updated, script present | invariant PASS, guard intact |
| allow-list lists a script not on disk | invariant RED (missing) — protects against silent removal |

## Key Test Scenarios

1. After adding the script AND the allow-list line, `test_no_new_smoke_script`
   passes with the new script present (R-15 sc.1).
2. The invariant still fails if an *unaccounted* future `*smoke*.sh` is added
   (teeth retained) (R-15 sc.2).
3. The new script honors the verify-by-name / exit-code contract (#5180)
   (R-15 sc.3).
4. The allow-list edit and the new script land in the **same PR**, cross-linked on
   #815 (delivery/validator check).
