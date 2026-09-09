---
name: "collective-participation"
description: "Report this Program's collective participation: live-resolve its collective identity and adopted baseline release from the local .collective/ surface, with a Program-local integrity cross-check of the identity. Read-only, deterministic, offline; resolves everything at invocation and bakes in nothing."
---

# Collective Participation

This Program participates in the collective. The authoritative surface is `.collective/`, which is
collective-authoritative and not edited locally. This skill answers one question, honestly, every time
it runs:

> What is this Program's collective identity, and which baseline release has it adopted?

It is **reporting-only**. It reads two facts from live state and returns them, plus a Program-local
integrity cross-check of the identity. It performs no network access, changes no file, and carries no
effect beyond reporting.

## Invariants (why the answer can be trusted)

- **Live at invocation.** Every value is read from the `.collective/` files at the moment the skill
  runs. Nothing is compiled into this skill. Editing a copy of a fact stored anywhere else cannot
  change what this skill returns.
- **No baked-in values.** This skill contains no identity string, no release version, and no digest.
  If you see a concrete value in the output, it came from a live file read on this run.
- **Offline.** No network. The skill reads only files under this Program's own `.collective/` surface
  and never traverses outside it.
- **Fail honestly.** When the surface is absent, unadopted, or unreadable, return an explicit
  unresolved result — never a false positive.
- **Not self-referential.** The identity cross-check is against a *different* file from the one the
  identity is read from, and that different file is integrity-bound to the projection manifest. The
  authoritative registry and release-index cross-checks are performed collective-side, not here.

## Step 1 — Live read the two facts

Read `.collective/ADOPTED.yaml` (`collective.adopted-release/1`).

- If the file does **not** exist, stop and return:
  `{ "resolved": false, "reason": "surface-absent-or-unadopted" }`
- Parse it strictly. It must have exactly these keys and no others:
  `schema` (= `collective.adopted-release/1`), `authority` (= `baseline`), `program_identity`, and
  `adopted_release` with exactly `release_identity` and `content_digest`.
- If it is missing, malformed, has extra keys, or `authority` is not `baseline`, stop and return:
  `{ "resolved": false, "reason": "record-unreadable" }`

From the parsed record take:

- `identity = program_identity`
- `baseline = adopted_release` (i.e. `{ release_identity, content_digest }`)

These two are the reported facts. `content_digest` is a **string** — report it as read; do not
recompute it (see Step 3).

## Step 2 — Program-local identity cross-check

Cross-check the identity against a **distinct** file — never `.collective/ADOPTED.yaml` against itself.
The result of this step is `local_identity_crosscheck`, either `PASS` or `FAIL(<reason>)`. On any
failure here, still return the live facts from Step 1; only the cross-check is marked `FAIL`.

**(a) Membership.** Read `.collective/contract/REGISTRATIONS.yaml`
(`collective.registration-set/2`). It lists `programs`, each with a `program_identity`.

- If the file is absent or malformed: `local_identity_crosscheck = FAIL("registrations-unreadable")`.
- If `identity` is **not** among `programs[].program_identity`:
  `local_identity_crosscheck = FAIL("identity-absent-from-registration-set")`. A forged identity fails
  here.

**(b) Integrity binding.** The registration file only counts as an independent side if it is the
genuine projected copy. Bind it to the projection manifest:

- Read `.collective/MANIFEST.yaml` (`collective.projection-manifest/1`); it lists `files`, each with a
  `path`, a `digest` (`sha256:<hex>`), and an `authority`.
- Find the row whose `path` is `.collective/contract/REGISTRATIONS.yaml`. If there is no such row:
  `local_identity_crosscheck = FAIL("registrations-not-manifest-bound")`.
- Compute the SHA-256 of the exact bytes of `.collective/contract/REGISTRATIONS.yaml` and compare it,
  as `sha256:<hex>`, to that row's `digest`. If they differ:
  `local_identity_crosscheck = FAIL("registrations-not-manifest-bound")`. A co-mutated registration
  file fails here.

**Pass.** Only if (a) membership holds **and** (b) the manifest binding holds:
`local_identity_crosscheck = PASS`.

## Step 3 — Baseline: report, do not verify locally

Report `baseline.content_digest` as the string read in Step 1. Do **not** attempt to recompute or
verify it locally: the `.collective/` surface does not carry the release-level and clearance inputs
needed to reproduce that aggregate, so no local recomputation is possible and none is claimed here.

The authoritative baseline check — matching this digest **string** against the collective's own release
index — is done collective-side and bound to this invocation's returned value by a separate observation.
This skill makes no self-validation claim about the baseline.

## Step 4 — Return

On a clean resolution, return exactly:

```
{
  "resolved": true,
  "program_identity": "<identity from Step 1>",
  "adopted_release": {
    "release_identity": "<release_identity from Step 1>",
    "content_digest": "<content_digest string from Step 1>"
  },
  "local_identity_crosscheck": "PASS"   // or "FAIL(<reason>)"
}
```

A resolution is only clean when the live read (Step 1) succeeds **and** the cross-check (Step 2) is
`PASS`. A `PASS` is never producible from `.collective/ADOPTED.yaml` alone — it requires the distinct,
manifest-bound registration file to agree.

## Fail-closed summary

| Condition | Result |
|---|---|
| `.collective/ADOPTED.yaml` absent | `{ resolved: false, reason: "surface-absent-or-unadopted" }` |
| `.collective/ADOPTED.yaml` malformed / wrong schema / `authority != baseline` | `{ resolved: false, reason: "record-unreadable" }` |
| Registration file absent/malformed | facts returned; `local_identity_crosscheck = FAIL("registrations-unreadable")` |
| Identity not in the registration set | facts returned; `FAIL("identity-absent-from-registration-set")` |
| Registration file not bound to the manifest, or manifest absent | facts returned; `FAIL("registrations-not-manifest-bound")` |
| Live read succeeds and cross-check binds | `{ resolved: true, ... , local_identity_crosscheck: "PASS" }` |

This skill never guesses, never reads outside `.collective/`, never reaches the network, and never
returns a positive when a control is absent or a cross-check does not bind.
