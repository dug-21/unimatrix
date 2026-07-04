# Test Plan — Two-Protocol Retro Lifecycle (merge → close → retro)

**Files:** `.claude/protocols/uni/uni-delivery-protocol.md` (pr-review phase),
`.claude/protocols/uni/uni-bugfix-protocol.md` (bug-review phase)
**Risks:** R-04 (Critical, feature-wide blast radius), R-08 · **ACs:** AC-17

> This restructure rewires the close-of-cycle harvest for **every future delivery and bugfix session**. A
> mis-wire (stop-before-merge, or omit/misorder the post-close `/uni-retro`) silently breaks
> attribution/verbatim harvest feature-wide. Verified **end-to-end and PER protocol** — a server-only or
> single-protocol green suite does NOT satisfy the gate (#5383).

---

## R-04 — per-protocol end-to-end lifecycle (AC-17)

### Simulated full-cycle trace, run for BOTH protocols separately
- `test_delivery_protocol_merge_close_retro_order` and `test_bugfix_protocol_merge_close_retro_order` —
  drive: cycle open → review phase → **simulated human merge** → `context_cycle(phase-end)` →
  `context_cycle(stop)` → `/uni-retro`. Assert:
  - (a) the cycle is still **OPEN at merge** (review phase not stopped pre-merge);
  - (b) `context_cycle(stop)` fires **only after** merge;
  - (c) the post-close `/uni-retro` retrieval returns a **non-empty candidates section with loss**.
  Run for delivery AND bugfix — they are **separate wirings**; a fix in one does not cover the other.
- `test_executed_sequence_is_exactly_merge_close_retro` — a retro before close, or a close before merge,
  FAILS. (R-04 sc.2.)

### Protocol-parity grep (code-derived cross-check, #4915)
- `test_both_protocols_have_post_close_retro_step` — both files contain the post-close `/uni-retro` step.
- `test_neither_protocol_retains_pre_merge_stop` — neither file retains a pre-merge `context_cycle(stop)`.
  A single-protocol fix fails. (R-04 sc.3; human merge gate unchanged.)

---

## R-08 — cycle-close is non-purging (the ordering rests on this)

> merge→close→retro composes ONLY because `context_cycle(stop)` is non-purging (ADR-005: it drains only the
> retrospective queue + writes an audit row; touches no buffer). Guard it.

- `test_close_then_transcript_retrieval_still_delivers` — register buffers → `context_cycle(stop)` → run a
  `transcript:{}` retrieval. Assert the buffers **survived** `stop` (synchronous read: still present) and the
  post-close retrieval returns non-empty candidates. Keyed on synchronous buffer observation (R-10), never on
  the absence of an async purge event. (R-08 sc.1; server-observable — also lands in `test_lifecycle.py`.)
- `test_cycle_stop_is_buffer_inert` — a `stop` on a cycle with registered ∪ held buffers leaves buffer
  count/content **unchanged** (synchronous before/after) and writes ONLY its queue-drain audit — no
  purge/reclamation audit, no registration change. (R-08 sc.2.)
- `test_retrieve_before_and_after_close_same_set` — retrieve-before-close yields the same candidate set as
  retrieve-after-close (close is inert w.r.t. buffers). (R-08 sc.3.)

## Integration anchor
The server-observable half of R-08 is `test_cycle_close_then_transcript_retrieval_returns_candidates` in
`suites/test_lifecycle.py` (OVERVIEW §6c). The protocol-ordering and parity assertions are doc-level grep
verifications (Rust or shell), NOT server unit tests — the two-protocol blast radius is not reachable from
the crt-057 handler surface.
