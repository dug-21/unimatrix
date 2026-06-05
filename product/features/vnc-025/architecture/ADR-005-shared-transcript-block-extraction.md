## ADR-005: One Shared Transcript-Block Extraction Core (`from_path` for the Hook, `from_bytes` for the Server) — Parity by Construction

### Context

Goal 5 requires the server-built PreCompact block to be like-for-like with what the local Rust
hook delivers (`extract_transcript_block` + `prepend_transcript`, `hook.rs:1383/:1442`).
Assumption A3 flags the trap: if the extraction logic stays hook-private, the server side
becomes a maintained duplicate and parity drifts (SR-05 — ships dark, so drift would be
invisible until F3). The hook and the server live in the **same crate**
(`unimatrix-server/src/uds/hook.rs`), so sharing is a refactor, not an API design. `hook.rs`
is also far over the 500-line rule; moving extraction out shrinks it.

Note on the "no transcript parsing" non-goal: that non-goal bans *interpretation* (knowledge
extraction, distillation — crt-052). The mechanical JSONL→exchange-turn formatting is exactly
what the local hook already does and what like-for-like parity requires; it is in scope by
goal 5's own definition.

### Decision

Create `uds/transcript_block.rs` and move, verbatim where possible:
`ExchangeTurn`, `build_exchange_pairs`, `format_turn`, `MAX_PRECOMPACT_BYTES = 3000`,
`TAIL_MULTIPLIER = 4` (window = 12,000 bytes), `extract_transcript_block(path)`,
`prepend_transcript`. `hook.rs` call sites (`:220`, `:252`, `:295`) re-import — hook behavior
is unchanged by construction.

Add one new entry point:

```rust
pub fn extract_transcript_block_from_bytes(bytes: &[u8]) -> Option<String>
```

It feeds the same core: split into lines, parse each as JSONL, `build_exchange_pairs`,
budget loop, identical header/footer. A partial first line (the buffer's contiguous tail
starts mid-line) fails the JSON parse and is filtered — **the same behavior** the path variant
exhibits when its seek lands mid-line. Parity is structural, not asserted.

Server integration (`handle_compact_payload`, `listener.rs:1504`): after step 7 formatting,
read `snapshot.transcript.lock().contiguous_tail(MAX_PRECOMPACT_BYTES * TAIL_MULTIPLIER)`
(the snapshot from `:1521` already shares the live buffer — ADR-001), build the block from
bytes, `prepend_transcript(block.as_deref(), &content)` before `token_count` is computed.
Empty buffer, absent session, or `None` block → content byte-identical to pre-vnc-025
(AC-11). The legacy local hook never streams deltas, so its sessions always hit the empty
branch — no double-prepend; the "never both stream and locally prepend" invariant is recorded
as an F3 contract (A2, ARCHITECTURE.md SR-09 disposition).

**Golden parity test (SR-05, pattern #3426)**: one fixture JSONL transcript; expected =
`extract_transcript_block(path)`; actual = stream the same file bytes as deltas (shuffled +
duplicated), then `extract_transcript_block_from_bytes(contiguous_tail(...))`. Byte-for-byte
equality — no hand-written expectation.

### Consequences

- Easier: parity cannot drift (one core); A3 is closed; `hook.rs` shrinks toward the file-size
  rule; the golden test is cheap to write and self-maintaining.
- Harder: moving code touches `hook.rs`'s large test module (imports); the implicit
  delta-content contract — F3 streams raw transcript-file bytes (JSONL), offsets are file
  byte offsets — becomes load-bearing and must be stated in the spec for F3 to inherit.
- Cross-references: ADR-001 (snapshot read path), ADR-002 (`contiguous_tail` never serves
  holes — the block builder can trust its input).
