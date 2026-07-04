## ADR-003: Loss-Propagation Contract — Per-Session `SessionLossInfo`, `search_complete`, and an INDETERMINATE No-Match (never a silent false negative)

Feature: crt-057 · GH #894 · Re-scoped by human 2026-07-04 after ass-091 (#898)
Reworked: the prior "warn-and-proceed on a provably-incomplete *extraction*" framing is superseded.
Extraction was one-shot and destructive; retrieval is now repeatable and read-only, so the honesty
mechanism is not a premature-extraction warning but a per-session loss contract on every returned
session.

### Context

A scoped `transcript` retrieval runs a `match`/`anchor`/`phase` filter over what the buffer *retained*
— not over the full raw stream. ass-091 Q3 established the mechanism precisely:

- `match` runs over whole `TranscriptCandidate.text` blocks (unwindowed), not raw ring bytes, so
  truncation bites at selection (buffer bounds), not at the regex — a block is present whole or absent.
- Buffer loss therefore governs what a no-match *means*. The discriminator already exists per session:
  `SessionLossInfo` (`observe/src/types.rs:633-646`) — `elided_bytes`, `has_holes`, `provenance`
  (`Primary` | `Reconstructed`), `dropped_candidates`. A clean `Primary` session with no loss is
  OMITTED (silence = nothing to report).
- A no-match over a clean session = "didn't happen within retention" (trustworthy negative). A no-match
  over a session with any loss = **INDETERMINATE** — the string could be past the 4 MiB tail, inside a
  hole, or absent from the 0.81-fidelity `Reconstructed` rebuild.

Because retrieval is now non-destructive and repeatable (ADR-001), the prior contract's motivating
danger — a premature one-shot extraction losing candidates forever — no longer exists. What remains,
and what MUST be contractual, is that a negative result is never silently trusted when the buffer was
lossy.

### Decision

**Every returned session carries its loss row; a `match` never collapses to a bare boolean.** Per
returned session, the `transcript` response surfaces:

- `matched: bool`
- `search_complete: bool` — derived `false` iff `elided_bytes > 0 || has_holes ||
  provenance == Reconstructed`. A no-match with `search_complete == false` is **INDETERMINATE**, not
  "didn't happen."
- `elided_bytes` and `provenance` alongside — high `elided_bytes` (past the 4 MiB tail) and
  `Reconstructed` each independently flag a negative as untrustworthy.

For `anchor` / `phase`: return the evidence-ts span / phase bounds that defined the window, and fall
back to `byte_offset` proximity for `ts:None` candidates so they never silently drop out of a windowed
query. Per-session degradation reuses the existing `SessionLossInfo` rows unchanged.

All of it is response-transient, on the candidates channel, OUTSIDE `RetrospectiveReport` — no
persistence (AC-14), the summary ⟂ Plane-B invariant intact (no transcript-derived signal enters the
summary). `search_complete` is a per-session derivation over the existing loss fields; it needs no new
buffer read.

**Premature-extraction warning demoted to optional advisory.** The prior live-sibling detector
(`live_session_ids_for_feature`, excluding the reviewer's own `session_id`) was load-bearing only
because extraction was a one-shot that destroyed the buffer. Now that retrieval is repeatable and
non-destructive, a still-live sibling is low-stakes — the retro simply retrieves again later. If
retained at all, it is an optional advisory on the candidates channel, not a contract requirement; the
loss-propagation contract above is the honesty guarantee.

### Consequences

Easier: a negative is never silently trusted over a lossy/`Reconstructed` session — the exact false
negative this redesign exists to prevent is structurally impossible; degradation is visible per
session, never a crash (AC-06); the contract is a pure derivation over data the buffer already
produces (no new read, no persistence).

Harder: consumers must treat any `search_complete == false` no-match as INDETERMINATE and not "didn't
happen" — a discipline the tool surfaces but cannot enforce on the reader; a session that never
registered or was reclaimed by a backstop before retrieval is invisible (no loss row to emit), so
candidate completeness is best-effort — but, unlike the old contract, a later non-destructive retrieval
can re-check, so this is no longer a terminal loss.

Cross-refs: ADR-001 (non-destructive review / cap+TTL), ADR-006 (retrieval mechanism / clock / window),
#4856 (loss visibility / degraded provenance mandatory), #4799 (per-turn drain starvation — not fixed
here).
