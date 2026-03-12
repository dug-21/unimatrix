# Test Plan: Seed State Machine (Component 5)

## Content Review Checks

### CR-01: State Completeness
- [ ] All states present in SKILL.md instructions: PREFLIGHT, EXISTING_CHECK, LEVEL_0, GATE_0, LEVEL_1, GATE_1, LEVEL_2, GATE_2, DONE

### CR-02: STOP Gate Phrasing (R-01, ADR-001)
- [ ] GATE_0 contains explicit STOP instruction
- [ ] GATE_1 contains explicit STOP instruction
- [ ] GATE_2 contains explicit STOP instruction (or direct transition to DONE)
- [ ] EXISTING_CHECK contains STOP when warning displayed
- [ ] Bold/emphasized phrasing used for STOP gates
- [ ] grep: count of "STOP" or "Wait for human" occurrences >= 3

### CR-03: Level Transitions
- [ ] PREFLIGHT fail -> halt with error
- [ ] EXISTING_CHECK -> LEVEL_0 (clean) or warn+ask (entries found)
- [ ] LEVEL_0 -> GATE_0 (always)
- [ ] GATE_0 approved + deeper -> LEVEL_1
- [ ] GATE_0 approved + no deeper -> DONE
- [ ] GATE_0 rejected -> DONE
- [ ] GATE_1 approved + deeper -> LEVEL_2
- [ ] GATE_1 no deeper -> DONE
- [ ] GATE_2 -> DONE (always, terminal)

### CR-04: Depth Limit (R-07, AC-09)
- [ ] SKILL.md states Level 2 is the final level
- [ ] No Level 3 offered, mentioned, or hinted at
- [ ] After GATE_2 completion, only DONE state follows
- [ ] grep: no "Level 3" string in SKILL.md

### CR-05: Approval Modes (R-08)
- [ ] Level 0: batch approval explicitly instructed
- [ ] Level 1: per-entry individual approval explicitly instructed
- [ ] Level 2: per-entry individual approval explicitly instructed
- [ ] Clear distinction between batch and individual modes

### CR-06: Exploration Scope
- [ ] Level 0: README, manifests, CLAUDE.md, .claude/ listing (no deep reads)
- [ ] Level 1: module dirs, test dirs, config files
- [ ] Level 2: deeper reads within Level 1 selections
- [ ] Each level's scope is bounded (not open-ended)

## Risk Coverage

| Risk | Check |
|------|-------|
| R-01 | CR-02 (STOP gates), CR-03 (transitions) |
| R-07 | CR-04 (depth limit) |
| R-08 | CR-05 (approval modes) |
