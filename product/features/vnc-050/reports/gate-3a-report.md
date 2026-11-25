# Gate 3a Report: vnc-050

> Gate: 3a (Design Review), re-run after rework iteration 1
> Date: 2026-11-25
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | PASS | The ADR-004 latch scope is resolved by HD 10. ADR-004 now reads "per credential", with a marker file that is 0600, keyed on SHA-256, has no expiry, and fails open. |
| Specification coverage | PASS | P2: Admin tools listed (cap-ceiling.md:128). P3: non-ASCII policy stated (remote-resolve.md:91,98). |
| Risk coverage | PASS | All 22 risks are mapped. RTS R-17 and line 170 are aligned with the marker design. |
| Interface consistency | PASS | T1, T2 and P1 are resolved. The test plan matches the amended observe-403-client pseudocode. |
| Knowledge stewardship | WARN | Unchanged from the prior run. The testplan agent's queries were not run, and a reason was given. |

## Prior Items

| Item | Status | Evidence |
|------|--------|----------|
| H1 ADR-004 latch | Resolved | HD 10 APPROVED. ADR-004 lines 1, 13-18 are amended. RTS lines 27, 126-128 and 170 are updated. |
| T1 latch tests vs marker | Resolved | `latch_per_process` is removed. test-plan/observe-403-client.md:12-21 now covers: N spawns give 1 POST; latch only on the SessionWrite 403; full SHA-256 key, 0600 and no token in the marker; no expiry; credential change resets; corrupt marker gives one retry and a rewrite; a read-write credential creates no marker; missing or unreadable marker fails open; write failure causes no behavior change. |
| T2 hook exit 0 | Resolved | test-plan/bridge-transports.md:30: the bridge exits 1 and the hook exits 0 with `{ok:false, reason:"misconfig"}` and one stderr line. |
| P1 test seams | Resolved | pseudocode/OVERVIEW.md:111-121 defines `DISPATCH_SPY`, `RESOLVE_SPY`, `StripPrincipalLayer`, the UsageService test hooks, the scan counter and `AFTER_TMP_CREATE`. All are `#[cfg(test)]` with no production surface, and delivery may not add new seams. |
| P2 Admin tools | Resolved | `context_quarantine` and `context_enroll` are listed, and the test plan matches (cap-ceiling.md:23). |
| P3 non-ASCII | Resolved | Values are limited to HTAB or 0x20-0x7E. Anything else is rejected with `bad_headers`. Test rows match. |

## Marker Consistency (test plan vs pseudocode)
- No URL in the marker. `isDenied` compares only `m.cred === credId(token)`, and the test plan has no URL assertion.
- Full SHA-256 hex is used (`credId`), as the `marker_keyed_on_credential_hash` test asserts.
- Mode 0600 is set via `atomicWrite(..., {mode: 0o600})`, as the test asserts with a `stat`.
- A corrupt marker is caught by the catch block and treated as not denied. The next 403 rewrites it, which matches `corrupt_marker_retries_once_and_rewrites`.

## Notes for Delivery (non-blocking)
- `state.atomicWrite` may need to be exported and given a `mode` option (pseudocode line 42).

## Knowledge Stewardship
- Stored: nothing novel to store -- the rework resolved feature-specific items. The only lesson is a single instance (parallel pseudocode and test plans diverge on open questions), which is not yet a cross-feature pattern.
