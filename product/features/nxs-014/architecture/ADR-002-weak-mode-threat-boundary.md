## ADR-002: Weak-Mode Threat Boundary — Tamper-RECORDED, `compute_content_hash` Frozen, No Cascade/Anchor/Migration

### Context

The phrase "cross-version cryptographic chain / tamper-evident" resolves at two strengths. D-1
(SCOPE) settled **weak mode**: populate the `previous_hash` link, but make **no** hash-format
change, **no** external anchor, **no** re-hash migration. The strong cryptographic cascade
(folding `previous_hash` into `content_hash`) plus a head anchor is an explicit NON-GOAL /
north-star (goal #5474).

Two risks make this boundary durable rather than incidental:

- **SR-01:** an implementer "improves" the fix inline by folding `previous_hash` into
  `compute_content_hash`. This is a hash-FORMAT change: it breaks the `hash.rs` known-value
  vectors (`e3b0c442…` genesis, `"Test: Content"`) and collides **every** legacy `content_hash`
  in production, which a strong-format `validate_hashes` would then flag as a mismatch on every
  existing entry.
- **SR-04:** the README rewrite can under- or over-correct, reproducing the exact "tamper-evident"
  overclaim drift that GH #912 exists to fix (the "avoid overstating defensive structure" lesson).

The realistic adversary this product defends against is accidental/naive corruption and tamper via
the legitimate API surface. A root / raw-DB-write adversary is **out of tier** — the same adversary
owns the bearer token and every secret (Principle 8); defending the hash chain against it while it
owns the process is theater.

### Decision

Freeze the boundary and pin it in code, README, and this ADR so no downstream agent re-upgrades it:

1. **`compute_content_hash(title, content) -> String` is FROZEN** — signature and output format
   (`SHA256("title: content")`, ADR-004 / entry #74). Weak mode touches it **not at all**. A
   test-vector-stability check (the existing `hash.rs` known-value tests) is the tripwire; adding
   `previous_hash` to the digest is out of scope and must fail review.
2. **No cascade, no external anchor, no re-hash migration.** Schema version stays 30.
3. **The delivered guarantee is tamper-RECORDED, not tamper-evident against a DB-write adversary.**
   What is actually caught (the AC that matters): naive content mutation, via the **combination**
   of the content-hash recompute and the chain-link check in the shared verify core (ADR-001,
   ADR-003). What is explicitly **not** caught: a write-access adversary who edits content,
   recomputes that entry's `content_hash`, and updates the one successor's `previous_hash` — O(1)
   coordinated edits, because there is no cascade.
4. **README integrity wording states exactly this boundary** (AC-06): accidental-corruption +
   API-surface tamper are detectable; it does **not** claim tamper-evidence against a
   database-write adversary. The strong cascade + anchor remain a named north-star, not creep.

### Consequences

- **Easier:** small, contained blast radius — no test-vector regen, no legacy re-hash, no
  migration, no export change. The claim in the README becomes true and precisely scoped. Future
  strong-mode work has a clear, documented starting point (its real prerequisite is the external
  HEAD anchor, not the digest change).
- **Harder / accepted:** the chain does not defend against a root DB-write adversary — an
  accepted, in-tier limitation, stated plainly rather than papered over. Capability #5478
  (KI-CHAIN-XV) names tamper-EVIDENCE but weak mode delivers tamper-RECORDED; the capability
  wording must be reconciled (or a `KI-CHAIN-XV-STRONG` sibling split) before it is marked
  `proven`, so "proven" is not an overclaim (flagged to the vision session, non-blocking here).
- **Guardrail:** any PR that changes `compute_content_hash`'s signature or digest input under this
  feature is out of scope and must be rejected at review (SR-01 tripwire).

Related: ADR-003 (the write + verify that operate within this frozen boundary).
