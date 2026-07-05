# Test Plan — readme-integrity (`README.md`, changed) + frozen-hash tripwire

> Non-code verification: diff/grep + manual inspection. Covers R-11 (README claim re-drift),
> R-06/AC-10 (frozen hash — verified via the existing `hash.rs` tests, restated here as the tripwire),
> and the durability of the threat model (AC-07). Ships in the SAME PR as the code (C-08, AC-06).

## R-11 / AC-06 — README wording corrected (grep + manual)

### Check: no unqualified "tamper-evident" claim survives
- Grep the README diff (and full README) for `tamper-evident` / `tamper evident` applied to the
  correction chain vs a DB-write adversary. Assert NONE remains unqualified.
- Assert the corrected text states the delivered guarantee: **tamper-recorded** / correction-history
  integrity (accidental corruption + single-point API-surface tamper detectable).

### Check: shipped integrity NOT under-sold (manual)
- Assert the corrected section still credits the real, shipped integrity: per-entry `content_hash`,
  append-only audit, authoritative supersession chain. Over-correcting into "no integrity" fails this
  (the exact over-correction failure mode in R-11).

## AC-07 — threat model documented durably (file-check + manual)
- Assert the threat-model boundary is present in a DURABLE location — the README integrity section
  AND/OR ADR-002 — not only a transient artifact:
  - What IS detectable: accidental corruption, single-point API-surface tamper.
  - What is OUT of tier: a root / DB-write adversary who coordinates edits across an entry and its
    successor (Non-Goal 3).
- This pins the boundary so a downstream agent cannot silently re-upgrade the claim (the #912 drift,
  reproduced one level up if unpinned).

## R-06 / AC-10 — frozen hash tripwire (test, restated)
The hash format is FROZEN. Verified by the EXISTING `hash.rs` `#[cfg(test)]` vectors — run unchanged:
- `test_content_hash_known_value` — `compute_content_hash("Test","Content") == SHA256("Test: Content")`.
- `test_content_hash_both_empty` — `== e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`.
- `test_content_hash_empty_title` / `empty_content` / `unicode` — unchanged.

Assertions:
- These vectors remain BYTE-IDENTICAL and pass. Any diff = AC-10 fail (SR-01 scope-creep into strong mode).
- Grep the signature: `pub fn compute_content_hash(title: &str, content: &str) -> String` — unchanged.
- Assert `previous_hash` is NOT folded into the digest (no new argument, no digest-input change);
  the digest input remains `format!("{title}: {content}")` per `hash.rs`. C-01.

## Verification methods summary
| Item | Method | AC |
|------|--------|-----|
| No unqualified "tamper-evident" | grep README diff | AC-06 |
| Tamper-recorded wording present | manual read README diff | AC-06 |
| Shipped integrity not under-sold | manual read | AC-06 |
| Threat model durable (README/ADR-002) | file-check + manual | AC-07 |
| Hash vectors byte-identical | `cargo test -p unimatrix-store hash` (existing) | AC-10 |
| Signature unchanged, no digest fold | grep | AC-10 |
| README ships same PR as code | PR diff contains both | C-08 |
