# nxs-014 — Wire the Cross-Version Hash Chain in `context_correct`

> Status: SETTLED — all four open questions resolved by doug (2026-07-05; scope-review comment posted on GH #912). Decision: **weak (column population) + honest README wording + strong-chain-as-non-goal**, forward-only legacy, correction-time + import + on-demand-CLI verify, CLI subcommand over a transport-agnostic verify core (no MCP tool in v1). See Settled Decisions.

## Problem Statement

`context_correct`'s write path hardcodes `previous_hash: String::new()` and `version: 1` on every correction (`crates/unimatrix-store/src/write_ext.rs:539-540`, and again in the INSERT bind at `:582-583`). The cross-version chain column exists in the schema (`db.rs`, default `''`) but nothing ever populates it.

Consequences:
- **Violates Architectural Principle 1** (PRODUCT-VISION.md:56): *"`content_hash` and `previous_hash` on every entry — never skipped."* `previous_hash` is skipped on every correction.
- **Unbacked README claim** — README says correction chains are "tamper-evident" via a "`previous_hash` chain." No successor commits to its predecessor's hash, so the claim does not hold as written.
- Blocks the Knowledge Integrity goal's **north-star** (goal #5474) and capability **KI-CHAIN-XV** (#5478), both of which name this exact gap. Tracked as GH #912.

Who is affected: every consumer that trusts the "tamper-evident correction history" guarantee — i.e. the product's headline integrity promise.

## Central Decision: Threat Model (drives everything)

The phrase "cross-version cryptographic chain / tamper-evident" resolves at two strengths. **The scope must pick one and state the threat model explicitly.**

### The actual adversary this product defends against

Unimatrix runs as a personal knowledge cloud: one container, one bearer token (vision goal `personal-cloud`; Principle 8 keeps secrets out of the DB). An adversary with **root / raw file access to the SQLite DB** is explicitly *out of tier* — the same adversary reads the bearer token and every secret; defending the hash chain against them while they own the process is theater. That is the "avoid overstating defensive structure" lesson (MEMORY).

The **realistic** threats the chain must catch:
1. **Accidental / naive corruption** — buggy migration, partial restore, disk bit-rot, a botched manual edit.
2. **Tamper via the legitimate API surface** — `content_hash` is *engine-computed* in the write path (`write_ext.rs:516`), never taken from caller input, so it is already forge-resistant against the context tools. The chain extends that: a correction cannot silently detach from its predecessor.

### Option 1 — Weak (column population) — CHOSEN (doug, OQ-3)

- On correction: `previous_hash = superseded.content_hash`, `version = superseded.version + 1`.
- Extend the existing `validate_hashes` (`import/mod.rs:396`) with a **chain-verify**: walk the supersedes chain and assert `successor.previous_hash == predecessor.content_hash`.
- **No hash-format change.** `compute_content_hash` stays `SHA256("title: content")`.

Known limitation (must be stated in README, not hidden): because `content_hash` does **not** fold in `previous_hash`, there is **no cascade**. An actor who can write DB columns directly defeats detection by editing the content, recomputing that entry's `content_hash`, and updating the one successor's `previous_hash` (a handful of coordinated edits). This is **tamper-recorded, not tamper-evident against a write-access adversary** — but per the threat model above, that adversary is out of tier.

What it *does* catch (the AC that matters): naive content mutation is caught by the **combination** of (a) `validate_hashes`'s existing content-hash recompute (`import/mod.rs:421`) and (b) the new chain-link check. Mutating a superseded entry's content without perfectly rewriting both its hash and its successor's link fails loud.

Blast radius: **small.** `write_ext.rs` correction path (2 lines + the INSERT binds), a chain-verify extension to `validate_hashes`, tests. No hash-format change, no test-vector regen, no legacy re-hash, no migration required.

### Option 2 — Strong (true cryptographic cascade) — NON-GOAL (deferred, doug OQ-3)

- `content_hash = H(title, content, previous_hash)`, so each link commits to its predecessor; tampering any historical entry breaks every downstream hash to the head.
- Raises tamper cost from O(1) coordinated edits to O(chain length) + re-anchoring.

Why deferred:
1. **It is a hash-FORMAT change** with a large blast radius:
   - `compute_content_hash` signature changes → every call site: initial insert (`write.rs`), correction (`write_ext.rs`), import recompute (`validate_hashes`), export round-trip, migration.
   - **Every stored known-value test vector breaks** (`hash.rs` tests: the `e3b0c442…` genesis vector and the `"Test: Content"` vectors) and must be regenerated.
   - **Legacy collision (hard):** every existing `content_hash` in production was computed under the old format. A strong-format `validate_hashes` flags **every existing entry** as a content-hash mismatch. Resolving this needs either a full re-hash migration (which changes every `content_hash` and breaks any external reference to a hash) or a per-entry hash-format-version tag with format-aware validation — both are their own feature.
2. **It does not reach true tamper-evidence without an external anchor.** A cascade only defeats a full-DB-write adversary if the HEAD (terminal-active) hash is anchored **outside the mutable DB** — signed, published, or in an append-only log the adversary cannot rewrite. Our audit log is append-only *by DDL trigger in the same DB* (Principle 2); a root adversary drops the trigger. The issue proposes **no anchor**. Shipping the cascade with no anchor would let README keep saying "tamper-evident" while the guarantee still fails against the stated adversary — reproducing exactly the drift #912 is filed to fix, one level up. That is the trap the "avoid overstating defensive structure" lesson warns against.

Strong-mode is therefore captured as a tracked north-star (goal #5474; a future `KI-CHAIN-XV-STRONG` capability) whose real prerequisite is the **external HEAD anchor**, not the digest change.

## Settled Decisions (doug, 2026-07-05 — GH #912 scope-review comment)

- **D-1 (threat model / feature size — was OQ-3): WEAK MODE.** Populate `previous_hash = superseded.content_hash` and `version = superseded.version + 1`. **No** cascade digest change, **no** external anchor, **no** re-hash migration. The strong cryptographic chain (folding `previous_hash` into `content_hash`) plus a head anchor is an explicit **NON-GOAL / north-star**, gated on the enterprise anchor work (see Non-Goals 1-2). README's "tamper-evident" wording **must** be corrected to describe tamper-**RECORDED** integrity (correction-history integrity against accidental corruption + API-surface tamper), **not** tamper-evidence against a database-write adversary.
- **D-2 (legacy backfill — was OQ-1): FORWARD-ONLY.** Existing corrected entries keep empty `previous_hash`; `validate_hashes` already tolerates empty (`import/mod.rs:429`). **No migration.** Caveat, and the reason we don't backfill: a backfill would only bless *current* stored content, it cannot verify past history — so it buys a false baseline, not proof.
- **D-3 (verify cadence — was OQ-2):** correction-time link invariant (the correction writes a correct link — near free) **+** full chain-verify at import (existing path, `import/mod.rs:212`) **+** on-demand explicit check via CLI. **Defer** any maintenance-tick / periodic scan.
- **D-4 (verify exposure — was OQ-4):** CLI subcommand + server-internal core in v1; **no MCP tool in v1**. **Architecture directive:** factor the chain-verify as a **transport-agnostic core function** so a future enterprise admin MCP tool (post-RBAC) is a thin wrapper over the same core — CLI is simply the only caller today. **Factual correction to flag for the architect:** `validate_hashes` today lives in `unimatrix-server` (`crates/unimatrix-server/src/import/mod.rs:396`), is **private**, and runs **only at import** (`:212`). So the on-demand live-DB verify is genuinely new surface, and the **cross-crate placement of the shared verify core (store vs server)** is an open design decision for the architect.

**Note for the vision session (non-blocking):** capability #5478 (KI-CHAIN-XV) name/why promise tamper-**EVIDENCE**, but weak mode delivers tamper-**RECORDED**. Reconcile the capability wording (or split a `KI-CHAIN-XV-STRONG` sibling) before it is ever marked `proven`, so "proven" is not an overclaim.

## Goals

1. Populate `previous_hash` and increment `version` on every correction, so Architectural Principle 1 holds literally and by construction.
2. Provide a chain-verify that walks a supersedes chain and fails loud, naming the break, when a link is inconsistent.
3. Make the README's integrity wording **true and precisely scoped** — describe what is actually guaranteed (accidental corruption + API-surface tamper detectable) and drop or qualify any claim of tamper-evidence against a database-write adversary.
4. Ensure `previous_hash` / `version` round-trip losslessly through export/import (columns already exported per `export.rs:310,383,392`).
5. Advance capability KI-CHAIN-XV (#5478) from `missing`/UNWIRED to `proven`, with behavioral evidence.

## Non-Goals

1. **Strong cryptographic cascade** (`content_hash = H(title, content, previous_hash)`) — deferred; see Option 2. This is a hash-format change with legacy-collision + test-vector + external-reference blast radius.
2. **External HEAD anchor** (signed / published / out-of-DB append-only log) — the true prerequisite for tamper-evidence against a DB-write adversary. Separate, larger feature.
3. **Defending against a root / raw-DB-write adversary** — out of tier (same adversary owns the bearer token and all secrets, Principle 8).
4. **Legacy backfill migration** (setting `previous_hash` on pre-existing corrected entries) — recommended forward-only; see Open Questions OQ-1. A backfill only blesses current stored content, it cannot retro-verify history.
5. **Continuous background chain-verify on the maintenance tick** — recommended on-demand + at import only for v1; see OQ-2.
6. Changing the supersession chain semantics (`supersedes`/`superseded_by`) — already wired and correct.

## Background Research

- **Correction write path** (`write_ext.rs:439-620`): reads original, deprecates it (`superseded_by = new_id`), allocates the new entry, computes `content_hash` from title+content, then hardcodes `previous_hash: String::new()` / `version: 1` (struct at :539-540, INSERT binds at :582-583). Both the struct field and the bind must change — they are independent literals.
- **Hash function** (`hash.rs`): `compute_content_hash(title, content) = SHA256(format!("{title}: {content}"))` (ADR-004, entry #74). Does **not** fold `previous_hash`. Confirms the weak-mode "no cascade" limitation. Known-value test vectors are embedded in `hash.rs` tests and would break under any format change.
- **`validate_hashes`** (`import/mod.rs:396-442`): the only existing verifier. Runs on import (`:212`). Already does two things: (a) recomputes `content_hash` from title+content and compares (catches content mutation), (b) checks that a non-empty `previous_hash` references a known entry's hash — and **already skips empty `previous_hash`** (`:429`). This means forward-only legacy entries (empty `previous_hash`) are already tolerated. The chain-verify (successor.previous_hash == predecessor.content_hash *along the supersedes chain*) is a new, stronger check to add here — and it too must skip empty `previous_hash` as "unverifiable legacy," not "broken."
- **Export** (`export.rs`): `content_hash`, `previous_hash`, `version` already serialized (`:310, :383-392`); round-trip test asserts values (`:1010-1012`). Weak-mode change needs no export change.
- **Vision** (`PRODUCT-VISION.md:56` Principle 1; goal #5474): integrity is claim-floor = per-entry content_hash + append-only audit + authoritative supersession chain (all proven); north-star = cross-version crypto chain (UNWIRED) + poison-resistance + contradiction-free. The cross-version chain is explicitly the *north-star*, and the goal text itself flags "currently UNWIRED, violates Principle 1, GH #912." nxs-014 delivers the wiring; strong-mode + anchor remain north-star.
- **Capability** #5478 KI-CHAIN-XV: `done_when` = "on correction, previous_hash = predecessor.content_hash AND version increments; a chain-verify walks a supersedes chain and fails loud if any previous_hash != predecessor.content_hash." **The capability's own done-when is satisfied by weak mode** — it does not require the cascade.
- **Maintenance tick** exists (`background.rs`, 15-min default) as a possible home for continuous verify, if OQ-2 chooses it.
- **Schema version** currently 30 (`migration.rs:26`). Weak mode needs **no** migration; a legacy backfill (if chosen) would be v31.

## Proposed Approach (weak mode)

1. **Correction path** (`write_ext.rs`): set `previous_hash = original.content_hash` and `version = original.version + 1` in both the `EntryRecord` struct (:539-540) and the INSERT binds (:582-583). `original` is already loaded (`:462`).
2. **Chain-verify**: add a function that, given an entry, walks its supersedes chain (or validates all chains corpus-wide) and asserts `successor.previous_hash == predecessor.content_hash`, skipping empty `previous_hash` (legacy). Surface it by extending `validate_hashes` and exposing an explicit integrity-check entry point.
3. **Cadence**: enforce the well-formed-link invariant at correction time (free — it is a write invariant, not a scan); run the full chain-verify at import and on an explicit integrity check. (Background-tick cadence: OQ-2.)
4. **README**: rewrite the integrity paragraph to state the real, threat-model-scoped guarantee.
5. **Tests**: correction sets `previous_hash`/`version` correctly; multi-step chain increments `version` and links each hop; tamper test mutates a superseded entry's content and asserts chain-verify (via `validate_hashes`) fails loud and names the entry.

## Acceptance Criteria

- **AC-01:** After a correction, the new entry's `previous_hash` equals the superseded entry's `content_hash`.
- **AC-02:** After a correction, the new entry's `version` equals `superseded.version + 1`; across an N-step correction chain, versions are `1..N` monotonic.
- **AC-03:** A chain-verify walks a supersedes chain and, for every hop with a non-empty `previous_hash`, asserts `successor.previous_hash == predecessor.content_hash`; empty `previous_hash` (legacy / genesis) is treated as unverifiable-legacy, not a break.
- **AC-04:** Tamper-detection: mutating a superseded entry's content (without a perfectly coordinated rewrite of its `content_hash` and successor link) makes verification fail loud, naming the offending entry id. (Satisfied by the content-hash recompute + chain-link check in `validate_hashes`.)
- **AC-05:** `previous_hash` and `version` round-trip losslessly through export → import, and `validate_hashes` passes on a clean re-import of corrected entries.
- **AC-06:** README's integrity wording is true and threat-model-scoped: it does not claim tamper-evidence against a database-write adversary; it states what accidental-corruption / API-surface-tamper detection is actually provided.
- **AC-07:** SCOPE's threat model is documented in a durable place (README integrity section and/or an ADR) so downstream agents do not silently upgrade the claim.

## Constraints

- **No hash-format change** in scope — `compute_content_hash` signature and output are frozen (touching them breaks the `hash.rs` known-value vectors and collides with every legacy `content_hash`). This is the boundary between weak (in scope) and strong (non-goal).
- Chain-verify **must** skip empty `previous_hash` — otherwise forward-only legacy corrected entries (`previous_hash=''`) fail verification. `validate_hashes:429` already establishes this convention for the existence check; the new chain check must match it.
- Both the struct literal (`:539-540`) and the INSERT bind (`:582-583`) must be changed; they are independent and either alone leaves the bug half-fixed.
- Max 500 lines/file (rust-workspace rule) — chain-verify logic may need its own module rather than bloating `import/mod.rs`.
- Weak mode requires **no** schema migration; a legacy backfill (OQ-1) would introduce migration v31 with its own risk surface.
- The tamper-detection AC (AC-04) leans on `validate_hashes`'s content-hash recompute; the chain-link check alone does not catch a content mutation that leaves hashes untouched. Both checks must run together to satisfy AC-04.

## Open Questions

All four open questions (OQ-1..OQ-4) were resolved by doug on 2026-07-05 and are recorded in **Settled Decisions** above as D-2, D-3, D-1, and D-4 respectively. None remain open.

## Tracking

- GH Issue: #912 (to be relabeled/linked after Session 1).
- Capability: KI-CHAIN-XV (#5478). Goal: Knowledge Integrity (#5474), north-star leg.
