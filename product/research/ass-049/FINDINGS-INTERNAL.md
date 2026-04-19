# FINDINGS: Multi-LLM MCP Client Compatibility — Internal Track

**Spike**: ass-049
**Date**: 2026-04-11
**Approach**: evaluation (investigation of internal codebase)
**Confidence**: directional (code read; no live Codex/Gemini tests executed on internal track)

---

## Findings

### Q: Tool description vocabulary and parameter schema risk (SCOPE §1 — internal side)

**Answer**: Three sections carry significant non-Claude-client risk. The vocabulary is largely generic MCP terminology but two descriptions contain Claude-specific cognitive framing. The parameter schema has one high-risk field (`id`/`original_id`) that other LLMs are likely to coerce to string or float.

**Evidence — tool descriptions (read from `mcp/tools.rs` `#[tool(...)]` attributes):**

`context_search` description:
> "Search for relevant context using natural language. Returns semantically similar entries ranked by relevance. Use when you need to find patterns, conventions, or decisions related to a concept."

`context_store` description:
> "Store a new context entry. Use to record patterns, conventions, architectural decisions, or other reusable knowledge discovered during work."

`context_get` description:
> "Get a specific context entry by its ID. Use when you have an entry ID from a previous search or lookup result."

`context_briefing` description:
> "Get a ranked index of knowledge entries relevant to your current task. Returns up to 20 active entries scored by semantic similarity and NLI entailment. Use at the start of any task to orient yourself before designing or implementing."

`context_cycle` description:
> "Declare the start or end of a feature cycle for this session. Call with type='start' at session beginning to set feature attribution. Call with type='stop' when feature work is complete. Attribution is best-effort via the hook path; confirm via context_cycle_review."

**Evidence — BriefingParams.task field doc comment (read from `mcp/tools.rs` line 232):**
> "the ranking uses NLI entailment scoring which works best with coherent sentences"

**Evidence — parameter schemas (read from struct definitions):**

- `GetParams.id`: `#[schemars(with = "i64")]` — the JSON Schema emits this as integer type. The serde deserializer (`deserialize_i64_or_string`) accepts both integer and string, so string coercion survives. However, float coercion (`3.0`) is explicitly rejected with an error.
- `CorrectParams.original_id`: same integer type, same deserializer. Same risk.
- `DeprecateParams.id`, `QuarantineParams.id`, `LookupParams.id`: same pattern.
- `StoreParams.tags`, `SearchParams.tags`, `LookupParams.tags`: `Option<Vec<String>>` — JSON Schema emits as array type. If a client sends a comma-separated string (a common LLM failure mode), serde will reject it with a parse error, not silently accept it.
- `BriefingParams.max_tokens`: `deserialize_opt_i64_or_string` — tolerant of string representation. Not a risk.
- String escaping: the `content` field of `StoreParams` accepts any string; control characters (except `\n`, `\t`) are rejected by `validate_store_params` → `check_control_chars`. Other providers passing escaped inner quotes will survive if the outer JSON is well-formed.

**Three highest-risk description sections:**

1. **`context_briefing` — "NLI entailment" vocabulary in the `task` parameter doc comment.** This term is a machine-learning research term not present in typical agent-interaction training data. The parameter doc says: "the ranking uses NLI entailment scoring which works best with coherent sentences." A Codex or Gemini agent may not associate this term with any behavioral implication and may produce terse or keyword-style task strings that degrade ranking quality — exactly the failure mode described. Claude agents are trained on RLHF data that includes researcher/developer discourse where "NLI" is present; other providers' training is less certain.

2. **`context_cycle` — "feature cycle" and "hook path" framing.** "Feature cycle" is Unimatrix-internal terminology not present in any standard. "Attribution is best-effort via the hook path" refers to a UDS-based side channel that only Claude Code uses (it fires hooks on tool calls). No other MCP client fires hooks. A non-Claude client reading this description has no referent for "hook path" and the attribution behavior differs (it will not work at all) without any indication that the parameter is the only attribution signal available to them.

3. **`context_briefing` — "Use at the start of any task to orient yourself before designing or implementing."** This instruction is a behavioral directive aimed at Claude's tendency toward proactive session initialization. Claude Code agents follow session-start conventions; Codex CLI and Gemini CLI agents operate more request/response — they do not maintain "session start" as a distinct behavioral moment unless the orchestrating prompt scaffolds it explicitly. The directive will be silently ignored by agents that do not have a concept of session initialization.

**Parameter schema risk summary:**

| Field | Type in schema | Risk for non-Claude client | Handling |
|---|---|---|---|
| `id` (GetParams) | integer | Medium — LLM may emit `"3267"` (string) | Tolerated by `deserialize_i64_or_string` |
| `id` (GetParams) | integer | Low — LLM may emit `3267.0` (float) | Rejected with error |
| `original_id` (CorrectParams) | integer | Same as above | Same |
| `tags` | array | Medium — LLM may emit `"tag1,tag2"` | Rejected with parse error |
| content string escaping | string | Low — depends on client JSON serializer | Standard JSON; no Unimatrix-specific risk |

**Recommendation**: For `context_briefing`, rewrite the `task` parameter doc comment to remove "NLI entailment scoring" and replace with a behavioral instruction: "Be specific and sentence-form; terse keyword strings return lower-quality results." For `context_cycle`, add "If hooks are unavailable (non-Claude clients), set `agent_id` explicitly — it is the only attribution signal." For `tags`, add an explicit example showing array form: `["adr", "design"]`.

---

### Q: Context injection size behavior (SCOPE §3 — internal side)

**Answer**: The `max_tokens` parameter for `context_briefing` is accepted and validated (range 500–10,000, default 3,000) but is **not currently enforced on results**. The briefing always returns up to k=20 entries. The character-count approximation is only used on the UDS path, not the MCP path. There is no provider-specific tokenizer. The MCP path does not truncate by character count at all.

**Evidence:**

From `infra/validation.rs` (lines 31–33):
```
const DEFAULT_MAX_TOKENS: usize = 3_000;
const MIN_MAX_TOKENS: usize = 500;
const MAX_MAX_TOKENS: usize = 10_000;
```

From `mcp/tools.rs` line 236 (the `max_tokens` field doc comment):
> "Reserved for future output truncation. Accepted and validated (500–10000, default 3000) but not currently enforced on results."

This is confirmed by `services/index_briefing.rs`: the `IndexBriefingParams.max_tokens` field is declared as:
> "Approximate token budget (for future ranked truncation; not enforced here)."

The `IndexBriefingService::index()` method does not reference `max_tokens` anywhere — the field is accepted, passed through, and ignored. The service always returns up to `effective_k` (20) entries regardless of token budget.

**UDS path does apply a character-count approximation:** In `uds/listener.rs` line 1246:
```rust
let token_count = (content.len() / 4) as u32;
```
And line 1508:
```rust
max_tokens: Some(max_bytes / 4),  // approximate token budget
```
This `/ 4` approximation (4 bytes per token) is a generic heuristic. There is no provider-specific tokenizer call anywhere in the codebase — confirmed by a search for `tiktoken`, `BPE`, `anthropic.*token`, `openai.*token`, and `gemini.*token`, none of which appear.

**What happens when a result set exceeds the limit:** On the MCP path, it is silently ignored — up to 20 entries are always returned regardless of `max_tokens`. There is no truncation, no error, no advisory note in the response. On the UDS path, the `token_count` is computed after the fact and returned in the `HookResponse::BriefingContent` payload for informational use; no truncation occurs there either.

**Size of a full 20-entry briefing response:** Each entry is one row in the flat index table. With `SNIPPET_CHARS = 150` (from `mcp/response/briefing.rs` line 16), a snippet is at most 150 Unicode characters. A 20-row table with ID, topic, category, confidence, and a 150-char snippet is approximately 20 × 250 chars = ~5,000 chars. At 4 chars/token that is ~1,250 tokens — comfortably within the 3,000-token default budget even when the budget is not enforced.

**Recommendation**: The current behavior is safe for Claude and for other providers at the 20-entry cap. The risk of `max_tokens` being meaningless is low in practice (table output is bounded by k=20 × SNIPPET_CHARS). However, the field should be either enforced or removed from the public schema — advertising a parameter that has no effect is a contract violation that will confuse any client that tries to reduce response size for small-context-window models. Implement enforcement (truncate entries until cumulative snippet length / 4 ≤ max_tokens) before Wave 2 launch, or explicitly document in the tool description that `max_tokens` is reserved and unused.

---

### Q: Eval harness structure and Claude-specific assumptions (SCOPE §4)

**Answer**: The eval harness is an offline in-process replay system that operates entirely at the `SearchService` level. It does not invoke any MCP tools, does not exercise client tool-calling behavior, and does not use LLM inference at all. Scenarios are raw query strings extracted from the `query_log` table. There are zero Claude-specific assumptions in the scenario format, the replay logic, or the metric computation. The harness is structurally provider-neutral today — but it measures the wrong thing for the multi-LLM compatibility goal.

**Evidence — scenario format (read from `eval/scenarios/types.rs`):**

A `ScenarioRecord` has:
- `id`: `"qlog-{query_id}"` (auto-generated)
- `query`: the raw `query_text` from `query_log`
- `context.agent_id`: populated from `session_id` (no dedicated agent_id column exists in `query_log` per the extract.rs comment)
- `context.retrieval_mode`: `"flexible"` or `"strict"`
- `context.phase`: nullable workflow phase string
- `baseline`: parallel arrays of `entry_ids` and `scores` (captured at query time)
- `source`: `"mcp"` or `"uds"`
- `expected`: always `null` for log-sourced scenarios

There are no fields for: LLM provider, system prompt structure, tool call format, multi-step reasoning patterns, or any Claude-specific metadata.

**Evidence — replay logic (read from `eval/runner/replay.rs`):**

`run_single_profile()` builds `ServiceSearchParams` directly from the scenario record and calls `SearchService.search()`. It does not call any MCP handler, does not construct a tool call message, and does not use a language model. Ground truth is resolved from `record.expected` (hand-authored, always null for log-sourced) or `record.baseline.entry_ids` (captured result IDs at log time).

**Evidence — metric computation (read from `eval/runner/metrics.rs`):**

MRR, P@K, Kendall tau, category coverage, Shannon entropy — all purely algorithmic over the search result list. No LLM inference involved.

**What fraction of scenarios would be valid for Codex or Gemini without modification:**

100%. Every scenario is a query string + baseline result IDs. The harness replays the query through `SearchService` in-process — there is no client involved at all. This means the 2,096-scenario set and MRR=0.2558 baseline measure Unimatrix's retrieval quality on queries Claude agents actually issued, but they say nothing about whether Codex or Gemini agents would issue equivalent queries, invoke the correct tools, or invoke tools in the right sequence.

**What Claude-specific assumptions exist in the scenario corpus (not the format):**

The scenarios are log-sourced from Claude Code sessions (the only client that has been running). This means:
- Query text uses Claude's natural language patterns: sentence-form, technical vocabulary consistent with Claude's developer training, likely to include phrases like "how does X work", "find the convention for", "what are the patterns for".
- Queries are biased toward the `context_search` / `context_briefing` → `context_get` flow that Claude Code agents follow.
- There are zero scenarios for: a Codex agent that issues keyword-only queries, a Gemini agent that does not use the briefing → work → store → cycle workflow, or any non-Claude invocation pattern.

The baseline MRR=0.2558 is a retrieval quality measure on Claude-query-shaped inputs. A Codex or Gemini agent issuing shorter, less sentence-like queries could produce meaningfully different retrieval recall — there is no data to bound this direction.

**Minimum scenario set for provider-neutral coverage:**

The harness architecture is already provider-neutral — the gap is in the corpus, not the code. A provider-neutral corpus requires:

1. **Hand-authored scenarios with `expected` labels** (the only type that does not depend on real prior usage). The `expected: Option<Vec<u64>>` field already supports this but is never populated by the extractor. Scenarios with `expected = [id_a, id_b]` give the replay runner ground truth independent of any prior session.

2. **Provider-neutral query types**: at minimum 4 query types — (a) sentence-form natural language ("what are the conventions for embedding pipelines"), (b) keyword-form ("embedding pipeline conventions"), (c) compound filter ("decisions tagged adr about confidence"), (d) lifecycle-phase queries ("what was stored during design phase of crt-005"). Each type tested with 5–10 distinct topics = 20–40 hand-authored scenarios.

3. **Tool sequence coverage is explicitly out of scope for the current harness** — the replay runner does not test tool invocation sequences. That is a separate eval concern (LLM-in-the-loop). The current harness covers retrieval quality only.

**Recommendation**: Do not add Claude-specific scenario types to the harness. Add 20–40 hand-authored scenarios with populated `expected` fields covering the 4 query types above. Mark them `source: "hand"` (new source tag, backward-compatible since `ScenarioSource` enum is exhaustive via `All`). These scenarios will validate retrieval quality on provider-neutral input shapes and give a baseline for comparing Codex/Gemini query patterns against. The harness itself requires no code changes.

---

## Unanswered Questions

None from the assigned questions. All three were answered from code evidence.

---

## Out-of-Scope Discoveries

1. **`max_tokens` is a dead parameter on the MCP path.** It is validated, passed to `IndexBriefingParams`, and ignored. The field doc comment acknowledges this ("reserved for future output truncation"). This is a contract integrity issue independent of multi-LLM compatibility — any client that tries to use `max_tokens` to manage context window use gets no effect. Warrants a separate spike or delivery task to implement enforcement.

2. **`context_cycle`'s "hook path" attribution is Claude Code-only.** The description refers to attribution via "the hook path (fire-and-forget)" which requires the Claude Code UDS hook mechanism. The MCP-path `context_cycle` acknowledgment text says "Attribution is applied via the hook path" — this is false for any non-Claude-Code client. For those clients, the `agent_id` parameter is the only attribution path. This is not just a description problem; it may be a behavioral contract gap requiring explicit server-side handling when no UDS hook path is present.

3. **Scenario `agent_id` is populated from `session_id` as proxy (no actual agent column).** The `extract.rs` comment explicitly notes this: "No agent_id column in query_log; use session_id as proxy." This means the 2,096 scenarios all have `agent_id == session_id`, which affects the eval runner's `ServiceSearchParams.caller_agent_id`. This is an accuracy limitation in the current baseline, not a blocking issue.

---

## Recommendations Summary

- **Q1 (Tool description vocabulary risk)**: Rewrite `context_briefing` `task` parameter doc to replace "NLI entailment scoring" with a behavioral instruction. Add explicit `agent_id` guidance to `context_cycle` description for non-hook clients. Add a `tags` example showing array form. These are the three highest-risk sections.
- **Q2 (Context injection size behavior)**: The `max_tokens` parameter is accepted but has no effect on the MCP path. This is a dead parameter. Either implement enforcement (truncate entries by cumulative snippet character count / 4) before Wave 2, or remove `max_tokens` from the MCP schema. The character-count heuristic is generic (`len / 4`), not provider-specific — safe across providers but not calibrated to any tokenizer.
- **Q3 (Eval harness provider-agnostic gap)**: The harness format and replay logic are already provider-neutral. The corpus is 100% Claude-session-derived query patterns. Add 20–40 hand-authored scenarios with `expected` labels covering sentence-form, keyword, filter, and lifecycle-phase query types. No code changes required. The harness cannot test tool invocation sequences — that is a separate LLM-in-the-loop eval that does not exist yet.
