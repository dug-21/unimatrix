# Security Review: crt-028-security-reviewer

## Risk Level: low

## Summary

The PR implements PreCompact transcript restoration (hook.rs file read + JSONL parse), an
observation source allowlist (listener.rs), and a quarantine regression test
(index_briefing.rs). The implementation follows the degradation contract and input bounds
documented in the architecture. No new dependencies are introduced. The code is structurally
sound. Four specific concerns are noted — all non-blocking — and one knowledge gap (R-09
field denylist) is flagged as a recommended follow-up.

---

## Findings

### F-01: R-09 — extract_key_param fallback may select sensitive fields from unknown tools

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/uds/hook.rs:851-857`
- **Description**: For tool names not in the hardcoded map, `extract_key_param` falls back
  to the first string-valued field in the JSON input object. The iteration order of
  `serde_json::Map` is insertion order (BTreeMap-like in serde_json's default feature set).
  A custom tool whose first string field is named `api_key`, `token`, `password`, or
  `authorization` would have that value included — truncated to 120 bytes — in the
  transcript block written to stdout, and subsequently injected into the agent's context
  window via PreCompact. The RISK-TEST-STRATEGY.md acknowledges this (R-09) and recommends
  a field denylist. The test `extract_key_param_unknown_tool_first_string_field_fallback`
  confirms the fallback works but explicitly documents in a comment that "sk-xxx" would be
  returned first if it were the first field.
- **Recommendation**: Before this path reaches user sessions that invoke custom MCP tools
  with credential fields, add a field name denylist check before the generic fallback:
  skip fields named `api_key`, `token`, `secret`, `password`, `authorization`, `key`.
  This requires a one-line set check before `val.as_str()` is returned. Not blocking for
  this PR because the current production tool set (Claude Code built-in tools) is fully
  covered by the hardcoded map.
- **Blocking**: no

---

### F-02: R-03 — seek_back cast from u64 to i64 is safe but undocumented

- **Severity**: low (informational)
- **Location**: `crates/unimatrix-server/src/uds/hook.rs:1069,1072`
- **Description**: `seek_back` is bounded by `window.min(file_len)` where `window =
  MAX_PRECOMPACT_BYTES * TAIL_MULTIPLIER = 12,000`. The cast `-(seek_back as i64)` is safe
  because `seek_back <= 12000`, which is far below `i64::MAX`. The RISK-TEST-STRATEGY.md
  (OQ-SPEC-2) identifies this and recommends using the clamp form, which is exactly what
  the implementation uses. No overflow is possible. However, the cast is unguarded with no
  comment explaining why it is safe (e.g., no `debug_assert!(seek_back < i64::MAX as u64)`).
  On a 32-bit platform with `usize` overflow in the constant computation
  `MAX_PRECOMPACT_BYTES * TAIL_MULTIPLIER`, the cast would be to a wrong value, but Rust
  panics on debug overflow in constants so this is caught at build time.
- **Recommendation**: Add a brief comment above the cast: `// seek_back <= 12_000; cast is
  safe`. Purely cosmetic — the logic is correct as written.
- **Blocking**: no

---

### F-03: R-11 — No path restriction on transcript_path (accepted by design)

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/uds/hook.rs:85-93`
- **Description**: The `transcript_path` value comes from Claude Code stdin JSON and is used
  directly with `std::fs::File::open(path)`. There is no restriction to `~/.claude/`
  subdirectories, no path normalization, and no path traversal prevention. The ARCHITECTURE.md
  (SR-05) explicitly documents this as deliberate: the path is trusted as a Claude Code
  system field; the hook process already has read access to whatever it points to; the
  fail-open JSONL parser means non-JSONL files (e.g., `/etc/passwd`) produce `None` via
  the `serde_json::from_str` failure path; and there is no exfiltration path beyond stdout
  injection of JSONL-shaped content.

  Verified: `/etc/passwd` would parse no valid `{"type":"user",...}` records, so
  `build_exchange_pairs` returns an empty vec, and `extract_transcript_block` returns `None`.
  No data exfiltration is possible unless the file is deliberately crafted to look like
  Claude Code JSONL transcript format, which requires prior code execution in the same
  account — at which point the attacker already has full access.

  The only non-empty-check filter is `.filter(|p| !p.is_empty())`. This is not a security
  boundary — it is documented as such.
- **Recommendation**: Accept as documented. If a defense-in-depth requirement is ever added
  (e.g., restrict to files within a known parent directory), that should be done at the
  configuration layer, not here.
- **Blocking**: no

---

### F-04: R-01 — Degradation boundary: structural verification passes

- **Severity**: (no finding — positive confirmation)
- **Location**: `crates/unimatrix-server/src/uds/hook.rs:84-93` and `1063-1109`
- **Description**: The most critical risk from RISK-TEST-STRATEGY.md (R-01, rated Critical)
  is that a miscoded degradation boundary would suppress the `BriefingContent` write if the
  transcript read fails. I verified:
  1. `extract_transcript_block` returns `Option<String>`, not `Result`. No error propagates.
  2. The call site uses `.and_then(|p| extract_transcript_block(p))` — if `extract_transcript_block`
     returns `None`, the outer `transcript_block` is `None`, and processing continues.
  3. The `BriefingContent` write branch at line 131-136 calls `prepend_transcript(transcript_block.as_deref(), content)` — when `transcript_block` is `None`, `prepend_transcript` returns `briefing` verbatim (verified in the match arm `(None, false) => briefing.to_string()`).
  4. The inner closure pattern `let inner = || -> Option<String> { ... }; inner()` correctly
     contains all `?` operators. No `?` escapes the closure. No `unwrap()` appears in the
     transcript extraction path.
  This risk is fully mitigated.
- **Blocking**: no (no finding)

---

### F-05: R-07 — sanitize_observation_source allowlist is correct and tested

- **Severity**: (no finding — positive confirmation)
- **Location**: `crates/unimatrix-server/src/uds/listener.rs:101-107`
- **Description**: The allowlist match is exhaustive. The wildcard arm `_` catches all
  unknown values including `None`, empty string, prefix-extended strings, and injection
  attempts. The function is called at the sole write site for the `hook` column. Six unit
  tests cover all documented cases (AC-11). The doc comment explicitly identifies this as
  the sole write gate with a warning against adding a second write site. No injection path
  into the observations table via the `source` field is present post-fix.
- **Blocking**: no (no finding)

---

### F-06: contradiction.rs and status.rs — GH #358 fix: no security concern

- **Severity**: (no finding — positive confirmation)
- **Location**: `crates/unimatrix-server/src/infra/contradiction.rs`, `src/services/status.rs`
- **Description**: The GH #358 fix (pre-fetch entries in Tokio context before rayon dispatch)
  removes the `Handle::current().block_on(...)` call from rayon worker threads. This is a
  correctness fix, not a security change. The `active_entries` vector is fetched from the
  store in the Tokio context (trusted internal path), passed as `Vec<EntryRecord>` into the
  rayon closure (owned, no reference to the store), and the `entry_map` is built from it for
  O(1) lookups. No new trust boundary is crossed. The graceful degradation in `status.rs`
  (empty vec on store error) is safe — the embedding consistency check is opt-in and its
  silent skip is acceptable.
- **Blocking**: no (no finding)

---

## OWASP Evaluation

| OWASP Category | Applicable? | Assessment |
|----------------|-------------|------------|
| A01 Broken Access Control | No | No privilege levels involved; hook reads only its own user's files |
| A02 Cryptographic Failures | No | No cryptography |
| A03 Injection | Partial | See F-01 (key-param field leakage, low severity) and F-03 (path traversal, accepted) |
| A04 Insecure Design | No | Degradation contract is explicit and structurally enforced |
| A05 Security Misconfiguration | No | No configuration changes |
| A06 Vulnerable Components | No | No new dependencies added |
| A07 Auth/Identity Failures | No | No auth changes |
| A08 Data Integrity | No | No serialization of untrusted types into executable paths |
| A09 Logging/Monitoring Failures | No | Errors log to stderr, not suppressed |
| A10 SSRF | No | No network calls from transcript read path |

---

## Blast Radius Assessment

**Worst case scenario**: A subtle bug in `extract_transcript_block` (e.g., a panic via an
undetected `unwrap`) causes the hook process to exit non-zero on PreCompact events.

- Impact: Agent receives no context injection at compaction time. Knowledge continuity is
  broken for that session. No data corruption; no privilege escalation; no information
  leakage.
- Scope: Limited to PreCompact hook events. All other hook events (`UserPromptSubmit`,
  `SubagentStart`, `SessionRegister`, etc.) are unaffected — the transcript extraction is
  guarded by `if matches!(request, HookRequest::CompactPayload { .. })`.
- Recovery: Graceful — exit 0 is always returned per FR-03.7. Claude Code receives exit 0
  and proceeds with compaction using whatever context the Unimatrix server returned.
- Probability: Low. The structural analysis (F-04) shows no `unwrap()` in the transcript
  path and all `?` operators contained within the inner closure.

**Second worst case**: The `sanitize_observation_source` function is bypassed by a future
contributor adding a second write site (R-07).

- Impact: Arbitrary string content written to the `hook TEXT NOT NULL` column. The column
  is used in observation queries, not in SQL string interpolation, so SQL injection is not
  a risk. The blast radius is schema pollution and potential query result contamination.
- Mitigation: The doc comment, the six unit tests, and the code review checklist all guard
  against this. The risk is maintenance, not current.

---

## Regression Risk

**Non-PreCompact hook paths**: The `transcript_block` extraction is gated by
`if matches!(request, HookRequest::CompactPayload { .. })`. All other event types
(`ContextSearch`, `SessionRegister`, `RecordEvent`, etc.) produce `transcript_block = None`.
The `BriefingContent` response arm was previously routed through `write_stdout(&response)`
and now routes through `prepend_transcript(None, content)` which returns `content` verbatim
when transcript is None. Regression risk: negligible.

**SubagentStart path**: The conditional `if req_source.as_deref() == Some("SubagentStart")`
at line 126 takes priority over the `BriefingContent` match. SubagentStart still routes to
`write_stdout_subagent_inject_response`. No regression.

**index_briefing.rs**: The only changes are a doc comment and a new test. No logic changed.
No regression risk.

**contradiction.rs / background.rs / status.rs**: The GH #358 fix changes the interface of
`scan_contradictions` and `check_embedding_consistency` from `&Store` to `Vec<EntryRecord>`.
All callers in `background.rs` and `status.rs` are updated. The test
`test_scan_contradictions_does_not_panic_in_rayon_pool` explicitly verifies the fix. No
regression in the contradiction scan path; the fix restores functionality that was
previously silently broken.

---

## PR Comments

- Posted 1 comment on PR #357 via `gh pr review`.
- Blocking findings: no.

---

## Knowledge Stewardship

- Stored: nothing novel to store — the key-param field denylist recommendation (R-09) is
  already documented in RISK-TEST-STRATEGY.md by the risk strategist, and the degradation
  boundary pattern (R-01) is captured in Lesson #699 in Unimatrix. No cross-feature
  generalizable anti-pattern emerged that is not already stored.
