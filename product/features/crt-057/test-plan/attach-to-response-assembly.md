# Test Plan — `attach_to_response_assembly` `[UNCHANGED]` + no-new-persistence

**File:** `unimatrix-server/src/mcp/distill_handler.rs:281`
**Risks:** R-03 (Critical) · **ACs:** AC-14

> Signature unchanged — no-ops on `None`/`Err`. The load-bearing crt-057 obligation here is **structural**:
> candidates + loss are attached at ASSEMBLY level, out-of-band, NEVER onto the memoized
> `RetrospectiveReport` (#4850). The loss carrier (`SessionLossInfo`/`search_complete`) is response-transient.

---

## Behavior (preserve existing coverage)
- `test_attach_none_leaves_response_unchanged` — `None` section → response unchanged (existing test, keep).
- `test_attach_some_appends_json_content_item` — `Some` section → candidates appended (existing test, keep).
- `test_attach_on_err_is_noop` — `Err` result → no-op (existing test, keep).

## R-03 — struct-shape / no-new-persistence guard (AC-14)
- `test_candidates_structurally_absent_from_memoized_report` — the persisted `RetrospectiveReport` has **NO**
  candidate field and **NO** transcript-content field; serializing it can never contain candidate text
  (compile-time + serialized-form assertion — existing test at `distill_handler.rs:775`, keep and extend).
- `test_loss_carrier_response_transient_only` — `SessionLossInfo`/`search_complete` appear ONLY in the
  response, never in any persisted row or the memoized report. (R-03 sc.1, sc.4.)
- `test_scoped_retrieval_path_no_new_persistence` — for default, json, force, `transcript:{}`, scoped
  `match`, scoped `anchor/phase`: scan every SQL row / file / log line written and assert none contains
  buffer or candidate byte-content (#5089 shape: no 64+ hex run / no verbatim delta text). The
  reclamation-without-review path is covered in `backstop-reclaim.md`. (R-03 sc.2.)

**Coverage requirement (AC-14):** candidates + loss stay response-transient, outside the memoized report, on
ALL changed paths; content-scan (not just a field-name check) on every write sink.
