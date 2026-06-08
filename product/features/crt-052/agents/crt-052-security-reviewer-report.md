# Security Review: crt-052-security-reviewer

PR: #706 · Branch: feature/crt-052 · GH Issue: #689 · Fresh-context review.

## Risk Level: low

## Summary
crt-052 (transcript-fed cycle-review distillation) is an additive, defense-hardened
feature with an unusually thorough security design already baked in. Every threat-relevant
surface named in the brief — untrusted JSONL parsing, content-leak posture, lock discipline,
held-buffer bounding — is correctly implemented and proven by passing merge-gate tests.
No blocking findings; no changes requested. Merge-ready from a security standpoint.

## Findings

No blocking or high/critical findings. The items below are confirmations and one low-severity note.

### F1 — Untrusted-input JSONL parser is correctly hardened (confirmation)
- **Severity**: informational (PASS)
- **Location**: `crates/unimatrix-observe/src/distill/jsonl.rs`
- **Description**: `parse_blocks` is per-line parse-or-skip on `&[u8]`, never `Err`, never panics.
  Guards: oversized line (`MAX_LINE_BYTES` 1 MiB), non-UTF-8/embedded-NUL (skip-with-count),
  truncated final line (tolerated), deeply-nested (`MAX_JSON_DEPTH` 64 in `collect_text`),
  gigantic-field (`MAX_TEXT_BYTES` bounded, char-boundary truncation). Verified that
  serde_json 1.0.149's own 128-deep recursion limit returns `Err` (not panic) before our
  `collect_text` recursion runs — two independent stack-overflow defenses. 63 distill tests
  pass incl. the malformed corpus; handler-level corrupt-snapshot test confirms no panic at
  the MCP boundary.
- **Recommendation**: none.
- **Blocking**: no.

### F2 — Content-leak / secrets posture structurally enforced (confirmation)
- **Severity**: informational (PASS)
- **Location**: `mcp/distill_handler.rs` (attach_to_response_assembly), `infra/session_transcript.rs`
  (TranscriptSnapshot Debug), `infra/transcript_hold.rs` (HeldBuffer Debug)
- **Description**: Candidates ride the response only, attached after `store_cycle_review()`;
  `RetrospectiveReport` has no candidate slot (compile-level guarantee, gate test passes).
  Metadata-only hand-written `Debug` on all content-bearing snapshot/held types (leak-gate
  tests pass). Single `tracing` call in new code (readopt-mismatch) is metadata-only.
- **Recommendation**: none.
- **Blocking**: no.

### F3 — Lock discipline two-phase, registered∪held dedup (confirmation)
- **Severity**: informational (PASS)
- **Location**: `infra/session.rs` (take_transcripts_for_feature, apply_transcript_delta)
- **Description**: Registry lock → scan + Arc-clone only; buffer lock → byte-copy/snapshot only;
  all parsing off-lock. Arc-identity dedup prevents double-counting a registered+held buffer.
  Poison recovery treat-as-empty + clear_poison, surfaces as loss not silent drop.
- **Recommendation**: none.
- **Blocking**: no.

### F4 — Held-buffer bounding + loud re-adoption (confirmation)
- **Severity**: informational (PASS)
- **Location**: `infra/transcript_hold.rs`
- **Description**: Independent held-count cap (oldest-first eviction, audited) and TTL sweep
  (reclaims without cycle review). Re-adopt only on exact feature_cycle match; mismatch/empty
  fails loud with audit — #981 mis-scope impossible. AC-11 continuity_simulated_lifecycle and
  all R-01/R-02/R-03 gate tests pass.
- **Recommendation**: none.
- **Blocking**: no.

### F5 — Pre-existing transitive CVE outside crt-052 chain (note)
- **Severity**: low (not introduced by this PR)
- **Location**: workspace Cargo.lock (rsa via sqlx-mysql, RUSTSEC-2023-0071)
- **Description**: Present on `main`, outside crt-052's dependency chain. The only new runtime
  dep introduced here is `regex = "1"` on unimatrix-observe (already vetted in-workspace).
- **Recommendation**: track separately (not a crt-052 merge blocker); flagged in the brief.
- **Blocking**: no.

## Blast Radius Assessment
Worst case from a subtle bug is contained to one `context_cycle_review` call's
`transcript_candidates` section. Parser failure modes degrade to zero candidates
(fail-safe), never panic — so DoS-on-every-review is structurally precluded. The
secrets model (in-memory + purge) is the sole confidentiality guarantee and is enforced
structurally (no candidate field on the persisted type, metadata-only Debug, content-free
audit) — no durable leak surface was introduced. Wave A is independently correct; reverting
Wave B leaves Wave A degrading cleanly to the reconstruction fallback (R-11 gate passes).

## Regression Risk
Low. Additive response field is `skip_serializing_if`/None-when-empty (golden output preserved,
AC-04). `clear_transcripts_for_feature` and the 23 detection rules' inputs are untouched. The
new seam is a sibling of the existing clear path, not a modification of it. Files at the
500-line limit received thin wiring only.

## Verification Performed
- Read full git diff (101 files), ARCHITECTURE.md, RISK-TEST-STRATEGY.md, and all new
  security-relevant source files in full.
- Empirically confirmed serde_json recursion-limit behavior vs the MAX_JSON_DEPTH guard.
- Ran merge-gate suites: 63 observe distill tests, 24 transcript_hold (incl. AC-11), 20
  distill_handler (incl. structural content-leak + wave-boundary gates), 47 session_transcript
  (incl. Debug-leak gates) — all pass.
- clippy on unimatrix-observe: zero warnings from new distill/handler/hold files.
- Diff-wide secrets scan of added lines: clean (only docs + test sentinels).

## PR Comments
- Posted 1 review comment on PR #706 (gh pr review --comment).
- Blocking findings: no. No --request-changes issued.

## Knowledge Stewardship
- Stored: nothing novel to store -- the recurring traps this feature guards against
  (memoization-persist secrets trap #3793, four-return gating #4750, snapshot-not-relock #3753,
  poison-recovery #4764) are already captured in Unimatrix, and the crt-052-specific risk
  analysis lives in RISK-TEST-STRATEGY.md per stewardship rules. No cross-feature anti-pattern
  visible across 2+ features that is not already stored.
