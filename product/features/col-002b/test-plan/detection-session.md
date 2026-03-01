# Test Plan: detection-session

## Component: 5 session rules in `detection/session.rs`

## Test Module: `#[cfg(test)] mod tests` within `session.rs`

### Existing Rule Tests (moved from detection.rs)

SessionTimeoutRule tests move verbatim:
- `test_session_timeout_three_hour_gap`
- `test_session_timeout_one_hour_gap`
- `test_session_timeout_empty_records`
- `test_session_timeout_single_record`

These must pass unchanged after the move (R-05).

### ColdRestartRule Tests

| Test | Scenario | Expected |
|------|----------|----------|
| `test_cold_restart_fires` | Read files A,B at ts=0-1000. Gap of 35min. Read file A again at ts=35min+100 | Finding (overlap with prior reads) |
| `test_cold_restart_new_files_only` | Read files A,B. Gap of 35min. Read NEW file C only | No finding (no overlap) |
| `test_cold_restart_short_gap` | Read files A,B. Gap of 25min. Re-read A | No finding (gap < 30min threshold) |
| `test_cold_restart_empty` | Empty input | No findings |
| `test_cold_restart_single_record` | One record | No finding (no gap possible) |

Risk coverage: R-01, R-08 (false positives), R-12

### CoordinatorRespawnsRule Tests

| Test | Scenario | Expected |
|------|----------|----------|
| `test_coordinator_respawns_fires` | 4 SubagentStart with "uni-scrum-master" | Finding (> 3) |
| `test_coordinator_respawns_below` | 2 SubagentStart with coordinator names | No finding |
| `test_coordinator_respawns_mixed` | 3 "uni-scrum-master" + 5 "uni-rust-dev" | No finding (only 3 coordinators, not > 3) |
| `test_coordinator_respawns_various_names` | "scrum-master", "coordinator", "lead" all match | 3 matches (but not > 3 so silent -- adjust to 4 to fire) |
| `test_coordinator_respawns_empty` | Empty input | No findings |
| `test_coordinator_respawns_case_insensitive` | "Scrum-Master", "COORDINATOR" | Should match |

Risk coverage: R-01, R-12

### PostCompletionWorkRule Tests

| Test | Scenario | Expected |
|------|----------|----------|
| `test_post_completion_fires` | 100 records. TaskUpdate "completed" at record 80. 20 records after -> 20% | Finding (> 8%) |
| `test_post_completion_below` | 100 records. Completion at record 96. 4 after -> 4% | No finding |
| `test_post_completion_no_taskupdate` | 100 records, no TaskUpdate | No finding (no boundary) |
| `test_post_completion_empty` | Empty input | No findings |
| `test_post_completion_last_boundary` | Two completions at record 50 and record 90. 10 records after -> 10% | Finding uses LAST completion |

Risk coverage: R-01, R-09 (boundary detection), R-12

### ReworkEventsRule Tests

| Test | Scenario | Expected |
|------|----------|----------|
| `test_rework_fires` | TaskUpdate "completed" then TaskUpdate "in_progress" for same task | Finding |
| `test_rework_normal_flow` | TaskUpdate "in_progress" then "completed" | No finding |
| `test_rework_completed_only` | TaskUpdate "completed" only | No finding |
| `test_rework_multiple` | 2 separate tasks reworked | Finding with measured=2 |
| `test_rework_empty` | Empty input | No findings |
| `test_rework_missing_status` | TaskUpdate with no status field in input | No finding (skipped) |

Risk coverage: R-01, R-09 (status parsing), R-12

### find_completion_boundary Tests

| Test | Scenario | Expected |
|------|----------|----------|
| `test_boundary_found` | Records with TaskUpdate "completed" at ts=5000 | Some(5000) |
| `test_boundary_last_used` | Two completions at ts=3000 and ts=8000 | Some(8000) |
| `test_boundary_not_found` | No TaskUpdate records | None |
| `test_boundary_non_completed_status` | TaskUpdate "in_progress" only | None |

Risk coverage: R-09 (shared by PostCompletionWork and PostDeliveryIssues)
