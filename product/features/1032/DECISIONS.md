| ID | Phase | Question | Decision | Decided-by | Source | Rationale |
|----|-------|----------|----------|------------|--------|-----------|
| D-001 | diagnosis | Approve test-only fix (sync register w/ bounded retry, poll positive reads, sync register in truncated-frame test) | Approved | uni-zero-bugfix | #1032 design review 7194611278 | Race evidenced in code, architect concurs, no assertion weakened, no contract change |
| D-002 | diagnosis | Queue-replay delta loss (F5) | Filed as separate issue; not part of this fix | uni-zero-bugfix | ADR-003 (vnc-027) | Pre-existing production behaviour; out of scope; tracked, not accepted |
