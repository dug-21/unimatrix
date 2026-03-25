# Design Review Report: 381-design-reviewer

## Assessment: APPROVED WITH NOTES

The proposed fix is fundamentally sound. All five sub-proposals are correct and safe to implement as described. Three notes require attention before delivery, none of which block the approach.

---

## Findings

### 1. Hot-Path Risks — LP-1 and LP-2 (Severity: LOW)

**Finding**: The investigator correctly assesses that ContextSearch fires O(tens) of times during a feature delivery session, not O(hundreds per second). This is not a hot path in any latency or throughput sense. The log lines themselves are trivially cheap — field captures are all `String`/`usize`/`Vec<u64>` already allocated by the time the log line fires.

**LP-2 title collection concern**: LP-2 allocates a `Vec<&str>` from `filtered.iter()` solely for the log line. This is negligible at O(k) where k=5 by default (INJECTION_K), but it is allocation performed even at `INFO` level. This is the standard tracing trade-off: structured fields are evaluated before the filter check at the callsite level. At the proposed call frequency this is a non-issue. However, the `entry_titles` field should use `%` (Display) or `?` (Debug) consistently with the existing log style in the file.

**Verdict**: No flooding risk. No hot-path concern at the stated call frequency.

---

### 2. EnvFilter Change Blast Radius — RUST_LOG Semantics (Severity: LOW)

**Finding**: The investigator's analysis is confirmed correct. The three sites in `main.rs` (lines 407–411, 791–795, 1183–1187) all pass a `&str` literal via `with_env_filter(filter)`. The `From<&str>` impl calls `EnvFilter::new(s)` which parses the literal as a static directive list and never consults `RUST_LOG`. This is confirmed by the tracing-subscriber source reference the investigator cites.

**Fallback semantics**: `EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_level))` is exactly the correct pattern. The `try_from_default_env` function:
- Returns `Err` when `RUST_LOG` is unset or empty
- Returns `Err` when `RUST_LOG` is set but unparseable
- In both `Err` cases the `unwrap_or_else` arm provides `EnvFilter::new(default_level)` — identical to current behavior

**One edge case to verify**: `RUST_LOG=""` (explicitly set to empty string). `try_from_default_env` will attempt to parse an empty string as filter directives. An empty string parses as "no directives" which is not the same as `EnvFilter::new("info")` — it would produce a filter with no directives, meaning the default tracing level applies. The tracing-subscriber default is `ERROR`. This could silently suppress `INFO` logs if a user sets `RUST_LOG=""`. The fix should handle this:

```rust
let filter = EnvFilter::try_from_default_env()
    .unwrap_or_else(|_| EnvFilter::new(default_level));
```

This already handles the `RUST_LOG=""` case correctly because `try_from_default_env` returns `Err` when the variable is empty or contains only whitespace — the `unwrap_or_else` then applies `default_level`. Delivery should confirm this behavior against the actual tracing-subscriber 0.3.23 source, which the investigator has on disk at `/usr/local/cargo/registry/src/.../tracing-subscriber-0.3.23/src/filter/env/mod.rs`.

**Verdict**: The approach is correct. Delivery should add a code comment at each site documenting the RUST_LOG override semantics.

---

### 3. `source` Signature Extension vs. Tracing Span (Severity: LOW — Note Only)

**Finding**: The investigator proposes passing `source: Option<String>` as a new parameter to `handle_context_search`. This is a one-parameter addition to a private async function called at exactly one site (line 1058). The change is minimal and unambiguous.

**Span alternative assessment**: Using a tracing span (`tracing::Span::current()` or `#[instrument]`) to carry `source` from `dispatch_request` into `handle_context_search` would work but introduces more complexity than this use case warrants:
- There is no existing span infrastructure in `dispatch_request` — zero `tracing::span!` or `#[instrument]` uses in production code in listener.rs
- A span would need to be entered before the `handle_context_search` call and the field would need to be on the span, not the specific log event
- This conflates two concerns: structured logging of a specific field with context propagation

The direct parameter approach is the right choice here. It is explicit, locally readable, and matches the existing style of the function (all context passed as parameters, no thread-local state).

**Verdict**: Direct parameter extension is the better approach. Span-based context propagation is over-engineered for a single optional string on one private function.

---

### 4. Architectural Fit — `target: "unimatrix_server::obs"` Convention (Severity: NOTE)

**Unimatrix query result**: No existing ADR establishes a logging/tracing target convention for this codebase. The search on "logging tracing EnvFilter conventions" returned ADRs about fire-and-forget semantics and query-log writes, not tracing target namespaces. There is no established prior pattern here.

**Assessment of the proposed convention**: `unimatrix_server::obs` is a deliberate synthetic namespace — not a real module path. This is intentional per the investigator's design: a real module path (e.g., `unimatrix_server::uds::listener`) would be silenced by a directive that also silences other logs in that module. The synthetic target makes the filter surgical.

This pattern is standard in the Rust tracing ecosystem (the `sqlx` crate uses `sqlx::query` as a synthetic target for query logging for exactly this reason). The convention is appropriate.

**Gap**: There is no convention entry in Unimatrix documenting this target. The post-delivery Knowledge Stewardship step should store it so future agents don't invent a different target name.

---

### 5. Reversibility — RUST_LOG Silencing Mechanism (Severity: NONE — Confirmed Correct)

**Finding**: Once Step 1 is applied, `RUST_LOG=info,unimatrix_server::obs=off` is the correct and complete silencing mechanism. Directive specificity rules in tracing-subscriber mean the more specific `unimatrix_server::obs=off` overrides the broader `info` directive for events with that target.

The three silencing modes documented in the investigator report are all correct:
- `RUST_LOG=info` — shows all INFO including obs (default after fix)
- `RUST_LOG=info,unimatrix_server::obs=off` — silences obs, preserves server INFO
- `RUST_LOG=debug` — shows everything

**One omission**: The investigator should also document the "obs only" mode for debugging: `RUST_LOG=off,unimatrix_server::obs=info`. This is useful when the operator wants obs output without the server's routine INFO chatter. Not a blocker — just useful documentation for the operator.

---

### 6. Security Surface (Severity: LOW — Action Required)

**Finding**: Two of the four log points expose user-controlled input:

**LP-1** (`query = %query`): Logs the full query string passed to ContextSearch. This is the agent's prompt text — it can be arbitrarily long and contain anything the agent types. The investigator's proposed code does not truncate this. At INFO level this will appear in the server's stderr log file.

**Concern level**: Low but real. The query is already stored in `QUERY_LOG` (line 1273–1285 in `handle_context_search`) and in `OBSERVATION` table (in `dispatch_request` before the call). The log file is no more sensitive than the database. However, log files are often aggregated by external tooling (syslog, log shippers) that may not have the same access controls as the SQLite file.

**Recommended mitigation**: Truncate the query in LP-1 to a bounded length, consistent with the `truncate_at_utf8_boundary` pattern already used in this file for LP-4 (`goal_preview = %truncate_at_utf8_boundary(goal_text, 50)`). A 120-byte preview is sufficient to identify which query fired the search without exposing full prompt content. The `truncate_at_utf8_boundary` function is already in scope.

**LP-2** (`entry_titles`): Logs entry titles — these are titles of knowledge entries stored by the project team. These are not user-controlled and not sensitive. No concern.

**LP-3** (`query = %query`): Same issue as LP-1. Apply same truncation.

**LP-4** (goal preview): Already uses `truncate_at_utf8_boundary(goal_text, 50)`. Correct as-is.

---

## Revised Fix Approach

The investigator's fix approach is approved with two targeted modifications:

**Modification 1 — Truncate query in LP-1 and LP-3**

LP-1 (in `handle_context_search`, after `filtered` is built):
```rust
tracing::info!(
    target: "unimatrix_server::obs",
    session_id = ?session_id,
    result_count = filtered.len(),
    query_preview = %truncate_at_utf8_boundary(&query, 120),
    "UDS: ContextSearch executed"
);
```

LP-3 (in `dispatch_request`, top of SubagentStart block):
```rust
tracing::info!(
    target: "unimatrix_server::obs",
    session_id = ?session_id,
    query_preview = %truncate_at_utf8_boundary(&query, 120),
    "UDS: SubagentStart received"
);
```

Rename the field from `query` to `query_preview` to signal that the value is deliberately truncated. This is a self-documenting signal to operators and future readers.

**Modification 2 — Comment at each EnvFilter site**

At each of the three `with_env_filter` sites, add a comment:
```rust
// RUST_LOG overrides the default level when set.
// E.g., RUST_LOG=info,unimatrix_server::obs=off silences injection obs logs.
let filter = EnvFilter::try_from_default_env()
    .unwrap_or_else(|_| EnvFilter::new(default_level));
```

No other changes to the proposed fix.

---

## Open Questions for Delivery

1. Confirm `EnvFilter::try_from_default_env()` returns `Err` for `RUST_LOG=""` in tracing-subscriber 0.3.23 (the investigator has the source on disk). If it returns `Ok(empty filter)`, a guarded fallback is needed.

2. The goal-absent log (added to the `} else {` branch at line 1018) should confirm there are no early-return paths between the `if let Some(ref goal_text) = maybe_goal {` block close and the `// goal absent or empty` comment — the investigator's report implies the `else` attaches cleanly there. The source at lines 957–1019 confirms this: the `if let Some` block closes at line 1017, and line 1018 is the comment before the closing `}` of the outer `if source.as_deref() == Some("SubagentStart")`. The `else` should be added before that closing brace (i.e., as the else branch of the `if let Some(ref goal_text)` check), not after the outer `if source` block.

---

## Knowledge Stewardship

**Queried**:
- `mcp__unimatrix__context_search` — "logging tracing EnvFilter conventions", category: decision — no existing ADR on tracing targets or filter initialization
- `mcp__unimatrix__context_search` — "UDS listener injection observation hot path" — found pattern #763 (Server-Side Observation Intercept Pattern) and ADR #290 (UsageService scope); neither constrains this fix

**Stored**: Declined — the investigator already stored the RUST_LOG lesson (#3453). The `unimatrix_server::obs` target convention should be stored after delivery confirms the final target name, not during design review. No new generalizable pattern emerged from this review beyond what the investigator captured.
