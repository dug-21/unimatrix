# C6 — Read-path attribution (prefer stored, legacy fallback)

**Location:** `unimatrix-server/src/services/observation.rs` (`parse_observation_rows:576`,
`DEFAULT_HOOK_SOURCE_DOMAIN:572`); `unimatrix-store/src/observations.rs`
(`fetch_observations_since:44`, `load_observations_for_sessions:124`, `load_observation_session_stats:173`).
**ADRs:** ADR-001 (prefer stored, legacy fallback), ADR-002 (surface model_id). **Risks:** R-02, R-04.2.
**Wave:** 3 (needs C5 columns + C7 fallback pack).

## Purpose

Read back the persisted `source_domain`/`model_id` for new rows (prefer stored), while legacy NULL rows
(pre-vnc-049) keep today's read-derived behavior via `resolve_source_domain`/`DEFAULT_HOOK_SOURCE_DOMAIN`.
This is the read half of the ADR-001 fork; combined with C5 it makes AC-03/AC-06 assertable on the
stored/queried record.

## Store SELECTs (`observations.rs`) — add columns

Each of the three queries currently selects
`id, ts_millis, hook, session_id, tool, input, response_size, response_snippet`. Append the two columns:
```
SELECT id, ts_millis, hook, session_id, tool, input, response_size, response_snippet,
       source_domain, model_id
FROM observations ...
```
And extend the read struct `ObservationRow` (`observations.rs:14`):
```
pub struct ObservationRow {
    ... response_snippet: Option<String>,
    pub source_domain: Option<String>,   // vnc-049: stored attribution, NULL for legacy/non-opencode
    pub model_id: Option<String>,        // vnc-049
}
```
Bind the new column indices (8, 9) in each row-construction loop (`fetch_observations_since:66`,
`load_observations_for_sessions`, `load_observation_session_stats`). All three MUST change (a missed
query silently loses the column downstream).

## `parse_observation_rows` (`observation.rs:576`) — prefer stored

Today (`:597-604`) source_domain is ALWAYS read-derived from `event_type`. Change to a fork:
```
# row now carries stored source_domain (col idx 8) and model_id (col idx 9)
let stored_source_domain: Option<String> = row.get(8);
let stored_model_id: Option<String>      = row.get(9);

let source_domain: String = match stored_source_domain {
    Some(sd) if !sd.is_empty() => sd,                      # PREFER stored (ADR-001) — opencode rows land here
    _ => {                                                 # legacy NULL row → today's behavior EXACTLY
        let resolved = registry.resolve_source_domain(&event_type);
        if resolved != "unknown" { resolved } else { DEFAULT_HOOK_SOURCE_DOMAIN.to_string() }
    }
};
```
- `DEFAULT_HOOK_SOURCE_DOMAIN` (`:572`) is now the **legacy-row fallback only** (comment updated), never
  reached for a stored-opencode row.
- Surface `model_id` on the output record (`ObservationRecord` gains a `model_id: Option<String>` field
  so queries can distinguish local vs cloud — AC-06).

## Derivation-site pin (R-02.3 / OQ-4)

The stored value is authoritative; `resolve_source_domain` is reachable ONLY when the column is NULL.
A test pins this (see scenarios) so a future edit to the read fork cannot silently reintroduce
`claude-code` for opencode rows.

## Data flow

IN: rows from `observations` (with new cols, C5). OUT: `ObservationRecord { source_domain, model_id, ...}`
where opencode rows show `source_domain="opencode"` + their `model_id`; legacy rows unchanged.

## Error handling

- NULL columns → `None` → legacy fallback branch (no error).
- Existing security bounds (payload size 64KB `:608`, JSON depth 10 `:632`) unchanged.
- No panics; `row.get` on a newly-added column requires the SELECT to include it (coupling with the
  store SELECTs above — enforce both in one change).

## Key test scenarios (hints for tester)

1. **Stored preferred** (R-02.1/AC-03): a row with `source_domain="opencode"` reads back `"opencode"`,
   NOT read-derived (R-02.2 negative vs `claude-code`).
2. **Legacy NULL fallback** (R-04.2): a pre-migration row (NULL source_domain) reads back via
   `resolve_source_domain`/`DEFAULT_HOOK_SOURCE_DOMAIN` exactly as today. **Both fork branches asserted.**
3. **model_id surfaced** (AC-06): local-model row exposes `model_id="ollama/qwen3-coder"`, distinguishable
   on query from a cloud-model row and from a NULL/claude-code row.
4. **All three store SELECTs** carry the new columns (no silent loss).
5. **T-SEC-12/13 green unchanged** (R-05): claude-code rows (NULL) resolve their expected read-derived value.
</content>
