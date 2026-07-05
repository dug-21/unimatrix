# Component 5 — readme-integrity

**File:** `README.md` (repo root). Same PR as the code (C-08).
**Source of truth:** ADR-002 (weak-mode threat boundary), SPEC FR-11, ARCHITECTURE §System Overview threat model.
**Traces:** FR-11; AC-06, AC-07; R-11; C-08.

## Purpose

Correct the README's integrity claim from the overclaimed "tamper-**evident**" to the truthful
tamper-**recorded** / correction-history-integrity guarantee (ADR-002), WITHOUT under-selling the integrity
that actually ships (per-entry `content_hash`, append-only audit, authoritative supersession chain, and — now
that nxs-014 lands — a populated cross-version `previous_hash` chain verifiable at import and on demand).

This is doc-only; no code. It is a required part of the SAME PR (C-08). Two occurrences of the overclaim exist
today — BOTH must change.

## The claim boundary to state (fixed by ADR-002 — do not re-drift, R-11)

- **Defended (detectable):** accidental corruption and single-point / API-surface tampering — a break in a
  populated hash chain, or a content mutation that is not perfectly mirrored across the entry's `content_hash`
  AND its successor's `previous_hash`, is detected by `verify_entries` (import + `unimatrix verify` CLI).
- **NOT defended (out of tier):** a root / raw-DB-write adversary who owns the bearer token and all secrets
  and coordinates edits across an entry and its successor — explicitly out of scope (Non-Goal 3). The chain is
  tamper-**recorded**, not tamper-**evident**, against that adversary.
- Strong cryptographic cascade + external HEAD anchor (true tamper-evidence vs a DB-write adversary) are the
  north-star, NOT shipped here — do not imply them.

## Edit 1 — `README.md:235` (§Correction Chains with Audit Trails)

Current last sentence:
> Correction chains are tamper-evident: any break in the hash chain is detectable.

Replace with wording that states the tamper-RECORDED boundary and names what IS/ISN'T defended. Intent (dev
writes final prose):
> Correction chains are tamper-**recorded**: every correction links to its predecessor by SHA-256
> `content_hash` (the `previous_hash` chain), and `unimatrix verify` (and import-time validation) detect
> accidental corruption or single-point tampering — a content edit not perfectly mirrored across an entry's
> `content_hash` and its successor's `previous_hash` fails verification, naming the offending entry. This is
> correction-history integrity, not tamper-evidence against an adversary with raw database write access (who
> holds all secrets — out of tier); cryptographic tamper-evidence against that adversary is a future
> hardening step.

Keep the preceding true sentences about `context_correct`, the `previous_hash` chain, and the append-only
audit log intact (do not under-sell — AC-06/AC-07).

## Edit 2 — `README.md:722-724` (§Hash-Chained Corrections)

Current:
> ### Hash-Chained Corrections
> SHA-256 content hashes with `previous_hash` links create tamper-evident correction chains. Any break in the
> chain is detectable.

Replace the claim with the same tamper-recorded boundary; add that verification is now runnable. Intent:
> ### Hash-Chained Corrections
> Each correction links to the entry it supersedes via SHA-256 `content_hash` (the `previous_hash` chain), so
> the correction history is tamper-**recorded**: `unimatrix verify` and import-time validation recompute every
> entry's content hash and check each chain link, failing loud and naming any entry whose content or link is
> inconsistent. This detects accidental corruption and single-point API-surface tampering; it does not defend
> against a coordinated raw-database-write adversary (out of tier — that requires a cryptographic cascade and
> external anchor, tracked separately).

## Scan requirement (AC-06 — no residual overclaim)

After both edits, grep the whole README for `tamper-evident` / `tamper evident`: NO remaining occurrence may
claim tamper-evidence for the correction chain against a DB-write adversary. (Other unrelated true uses of
"tamper", e.g. mounting `/shared` read-only "to harden against tampering" at `:62`, are about a different
surface and stay.)

## Threat-model durability (AC-07)

The threat model (what is / is not defended) must live in a durable location. It IS recorded in ADR-002
(`product/features/nxs-014/architecture/ADR-002-weak-mode-threat-boundary.md`) AND now in the README integrity
sections above — satisfying AC-07 (not only
in a transient artifact). Downstream agents reading either will not re-upgrade the claim.

## Key checks (hints)

1. **AC-06 diff inspection.** No unqualified "tamper-evident" claim about the correction chain vs a DB-write
   adversary remains; corrected text states tamper-recorded / correction-history integrity.
2. **AC-07 durability.** Threat model present in README AND ADR-002.
3. **Not under-sold (R-11).** The true shipped guarantees (per-entry `content_hash`, append-only audit,
   authoritative supersession chain, populated `previous_hash` chain, runnable `unimatrix verify`) remain
   stated.
4. **Same PR (C-08).** These edits ship with the code changes, not in a follow-up.
