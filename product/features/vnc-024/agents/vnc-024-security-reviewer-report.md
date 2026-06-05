# Security Review — vnc-024 (PR #686)

> Recorded by the Delivery Leader from the reviewer's return; the reviewer ran in a temporary
> worktree (cleaned up) and posted findings as a PR comment, so this file captures the result.

**Risk level: LOW. No blocking findings. Recommend merge.**

Full diff reviewed cold against ADR-004, ARCHITECTURE.md, and RISK-TEST-STRATEGY.md; every load-bearing claim independently verified against source.

## Central security property — VERIFIED (principle 8: no raw transcript bytes to durable storage)
- **RecordEvent arm** (`listener.rs:774`): early `return Ack` after the `SessionWrite` capability check + `sanitize_session_id`, before all persistence. Disk insert provably unreachable. Does NOT reuse the col-022 specialize-then-fall-through anti-pattern (#1266).
- **Batch arm** (`:1009`): `.filter()` drops deltas before `obs_batch`. Pre-persistence loops read only `feature_cycle`/`topic_signal` metadata, never the `bytes` payload. No new durable-write arm introduced (ADR-004 assumption A3 holds for this diff).
- **Typed parse is observability-only** — never changes control flow; malformed payload still drops with `Ack`, no panic, no Error leak. Matches stored pattern #4723.

## Other axes
- **Access control**: guard sits after the capability check — no auth bypass; test asserts -32003 for no-write caps (NFR-04).
- **Content negotiation**: allowlist is exactly `{Entries, BriefingContent}`; Pong/Ack/Error stay JSON (R-06); `Accept` read before `into_parts` (R-07); reuses production `format_injection` + `MAX_INJECTION_BYTES` (byte-identical, panic-free, no UDS regression).
- **Config**: OSS `validate()` genuinely rejects `RetainDays(any N)` as enterprise-only (not a range error); merged config re-validated; bare-u32 rejected.
- **Dependency safety**: ts-rs confirmed dev-only (absent from all `--edges normal`; derives `cfg_attr(test)`-gated); no advisories on ts-rs or its transitive deps. The single `cargo audit` finding (RUSTSEC-2023-0071 rsa via sqlx-mysql) is pre-existing and unrelated.
- **Secrets**: none in the diff; all fixtures use clearly-fake values.

**Blocking findings: NO.** No changes requested (review posted non-blocking on PR #686).

## Knowledge Stewardship
- Queried: `context_search` / governing anti-patterns #4723, #4711, #1266.
- Stored: nothing novel — this PR is a textbook application of #4723 (accept-and-drop: early-return single arm, filter() batch arm — not symmetric), not a new generalizable lesson.
