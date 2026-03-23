# Security Review: bugfix-358-security-reviewer

## Risk Level: low

## Summary

The fix surgically removes a runtime panic caused by calling `Handle::current()` inside rayon worker threads. The change moves the async DB read to Tokio context before the rayon spawn and threads the resulting `Vec<EntryRecord>` into the rayon closure. No new trust boundaries, no new inputs from external sources, no new dependencies introduced. The change strictly reduces the risk surface by eliminating an async runtime misuse.

## Findings

### Finding 1: Empty-entries fallback in background.rs skips scan silently
- **Severity**: low
- **Location**: `crates/unimatrix-server/src/background.rs:583-586`
- **Description**: When `store.query_by_status(Status::Active)` fails, `active_entries` is set to `vec![]` and the scan proceeds with an empty list. This produces an empty contradiction result, which is stored in the cache as `None` (since the `Ok(Ok(pairs))` arm only fires when scan returns `Ok`). Wait — actually `vec![]` does not skip the scan: the rayon spawn still runs, `scan_contradictions` returns `Ok(vec![])`, and `contradiction_cache` is written to `Some(ContradictionScanResult { pairs: [] })`. This is safe: an empty contradiction list is a valid (if incomplete) result. The tracing warn is emitted, providing observability. No silent data corruption.
- **Recommendation**: Non-blocking. The existing `tracing::warn` is adequate signal.
- **Blocking**: no

### Finding 2: Empty-entries fallback in status.rs silently skips embedding check
- **Severity**: low
- **Location**: `crates/unimatrix-server/src/services/status.rs:562-565`
- **Description**: When `query_by_status` fails, `active_entries = vec![]` and `check_embedding_consistency` returns `Ok(vec![])`. The `embedding_check_performed` flag is set to `true` and an empty inconsistency list is written to the report. A caller reading `embedding_check_performed = true` but `embedding_inconsistencies = []` cannot distinguish "checked and found none" from "DB fetch failed, no check performed." This is a minor observability gap — not a security concern, but could mask a failing store.
- **Recommendation**: Non-blocking. A future improvement could distinguish the two cases with a separate `embedding_fetch_failed` flag, but this is outside the bugfix scope.
- **Blocking**: no

### Finding 3: check_entry_contradiction still uses Handle::current().block_on inside rayon
- **Severity**: low
- **Location**: `crates/unimatrix-server/src/infra/contradiction.rs:111-115`
- **Description**: `check_entry_contradiction` (untouched by this PR) uses `Handle::current().block_on(store.get(neighbor.entry_id))` inside the rayon quality-gate closure in `background.rs:1613`. This is the same class of bug as GH #358 but at a different call site. It is out of scope for this PR (approved fix was explicitly scoped to `scan_contradictions` and `check_embedding_consistency`), but it should be tracked.
- **Recommendation**: Non-blocking for this PR. File a follow-up GH Issue. This call site uses the same pattern and is susceptible to the same panic if the runtime context is ever absent from that rayon worker.
- **Blocking**: no

## OWASP Assessment

| Check | Result |
|-------|--------|
| Injection (SQL, command) | Not applicable — no new user inputs or SQL construction |
| Access control | Not applicable — no permission model changes |
| Deserialization | Not applicable — `EntryRecord` is read from internal SQLite, not external input |
| Input validation | Not applicable — `Vec<EntryRecord>` is an internal type, pre-validated by the store |
| Error handling | Safe — both call sites handle DB fetch errors with graceful fallback + logging |
| Secrets | None — no hardcoded credentials, tokens, or keys in the diff |
| New dependencies | None — `HashMap` is from `std::collections`, no external crates added |
| Unsafe code | None introduced |

## Blast Radius Assessment

**Worst case**: If `scan_contradictions` or `check_embedding_consistency` receive a `Vec<EntryRecord>` that is stale (fetched moments before an entry was deprecated or quarantined), the scan may flag a false contradiction or miss a true one for that tick. This is identical to the pre-existing window that existed in the original design (entries were fetched immediately before the scan anyway). No correctness regression relative to the intended design.

If `query_by_status` fails at the background tick call site, the contradiction cache retains its previous value (or stays `None` on cold start). This is safe — the server continues operating; contradiction detection simply skips one scan interval.

If `query_by_status` fails at the status.rs call site, the embedding check returns an empty result. The `context_status` response reports no inconsistencies. This is a false negative, not a false positive — no entries are incorrectly flagged.

## Regression Risk

**Low.** The public API of `scan_contradictions` and `check_embedding_consistency` changed (removed `store: &Store`, added `entries: Vec<EntryRecord>`). Both call sites were updated in the same PR. No external callers exist (confirmed by search). The regression test `test_scan_contradictions_does_not_panic_in_rayon_pool` directly exercises the fixed code path from a rayon worker context. The existing contradiction heuristic logic is untouched.

## Minimality

The diff is minimal. Three files changed, all directly related to the bug:
- `contradiction.rs`: signature change + remove `read_active_entries` + HashMap lookup
- `background.rs`: pre-fetch before spawn + regression test
- `status.rs`: pre-fetch before spawn

No unrelated changes included.

## PR Comments
- Posted 1 comment on PR #359 (see below)
- Blocking findings: no

## Knowledge Stewardship
- Stored: nothing novel to store — the rayon/Tokio runtime boundary anti-pattern is already present in the project's lesson entries (bugfix-351 covered the same category). The follow-up finding about `check_entry_contradiction` is filed as a GH Issue rather than a stored lesson since it is a specific instance of a known pattern.
