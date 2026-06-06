# Agent Report: vnc-026-researcher

## Deliverable
- `product/features/vnc-026/SCOPE.md` — written, all required sections, AC-01..AC-14.

## Key Findings
- Dependency status verified: vnc-024/#672 CLOSED (shipped: ts-rs bindings + fixtures, `Accept: text/plain` negotiation, `transcript_delta` accept-and-drop guard). vnc-025/#670 OPEN, in flight on `feature/vnc-025` (waves 0–3b complete) — F3 delivery gated on its merge; design is not.
- Parity target is `crates/unimatrix-server/src/uds/hook.rs` (4,183 lines): fire-and-forget vs sync split at `hook.rs:244-251`; envelopes at `write_stdout`/`write_stdout_subagent_inject` (`hook.rs:963/994`).
- Server formatting parity is inherited: `observe_response_to_http` reuses production `format_injection` budget (vnc-024 AC-07), shrinking client transform to ~40 lines.
- npm package is plain CJS, zero-dep, `node:test`; `files` already includes `lib/`. Two latent issues affecting F3: `HOOK_EVENTS` omits PreCompact/PostToolUseFailure (known bug), and `merge-settings.js` `UNIMATRIX_PATTERNS` will not recognize a `node …/hook-client/index.js` command as unimatrix-owned.
- Spike conflict surfaced: ass-067 Q4 (no event queue for HTTP) vs ass-068 Q5 (disk queue for both transports) — raised as Open Question 1.
- ass-071 sidechain capture confirmed additive/out-of-scope; ass-070 feeds crt-052 only.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — surfaced vnc-024/vnc-025 ADRs (#4714, #4720, #4726, #4739–4743) and pattern #4703 (HTTP sync-event raw-JSON gap); all applied in SCOPE.md.
- Stored: nothing novel to store — all findings are feature-specific (scope boundaries, dependency state, parity surface) and live in SCOPE.md; no generalizable pattern beyond what #4703 already captures.
