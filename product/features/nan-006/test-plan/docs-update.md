# Test Plan: C4 — USAGE-PROTOCOL.md update

## Verification Approach

### Pre-Release Gate section present
- Read USAGE-PROTOCOL.md and verify section heading "Pre-Release Gate" exists
- Verify `pytest -m availability` command is documented
- Verify run time (~15-20 min) is mentioned

### When to Run table updated
- Verify table has a row for availability tier
- Verify it states "Pre-release only"

### Suite Reference updated
- Verify `availability` suite listed under Suite Reference section
- Verify xfail explanation present

## Acceptance
- USAGE-PROTOCOL.md contains "Pre-Release Gate" (grep check)
- USAGE-PROTOCOL.md contains "availability" mark reference
