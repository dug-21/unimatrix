# Test Plan: C5 Usage Dedup

## Risk Coverage: R-03 (Dedup bypass)

### T-C5-01: filter_access returns entries on first call (R-03 scenario 1)
- Create UsageDedup
- filter_access("agent-1", [1,2,3]) returns [1,2,3]
- filter_access("agent-1", [1,2,3]) returns [] (all counted)
- Verifies: AC-04, AC-15

### T-C5-02: filter_access per-agent isolation (R-03 scenario 2)
- filter_access("agent-1", [42]) returns [42]
- filter_access("agent-2", [42]) returns [42] (different agent)

### T-C5-03: check_votes returns NewVote on first call (R-03 scenario 3)
- check_votes("agent-1", [1,2], true) returns [(1, NewVote), (2, NewVote)]
- check_votes("agent-1", [1,2], true) returns [(1, NoOp), (2, NoOp)]
- Verifies: AC-15

### T-C5-04: check_votes per-agent isolation (R-03 scenario 4)
- check_votes("agent-1", [42], true) returns [(42, NewVote)]
- check_votes("agent-2", [42], true) returns [(42, NewVote)]

### T-C5-05: filter_access and check_votes independent (R-03 scenario 5)
- filter_access("agent-1", [42]) -> [42]
- check_votes("agent-1", [42], true) -> [(42, NewVote)]
- Both work independently

### T-C5-06: filter_access mixed new and old (R-03 scenario 6)
- filter_access("agent-1", [1,2]) -> [1,2]
- filter_access("agent-1", [2,3]) -> [3] (2 already counted)

### T-C5-07: Large batch dedup (R-03 scenario 7)
- filter_access with 100 entries
- Second call returns empty

### T-C5-08: No redb tables for dedup state (R-03 scenario 8)
- After dedup operations, verify no dedup-related tables in store

## Risk Coverage: R-16 (Vote correction atomicity)

### T-C5-09: Vote correction unhelpful->helpful (R-16 scenario 1)
- check_votes("agent-1", [42], false) -> [(42, NewVote)]
- check_votes("agent-1", [42], true) -> [(42, CorrectedVote)]
- Verifies: AC-16

### T-C5-10: Same vote repeated (R-16 scenario 2)
- check_votes("agent-1", [42], true) -> [(42, NewVote)]
- check_votes("agent-1", [42], true) -> [(42, NoOp)]

### T-C5-11: Vote correction helpful->unhelpful (R-16 scenario 3)
- check_votes("agent-1", [42], true) -> [(42, NewVote)]
- check_votes("agent-1", [42], false) -> [(42, CorrectedVote)]

### T-C5-12: Batch correction mixed (R-16 scenario 5)
- Vote on 5 entries as helpful
- Change vote on 3 of them to unhelpful
- Verify 3 CorrectedVote, 2 NoOp
