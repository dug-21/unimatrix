# SPECIFICATION: crt-027 — WA-4 Proactive Knowledge Delivery

## Objective

When the SM spawns a subagent via SubagentStart, the subagent currently receives zero
knowledge from Unimatrix before its first token because the hook falls through to a
fire-and-forget `RecordEvent`. This feature routes SubagentStart to `ContextSearch` so
the subagent receives injected knowledge before it starts working. In parallel, the
`context_briefing` MCP tool is replaced with an index-format implementation —
active-only entries, flat table output, k=20 default — and the CompactPayload UDS path
is migrated from `BriefingService` to the same index format, providing WA-5 a clean
surface for transcript prepend.

---

## Functional Requirements

### WA-4a: SubagentStart Hook Injection

**FR-01** — `build_request` in `hook.rs` MUST add a `"SubagentStart"` match arm before
the `_` fallthrough. When `prompt_snippet` is non-empty, the arm returns
`HookRequest::ContextSearch` with:
- `query` = `input.extra["prompt_snippet"]` as `String`
- `session_id` = `input.session_id` (the parent session; not the ppid fallback)
- `source` = `Some("SubagentStart".to_string())`
- `role`, `task`, `feature`, `k`, `max_tokens` = `None`

**FR-02** — When `prompt_snippet` is absent or empty in a SubagentStart event,
`build_request` MUST fall through to `generic_record_event` (fire-and-forget
`RecordEvent`). No `ContextSearch` is emitted and no content is written to stdout.
The hook exits 0.

**FR-03** — `HookRequest::ContextSearch` MUST gain an optional `source: Option<String>`
field with `#[serde(default)]`. When absent, the field deserializes to `None` and is
treated as `"UserPromptSubmit"`. `dispatch_request` in `listener.rs` MUST use this field
for the observation `hook` column instead of the hardcoded literal `"UserPromptSubmit"`.

**FR-04** — SubagentStart `ContextSearch` requests MUST be synchronous (not
fire-and-forget). The response is written to stdout by the hook process. The existing
`is_fire_and_forget` match in `hook.rs` does not include `ContextSearch` and requires
no change.

**FR-05** — `hook.rs` MUST define a compile-time constant `MIN_QUERY_WORDS: usize = 5`.
The `UserPromptSubmit` arm in `build_request` MUST count whitespace-delimited words in
the query string. If `word_count < MIN_QUERY_WORDS`, the arm MUST fall through to
`generic_record_event` (no `ContextSearch` emitted, no injection). This guard applies
ONLY to `UserPromptSubmit`. The SubagentStart arm uses the existing empty-string guard
(FR-02) and is unaffected by `MIN_QUERY_WORDS`.

**FR-06** — The hook exit code MUST remain 0 for all SubagentStart outcomes: empty
`prompt_snippet`, server unavailable, search error, and successful injection. This
preserves the existing FR-03.7 invariant from `hook.rs`.

### WA-4b: IndexBriefingService

**FR-07** — A new `IndexBriefingService` struct MUST be introduced that replaces
`BriefingService` as the backend for both the `context_briefing` MCP tool and
`handle_compact_payload`. `IndexBriefingService` MUST accept:
- `topic` (required `String`): the search query (see FR-11 for derivation)
- `session_id` (optional `String`): used for WA-2 histogram boost via
  `ServiceSearchParams.session_id`
- `k` (`usize`, default 20): maximum result count

**FR-08** — `IndexBriefingService` MUST query `status = Active` entries only. Deprecated
entries MUST be suppressed at query time and MUST NOT appear in the output.

**FR-09** — `IndexBriefingService` ranking MUST use the existing fused score
(similarity + confidence + WA-2 histogram boost) via `ServiceSearchParams`. No new
phase-conditioned weighting is added in this feature.

**FR-10** — `IndexBriefingService` MUST return `Vec<IndexEntry>`, where `IndexEntry` is
defined as:
```
pub struct IndexEntry {
    pub id: u64,
    pub topic: String,       // entry.topic (direct field, no join)
    pub category: String,
    pub confidence: f64,
    pub snippet: String,     // first 150 chars of entry.content, UTF-8 boundary safe
}
```
The `snippet` field MUST be truncated at a valid UTF-8 character boundary at or before
150 characters. The `name` field from `EntryRecord` is NOT part of `IndexEntry` (the
flat table uses `topic` in its place per the SCOPE design decision).

**FR-11** — Query derivation for `IndexBriefingService` MUST follow the same three-step
priority in both call sites (MCP tool and `handle_compact_payload`). The logic MUST be
extracted to a shared helper function to prevent divergence:
1. If `task` param is explicitly provided and non-empty: use it as the search query.
2. If `task` is absent/empty AND `session_id` is present: synthesize from
   `feature_cycle` + top 3 `topic_signals` by vote count, looked up from
   `SessionRegistry` (MCP path) or the held session state (UDS path).
3. If no session state or `topic_signals` is empty: use the `topic` param string
   (e.g., `"crt-027"`) as the query.

**FR-12** — The output format for both `context_briefing` and `handle_compact_payload`
MUST be a flat indexed table with the following exact columns, in this order:
```
#    id   topic               cat             conf   snippet
─────────────────────────────────────────────────────────────────────────────────────
 1   2    product-vision      decision        0.60   Unimatrix is a self-learning...
```
- Row number (right-justified in 2 chars minimum)
- Entry ID
- Topic (truncated to fit column)
- Category
- Confidence (2 decimal places)
- Snippet (150 chars, UTF-8 boundary safe)

No section headers (`Decisions` / `Injections` / `Conventions`) appear in the output.
Active entries only.

**FR-13** — `context_briefing` default `k` MUST be 20. The `UNIMATRIX_BRIEFING_K`
environment variable MUST be treated as deprecated on the new index path: it is ignored
by `IndexBriefingService`. The default of 20 cannot be reduced via env var. Callers
that need a different `k` MUST pass it as a parameter. This decision MUST be documented
in the code with a comment at the point where the old env var was read.

**FR-14** — The `context_briefing` MCP tool signature MUST remain unchanged for backward
compatibility. `role` and `task` remain present as declared fields; `role` is ignored by
the new index path. Query derivation uses `task` if present, otherwise session state,
otherwise `topic` (FR-11). The `#[cfg(feature = "mcp-briefing")]` compilation guard
MUST continue to apply.

**FR-15** — `IndexBriefingService` MUST receive `EffectivenessStateHandle` (and the
`Arc<Mutex<EffectivenessSnapshot>>` cached snapshot) in its constructor, matching the
pattern established by `BriefingService`. Effectiveness-based ranking MUST NOT silently
degrade after `BriefingService` removal.

### Compaction Path Migration

**FR-16** — `handle_compact_payload` MUST be updated to call `IndexBriefingService`
instead of `BriefingService::assemble()`. The `CompactionCategories` struct and the
three-field partition (`decisions`, `injections`, `conventions`) MUST be removed.
`format_compaction_payload` MUST be rewritten to consume `Vec<IndexEntry>` and emit the
flat indexed table format (FR-12).

**FR-17** — The `category_histogram` block (crt-026 WA-2) MUST be preserved in the
`handle_compact_payload` output. The flat indexed table replaces the entry sections, but
the histogram summary block that existed in the prior format MUST still be rendered when
the histogram is non-empty.

**FR-18** — `BriefingService`, its `assemble()` method, all its unit tests, and all its
re-exports from `services/mod.rs` MUST be deleted once both call sites are migrated.
No dead code or deferred cleanup is permitted (AC-13). `HookRequest::Briefing` in
`wire.rs` is a separate variant not owned by this feature and MUST NOT be removed.

### Protocol Update

**FR-19** — `uni-delivery-protocol.md` MUST be updated to include a
`context_briefing(topic="{feature-id}")` call immediately after each of the following
points:
- After `context_cycle(type: "start", ...)`
- After each `context_cycle(type: "phase-end", ...)` (five call sites in the current
  protocol)

The briefing result MUST be included as a knowledge package in the context of each
spawned agent for the subsequent phase.

---

## Non-Functional Requirements

**NFR-01** — The SubagentStart hook round-trip (connect + request + response) MUST
complete within the existing 40 ms `HOOK_TIMEOUT` budget. No new timeout is introduced.

**NFR-02** — The `IndexBriefingService` query MUST complete within the existing MCP
handler timeout (`MCP_HANDLER_TIMEOUT`). k=20 semantic search with histogram boost is
within normal operating parameters for the existing `SearchService`.

**NFR-03** — The flat indexed table MUST respect the existing byte budget of
`MAX_COMPACTION_BYTES` on the `handle_compact_payload` path. Budget enforcement MUST
apply to the flat table as a whole (not per-section as before). If the full k=20 result
set exceeds the budget, rows are truncated from the end (lowest-ranked entries dropped
first).

**NFR-04** — `IndexEntry.snippet` MUST be truncated at a valid UTF-8 character boundary.
Multi-byte characters (e.g., CJK, emoji) MUST NOT be split. The truncation MUST use the
existing UTF-8 boundary-safe pattern already used in `format_compaction_payload`.

**NFR-05** — The `IndexBriefingService` construction MUST be compatible with the
`#[cfg(feature = "mcp-briefing")]` flag. The flag guards only the MCP tool registration.
`IndexBriefingService` itself SHOULD compile regardless of the flag so the
`handle_compact_payload` path (always compiled) continues to function.

**NFR-06** — All existing tests in `listener.rs`, `hook.rs`, and `tools.rs` that are
not directly tied to the removed `CompactionCategories` struct or `BriefingService` MUST
pass without modification. Tests tied to removed constructs MUST be rewritten (not
deleted) to cover the surviving invariants listed in AC-16 through AC-21.

**NFR-07** — The SM briefing calls added to `uni-delivery-protocol.md` MUST specify a
`max_tokens` cap of 1000 tokens per call to bound context window consumption at phase
boundaries.

---

## Acceptance Criteria

### WA-4a: SubagentStart Routing

**AC-01** — Unit test on `build_request`: a SubagentStart event with
`input.extra["prompt_snippet"] = "implement the spec writer agent"` returns
`HookRequest::ContextSearch { query: "implement the spec writer agent", source: Some("SubagentStart"), session_id: <parent_sid>, ... }`.
Verification: `hook.rs` unit test asserting struct variant and field values.

**AC-02** — Unit test on `build_request`: a SubagentStart event with absent or empty
`prompt_snippet` returns `HookRequest::RecordEvent` (not `ContextSearch`).
Verification: `hook.rs` unit test covering (a) key absent, (b) key present with `""`.

**AC-02b** — `hook.rs` defines `MIN_QUERY_WORDS: usize = 5`. Unit tests:
- `UserPromptSubmit` with query `"yes ok thanks"` (3 words) → `RecordEvent`
- `UserPromptSubmit` with query `"ok"` (1 word) → `RecordEvent`
- `UserPromptSubmit` with query `"implement the spec writer agent"` (5 words) →
  `ContextSearch`
- `UserPromptSubmit` with query `"implement the spec writer agent today"` (6 words) →
  `ContextSearch`
- SubagentStart with `prompt_snippet = "ok"` (1 word, non-empty) → `ContextSearch`
  (SubagentStart uses empty-string guard only, not `MIN_QUERY_WORDS`)
Verification: five `hook.rs` unit tests on `build_request` boundary cases.

**AC-03** — SubagentStart `ContextSearch` carries `session_id = input.session_id`
(not the ppid fallback). When a registered session exists, `handle_context_search`
applies WA-2 histogram boost via `ServiceSearchParams.category_histogram`.
Verification: unit test asserting `session_id` field is taken from `input.session_id`,
not from the ppid fallback expression.

**AC-04** — SubagentStart is synchronous: `is_fire_and_forget` returns `false` for
`HookRequest::ContextSearch`. The response is written to stdout via
`transport.request()`.
Verification: verified by reading `is_fire_and_forget` match arms (no code change
needed); assert in unit test that `ContextSearch` is NOT matched by the
fire-and-forget pattern.

**AC-05** — `HookRequest::ContextSearch` carries `source: Option<String>`
(`#[serde(default)]`). SubagentStart sets `source = Some("SubagentStart")`.
Existing `UserPromptSubmit` callers that omit `source` deserialize to `None`.
`dispatch_request` uses `source.as_deref().unwrap_or("UserPromptSubmit")` for the
observation `hook` column.
Verification: (a) `hook.rs` unit test: SubagentStart builds `source = Some("SubagentStart")`; (b) `listener.rs` integration test: `handle_context_search` with `source = Some("SubagentStart")` writes `hook = "SubagentStart"` to the observations table; (c) existing `UserPromptSubmit` test still writes `hook = "UserPromptSubmit"`.

**AC-SR01** — SubagentStart stdout injection is confirmed to work in Claude Code:
the hook process writes `HookResponse::Entries` to stdout and Claude Code injects the
content into the subagent before its first token.
Verification: integration smoke test or architect-confirmed documentation reference.
If unconfirmed at architecture time, this AC remains OPEN and the architect must add a
spike task or pivot the design to session-state-only recording before delivery begins.

### WA-4b: IndexBriefingService

**AC-06** — `context_briefing` returns only `status = Active` entries. A test database
containing one Active entry and one Deprecated entry with the same topic: the briefing
response includes the Active entry and does NOT include the Deprecated entry.
Verification: `tools.rs` integration test.

**AC-07** — `context_briefing` default `k` is 20. `UNIMATRIX_BRIEFING_K` env var is
not read by `IndexBriefingService`. A call with no `k` parameter returns up to 20
entries. A call with `k=5` returns up to 5 entries.
Verification: `tools.rs` unit/integration test asserting max result count; code
inspection confirms `UNIMATRIX_BRIEFING_K` is not referenced in `IndexBriefingService`.

**AC-08** — Both `context_briefing` and `handle_compact_payload` output a flat indexed
table with columns: row number, id, topic, category, confidence (2 decimal places),
150-char snippet. No section headers appear in the output.
Verification: `tools.rs` test asserting flat table presence and absence of
`"## Decisions"` / `"## Injections"` / `"## Conventions"` strings.

**AC-09** — Query derivation priority order is correct at both call sites:
(1) explicit `task` param used as query when provided;
(2) when `task` absent and `session_id` present, query is synthesized from
`feature_cycle` + top 3 `topic_signals` by vote count;
(3) when no session state or empty signals, `topic` param is used as query.
Both call sites use the same shared helper function.
Verification: unit tests covering all three steps in isolation; code inspection confirms
a single shared function is used.

**AC-10** — `handle_compact_payload` query derivation uses the already-held session
state for step 2 (no `SessionRegistry` lookup by ID). `context_briefing` MCP tool
uses `SessionRegistry` for step 2. Both delegate to the same shared helper.
Verification: code inspection and unit test for the UDS path confirming no registry
lookup when session state is held directly.

**AC-11** — `context_briefing` `session_id` parameter, when provided, is passed as
`ServiceSearchParams.session_id`, which causes the category histogram lookup for WA-2
boost to apply.
Verification: integration test: register a session, accumulate a category histogram
entry, call `context_briefing` with that `session_id` — the result ranks higher-histogram-category entries first relative to a call without `session_id`.

**AC-12** — `handle_compact_payload` no longer calls `BriefingService::assemble()`. It
calls `IndexBriefingService` and produces `HookResponse::BriefingContent` with the flat
indexed table format.
Verification: `BriefingService` import is absent from `listener.rs` after migration;
`listener.rs` integration test covering `HookRequest::CompactPayload` asserts flat table
format in response.

**AC-13** — `BriefingService` struct, `BriefingParams`, `BriefingResult`,
`InjectionSections`, `InjectionEntry`, all `BriefingService` methods, all tests in
`briefing.rs`, and all re-exports from `services/mod.rs` are deleted. No dead code
remains. `cargo build --release` emits no `dead_code` warnings for these types.
Verification: `briefing.rs` file deleted; `grep -r "BriefingService" crates/` returns
no results (excluding this specification).

**AC-14** — `uni-delivery-protocol.md` includes `context_briefing(topic="{feature-id}")`
immediately after every `context_cycle(type: "phase-end", ...)` call (five instances)
and after `context_cycle(type: "start", ...)` (one instance). Each call specifies
`max_tokens: 1000`.
Verification: diff of `uni-delivery-protocol.md` showing six insertion points.

**AC-15** — `cargo test` passes without any pre-existing tests being silently deleted.
The test count at the module level for `listener.rs`, `hook.rs`, and `tools.rs`
(briefing path) is non-decreasing from the pre-feature baseline.
Verification: CI run; test count comparison before/after.

### Surviving format_compaction_payload Invariants (SR-04)

The following invariants existed in `format_compaction_payload` tests and MUST be
preserved by new tests against the rewritten function. Old tests on `CompactionCategories`
are deleted as the struct is removed; their invariants are re-expressed here for the
flat table format.

**AC-16** — Byte budget enforcement: when the total serialized flat table exceeds
`max_bytes`, the output is truncated. The output byte length MUST be less than or equal
to `max_bytes`. Low-ranked rows are dropped first.
Verification: unit test with large content entries that exceed `MAX_COMPACTION_BYTES`
and a small `max_bytes` override, asserting `output.len() <= max_bytes`.

**AC-17** — UTF-8 boundary truncation: snippet truncation at 150 chars MUST land on a
valid UTF-8 character boundary. CJK characters (3 bytes each) MUST NOT be split.
Verification: unit test using `"\u{4e16}\u{754c}".repeat(200)` as entry content;
assert `snippet.is_char_boundary(snippet.len())` and `snippet.len() <= 150`.

**AC-18** — Empty result handling: when `IndexBriefingService` returns zero entries and
the histogram is also empty, `format_compaction_payload` returns `None`. When the
histogram is non-empty but entries are empty, the function returns `Some(...)` containing
the histogram block.
Verification: two unit tests matching the old `format_payload_empty_categories_returns_none`
and `test_compact_payload_histogram_only_categories_empty` cases.

**AC-19** — Confidence sort order: the flat table MUST be sorted by fused score
descending (highest-confidence entries first). The output for a two-entry result with
scores 0.9 and 0.3 MUST list the 0.9 entry before the 0.3 entry.
Verification: unit test with two `IndexEntry` values in low-first order; assert row 1
has confidence 0.90 and row 2 has confidence 0.30 in the rendered output.

**AC-20** — Token limit override: when `token_limit` is supplied to
`handle_compact_payload`, the byte budget is `min(token_limit * 4, MAX_COMPACTION_BYTES)`.
A call with `token_limit = 100` produces output of at most 400 bytes.
Verification: unit test on `format_compaction_payload` with `max_bytes = 400` and large
content; assert `output.len() <= 400`.

**AC-21** — Histogram block: when category histogram is non-empty, the output MUST
include the histogram block. When empty, the histogram block MUST be absent. The top-5
cap and count-descending sort apply to the histogram block as before.
Verification: two unit tests covering histogram-present and histogram-absent paths on
the rewritten `format_compaction_payload`.

### `MIN_QUERY_WORDS` Guard (SR-05)

**AC-22** — Boundary test for `MIN_QUERY_WORDS = 5` in `UserPromptSubmit`:
- 4-word prompt → `generic_record_event` (fire-and-forget, no injection)
- 5-word prompt → `ContextSearch`
Verification: two `hook.rs` unit tests on `build_request` using exact 4-word and 5-word
inputs.

**AC-23** — `MIN_QUERY_WORDS` is inapplicable to SubagentStart. SubagentStart with a
1-word non-empty `prompt_snippet` returns `HookRequest::ContextSearch` (not
`RecordEvent`).
Verification: `hook.rs` unit test with `event = "SubagentStart"` and `prompt_snippet =
"implement"` (one word) asserting `ContextSearch` is returned.

### Feature Flag and Wire Protocol (SR-02, SR-07)

**AC-24** — `IndexBriefingService` compiles and is exercised by `handle_compact_payload`
tests regardless of the `mcp-briefing` feature flag. `cargo test` (without
`--features mcp-briefing`) MUST pass all `handle_compact_payload` tests.
Verification: CI run without `--features mcp-briefing`.

**AC-25** — Any test that constructs `HookRequest::ContextSearch` via struct literal
MUST compile after the `source` field addition. Existing struct literal construction
uses `..` spread or explicitly adds `source: None`.
Verification: `cargo build --release` and `cargo test` both pass without
`dead_code`/`non-exhaustive` compile errors.

---

## Domain Models

### IndexEntry

The value type returned by `IndexBriefingService`. Replaces the three-field
`InjectionSections` partition. Ubiquitous language: "index entry" means a single
knowledge record in the flat briefing index.

```
IndexEntry {
    id:         u64,    // entry primary key from ENTRIES table
    topic:      String, // entry.topic (direct field, no join required)
    category:   String, // e.g., "decision", "pattern", "convention"
    confidence: f64,    // fused score (similarity + confidence + WA-2 boost)
    snippet:    String, // first 150 chars of entry.content, UTF-8 boundary safe
}
```

### IndexBriefingService

The service that replaces `BriefingService`. Its domain contract:

- **Input**: `topic` (query string), `session_id` (optional, for histogram boost),
  `k` (max entries, default 20)
- **Output**: `Vec<IndexEntry>` sorted by fused score descending, `status = Active` only
- **Dependencies**: `Arc<Store>`, `SearchService`, `Arc<SecurityGateway>`,
  `EffectivenessStateHandle`, `Arc<Mutex<EffectivenessSnapshot>>`

The `EffectivenessStateHandle` dependency is a hard constructor requirement (not optional)
to prevent silent ranking degradation (SR-03). Missing wiring MUST be a compile error.

### Query Derivation (shared helper)

A free function (or associated function) shared by the MCP tool handler and
`handle_compact_payload`:

```
fn derive_briefing_query(
    task: Option<&str>,
    session_id: Option<&str>,
    topic: &str,
    session_state: Option<&SessionState>,  // or registry lookup for MCP path
) -> String
```

Priority: task → synthesized session signal → topic fallback (FR-11).

### MIN_QUERY_WORDS Guard

The constant `MIN_QUERY_WORDS: usize = 5` in `hook.rs` is the minimum whitespace-delimited
word count for a `UserPromptSubmit` query to route to `ContextSearch`. Below this
threshold the event falls through to `generic_record_event` (fire-and-forget, no
injection). This guard does not apply to SubagentStart.

---

## User Workflows

### Subagent receives knowledge at spawn

1. SM spawns a subagent with a `prompt_snippet`.
2. Claude Code fires `SubagentStart` hook; hook process reads `prompt_snippet` from
   stdin JSON.
3. `build_request` produces `HookRequest::ContextSearch` (FR-01).
4. Hook connects to server via UDS, sends request synchronously.
5. Server `dispatch_request` routes to `handle_context_search`.
6. Server returns `HookResponse::Entries` with up to k injected entries.
7. Hook writes entries to stdout (FR-04).
8. Claude Code injects stdout content into subagent context before first token.
9. Observation is recorded with `hook = "SubagentStart"` (FR-03, AC-05).

### SM calls context_briefing at phase boundary

1. SM completes a phase: calls `context_cycle(type: "phase-end", phase: "spec", ...)`.
2. SM immediately calls `context_briefing(topic="{feature-id}", max_tokens: 1000)`.
3. `context_briefing` MCP tool handler derives query via three-step priority (FR-11).
4. `IndexBriefingService` returns `Vec<IndexEntry>` (k=20 default, active-only).
5. MCP tool formats flat indexed table and returns it.
6. SM includes briefing output in each spawned agent's context for the next phase.

### PreCompact hook triggers index-format compaction payload

1. Claude Code fires `PreCompact` hook.
2. `build_request` builds `HookRequest::CompactPayload`.
3. `handle_compact_payload` calls `IndexBriefingService` (FR-16).
4. `format_compaction_payload` renders flat indexed table + histogram block (FR-12, FR-17).
5. `HookResponse::BriefingContent` returned with flat table.
6. WA-5 (future) can prepend transcript block without parsing section headers.

---

## Constraints

**C-01** — Hook exit code is always 0. SubagentStart path MUST degrade gracefully on
any error (server unavailable, empty prompt, search failure). Exit code 1 is never
permitted from the hook process.

**C-02** — `HookRequest::ContextSearch` `source` field is a backward-compatible wire
protocol addition (`#[serde(default)]`). All existing callers that omit the field
continue to function. The server treats `None` as `"UserPromptSubmit"`.

**C-03** — `BriefingService` is deleted in this feature. No deferred cleanup. No
`#[allow(dead_code)]` annotations on removed types.

**C-04** — `HookRequest::Briefing` wire variant (in `wire.rs`) is NOT removed in this
feature. It is used by `dispatch_request` but not by any active hook arm and is
unrelated to `BriefingService`.

**C-05** — Phase-conditioned ranking (phase-to-category affinity) is deferred to W3-1.
`IndexBriefingService` MUST be designed with extensible `ServiceSearchParams` so W3-1
can add ranking without replacing the service.

**C-06** — `injection_history` dedup filter is NOT added to `context_briefing`. The
index briefing is a comprehensive "entering a phase" package with no dedup.

**C-07** — The `mcp-briefing` feature flag guards the MCP tool registration only.
`IndexBriefingService` itself compiles unconditionally so the always-on CompactPayload
path is never broken by flag changes.

**C-08** — `UNIMATRIX_BRIEFING_K` env var is deprecated on the new index path. It MUST
NOT silently reduce the k=20 default. A code comment at the removal point documents the
deprecation.

---

## Dependencies

| Dependency | Version / Location | Notes |
|---|---|---|
| `unimatrix-store` (rusqlite/SQLite) | workspace | `EntryRecord`, `Store`, `Status` |
| `unimatrix-engine` wire types | workspace | `HookRequest`, `HookResponse`, `HookInput` |
| `SearchService` | `services/search.rs` | Provides fused search with histogram boost |
| `ServiceSearchParams` | `services/search.rs` | Carries `session_id` for WA-2 boost |
| `SecurityGateway` | `services/gateway.rs` | Auth wrapper on search calls |
| `EffectivenessStateHandle` | `services/effectiveness.rs` | Required constructor dep (ADR-001 crt-018b) |
| `SessionRegistry` | `infra/session.rs` | Provides `get_category_histogram`, `get_state` |
| `dirs` crate | workspace | Home dir resolution in hook.rs |
| `#[cfg(feature = "mcp-briefing")]` | `Cargo.toml` | Guards MCP tool registration |

Existing components modified:
- `crates/unimatrix-server/src/uds/hook.rs` — SubagentStart arm, `MIN_QUERY_WORDS`
- `crates/unimatrix-server/src/uds/listener.rs` — `dispatch_request` source field,
  `handle_compact_payload`, `format_compaction_payload`, remove `CompactionCategories`
- `crates/unimatrix-server/src/services/briefing.rs` — **deleted**
- `crates/unimatrix-server/src/services/mod.rs` — replace `BriefingService` with
  `IndexBriefingService`, update `ServiceLayer` construction
- `crates/unimatrix-server/src/mcp/tools.rs` — update `context_briefing` handler
- `.claude/protocols/uni/uni-delivery-protocol.md` — add six `context_briefing` calls

New file:
- `crates/unimatrix-server/src/services/index_briefing.rs` — `IndexBriefingService`,
  `IndexEntry`, `derive_briefing_query`

---

## NOT In Scope

- **WA-4a candidate cache**: Phase-transition candidate set (product vision WA-4a
  description) is deferred. This feature routes SubagentStart to ContextSearch only.
- **Phase-to-category config mapping**: Phase-conditioned ranking deferred to W3-1.
- **`feature_cycle` ranking boost formula**: W3-1 owns scoring changes.
- **injection_history dedup filter on `context_briefing`**: No dedup on briefing index.
- **Successor pointer display for deprecated entries**: Post-WA-4 refinement.
- **WA-5 PreCompact transcript extraction**: WA-5 is a separate feature. This feature
  delivers only the format surface WA-5 requires.
- **Changes to the `context_briefing` MCP tool signature**: `role` and `task` params
  remain present as declared fields.
- **New `UNIMATRIX_BRIEFING_K` replacement env var**: The old var is deprecated and
  ignored; no replacement is introduced in this feature.
- **HookRequest::Briefing removal**: Not owned by this feature.
- **`context_enroll`, `context_cycle`, or any other MCP tool changes**.

---

## Open Questions

**OQ-SR01 (BLOCKING for architecture)** — SR-01: SubagentStart stdout injection is
unverified. The SCOPE asserts Claude Code reads SubagentStart hook stdout and injects it
into the subagent context, but no ASS spike or documentation reference is cited. The
architect MUST resolve this before delivery begins — either confirm with a 30-minute
spike or documentation reference, or pivot WA-4a to session-state-only recording with
no stdout response content. AC-SR01 tracks this verification.

**OQ-SR08 (Low risk)** — Step 3 fallback quality when `topic` is a feature ID with no
knowledge base entries (e.g., `"crt-027"` returns zero semantic matches). The architect
should validate this case against the live knowledge base and decide whether the `topic`
param should broaden to a phase name when zero results are returned.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for hook injection, briefing, compaction — found entries
  #3230 (SubagentStart routing pattern), #3231 (BriefingService caller map), confirming
  these are pre-established crt-027 scoping artifacts. No conflicting AC patterns from
  prior features.
