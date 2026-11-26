# Decision Register — #1003

| ID | Phase | Question | Decision | Decided-by | Source | Rationale |
|----|-------|----------|----------|------------|--------|-----------|
| D-001 | bugfix | Work location | Worktree .claude/worktrees/1003, branch bugfix/1003-audit-event-id | human | — | requested |
| D-002 | bugfix | Approve diagnosis + audit-only allocator (4 tests) | Approved, conditional on architect concurrence after reading full diagnosis comment | human | — | architect previously reviewed summary only |
| D-003 | bugfix | Unrecoverable audit events dropped since 2026-05-23 (H4) | Accepted; note in PR | human | — | — |
| D-004 | bugfix | audit_fire_and_forget UNIQUE vs other failure logging (H2) | Not explicitly approved; out of scope unless human adds it | human | D-002 | human approved allocator only |
| D-005 | bugfix | audit_fire_and_forget failure-specific logging | Excluded from this fix (confirms D-004) | human | D-004 | — |
| D-006 | bugfix | D-002 condition: architect concurrence after full re-read | Met — CONCUR; counter stores last-issued id, SQL unchanged | uni-zero-bugfix | issuecomment-7176030150 | condition satisfied |
| D-007 | bugfix | 5 workspace clippy lints (transcript_hold.rs:268, main.rs:666/690, verbosity.rs:192/208) outside fix files (H8) | Pre-existing; move to new GH issue, not fixed here | human | — | — |
| D-008 | bugfix | 3 release-binary core dumps during smoke run | Determine cause within #1003 scope before PR | human | — | — |
| D-009 | bugfix | Rerun integration suites before gate? | No — final tester report shows complete coverage (lifecycle 108+6xf+1xp, volume 11, tools 1–176 + 170–231 rerun = all 231); timed-out runs were superseded | uni-zero-bugfix | issuecomment-7178497630 | coverage already complete |
| D-010 | bugfix | Delete 3 worktree core dumps | Yes, after diagnosis complete | uni-zero-bugfix | issuecomment-7179125716 | evidence captured in comment + Unimatrix #5895 |
| D-011 | bugfix | Shutdown abort on closed stderr (main.rs panic hook/log writer) | Out of scope; new GH issue with D-008 findings; #1003 proceeds to gate + PR | human | D-008 | — |
