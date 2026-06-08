# Agent Report — vnc-027-researcher

## Task
Problem-space research and SCOPE.md authoring for vnc-027 (F4: TS UDS hook client, contractual cycle attribution, hook-set reduction).

## Output
- `product/features/vnc-027/SCOPE.md` — 19 ACs across transport/parity (AC-01..07), attribution (AC-08..15), hook-set reduction (AC-16), carry-items (AC-17..19).

## Key Findings
1. **Framing oracle is small and well-tested**: `wire.rs:345-400` (4-byte BE u32 + JSON, 1 MiB cap, zero-length reject) with round-trip tests; `LocalTransport` (transport.rs) defines the sync (write+read-one-frame) vs FNF (write+disconnect, no read) semantics to mirror.
2. **Transport seam in the TS client is narrow**: `index.js` routes everything through `transport.post(config, frame, opts) -> SendResult`; `queue.js` frames are transport-agnostic HookRequest JSON. UDS slots in behind the same contract.
3. **Formatting asymmetry (new discovery)**: UDS sync responses are client-formatted (`format_injection`, hook.rs:1034); HTTP is server-formatted (vnc-024 content negotiation). F4 must port format_injection or add a server-side preformatted UDS option — architecture decision with size-budget impact. Stored as pattern #4798 (supersedes #4703).
4. **Size gate is at 99,997/100,000 bytes (3 bytes headroom)** — the C-04 redefinition carry-item is load-bearing and must land first.
5. **ass-072 supersedes the issue body's attribution design**: stamp (write-time, contractual) is primary, not transcript-marker reads; marker is review-time recovery; vote demoted to NULL-only permanently. FINDINGS live on unmerged branch `docs/ass-072-findings` (commit 8881cbc2) — should merge to main for downstream agents.
6. **Worktree gitdir port already landed in F3** (`config.js::resolveGitFile`) — ass-072 precondition #1 satisfied; F4 needs only a stamp-path regression test.
7. **Two server-side declared-vs-vote inversions** must be fixed regardless of the stamp: `infra/session.rs:628` (sweep) and the close path.
8. **UDS listener already accepts `transcript_delta`** (listener.rs:785, 1025) — delta streaming over UDS is server-ready; the TS client must NOT client-prepend the PreCompact transcript block (double-prepend hazard per #680 comment).

## Open Questions for Human
See SCOPE.md Open Questions — highest-leverage: (1) transport-selection semantics when both remote config and a live socket exist, (2) format_injection port vs server-side UDS formatting, (3) SubagentStop default, (5) single delivery vs F4a/F4b split, (6) dogfooding switchover timing.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — surfaced vnc-025 ADR-005 (shared transcript-block extraction core), vnc-026 ADR-001 (Rust-hook-as-oracle parity corpus), vnc-024 ADR-004 (transcript_delta additive event), lesson #4780 (hook-client size-gate trip); all incorporated into SCOPE.
- Stored: entry #4798 "Sync-injection response formatting is transport-asymmetric: HTTP formats server-side (vnc-024), UDS formats client-side (format_injection)" via context_correct superseding #4703.
