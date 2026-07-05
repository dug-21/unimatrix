# Scope Risk Assessment: nxs-014

Feature: wire the cross-version hash chain in `context_correct` (weak mode). Scope is SETTLED (D-1..D-4). This pass flags risks that should shape architecture/spec BEFORE design.

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | Scope-creep into strong mode: an implementer "improves" the fix by folding `previous_hash` into `compute_content_hash`, silently breaking `hash.rs` known-value vectors and colliding every legacy `content_hash`. Non-Goal 1, but the temptation is inline. | High | Med | Spec must state `compute_content_hash` signature is FROZEN as a hard constraint; add a test-vector-stability check as a tripwire. |
| SR-02 | New chain-verify does NOT match `validate_hashes:429`'s empty-`previous_hash` skip, so forward-only legacy corrected entries (`previous_hash=''`, D-2) fail verify as "broken" — a false-positive integrity alarm on existing data. | High | Med | Spec: chain check treats empty `previous_hash` as unverifiable-legacy, not a break (AC-03). Require a mixed legacy+new corpus test. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-03 | Cross-crate placement of the transport-agnostic verify core is unsettled: `validate_hashes` is private, in `unimatrix-server` (`import/mod.rs:396`), import-only. D-4 wants a shared core callable by CLI now + a future MCP wrapper. store-vs-server placement is an open architect decision with re-export/dependency-direction consequences. | Med | High | Architect must decide crate home in an ADR; verify core belongs where both CLI and a future server MCP tool can depend on it without a cycle (likely `unimatrix-store` with server re-export). |
| SR-04 | README rewrite (D-1, AC-06/07) is subjective: under-correcting leaves an overclaim ("tamper-evident" vs the delivered tamper-RECORDED); over-correcting under-sells shipped integrity. Recurrence of the exact drift #912 exists to fix. | Med | Med | Spec must give the exact claim boundary (accidental-corruption + API-surface tamper detectable; NOT DB-write adversary). Pin threat model in README AND an ADR so no downstream agent re-upgrades it. |
| SR-05 | New on-demand live-DB verify surface (CLI subcommand) is genuinely new, not a refactor — arg parsing, exit codes, output on a break. Easy to under-spec vs the "small blast radius" framing. | Low | Med | Spec the CLI contract: what it scans, exit code on break, and that it names the offending entry id (AC-04). |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-06 | **Two-literal half-fix (headline).** The INSERT at `write_ext.rs:581-583` hardcodes `.bind("")` / `.bind(1_i64)` INLINE — it does not read `new_rec.previous_hash`/`version`. Fixing only the struct (:539-540) compiles clean and STILL persists empty; a struct-level test passes while the DB is wrong. | High | High | Spec: both sites change; the INSERT must bind from the record fields. Non-negotiable test must read back FROM THE DB (not the in-memory record) so the half-fix cannot pass green. |
| SR-07 | Export/import round-trip of `previous_hash`/`version` (nxs-012 BACKUP-RESTORE surface). Values already serialized (`export.rs:310,383`), but a corrected chain exported then re-imported must re-validate under `validate_hashes` at import (`:212`). Mixed legacy(empty)+populated corpus is the real round-trip case. | Med | Med | Require an export→import round-trip test over a multi-hop corrected chain incl. a legacy empty-hash entry (AC-05); confirm import chain-verify passes on clean re-import. |

## Assumptions

- **A-1** (SCOPE §Proposed Approach step 1): `original` (loaded at `:462`) has a correct, populated `content_hash` and `version`. If any pre-existing active entry has an empty/stale `content_hash`, the new correction inherits a bad `previous_hash`. Spec should assert `original.content_hash` non-empty at correction time.
- **A-2** (SCOPE Non-Goal 6): the `supersedes`/`superseded_by` chain is already correct — chain-verify walks it and trusts it. If supersession is ever broken, chain-verify mis-reports. Accepted as out of scope but noted.
- **A-3** (SCOPE D-2 / `validate_hashes:429`): the existing empty-`previous_hash` tolerance is load-bearing for forward-only legacy. If that convention changes elsewhere, this feature's legacy tolerance silently breaks.

## Design Recommendations

1. **SR-06 + SR-02 are the two ways this ships broken.** The non-negotiable tests must (a) read `previous_hash`/`version` back from the DB after correction, and (b) verify a mixed legacy+new corpus passes. A struct-only or in-memory-only assertion is a false-green (cf. tautological-assertion + warn-continue lessons #4177/#4473).
2. **SR-03**: architect issues an ADR fixing the verify-core crate home before spec, so CLI-now / MCP-later share one core without a dependency cycle.
3. **SR-01 + SR-04**: spec pins the frozen-hash constraint and the exact README claim boundary; both trace to an ADR so the threat model is durable (AC-07) and the strong-mode/anchor line stays a north-star, not creep.
