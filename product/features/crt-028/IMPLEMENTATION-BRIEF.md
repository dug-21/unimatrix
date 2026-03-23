# crt-028: WA-5 PreCompact Transcript Restoration — Implementation Brief

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/crt-028/SCOPE.md |
| Scope Risk Assessment | product/features/crt-028/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/crt-028/architecture/ARCHITECTURE.md |
| Specification | product/features/crt-028/specification/SPECIFICATION.md |
| Risk Strategy | product/features/crt-028/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/crt-028/ALIGNMENT-REPORT.md |

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| hook.rs (transcript extraction + prepend) | pseudocode/hook.md | test-plan/hook.md |
| listener.rs (source allowlist fix GH #354) | pseudocode/listener.md | test-plan/listener.md |
| index_briefing.rs (quarantine test + doc GH #355) | pseudocode/index_briefing.md | test-plan/index_briefing.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

---

## Goal

crt-028 completes the WA-5 PreCompact transcript restoration path by reading the session
transcript locally in the hook process and prepending a structured context restoration block
to the compaction output, giving agents both task continuity and Unimatrix knowledge context
after a context-window compaction. It also fixes two security/test gaps left open by the
crt-027 security review (GH #354: `source` field allowlist in `listener.rs`; GH #355:
quarantine exclusion regression test in `IndexBriefingService`).

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| D-1: Transcript read location | Local in hook process; no transcript content sent to server; no server schema changes | SCOPE.md D-1, D-7 | — |
| D-2: Extraction strategy | Type-aware reverse scan: user `type:"text"` only; assistant `type:"text"` + `type:"tool_use"` paired with results; `type:"thinking"` and `type:"tool_result"` in user turns skipped | SCOPE.md D-2 | — |
| D-3: Tool results included as snippets | `type:"tool_result"` truncated to ~300 bytes per result; paired with tool_use via adjacent-record scan | SCOPE.md D-3 | architecture/ADR-002-tool-use-result-pairing.md |
| D-4: Separate injection budget | `MAX_PRECOMPACT_BYTES = 3000` is a distinct constant from `MAX_INJECTION_BYTES = 1400` | SCOPE.md D-4, Constraint 5 | — |
| D-5: Transcript prepends briefing | Transcript block always precedes `BriefingContent`; task continuity before knowledge context | SCOPE.md D-5 | — |
| D-6: Graceful degradation | All transcript read/parse failures → `None`; `BriefingContent` always written; hook always exits 0 | SCOPE.md D-6, Constraint 3 | architecture/ADR-003-graceful-degradation-contract.md |
| D-7: No server changes for transcript | Extraction entirely hook-side; wire protocol unchanged | SCOPE.md D-7 | — |
| D-8: Index format at compaction | `handle_compact_payload` uses `IndexBriefingService` (crt-027); full-content briefing already replaced | SCOPE.md D-8 | — |
| D-9: GH #354 and #355 in scope | `source` field allowlist fix in `listener.rs`; quarantine regression test + doc in `index_briefing.rs` | SCOPE.md D-9 | architecture/ADR-004-source-field-allowlist.md |
| ADR-001: Tail-bytes read strategy | Seek to `max(0, file_end - TAIL_WINDOW_BYTES)` where `TAIL_WINDOW_BYTES = MAX_PRECOMPACT_BYTES * TAIL_MULTIPLIER = 12,000 bytes`; first line after mid-file seek discarded by fail-open parser | ARCHITECTURE.md, SCOPE-RISK-ASSESSMENT SR-02 | architecture/ADR-001-tail-bytes-transcript-read.md |
| ADR-002: Tool-use/result pairing | Adjacent-record scan (one-pass look-ahead by one record); match by `tool_use_id`; unmatched tool_use emits empty snippet; orphaned tool_result silently skipped | ARCHITECTURE.md | architecture/ADR-002-tool-use-result-pairing.md |
| ADR-003: Degradation contract scope | `extract_transcript_block` returns `Option<String>`; all failures return `None`; call site uses `and_then`; no Result propagates | ARCHITECTURE.md, RISK-TEST-STRATEGY R-01 | architecture/ADR-003-graceful-degradation-contract.md |
| ADR-004: Source field allowlist strategy | Allowlist with fallback to default (`"UserPromptSubmit"`); named helper `sanitize_observation_source`; no length cap needed | ARCHITECTURE.md, RISK-TEST-STRATEGY R-07 | architecture/ADR-004-source-field-allowlist.md |
| OQ-2 (resolved): Transcript read timing | Read transcript before `transport.request()`; store as `Option<String>`; prepend in response handler; `write_stdout` is not responsible for file reading | ARCHITECTURE.md OQ-2 | — |
| OQ-3 (resolved): Key-param extraction | Hardcoded map for 10 known Claude Code tools (Bash→command, Read/Edit/Write/MultiEdit→file_path, Glob/Grep→pattern, Task→description, WebFetch→url, WebSearch→query); first-string-field fallback for unknowns; key-param truncated to 120 bytes | ARCHITECTURE.md Key-Param Map | — |
| OQ-SPEC-1 (resolved): Tool-only assistant turns | If assistant turn has zero `type:"text"` but at least one `type:"tool_use"`, emit the pair with tool lines only (no `[Assistant]` header line); suppress entirely only if both text and tool_use are absent | RISK-TEST-STRATEGY OQ-SPEC-1, ALIGNMENT-REPORT WARN 1 | — |

---

## Files to Create or Modify

| File | Change | Summary |
|------|--------|---------|
| `crates/unimatrix-server/src/uds/hook.rs` | Modify | Add `MAX_PRECOMPACT_BYTES`, `TAIL_MULTIPLIER`, `TOOL_RESULT_SNIPPET_BYTES`, `TOOL_KEY_PARAM_BYTES` constants; add `ExchangeTurn` enum; add `extract_transcript_block`, `build_exchange_pairs`, `prepend_transcript`, `extract_key_param` functions; modify PreCompact arm in `run()` to read transcript and prepend to response |
| `crates/unimatrix-server/src/uds/listener.rs` | Modify | Add `sanitize_observation_source` private helper; replace inline `source.as_deref().unwrap_or(...)` with `sanitize_observation_source(source.as_deref())` in `dispatch_request` ContextSearch arm (GH #354) |
| `crates/unimatrix-server/src/services/index_briefing.rs` | Modify | Add doc comment on `index()` documenting validation delegation to `SearchService`; add regression test `index_briefing_excludes_quarantined_entry` (GH #355) |

No new crates. No schema changes. No migrations. `unimatrix-engine/src/wire.rs` requires no changes.

---

## Data Structures

### `ExchangeTurn` (internal enum, `uds/hook.rs`)

```rust
enum ExchangeTurn {
    UserText(String),
    AssistantText(String),
    ToolPair { name: String, key_param: String, result_snippet: String },
}
```

Used internally by `build_exchange_pairs` and `extract_transcript_block`. Not exported.

### Constants (`uds/hook.rs`)

| Constant | Value | Purpose |
|----------|-------|---------|
| `MAX_INJECTION_BYTES` | 1400 (existing) | UserPromptSubmit / SubagentStart injection budget |
| `MAX_PRECOMPACT_BYTES` | 3000 (new) | Transcript block budget at PreCompact — separate constant (D-4, AC-10) |
| `TAIL_MULTIPLIER` | 4 (new) | Raw-to-extracted ratio for tail-bytes window (`TAIL_WINDOW_BYTES = 12,000 bytes`) |
| `TOOL_RESULT_SNIPPET_BYTES` | 300 (new) | Per-tool-result truncation budget |
| `TOOL_KEY_PARAM_BYTES` | 120 (new) | Key-param field truncation in compact tool representation |
| `HOOK_TIMEOUT` | 40ms (existing) | Transport timeout |

`MAX_PRECOMPACT_BYTES` must carry a doc comment noting it is a tunable for a future `config.toml` pass (SR-03 acknowledgment).

### Existing wire types consumed (no changes)

- `HookInput.transcript_path: Option<String>` — already present with `#[serde(default)]`
- `HookResponse::BriefingContent { content: String, token_count: u32 }` — existing variant

---

## Function Signatures

### `extract_transcript_block(path: &str) -> Option<String>` (new, `uds/hook.rs`)

Reads tail bytes of transcript file at `path`, parses JSONL window, formats exchange pairs
within `MAX_PRECOMPACT_BYTES`. Returns `None` on any I/O or parse failure. Never panics.
Uses an inner closure to contain all `?` operators and map all errors to `None` (ADR-003).

Internal steps: open file → get file length → compute seek window → seek or read from start
→ collect `BufReader` lines → call `build_exchange_pairs` → fill budget most-recent-first
→ format with `=== Recent conversation ===` / `=== End recent conversation ===` headers.

### `build_exchange_pairs(lines: &[&str]) -> Vec<ExchangeTurn>` (new, `uds/hook.rs`)

Parses JSONL lines defensively (unknown `type` values skipped; malformed lines skipped).
Performs adjacent-record scan for tool-use/result pairing (ADR-002). Returns `Vec<ExchangeTurn>`
in reverse-chronological order (Vec reversed before return).

OQ-SPEC-1 rule: an assistant turn with zero `type:"text"` blocks but at least one
`type:"tool_use"` block → emit `ToolPair` entries only (no `AssistantText`). A turn with
neither text nor tool_use (e.g., thinking-only) → suppressed entirely.

### `prepend_transcript(transcript: Option<&str>, briefing: &str) -> String` (new, `uds/hook.rs`)

Combines optional transcript block with briefing content. Four explicit cases:
- Both present: `transcript + "\n" + briefing`
- Transcript only (briefing empty): transcript block verbatim (header already present)
- Briefing only (transcript None): briefing verbatim (no header injected)
- Both empty: `""`

Section separator between transcript footer and briefing content when both are non-empty
(FR-02.6). No byte-cap applied here; transcript block is already within `MAX_PRECOMPACT_BYTES`.

### `extract_key_param(tool_name: &str, input: &serde_json::Value) -> String` (new, private, `uds/hook.rs`)

Hardcoded map for 10 known Claude Code tools; first-string-field fallback for unknowns.
Key-param truncated to `TOOL_KEY_PARAM_BYTES` (120 bytes) via `truncate_utf8`.

Known tool → field mappings: `Bash`→`command`, `Read`/`Edit`/`Write`/`MultiEdit`→`file_path`,
`Glob`/`Grep`→`pattern`, `Task`→`description`, `WebFetch`→`url`, `WebSearch`→`query`.

### `sanitize_observation_source(source: Option<&str>) -> String` (new, private, `uds/listener.rs`)

Allowlist match against `{"UserPromptSubmit", "SubagentStart"}`; any other value (including
`None`, empty string, excessively long strings) falls back to `"UserPromptSubmit"` (ADR-004).
This function is the sole write gate for the `hook TEXT NOT NULL` column in the observations
table. Must be documented as such to prevent bypass in future code.

---

## Output Format Contract

```
=== Recent conversation (last N exchanges) ===
[User] {user text}
[Assistant] {assistant text}
[tool: {name}({key_param}) → {snippet}]

[User] {next user text}
[tool: {name}({key_param}) → {snippet}]

=== End recent conversation ===

{BriefingContent from IndexBriefingService}
```

When `BriefingContent.content` is empty and transcript block is present: transcript block
only, no Unimatrix section. When both are empty: empty stdout (FR-01.4 invariant).
Tool pairs appear in exchange order; `[Assistant]` header line is omitted when the assistant
turn has no text blocks (only tool_use — OQ-SPEC-1 resolution).

---

## Constraints

1. **crt-027 must be merged first.** WA-5 depends on `IndexBriefingService`, `IndexEntry`,
   and `format_index_table` from crt-027 (GH #350). Building without them is not viable.
2. **No tokio runtime in hook process.** All I/O in `hook.rs` must use `std::io`
   (`std::fs::File`, `std::io::BufReader`, `std::io::Seek`). No `tokio::fs`.
3. **Hook must always exit 0.** FR-03.7 is a hard invariant. All transcript read failures
   return `None` from `extract_transcript_block`; `BriefingContent` is always written.
4. **No server protocol changes.** `HookRequest::CompactPayload` is unchanged.
5. **`MAX_PRECOMPACT_BYTES` is a separate constant.** Must not alias or reuse
   `MAX_INJECTION_BYTES`. Separate budget for the highest-value hook path.
6. **Transcript JSONL may contain malformed lines and empty lines.** Parser must skip them
   silently (AC-08). The first line after a mid-file seek may be truncated JSON — skipped
   by fail-open `serde_json::from_str` failure.
7. **GH #354 write site is in `listener.rs`.** The `source` field travels over UDS from
   hook to server; all validation occurs server-side at the single `sanitize_observation_source`
   call site.
8. **`SeekFrom::End(-N)` must never issue N > file_len.** Use explicit clamp:
   `let seek_back = window.min(file_len); if seek_back > 0 { seek(SeekFrom::End(-(seek_back as i64))) }`.
   Handles zero-byte files and files smaller than the window (OQ-SPEC-2 / R-03 mitigation).
9. **No stderr output on graceful degradation.** `extract_transcript_block` failures are
   silent. No logging of missing or malformed transcript files (NFR-03 / ADR-003).

---

## Dependencies

### Feature dependencies

- **crt-027** (GH #350) — must be merged and passing `cargo check` before crt-028 delivery
  begins. Provides: `IndexBriefingService::index()`, `IndexEntry`, `format_index_table`,
  migrated `handle_compact_payload`. Any rename of these symbols is a compile-time break.

### Crate dependencies (no new crates)

- `serde_json` — already in `unimatrix-server` dependency tree; used by `build_exchange_pairs`
  for `serde_json::Value` JSONL parsing.
- `std::io::{BufRead, BufReader, Seek, SeekFrom}` — stdlib; no new external dep.

### Tracked GH issues in scope

- **GH #354** — `source` field allowlist in `listener.rs` observations write site
- **GH #355** — quarantine exclusion regression test + doc comment in `index_briefing.rs`

---

## NOT in Scope

- Server-side transcript storage — transcript is read locally only; no schema changes.
- Persistent transcript summaries — extraction is ephemeral; not stored in Unimatrix.
- PostCompact hook changes — restoration only at PreCompact.
- Full verbatim message replay — `tool_result` blocks in user turns are skipped; only
  compact tool_use+result pairs are extracted.
- Configurable `MAX_PRECOMPACT_BYTES` at runtime — compile-time constant only.
- Session-injection affinity ranking boost at compaction (OQ-1) — deferred; pure fused
  score is acceptable for the initial implementation.
- GH #303, #305, or any pre-existing open issues not listed in D-9.
- Field denylist for key-param fallback (R-09 follow-up) — documented limitation; deferred.

---

## Alignment Status

**Overall: PASS with two resolved pre-synthesis issues.**

Vision alignment is confirmed: crt-028 directly implements WA-5 from the product roadmap
(Wave 1A). All four vision-to-scope mappings checked: local-only transcript read, `===`
output headers, 3000-byte injection limit, and independence from WA-1 through WA-4. The
sole external dependency (crt-027) is a Wave 1A predecessor. No future-milestone
capabilities were pulled in.

**VARIANCE 1 — Header string mismatch (RESOLVED before synthesis)**

RISK-TEST-STRATEGY.md R-12 used `"--- Recent Context ---"` / `"--- Unimatrix Knowledge ---"`
while SPECIFICATION.md FR-02 defines `"=== Recent conversation (last N exchanges) ==="` /
`"=== End recent conversation ==="`. The vision guardian identified this as a testability
gap: tester assertions would have contradicted the spec. Resolution: RISK-TEST-STRATEGY
R-12 assertions must reference the `===` format from the spec. Implementers must use the
`===` headers from SPECIFICATION.md. The `---` headers in ARCHITECTURE.md data-flow
diagrams are informal illustrations only.

**WARN 1 — OQ-SPEC-1 resolution not in SPECIFICATION.md (RESOLVED before synthesis)**

RISK-TEST-STRATEGY identified that FR-02.4 required extension to cover assistant turns
with no text blocks (tool-only turns). The tester's R-10 scenarios were blocked on this
clarification. Resolution added to this brief: if an assistant turn has zero `type:"text"`
blocks but at least one `type:"tool_use"` block, emit the pair with tool pair lines only
(omit `[Assistant]` header line). Suppress the exchange entirely only if both text and
tool_use are absent. Implementer must apply this rule in `build_exchange_pairs`.

**Open question deferred (non-blocking):**
OQ-1 — session-injection affinity ranking at compaction time. Pure fused score is
acceptable for the initial implementation. A future feature may reintroduce injection-history
tie-breaking in `handle_compact_payload`. This does not affect hook.rs scope.
