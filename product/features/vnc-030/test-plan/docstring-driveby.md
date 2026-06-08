# Test Plan — C9 docstring drive-by

Source: ADR-004 §C9, FR-25. Risk: — (comment-only, no behavior change). Files: `crates/unimatrix-observe/src/attribution.rs`, `packages/unimatrix/lib/hook-client/topic-signal.js`. Verification method: **grep / diff review** (no runtime test).

The misleading `{alpha}-{digits}` docstrings are corrected to describe the actual filter: hyphen required, `[A-Za-z0-9-_.]`, **no digit requirement** (`is_valid_feature_id` has no digit requirement — OQ2 resolution). Comment-only — **no behavior change** to the extractor.

## Verification

### FR-25 — docstrings corrected (diff review)
- `attribution.rs`: the `{alpha}-{digits}` docstring is replaced with the actual filter description (hyphen required, `[A-Za-z0-9-_.]`, no digit requirement).
- `topic-signal.js`: same correction.

### no_behavior_change (regression guard)
- The existing `topic-signal.js` extraction tests and `attribution.rs` tests pass **byte-unchanged** — the diff is comment-only. Assert no test assertion on extractor behavior changes.

### grep confirmation
```bash
grep -rn '{alpha}-{digits}' crates/unimatrix-observe/src/attribution.rs \
  packages/unimatrix/lib/hook-client/topic-signal.js
```
- **Assert**: zero matches after the fix (the misleading pattern is gone).

## Coverage requirement
Comment-only diff; the extractor's behavior and its existing tests are unchanged; the `{alpha}-{digits}` claim is removed from both files.

## NOT in scope
Tightening the extractor's permissive filter (ass-072 discovery 4) — only the docstrings are corrected here; behavior change is a separate, out-of-scope effort.
