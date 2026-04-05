# Agent Report: bugfix-523-agent-5-session-sanitization

**Feature**: bugfix-523 — Server Hardening Batch  
**Item**: Item 4 — `sanitize_session_id` guard in `post_tool_use_rework_candidate` arm  
**Agent ID**: bugfix-523-agent-5-session-sanitization

---

## Task

Insert a `sanitize_session_id` guard into the `post_tool_use_rework_candidate` dispatch arm
in `crates/unimatrix-server/src/uds/listener.rs`, closing the last session injection gap in
`dispatch_request`. Add two tests per the test plan (AC-28 and AC-29).

---

## Files Modified

- `crates/unimatrix-server/src/uds/listener.rs`

---

## Changes Made

### Guard Insertion (lines 666–678 post-edit)

Inserted the `sanitize_session_id` guard in the `post_tool_use_rework_candidate` arm
immediately after the capability check block (line 665) and before
`event.payload.get("tool_name")` (line 679). The exact guard pattern from the
`RecordEvent` general arm (lines 731–738) was mirrored with message qualifier
`(rework_candidate)`.

Structural insertion order (SR-05 / C-07 compliance):
1. Capability check — existing, unchanged
2. `sanitize_session_id` guard — NEW (GH #523 SEC-02)
3. `event.payload.get("tool_name")` extraction — existing, unchanged
4. `session_registry.record_rework_event` — existing, unchanged

No `event.session_id` use appears between the capability check and the guard (R-04 verified
by code inspection).

### Tests Added

- `test_dispatch_rework_candidate_invalid_session_id_rejected` (AC-28 / T-08)
- `test_dispatch_rework_candidate_valid_session_id_succeeds` (AC-29 / T-09)

---

## Test Results

```
test uds::listener::tests::test_dispatch_rework_candidate_invalid_session_id_rejected ... ok
test uds::listener::tests::test_dispatch_rework_candidate_valid_session_id_succeeds ... ok
```

Full component run: **2763 passed, 1 failed** (the 1 failure is
`services::nli_detection_tick::tests::test_nli_gate_path_c_cosine_supports_edges_still_written_nli_disabled`
— in `nli_detection_tick.rs`, Items 1/2 scope, caused by another agent's in-progress
changes; confirmed pre-existing relative to this file by stash verification).

---

## Insertion Order Verification (R-04)

Code inspection confirms: the `post_tool_use_rework_candidate` arm structure is:

```
[line 660] capability check block
[line 665] }  ← closing brace of capability check
[line 666] // GH #523 (SEC-02): Validate session_id...
[line 668] if let Err(e) = sanitize_session_id(&event.session_id) {  ← GUARD (new)
[line 678] }  ← closing brace of guard
[line 679] let tool_name = event.payload.get("tool_name")...  ← payload extraction
[line 703] session_registry.record_rework_event(&event.session_id, ...)
```

No `event.session_id` reference appears between line 665 and line 668.

---

## Self-Check

- [x] `cargo build --workspace` passes (zero errors)
- [x] `cargo test --workspace` — no new failures introduced by this change
- [x] No `todo!()`, `unimplemented!()`, `TODO`, `FIXME`, or `HACK` in non-test code
- [x] All modified files are within scope defined in brief (listener.rs only)
- [x] Error handling uses `ERR_INVALID_PAYLOAD` constant, `HookResponse::Error` pattern
- [x] New structs: none added (all existing types used)
- [x] Code follows validated pseudocode — exact guard pattern from pseudocode/session-sanitization.md
- [x] Test cases match test plan (AC-28 → T-08, AC-29 → T-09)
- [x] No source file exceeds 500 lines — listener.rs is a pre-existing large file (not split; this agent did not create it)

---

## Knowledge Stewardship

- **Queried**: `mcp__unimatrix__context_briefing` — surfaced entries #3902 (lesson: adding
  registry reads to dispatch arms requires checking for sanitize_session_id) and #4141 (pattern:
  all HookRequest arms reading session_id must call sanitize_session_id first). Both directly
  relevant and confirmed the approach. Also queried `context_search` for
  "sanitize_session_id UDS dispatch pattern" (returned #4141, #322, #300) and for
  "bugfix-523 architectural decisions" (returned #4143 ADR-001).
- **Stored**: nothing novel to store — the pattern applied here (#4141, entry
  "UDS dispatch: all arms that use session_id must carry sanitize_session_id") is already
  in Unimatrix and fully describes what was implemented. The only learning is that the
  rework_candidate arm was the last gap, which is implicit from the fix being merged.
  Entry #3902 also already documents the guard-omission lesson. No new patterns discovered.
