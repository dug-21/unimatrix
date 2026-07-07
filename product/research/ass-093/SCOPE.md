# ass-093 — In-place mutation of non-content fields without context_correct

## Question

Can Unimatrix mutate **non-content, volatile metadata** on an entry — starting with a **tag**
— in place, without minting a new entry via `context_correct`, and without violating the
integrity model? If yes, by what mechanism, with what authorization, audit, and learning-signal
semantics? If no, why not, and what is the cheapest correct alternative?

The forcing case: capability delivery status (`missing | partial | proven | claimed`) is about
to be tracked by a **tag**. Every status transition would otherwise require `context_correct`,
which rewrites the entire record, mints a new version in the supersession chain, re-points
carried edges (vnc-035), and touches the learning layer — heavy churn for a one-tag change that
alters no content.

## Why it matters to the vision

- **Self-learning** — if every status flip runs through `context_correct`, each flip may reset or
  perturb confidence, usage counts, and co-access affinity. Status churn would be silently
  churning the learning layer, not just a row.
- **Domain-agnostic** — capability status is a *domain* concept. The mechanism MUST be
  domain-neutral. A bespoke first-class `status` field per domain concept is explicitly out
  (that field name is already taken by `EntryRecord.status` lifecycle, and per-use-case schema
  fields violate domain-agnosticism — see #5505). Tags are the generic lane; the answer has to
  live at that generic level.
- **Integrity** — the answer must not weaken hash-chain tamper-evidence (Principle 1), the
  append-only audit log (Principle 2), service-layer capability checks (Principle 3), or open a
  poison vector (SLN1). "Without `context_correct`" must NOT mean "without an authorized,
  audited event."

## The load-bearing question (settle first)

**Are tags inside the `content_hash` / supersession chain, or outside it?**

ADR-006 (#360) stores tags in a **junction table** (`entry_tags`), separate from the entry row.
Empirically establish whether `content_hash` folds in tags:
- If tags are **outside** the row hash → in-place mutation does not break tamper-evidence, and
  this is largely a matter of adding an authorized, audited mutate op. (Cheap path.)
- If tags are **inside** the hash → in-place mutation is a tamper-evidence violation; the answer
  is likely "no" for anything hashed, and the spike pivots to a non-hashed metadata lane.

Everything downstream forks on this. Do it first.

## Scope — what the researcher explores (bounded)

1. **Hash-chain membership** — what exactly `content_hash` and `previous_hash` are computed over
   (`store`/`schema` crates). Which fields are hashed; which are not. Is the `entry_tags` junction
   table part of any integrity chain? Is there an independent audit chain over tag writes?

2. **The full blast radius of an in-place tag mutation**, per field:
   - Audit log — what event, what attribution, is a dedicated op-type needed?
   - Capability check — what authorization gates a tag mutation vs. a content correction?
   - In-memory hot path — tags drive lookup; Principle 7 caches analytics-derived search data in
     `Arc<RwLock<_>>` rebuilt by tick. How does an in-place tag change stay consistent (immediate
     vs. tick-rebuilt)? Any stale-index window?
   - Learning signal — confidence, usage, co-access, phase affinity: does `context_correct`
     currently reset or carry these, and what would an in-place mutate preserve instead?
   - Edges — `context_correct` carries edges forward (vnc-035). In-place mutate avoids the
     re-point entirely; confirm no edge implications.
   - Search/embedding — does a tag change require re-embedding? (Expected: no; tags aren't in the
     embedded text — confirm.)

3. **Peer non-content fields** — enumerate other volatile, non-content metadata that shares this
   churn problem and could use the same mechanism (candidates: confidence, usage counters,
   `helpful` signals, any denormalized/derived fields). Which are ALREADY mutated in place today
   (and how)? The mechanism should generalize, not special-case tags.

4. **Mechanism comparison** (domain-agnostic only — no per-use-case schema fields):
   - (a) authorized + audited in-place tag mutate op on the generic tag lane;
   - (b) status as a typed **edge/annotation** rather than a tag;
   - (c) a general "mutable-metadata" lane distinct from hashed content.
   Score each against: integrity preservation, audit/provenance, authorization/poison-resistance,
   learning-signal correctness, index consistency, and generality across domains.

5. **Provenance of transitions** — a status transition (e.g. partial→proven) is itself meaningful
   history. If we drop the correction chain for tags, how (if at all) is transition history
   preserved? Is losing it acceptable, or does the mechanism need a lightweight transition log?

## Out of scope

- Implementation. This is a decision/feasibility spike; output is a recommendation, not code.
- Adding a domain-specific first-class `status` field to `EntryRecord` — ruled out up front
  (non-domain-agnostic; name collides with lifecycle status).
- Redesigning `context_correct` itself for content changes — it stays as-is for content.

## Known constraints / prior art to build on

- ADR-006 / pattern #360 — `entry_tags` junction table, FK CASCADE, `PRAGMA foreign_keys = ON`,
  DELETE-all + re-INSERT write pattern.
- #5505 — the two-status trap: `EntryRecord.status` (lifecycle) vs capability delivery status
  (domain, in content).
- vnc-035 (#749) — edges carry forward on `context_correct`.
- Architectural Principles 1 (hash chain), 2 (append-only audit), 3 (service-layer capability
  checks), 7 (in-memory hot path), and SLN1 (poison-resistance).
- uni-capability skill — capability status vocabulary and the "don't bury volatile status in the
  entry" guidance that motivates a mutable lane in the first place.

## Expected output (FINDINGS.md)

1. A definitive answer to the load-bearing question (tags in-hash or not), with evidence
   (file:line references to the hash computation).
2. A recommended mechanism (or a reasoned "no, keep context_correct" with the cheapest
   alternative), scored against the criteria above.
3. The list of peer non-content fields the mechanism should cover.
4. A statement on transition-history preservation.
5. Any integrity/poison risks that would gate implementation, with mitigations.
