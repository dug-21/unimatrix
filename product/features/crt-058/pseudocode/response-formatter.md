# Component: response-formatter

**File:** `crates/unimatrix-server/src/mcp/response/mutations.rs:16` — `format_status_change` and its
wrappers `format_deprecate_success:54`, `format_quarantine_success:70`, `format_restore_success:86`.

## Purpose

Add an additive, backward-compatible `edges_removed: Option<u64>` advisory to the shared status
formatter, rendered in all three formats for `Some(n)` (incl. `Some(0)`) and omitted for `None`.
One formatter, no branching by action (ADR-004).

## Changed Signature

```rust
pub fn format_status_change(
    entry: &EntryRecord,
    action: &str,
    status_key: &str,
    status_display: &str,
    reason: Option<&str>,
    edges_removed: Option<u64>,   // NEW — positioned BEFORE `format`
    format: ResponseFormat,
) -> CallToolResult
```

## Pseudocode

```
FUNCTION format_status_change(entry, action, status_key, status_display, reason, edges_removed, format):
    MATCH format:

      Summary:
          text = "{action} #{entry.id} | {entry.title}"
          IF edges_removed IS Some(n):
              text += " | {n} edges removed"        # Some(0) -> " | 0 edges removed"
          # None -> line unchanged (byte-identical to pre-feature)
          RETURN success([Content::text(text)])

      Markdown:
          text  = "## Entry {action}\n\n"
          text += "**Entry:** #{entry.id} - {entry.title}\n**Status:** {status_display}\n"
          IF reason IS Some(r):
              text += "**Reason:** {r}\n"
          IF edges_removed IS Some(n):
              text += "**Edges removed:** {n}\n"     # Some(0) -> "**Edges removed:** 0\n"
          # None -> no line added
          RETURN success([Content::text(text)])

      Json:
          # Build the base object exactly as today, then conditionally insert the field so the
          # key is ABSENT for None (not null) — a parser ignoring the slot is unaffected (NFR-04).
          obj = json!({ status_key: true, "entry": entry_to_json(entry), "reason": reason })
          IF edges_removed IS Some(n):
              obj["edges_removed"] = json!(n)        # numeric; Some(0) -> "edges_removed": 0
          RETURN success([Content::text(to_string_pretty(&obj).unwrap_or_default())])
```

Json note: keep `obj` as a mutable `serde_json::Value` (`json!` macro yields one) so the field can be
inserted only in the `Some` branch. Do NOT serialize `Option` directly into the literal — that would
emit `null` for `None`, which is a rendered advisory, not omission.

## Wrapper Changes

```
format_deprecate_success(entry, reason, edges_removed: Option<u64>, format):   # NEW param, forwarded
    RETURN format_status_change(entry, "Deprecated", "deprecated", "deprecated",
                                reason, edges_removed, format)

format_quarantine_success(entry, reason, format):        # signature UNCHANGED at wrapper surface
    RETURN format_status_change(entry, "Quarantined", "quarantined", "quarantined",
                                reason, None, format)     # passes None — quarantine deletes no edges

format_restore_success(entry, reason, format):           # signature UNCHANGED at wrapper surface
    RETURN format_status_change(entry, "Restored", "restored", "active",
                                reason, None, format)      # passes None — restore deletes no edges
```

Only `format_deprecate_success` gains a parameter (rippling to its two `tools.rs` call sites: the
step-5 early-return passes `None`, step 8 passes `Some(count)`/`None`). Quarantine/restore keep their
public arity and hardcode `None` internally — their callers (`tools.rs:1976, 2008, 2046`) are untouched.

## Error Handling

Pure formatting; no fallible paths beyond the existing `to_string_pretty(...).unwrap_or_default()`.

## Data Flow

- **In:** `entry`, `reason`, `edges_removed: Option<u64>`, `format`.
- **Out:** `CallToolResult`. `Some(n)` adds one advisory line/field; `None` leaves output byte-identical
  to pre-feature for that path.

## Key Test Scenarios (hints) — behavioral, parse-based (R-05 / SR-04, #5427)

- Per-format matrix `Some(n)`, n>0: Summary asserts rendered count value; Markdown asserts the
  `**Edges removed:** {n}` value; Json PARSES the integer field and compares (not substring).
- AC-05 `Some(0)`: all three formats render a literal `0` (Json field == 0), advisory NOT omitted.
- `None`: advisory absent in all three (Summary line unchanged, no Markdown line, Json key absent).
- Backward-compat: `format_quarantine_success` / `format_restore_success` output byte-identical
  before and after the change (they pass `None`).
- Existing tests at `mcp/response/mod.rs:700–990` call OLD arities — update them to the new signature
  (cumulative test infra; the arity break is the intended compile-time tripwire).
