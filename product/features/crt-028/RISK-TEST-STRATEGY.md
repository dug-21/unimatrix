# Risk-Based Test Strategy: crt-028 — WA-5 PreCompact Transcript Restoration

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Degradation boundary miscoded — a `?` or early return inside `extract_transcript_block` propagates to the caller, suppressing `BriefingContent` write | High | Med | Critical |
| R-02 | TAIL_MULTIPLIER (4×) is insufficient for thinking-heavy sessions — the 12 KB window contains no complete exchange pairs, leaving the agent with zero transcript restoration | High | Med | Critical |
| R-03 | `SeekFrom::End(-N)` where N > file size — undefined/platform-variant behavior; no explicit file-length clamp in some code paths | High | Med | Critical |
| R-04 | Adjacent-record pairing breaks for non-canonical JSONL — if Claude Code ever inserts a record between `tool_use` and `tool_result`, all subsequent pairings produce empty snippets silently | Med | Low | High |
| R-05 | `build_exchange_pairs` reversal is off-by-one or applied to the wrong slice — most-recent-first ordering wrong, causing agents to receive stale context first | Med | Med | High |
| R-06 | `truncate_utf8` not used for budget-fill truncation — a mid-character cut produces a malformed UTF-8 string written to stdout, potentially breaking the agent's parser | High | Low | High |
| R-07 | `sanitize_observation_source` bypass — a second write site is added in a future feature without going through the helper, reintroducing GH #354 | Med | Low | High |
| R-08 | `IndexBriefingService::index()` quarantine post-filter removed without test detection — quarantined entries appear in compaction briefings | High | Low | High |
| R-09 | Tool key-param fallback (first string field) returns a secret/large-value field — an unknown tool's first string key is a token, password, or multi-KB payload | Med | Low | Med |
| R-10 | Empty assistant turn (only `tool_use` + `thinking`, no `type: "text"`) — exchange pair emitted or suppressed incorrectly depending on OQ-SPEC-1 resolution | Med | Med | Med |
| R-11 | `transcript_path` pointing to a path outside `~/.claude/` — no path validation; if Claude Code is compromised, the hook reads arbitrary files | Low | Low | Med |
| R-12 | `prepend_transcript` emits the wrong separator when briefing is empty — agent receives malformed output with missing section header or extra blank lines | Low | Med | Low |
| R-13 | crt-027 symbol rename after merge breaks crt-028 at compile time — `IndexBriefingService`, `IndexEntry`, `format_index_table` are consumed but not owned | Med | Low | Med |

---

## Risk-to-Scenario Mapping

### R-01: Degradation boundary miscoded — briefing suppressed on transcript failure

**Severity**: High
**Likelihood**: Med
**Impact**: Agent receives no stdout on PreCompact. No Unimatrix knowledge, no task continuity. Compaction proceeds with a blank injection — worst possible regression (Lesson #699 precedent: silent None broke the entire feedback loop).

**Test Scenarios**:
1. Pass `transcript_path` pointing to a non-existent file; assert stdout is non-empty and contains the mock `BriefingContent` string (SR-07 explicit test, AC-07, FR-06.7).
2. Pass `transcript_path` pointing to a file containing only malformed JSON on every line; assert briefing still written (AC-08).
3. Pass `transcript_path = None`; assert briefing written unchanged (AC-06).
4. Construct a `transcript_path` where `file.seek()` would fail (e.g., a named pipe disguised as a file); assert degradation, not panic.

**Coverage Requirement**: All four failure classes must have individual tests. Structural verification: `extract_transcript_block` has no `Result` return type; no `.unwrap()` in the function body.

---

### R-02: 4× tail multiplier yields no parseable pairs in thinking-heavy sessions

**Severity**: High
**Likelihood**: Med
**Impact**: PreCompact hook silently delivers zero transcript context because the 12 KB window is entirely consumed by `thinking` blocks or large `tool_result` payloads from a single assistant turn. Agent receives briefing only — degradation that is correct per spec but may surprise operators tuning the budget.

**Test Scenarios**:
1. Construct a JSONL file where the last 12 KB is one assistant turn with three `thinking` blocks (each 3 KB), followed by a `tool_result`. Assert `build_exchange_pairs` returns at most one exchange pair (the tail exchange), not zero (confirms at least some extraction).
2. Construct a JSONL file where the last 12 KB is entirely `tool_result` content belonging to a prior assistant turn that is outside the window. Assert `extract_transcript_block` returns `None` (AC-09), not a partially-formed pair.
3. At constant boundaries: construct a JSONL file exactly 12,001 bytes; assert only the tail-bytes window is read (seek is called), not the full file.

**Coverage Requirement**: At least one test with a JSONL file large enough to trigger the seek path. Document `TAIL_MULTIPLIER = 4` as a known limitation; future empirical tuning may require increasing it.

---

### R-03: SeekFrom::End(-N) when N > file size

**Severity**: High
**Likelihood**: Med
**Impact**: On Linux, `seek(SeekFrom::End(-N))` where N > file size returns `Ok(0)` (positions at start) per POSIX. On some filesystems or future Rust stdlib versions this behavior may differ. The ADR-001 code path explicitly handles `file_len > window` with a conditional seek, avoiding the issue — but if the guard is implemented incorrectly (e.g., using `>=` vs `>`, or computing `window` as `i64` with overflow), the seek may be issued with N > file_len.

**Test Scenarios**:
1. Pass a file of 100 bytes with `TAIL_WINDOW_BYTES = 12000`; assert `SeekFrom::Start(0)` path is taken (no seek-from-end), all lines are parsed (OQ-SPEC-2 explicit coverage).
2. Pass a file of exactly `TAIL_WINDOW_BYTES` bytes; assert no seek is performed (boundary condition: `file_len == window`).
3. Pass a file of `TAIL_WINDOW_BYTES + 1` bytes; assert `SeekFrom::End(-(window as i64))` is called (boundary condition: seek triggers).
4. Pass a zero-byte file; assert `extract_transcript_block` returns `None` without error.

**Coverage Requirement**: Boundary tests at `file_len = 0`, `file_len = window - 1`, `file_len = window`, `file_len = window + 1`. The explicit clamp (`if file_len > window { seek } else { read from start }`) must be code-reviewed and tested at the boundary.

---

### R-04: Adjacent-record pairing breaks for non-canonical JSONL

**Severity**: Med
**Likelihood**: Low
**Impact**: If Claude Code inserts a `system` or unknown record between an assistant's `tool_use` and the expected `user` `tool_result`, the pairing produces empty snippets. Silent degradation — the agent sees tool names without results. Accumulation over many turns reduces restoration quality.

**Test Scenarios**:
1. Interleave a `type: "system"` record between an assistant `tool_use` and a user `tool_result`; assert tool pair is emitted with empty snippet (`[tool: Read(file_path) → ]`), not dropped entirely.
2. Back-to-back assistant records (no user record after `tool_use`); assert `ToolPair { result_snippet: "" }` is emitted.
3. Orphaned `tool_result` in a user record at window boundary (the corresponding `tool_use` was before the tail window); assert the `tool_result` is silently skipped (not included as spurious text).

**Coverage Requirement**: All three ADR-002 edge cases covered with explicit assertions.

---

### R-05: build_exchange_pairs reversal produces wrong order

**Severity**: Med
**Likelihood**: Med
**Impact**: Agents receive oldest exchanges first, then most-recent. The first item in the restoration block is the start of the conversation, not the most recent work. FR-02.2 / AC-02 violated. Quality of restoration degrades silently — the budget fills with the least-relevant exchanges.

**Test Scenarios**:
1. Build a JSONL file with 3 exchange pairs labelled A, B, C (oldest to newest); assert restored block begins with pair C (`[User] C`), then B, then A.
2. Build a JSONL file with 1 exchange pair; assert no reversal artifact (pair appears once).
3. Build a JSONL file that exceeds budget; assert the pairs included are the most-recent ones (C, B) not the oldest (A, B).

**Coverage Requirement**: At least one multi-pair, multi-budget test asserting ordering explicitly by content.

---

### R-06: UTF-8 boundary violation in truncation

**Severity**: High
**Likelihood**: Low
**Impact**: `BufReader` produces valid UTF-8 lines (Rust `String` invariant), but if `tool_result` content or user text contains multi-byte characters (emoji, CJK), truncating at a byte offset that lands mid-codepoint corrupts the output. The agent's parser or terminal may reject or misparse the block.

**Test Scenarios**:
1. Construct a tool result whose content is a 299-byte prefix of valid ASCII followed by a 4-byte CJK character; assert snippet is truncated to 299 bytes (not 303, not 300 mid-codepoint).
2. Construct user text that is exactly 3000 bytes of CJK characters; assert the budget-fill truncation lands on a codepoint boundary.
3. Assert `truncate_utf8` is used for all three truncation sites: tool result snippet (300 bytes), key-param (120 bytes), and budget-fill.

**Coverage Requirement**: One test per truncation site with a multi-byte character at the boundary.

---

### R-07: sanitize_observation_source bypass — second write site added later

**Severity**: Med
**Likelihood**: Low
**Impact**: A future feature adds a new hook event type and copies the observation row construction from the `ContextSearch` arm without calling `sanitize_observation_source`. GH #354 vulnerability reintroduced silently. The helper's doc comment guards against this but does not enforce it.

**Test Scenarios**:
1. Unit test `sanitize_observation_source` independently for all six cases from ADR-004: `Some("UserPromptSubmit")`, `Some("SubagentStart")`, `None`, `Some("unknown")`, `Some("")`, `Some("UserPromptSubmitXXXXX")` (AC-11, FR-07.6).
2. Integration test: call `ContextSearch` with `source: Some("Injected\nEvil")` over UDS; read back the observation; assert `hook` column = `"UserPromptSubmit"`.

**Coverage Requirement**: All six allowlist cases covered in a dedicated unit test. Integration test verifies end-to-end write path.

---

### R-08: Quarantine post-filter removed from IndexBriefingService without test failure

**Severity**: High
**Likelihood**: Low
**Impact**: Quarantined entries appear in compaction briefings delivered to agents. Agents receive invalidated/retracted knowledge as authoritative. This is the GH #355 gap — T-BS-08 was deleted with `BriefingService`; the post-filter in `index()` has no direct test.

**Test Scenarios**:
1. Store an entry with `status: Quarantined`; run `IndexBriefingService::index()` with a query matching that entry; assert the entry ID is absent from the result (AC-12, FR-08.1/08.2).
2. Store a `status: Active` entry alongside the quarantined one; assert the active entry appears in results (non-quarantine path not broken).
3. Remove the `status == Active` post-filter from `index()` and verify the test fails — the test must be designed to catch filter removal.

**Coverage Requirement**: The quarantine test must exercise the post-filter directly, not delegate to `SearchService` mock.

---

### R-09: Tool key-param fallback returns a secret or oversized field

**Severity**: Med
**Likelihood**: Low
**Impact**: An unknown tool whose first string field in `input` is a token, API key, or a multi-KB payload exposes sensitive data in the transcript restoration block. The block is written to stdout, which Claude Code injects into the agent's context window.

**Test Scenarios**:
1. Call `extract_key_param` with an unknown tool whose input has `{"api_key": "sk-xxx", "query": "foo"}`; assert key_param is `"sk-xxx"` truncated to 120 bytes, not `"foo"` — document this as a known limitation requiring a future denylist.
2. Call `extract_key_param` with an unknown tool whose first string field is 5000 bytes; assert truncation to 120 bytes.

**Coverage Requirement**: Document the limitation that the fallback may select sensitive fields; recommend adding a field denylist (`api_key`, `token`, `secret`, `password`) before the feature reaches production.

---

### R-10: OQ-SPEC-1 — Assistant turn with no text blocks (only tool_use + thinking)

**Severity**: Med
**Likelihood**: Med
**Impact**: The spec does not define whether an exchange pair where the assistant message has zero `type: "text"` blocks (only `tool_use` and `thinking`) is emitted or suppressed. Two failure modes:

- **Emit**: An exchange pair with empty `[Assistant]` text but non-empty tool pairs is output. The agent sees `[User] <text>` then `[Assistant]` on a blank line followed by tool pairs — parseable but potentially confusing.
- **Suppress**: The pair is dropped entirely. If the task context consists entirely of tool-call-only turns (common in autonomous delivery runs), zero pairs are emitted — worst-case false degradation.

**Recommendation**: Emit the pair with tool-use pairs when at least one `ToolPair` is present, even if assistant text is empty. Suppress only if both assistant text AND tool pairs are empty. This preserves maximum context.

**Test Scenarios**:
1. Construct a JSONL assistant turn with `tool_use` blocks and a `thinking` block but no `type: "text"` blocks; assert the pair is emitted with the tool pair line(s) and `[Assistant]` text is empty (FR-02.4: concatenation of zero text blocks = empty string).
2. Construct a JSONL assistant turn with only a `thinking` block (no text, no tool_use); assert no pair is emitted for this turn.
3. Construct a session consisting entirely of tool-call-only assistant turns; assert at least one pair is emitted if any tool_use is present.

**Resolution** (risk strategist + spec writer): Emit the pair when at least one `ToolPair` is present. Suppress only when both `asst_text` and `tool_pairs` are empty (pure-`thinking` turn). Spec FR-02.4 is authoritative. Scenarios 1 and 3 are unblocked.

**Coverage Requirement**: All three scenarios are implementable. Non-negotiable gate test.

---

### R-11: transcript_path outside expected directory — arbitrary file read

**Severity**: Low
**Likelihood**: Low
**Impact**: The `transcript_path` value comes from Claude Code via stdin JSON. In normal operation it points to `~/.claude/projects/...`. If the Claude Code process is compromised or misconfigured, it could supply a path to `/etc/passwd` or another sensitive file. The hook reads it as read-only JSONL, so any structured content would be silently skipped (fail-open), and unstructured content would be treated as malformed lines and discarded. The practical damage is limited — no data exfiltration path beyond stdout injection.

**Test Scenarios**:
1. Pass `transcript_path = "/etc/passwd"` (or `/dev/urandom`); assert `extract_transcript_block` returns `None` (no valid JSONL pairs) and no panic.
2. Verify the architecture's assertion (ARCHITECTURE.md SR-05 section) that `filter(|p| !p.is_empty())` is the only path sanitization — document as deliberate, not a gap.

**Coverage Requirement**: One test confirming non-JSONL content at arbitrary paths produces `None`, not an error or exfiltration. Accept as low-priority given the trust model.

---

### R-12: prepend_transcript separator format error when briefing is empty

**Severity**: Low
**Likelihood**: Med
**Impact**: `prepend_transcript(Some(block), "")` produces output with a missing or malformed section header. Canonical headers are defined in FR-02 / PRODUCT-VISION.md WA-5: `"=== Recent conversation (last N exchanges) ==="` / `"=== End recent conversation ==="`.

**Test Scenarios**:
1. Call `prepend_transcript(Some("block"), "")` — assert output contains `"=== Recent conversation"` header and `"=== End recent conversation ==="` footer.
2. Call `prepend_transcript(None, "")` — assert empty string returned.
3. Call `prepend_transcript(Some("block"), "briefing")` — assert both transcript header and briefing content present in correct order.
4. Call `prepend_transcript(None, "briefing")` — assert `"briefing"` returned verbatim (no header injected).

**Coverage Requirement**: All four branches of `prepend_transcript` have explicit assertions.

---

### R-13: crt-027 symbol rename breaks crt-028 at compile time

**Severity**: Med
**Likelihood**: Low
**Impact**: If `IndexBriefingService`, `IndexEntry`, or `format_index_table` are renamed in crt-027 after merge, crt-028 fails to compile. This is a merge-ordering risk, not a runtime risk — the compile-time error surfaces immediately. However, if crt-027 is merged as a hotfix and crt-028 is mid-delivery, the build breaks.

**Test Scenarios**:
1. Verify `cargo check` passes after crt-027 merge and before crt-028 delivery begins — include as a gate condition.
2. Pin the exact crt-027 symbols used in a comment in crt-028 source to make renames obvious to reviewers.

**Coverage Requirement**: CI compile check is sufficient. No runtime test needed.

---

## Integration Risks

### crt-027 Dependency

crt-028 cannot be built without crt-027 merged. The consumed symbols (`IndexBriefingService::index`, `format_index_table`, `IndexEntry`) are compile-time dependencies. Renaming in crt-027 is a hard build break for crt-028.

**Mitigation**: Gate crt-028 delivery start on a passing `cargo check` after crt-027 merge. The architecture explicitly lists the four consumed symbols and notes they are an immediate compile error if changed.

### Claude Code JSONL Format Contract

The transcript format is controlled by Claude Code, not by this codebase. The adjacent-record pairing assumption (ADR-002) is validated against current observed behavior but is not formally contractual. Any format change (e.g., `tool_result` moving to a separate record type, or a new wrapper layer) would silently degrade pairing quality — empty snippets everywhere — rather than failing.

**Mitigation**: The fail-open design means format drift produces degraded output, not broken output. The pairing edge cases are explicitly tested. If the format changes, T-01/T-04 tests will catch the breakage before production.

### Hook Latency Budget

Total PreCompact wall time = local file I/O + server round-trip. The 40ms `HOOK_TIMEOUT` applies only to the server leg. File I/O is bounded by the 12 KB tail window but not enforced by any timeout. A slow NFS mount or overloaded disk could cause the transcript read to consume the full hook latency budget before the server call begins.

**Mitigation**: The tail-bytes read cap is the primary defense (ADR-001). No additional timeout wrapper is architected — the degradation path (if file I/O returns an error, `None` is returned) handles the case where the OS kills the read. Empirical measurement on target hardware is recommended before tuning TAIL_MULTIPLIER.

### Synchronous I/O in Hook Process

`hook.rs` has no tokio runtime. All file I/O must be `std::io`. A future contributor adding `tokio::fs` to `hook.rs` would panic at runtime (no tokio executor). The NFR-02 constraint is a documentation guard, not a compile-time guard.

**Mitigation**: Code review must enforce NFR-02. A CI lint rule checking for `tokio::fs` usage in `hook.rs` would provide structural enforcement — recommend as a follow-up.

---

## Edge Cases

| Edge Case | Expected Behavior | Risk ID |
|-----------|------------------|---------|
| Zero-byte transcript file | `extract_transcript_block` returns `None`; briefing written | R-03 |
| File exactly equals TAIL_WINDOW_BYTES | Read from start (no seek), all lines parsed | R-03 |
| All JSONL lines are `type: "system"` | Zero pairs; returns `None` | R-04 |
| Assistant turn: only `thinking` blocks | No `ToolPair` present; pair suppressed entirely (FR-02.4, OQ-SPEC-1 resolved) | R-10 |
| Multiple `tool_use` in one assistant turn, all results present | All tool pairs emitted on separate lines | R-04 |
| `tool_result` content is a 10 KB grep output | Snippet truncated to 300 bytes at UTF-8 boundary | R-06 |
| `tool_result` content array has multiple text blocks | Only the first `type: "text"` block is used (spec FR-03.4) — verify implementation does not concatenate all blocks | R-04 |
| `transcript_path` is an empty string `""` | `.filter(|p| !p.is_empty())` returns `None`; briefing written | R-01 |
| Budget exactly at 3000 bytes after N pairs | N pairs included; N+1th pair attempted but not added (partial pairs not emitted) | R-02 |
| User turn with multiple `type: "text"` blocks | All blocks concatenated with newline separator | R-05 |
| `key_param` field value is itself valid JSON (e.g., a nested object) | `as_str()` returns `None`; fallback to next string field | R-09 |
| Seek lands in the middle of a multi-byte UTF-8 character | First line discarded by fail-open parser; subsequent lines intact | R-06 |

---

## Security Risks

### Untrusted Input Surface

This feature has a narrow untrusted input surface:

**1. `transcript_path` (from Claude Code stdin JSON)**
The path value is supplied by Claude Code. The hook process opens the file read-only. No path sanitization beyond non-empty check. If Claude Code is compromised, the hook reads arbitrary files. The fail-open JSONL parser means non-JSONL content produces `None` — no exfiltration path beyond stdout injection of JSONL-shaped content.

**Blast radius**: Low. The hook's stdout is injected into the agent's context window at compaction. A compromised `transcript_path` pointing to a JSONL-shaped file could inject crafted context into the agent. Trust model: `transcript_path` is trusted as a Claude Code system field.

**2. `source` field on `HookRequest::ContextSearch` (over local UDS)**
Pre-fix (GH #354): arbitrary string written to `hook TEXT NOT NULL` column. Post-fix: allowlist-validated to two known values. The UDS is local — an adversary needs code execution in the same user account. The allowlist is defense-in-depth.

**Blast radius post-fix**: Negligible. Unknown values fall back to `"UserPromptSubmit"`. No injection vector in the column value (it is used in queries, not eval).

**3. JSONL content parsed by `build_exchange_pairs`**
Content is parsed as `serde_json::Value` — no arbitrary code execution risk. Fields are extracted as strings and truncated. No deserialization of untrusted types. The only injection surface is if truncated content is later interpolated into a shell command — not a path in this feature.

**Recommendation**: Add a field denylist for the key-param fallback (R-09) before production: skip fields named `api_key`, `token`, `secret`, `password`, `authorization`. This prevents accidental inclusion of credential fields from custom tool invocations.

---

## Failure Modes

| Failure | Expected System Behavior |
|---------|-------------------------|
| `transcript_path` missing or unreadable | Briefing-only output; exit 0; no stderr (AC-07, FR-06.2) |
| All JSONL malformed | Briefing-only output; exit 0; no stderr (AC-08, FR-06.3) |
| `BriefingContent.content` empty AND transcript `None` | Empty stdout; exit 0 (FR-01.4 invariant) |
| `BriefingContent.content` empty AND transcript present | Transcript block only; section header present; exit 0 (SR-04) |
| crt-027 server not yet updated (old `BriefingService` response) | Response does not match `BriefingContent` variant; existing hook error path handles; exit 0 |
| Server UDS timeout (40ms exceeded) | `transport.request()` returns error; existing hook timeout path; transcript read already completed; exit 0 |
| `sanitize_observation_source` receives novel future source type | Falls back to `"UserPromptSubmit"` — observation recorded with approximate label; no error |
| `extract_transcript_block` panics (`.unwrap()` bug) | Hook exits non-zero; FR-03.7 violated; detected by R-01 SR-07 test |

---

## OQ-SPEC-1 Risk Assessment: Assistant turn with no text blocks

**Question**: When an assistant turn has only `tool_use` + `thinking` blocks (no `type: "text"`), should the exchange pair be emitted (with tool pairs only) or suppressed?

**Failure modes by choice**:

| Choice | Failure Mode | Severity |
|--------|-------------|----------|
| Always suppress | Autonomous delivery runs are almost entirely tool-call turns. All context is suppressed; agent gets zero restoration despite a full session history. | High |
| Always emit | Empty `[Assistant]` section emitted; agent sees paired `[User]` text and tool pairs without assistant commentary. Cosmetically odd but semantically complete. | Low |

**Recommendation**: Emit the pair if at least one `ToolPair` is present. If the pair contains neither `AssistantText` nor `ToolPair`, suppress it (pure-`thinking` turns carry no actionable content). This is the minimum viable rule: emit when there is something worth showing, suppress when empty.

**Required spec addition**: FR-02.4 should be extended with: "If an assistant message contains zero `type: 'text'` blocks but at least one `type: 'tool_use'` block, the `[Assistant]` line is omitted and only the tool pair lines are emitted under the `[User]` line of the exchange. If both text and tool_use are absent (e.g., only `thinking`), the exchange pair is suppressed entirely."

---

## OQ-SPEC-2 Risk Assessment: SeekFrom::End(-N) when N > file size

**Question**: `std::io::seek(SeekFrom::End(-N))` when N > file size — spec notes this returns `Ok(0)` on Linux but recommends an explicit clamp.

**Risk**: The ADR-001 implementation uses an explicit conditional (`if file_len > window { seek } else { /* read from start */ }`) which avoids the seek entirely for small files. This is correct. The risk is if the conditional is miscoded (wrong comparator, `file_len` computed incorrectly, integer overflow converting `u64` to `i64` for the seek offset).

**Failure modes by implementation approach**:

| Approach | Failure Mode |
|----------|-------------|
| Conditional (ADR-001 as designed) | If condition is wrong (`>=` instead of `>`), a file of exactly `window` bytes triggers an unnecessary seek to position 0 — harmless but wasteful. |
| Unconditional seek | If `N > file_len`, behavior is platform-dependent. Linux POSIX: seek to position 0. Rust stdlib documents this as `Ok(0)` on Linux; other platforms may error. |
| Explicit clamp (`let n = window.min(file_len)`) | Safest: always produces a valid seek position. `SeekFrom::End(-(n as i64))` where `n <= file_len` is always valid. |

**Recommendation**: Add the explicit clamp as the implementation, not the conditional. The clamp is shorter, clearer, and eliminates the N > file_size class of bugs entirely:

```rust
let window = (MAX_PRECOMPACT_BYTES * TAIL_MULTIPLIER) as u64;
let seek_back = window.min(file_len);
if seek_back > 0 {
    file.seek(SeekFrom::End(-(seek_back as i64))).ok()?;
}
```

This handles `file_len = 0` (seek_back = 0, no seek), `file_len < window` (seek_back = file_len, seeks to start = `SeekFrom::Start(0)` equivalent), and `file_len >= window` (seek_back = window, correct tail read). No risk of issuing a seek with `|offset| > file_len`.

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01: JSONL format could change silently across Claude Code versions | R-04 | ADR-001/002 design is fail-open: unknown `type` values silently skipped. Format drift produces degraded output (empty snippets), not errors. Adjacent-record pairing edge cases tested explicitly. |
| SR-02: Sync I/O on large JSONL file may violate sub-50ms budget | R-02 | ADR-001 tail-bytes read cap (12 KB) bounds I/O to ~0.1–1ms on SSD. TAIL_MULTIPLIER = 4 is the tuning knob; insufficiency degrades gracefully (fewer pairs). R-02 covers the edge case where the window contains no parseable pairs. |
| SR-03: MAX_PRECOMPACT_BYTES is a compile-time constant with no runtime override | — | Accepted. Constant carries doc comment pointing to `config.toml` as future surface (FR-04.4). No architecture risk — a future config pass addresses this. |
| SR-04: Output format when BriefingContent is empty | R-12 | `prepend_transcript()` handles all four output cases explicitly. SR-04 resolved: transcript-only output emits section header; no merged/ambiguous block. Tested via R-12 scenarios. |
| SR-05: GH #354 security fix may be under-reviewed | R-07 | `sanitize_observation_source` is a named helper with a doc comment identifying it as the sole write gate. Dedicated unit test required (AC-11, FR-07.6). Integration test over UDS rounds out coverage. |
| SR-06: crt-027 API dependency risk | R-13 | Consumed symbols are compile-time dependencies — any rename is an immediate build failure. Mitigated by gating crt-028 delivery on a passing `cargo check` post-crt-027 merge. |
| SR-07: Graceful degradation miscoped — briefing suppressed on transcript failure | R-01 | ADR-003 structurally enforces the boundary: `Option<String>` return type, no `Result` propagation, `and_then` call site. SR-07 explicit test required (FR-06.7). Lesson #699 evidence used to elevate this to Critical priority. |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 3 (R-01, R-02, R-03) | 11 scenarios minimum |
| High | 5 (R-04, R-05, R-06, R-07, R-08) | 15 scenarios minimum |
| Med | 4 (R-09, R-10, R-11, R-12) | 10 scenarios minimum |
| Low | 1 (R-13) | 2 scenarios (compile-time CI) |

**Total minimum scenarios**: 38 across 13 risks.

**Non-negotiable tests** (failure blocks gate):
- R-01: briefing written even when transcript path is non-existent (SR-07 invariant, Lesson #699)
- R-03: zero-byte file and window-boundary seek behavior (OQ-SPEC-2)
- R-07: `sanitize_observation_source` unit test, all 6 cases (AC-11 / GH #354)
- R-08: quarantine exclusion in `IndexBriefingService::index()` (AC-12 / GH #355)
- R-10: tool-only assistant turn emitted, pure-thinking turn suppressed (OQ-SPEC-1 resolved, FR-02.4)

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for lesson-learned, risk patterns, and hook/SQLite topics -- found entry #699 (silent None in hook pipeline, directly informed R-01 Critical rating) and entry #3331 (crt-028 WA-5 pattern, confirming architecture alignment).
- Stored: nothing novel to store -- the degradation-boundary risk pattern (R-01 / SR-07) is already captured in Lesson #699 and the crt-028 WA-5 architecture entry (#3331). No cross-feature pattern emerges that is not already documented.
