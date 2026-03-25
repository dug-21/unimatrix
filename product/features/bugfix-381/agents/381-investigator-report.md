# Bug Investigation Report: 381-investigator

## Bug Summary

GH #381: No INFO-level log visibility into UDS injection dispatch. During feature delivery, there is no way to observe in real time: what query was sent to ContextSearch, what entries were injected, or what query the SubagentStart event triggered. Additionally, the `RUST_LOG` environment variable is silently ignored due to how `tracing_subscriber` is initialized — meaning even if the log points existed, per-target filtering (the required reversibility mechanism) would not work without a source code change.

---

## Root Cause Analysis

Two independent root causes combine to produce the reported symptom.

### Root Cause 1: Missing log statements

The UDS dispatch path was built for correctness and performance, not observability. Log points were added defensively (warn on errors, info on lifecycle events like session open/close) but not at the data-flow points an operator would need to observe injection behavior. Specifically:

- `handle_context_search` emits no log when the search executes or when results are returned.
- The injection tracking block (lines 1229–1238) is silent.
- The `SubagentStart` dispatch block (line 950+) emits only a single `debug!` on the goal-present branch — nothing on the goal-absent branch, and nothing showing the incoming query before branching.

### Root Cause 2: RUST_LOG is silently ignored (blocks reversibility)

All three `main.rs` entry points initialize tracing with:
```rust
let filter = if cli.verbose { "debug" } else { "info" };
tracing_subscriber::fmt()
    .with_env_filter(filter)  // passes &str, not EnvFilter::from_default_env()
    ...
```

`with_env_filter` accepts `impl Into<EnvFilter>`. The `From<&str>` impl for `EnvFilter` calls `EnvFilter::new(s)`, which **parses the literal string as filter directives** — it does NOT read `RUST_LOG`. This is confirmed by reading the tracing-subscriber 0.3.23 source at `/usr/local/cargo/registry/src/.../tracing-subscriber-0.3.23/src/filter/env/mod.rs` line 759–765.

Consequence: setting `RUST_LOG=unimatrix_server::obs=off` at runtime has zero effect. The filter cannot be toggled without editing source code and redeploying.

### Code Path Trace

**Log points 1 & 2 (query + injection content):**
```
dispatch_request (listener.rs:919 match arm)
  → handle_context_search (listener.rs:1151)
      → services.search.search() → filtered built at line 1223
      → [LOG POINT 1 MISSING: after filtered built, line ~1228]
      → injection tracking block (lines 1229–1238)
      → [LOG POINT 2 MISSING: inside !filtered.is_empty() guard, line ~1236]
```

**Log points 3 & 4 (SubagentStart context + routing):**
```
dispatch_request (listener.rs:919 match arm)
  → source == "SubagentStart" check (line 950)
  → [LOG POINT 3 MISSING: before this check, line ~950, incoming query]
  → goal present branch → tracing::debug! (line 958) [LOG POINT 4: promote to info!]
  → goal absent branch → [LOG POINT 4b MISSING: no log at any level]
```

**Reversibility blocker:**
```
tokio_main_daemon / tokio_main_stdio / tokio_main_bridge (main.rs)
  → with_env_filter("info")
  → EnvFilter::from("info") → EnvFilter::new("info")
  → static parse, RUST_LOG not consulted
```

### Why It Fails

The combination means: (a) the log statements that would provide visibility don't exist, and (b) even after they are added, there is no way to toggle them off without a code change, because the tracing subscriber ignores `RUST_LOG`.

---

## Affected Files and Functions

| File | Function | Role in Bug |
|------|----------|-------------|
| `crates/unimatrix-server/src/uds/listener.rs` | `handle_context_search` (line 1151) | Missing log points 1 and 2; `source` not available here |
| `crates/unimatrix-server/src/uds/listener.rs` | `dispatch_request` match arm (line 919) | Missing log point 3; log point 4 exists at debug level (line 958); goal-absent branch has no log |
| `crates/unimatrix-server/src/main.rs` | `tokio_main_daemon` (line 407) | Static filter ignores RUST_LOG |
| `crates/unimatrix-server/src/main.rs` | `tokio_main_stdio` (line 791) | Static filter ignores RUST_LOG |
| `crates/unimatrix-server/src/main.rs` | `tokio_main_bridge` (line 1183) | Static filter ignores RUST_LOG |
| `crates/unimatrix-server/src/uds/hook.rs` | `run` (line 62) | Subprocess — no tokio, no tracing subscriber; uses `eprintln!` only |

---

## Proposed Fix Approach

### Step 1: Fix reversibility first (main.rs, 3 sites)

At each of the three `tokio_main_*` functions, replace the static filter with one that reads `RUST_LOG` first and falls back to the existing default:

```rust
// Before (all three sites):
let filter = if cli.verbose { "debug" } else { "info" };
tracing_subscriber::fmt()
    .with_env_filter(filter)

// After:
use tracing_subscriber::EnvFilter;
let default_level = if cli.verbose { "debug" } else { "info" };
let filter = EnvFilter::try_from_default_env()
    .unwrap_or_else(|_| EnvFilter::new(default_level));
tracing_subscriber::fmt()
    .with_env_filter(filter)
```

This is a purely mechanical change. Existing behavior is preserved when `RUST_LOG` is unset.

### Step 2: Add log point 3 — SubagentStart incoming query (listener.rs ~line 950)

In `dispatch_request`, immediately before the `source.as_deref() == Some("SubagentStart")` check, add:

```rust
if source.as_deref() == Some("SubagentStart") {
    tracing::info!(
        target: "unimatrix_server::obs",
        session_id = ?session_id,
        query = %query,
        "UDS: SubagentStart received"
    );
```

Variables available at this point: `query` (String, owned), `session_id` (Option<String>), `source` (Option<String>). No side effects — reads only.

### Step 3: Promote debug! → info! at line 958, add goal-absent log (listener.rs)

Change `tracing::debug!` at line 958 to `tracing::info!` with the same target:

```rust
tracing::info!(
    target: "unimatrix_server::obs",
    session_id = ?session_id,
    goal_preview = %truncate_at_utf8_boundary(goal_text, 50),
    "col-025: SubagentStart goal-present branch — routing to IndexBriefingService"
);
```

After the `}` that closes `if let Some(ref goal_text) = maybe_goal` and before the `// goal absent or empty` comment (line 1018), add:

```rust
} else {
    tracing::info!(
        target: "unimatrix_server::obs",
        session_id = ?session_id,
        "col-025: SubagentStart goal-absent — falling through to ContextSearch"
    );
}
```

### Step 4: Add log points 1 and 2 in handle_context_search (listener.rs ~lines 1228, 1236)

After `filtered` is built at line 1227, add log point 1:

```rust
tracing::info!(
    target: "unimatrix_server::obs",
    session_id = ?session_id,
    result_count = filtered.len(),
    query = %query,
    "UDS: ContextSearch executed"
);
```

Inside the `if !sid.is_empty() && !filtered.is_empty()` guard at line 1231, after `record_injection` (line 1236), add log point 2:

```rust
let entry_titles: Vec<&str> = filtered
    .iter()
    .map(|(e, _)| e.title.as_str())
    .collect();
tracing::info!(
    target: "unimatrix_server::obs",
    session_id = %sid,
    entry_count = filtered.len(),
    entry_ids = ?injection_entries.iter().map(|(id, _)| id).collect::<Vec<_>>(),
    entry_titles = ?entry_titles,
    "UDS: injecting entries"
);
```

Note: `injection_entries` is built immediately above at line 1232–1235 and contains `(u64, f64)` pairs. `filtered` is `Vec<(EntryRecord, f64)>` so `entry.title` is directly accessible without any new logic.

**Important structural note**: `handle_context_search` does not receive `source`. Log point 1 will log query text and result count but not the `source` field. If `source` is needed in log point 1, it must be added as a parameter to `handle_context_search` and threaded through the call at line 1058. This is a minimal, targeted change, but the issue text explicitly states it as a `?source` field — the fix should pass `source` to `handle_context_search`.

### Step 5: hook.rs subprocess — out of scope

`hook.rs` runs as a short-lived subprocess with no tokio runtime and no tracing subscriber. The existing `eprintln!` calls are the only observability mechanism available. The issue marks this as "evaluate in delivery." The server-side logs at points 1–4 make subprocess logging redundant for observability of the injection content — the server emits the log after the query is dispatched. The hook.rs subprocess is **out of scope** for this fix.

### Why This Fix

The `target: "unimatrix_server::obs"` approach is the correct reversibility mechanism. Once `RUST_LOG` is wired (Step 1), the operator can:

- **Enable obs logs**: `RUST_LOG=info` (default, shows all INFO including obs)
- **Silence obs logs, keep server INFO**: `RUST_LOG=info,unimatrix_server::obs=off`
- **Full debug**: `RUST_LOG=debug`

This requires zero source code changes to toggle — set `RUST_LOG` before starting the daemon. The target namespace `unimatrix_server::obs` is distinct from the module path so it cannot accidentally match other logs.

Feature flags or `cfg` attributes are the wrong approach: they require recompilation.

---

## Risk Assessment

- **Blast radius**: Step 1 (main.rs EnvFilter fix) affects all three server entry points. The change is purely additive — when `RUST_LOG` is unset, behavior is identical to today. Only when `RUST_LOG` is set does behavior change, which is the desired outcome.
- **Regression risk — Step 1**: Low. The fallback `unwrap_or_else(|_| EnvFilter::new(default_level))` preserves the current behavior identically when `RUST_LOG` is absent.
- **Regression risk — Steps 2-4**: The `info!` calls read existing local variables and perform no mutations. The only test concern is the existing `tracing_test::traced_test` tests that assert on the `debug!` at line 958 — specifically `test_subagent_start_goal_present_routes_to_index_briefing` (line 6367) and `test_subagent_start_goal_wins_over_nonempty_prompt_snippet` (line 6423), which assert `logs_contain("col-025: SubagentStart goal-present branch")`. Promoting `debug!` to `info!` with the same message does NOT break these tests — `tracing_test` captures all levels.
- **Hot path risk**: ContextSearch is called once per `UserPromptSubmit` and once per `SubagentStart`. During a feature delivery cycle this is O(tens) of calls, not O(hundreds per second). The log points are not on any hot path. `result_count=0` cases (empty injection) emit one line per search, which is negligible.
- **Confidence**: High. All insertion points confirmed by reading the exact lines. The RUST_LOG finding is confirmed from the tracing-subscriber 0.3.23 source.

---

## Missing Test

The `tracing_test::traced_test` pattern already exists in this file (7 usages). The missing tests are:

1. A test asserting `logs_contain("UDS: ContextSearch executed")` when a ContextSearch returns results — verifies log point 1 fires.
2. A test asserting `logs_contain("UDS: injecting entries")` when entries are returned — verifies log point 2 fires.
3. A test asserting `logs_contain("UDS: SubagentStart received")` on any SubagentStart dispatch — verifies log point 3.
4. A test asserting `logs_contain("col-025: SubagentStart goal-absent")` when session has no current_goal — verifies the goal-absent branch log (currently no test covers this branch having any log at any level).

The existing `test_subagent_start_goal_present_routes_to_index_briefing` already covers the goal-present promotion (it will pass after the debug→info change with no modification needed).

---

## Reproduction Scenario

Deterministic. Start the daemon without `--verbose`, run a SubagentStart hook event through the UDS socket, observe that the log shows no query text, no result count, no injection content. The absence is the symptom.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `uds logging tracing` — pattern #3452 (suppress expected I/O errors) and #3230 (SubagentStart routing pattern) found; no prior logging/observability patterns found
- Queried: `/uni-query-patterns` for `tracing RUST_LOG filter` — no relevant convention found; confirms gap
- Stored: entry #3453 "tracing-subscriber with_env_filter(&str) ignores RUST_LOG — use EnvFilter::try_from_default_env() for runtime filter control" via `/uni-store-lesson`
