# Security Review Report — vnc-013 (Second Pass)

**Agent ID:** vnc-013-security-reviewer-2
**Date:** 2026-04-17
**PR:** #568 (branch: feature/vnc-013)
**GH Issue:** #567
**Risk Level:** LOW
**Blocking findings:** No

---

## Previous Findings Verified Fixed

**F-01 (KNOWN_PROVIDERS allowlist)** — Correct and complete. `hook.rs` lines 150–161:
`const KNOWN_PROVIDERS: &[&str] = &["claude-code", "gemini-cli", "codex-cli"]`. Unknown
values log a warning via `eprintln!` and shadow `provider` to `None`, activating the
inference path. Logic is sound.

**F-02 (Exact match on context_cycle)** — Correct and complete. `build_cycle_event_or_fallthrough()`
now uses exact equality:
```
tool_name != "context_cycle" && tool_name != "mcp__unimatrix__context_cycle"
```
The `contains("context_cycle")` injection surface is fully closed. Both known forms
of the tool name are enumerated.

**F-03 (source_domain never persisted)** — Architecture doc accurate. `ObservationRow`
has no `source_domain` field. The observations table has no `source_domain` column.
Source_domain is runtime-derived at each read site. `content_based_attribution_fallback`
in listener.rs is confirmed as a DB read path using `DEFAULT_HOOK_SOURCE_DOMAIN`.

---

## New Findings — Second Pass

**N-01 (Low, Non-blocking): Dead sentinel empty string in hint path**

Location: `crates/unimatrix-server/src/uds/hook.rs` lines 163–177

When `provider.is_some()` (hint path), `provider_str` is assigned `""`. The downstream
assignment uses `if let Some(ref hint) = provider` — the same condition — so `provider_str`
is never dereferenced. The logic is currently safe. However, the two conditions are
maintained independently; a future refactor could silently produce `hook_input.provider = Some("")`.

Recommendation: Replace `""` with `unreachable!()` or restructure into a single `match`
arm. Not blocking for this PR.

**N-02 (Low, Non-blocking): Stale event list in RISK-TEST-STRATEGY.md**

`RISK-TEST-STRATEGY.md` lists `PostToolUseFailure` in the builtin claude-code pack, but
`builtin_claude_code_pack()` in `domain/mod.rs` does not include it (uses `SubagentStop`
instead). Documentation inaccuracy only — no code defect. Approach A fallback covers both.

**N-03 (Low, Non-blocking): SessionRegister does not carry provider field**

When Gemini fires `SessionStart` with `--provider gemini-cli`, the hint path sets
`hook_input.provider = Some("gemini-cli")`, but `HookRequest::SessionRegister` carries no
`provider` field — Gemini origin is lost at session registration. Consistent with documented
OQ-4 limitation (source_domain never persisted). Not blocking.

---

## OWASP Assessment

| Concern | Finding |
|---------|---------|
| SQL Injection | None — provider never in SQL construction; all DB writes use sqlx parameterized queries |
| Command Injection | None — provider value never reaches a shell or exec path |
| Path Traversal | None — no new file path operations |
| Injection via mcp_context.tool_name | Closed by F-02 fix — exact equality gate on both known tool name forms |
| Deserialization | `mcp_context: Option<Value>` with `serde(default)` — non-object values handled gracefully |
| Broken Access Control | None — no capability or trust boundary changes |
| Input Validation | KNOWN_PROVIDERS allowlist active; event names matched exhaustively |
| Secrets | None — no hardcoded credentials, API keys, or tokens in the diff |

---

## Blast Radius Assessment

Worst case: Gemini events attributed as `"claude-code"` — identical to pre-fix behavior
and the documented degraded mode. No data loss, no data corruption, no credential exposure,
no privilege escalation, no denial of service.

## Regression Risk

All 2910+ tests pass, zero failures. Existing Claude Code hook flows unchanged: `#[serde(default)]`
on all new fields means all existing JSON deserializes without error. The `_registry` → `registry`
rename in `parse_observation_rows` activates previously dead code intentionally and is covered
by 6 new Approach A tests.

---

## Verdict

**PASS — Merge Ready**

All previous findings (F-01, F-02, F-03) are correctly and completely fixed. Three new
low-severity non-blocking findings identified (N-01, N-02, N-03). None block merge.
