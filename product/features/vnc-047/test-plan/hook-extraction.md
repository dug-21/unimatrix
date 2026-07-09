# C4 — Hook tag extraction (`build_cycle_event_or_fallthrough`)

> File: `crates/unimatrix-server/src/uds/hook.rs` (:769, beside goal extraction :839-880).
> Extract `tags` Start-only, non-empty-filtered, into `payload["tags"]` (JSON array). Infallible.
> Risks: R-03 (payload contract), R-11 (opacity at intake), R-09 (start-only). ACs: AC-01, AC-02
> (extraction leg).

## Reuse
Hook test helpers in `hook.rs` test module (parity with existing goal-extraction tests). Build a
`tool_input` JSON with a `tags` array and assert the resulting `payload["tags"]`.

## Unit test expectations
- `test_hook_extracts_tags_on_start` — `tool_input["tags"] = ["arm:A","foo"]` on a Start event →
  `payload["tags"]` is a JSON array `["arm:A","foo"]` (parity with `payload["goal"]`,
  hook.rs:877-880).
- `test_hook_omits_tags_on_non_start` — same `tags` on a non-start (phase/outcome/next_phase) event →
  `payload` has NO `tags` key (Start-only; FR-4/R-09 first leg).
- `test_hook_filters_empty_string_tags` — `["a","","b"]` → `payload["tags"] == ["a","b"]` (non-empty
  filter at intake; R-11).
- `test_hook_no_tags_key_when_all_empty_or_absent` — `tags` absent, or `[]`, or `["",""]` → NO
  `tags` key in payload (so the listener routes to the unchanged `insert_cycle_event` arm — R-09).
- `test_hook_tags_read_from_tool_input_not_cycleparams` — the extracted value comes from
  `tool_input["tags"]` (parity with how `goal` is read at hook.rs:844), establishing the hook-only
  route (SR-03).

## Contract with listener (C5)
`payload["tags"]` MUST be a JSON array of strings. The listener reads
`payload.get("tags").and_then(|v| v.as_array())`; a malformed/object value must degrade to "no tags",
never panic (asserted at listener tier). Infallible extraction: filtering, never erroring.

## Edge cases
- Unicode / colon-prefixed tags pass through verbatim (no derivation at the hook).
- Whitespace-only tag is non-empty → passes the filter (per FR-2).
