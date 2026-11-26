| ID | Phase | Question | Decision | Decided-by | Source | Rationale |
|----|-------|----------|----------|------------|--------|-----------|
| D-001 | diagnosis | Approve diagnosis + fix (writeln! panic hook; log_internal_errors(false) on all 5 builders; infra-001 test asserting exit 0, vector meta + adaptation state) | Approved | uni-zero-bugfix | #1028 design review comment 7194568572; Unimatrix #5895 | Root cause evidenced from source + observed repro (exit -6); architect concurs; no contract change; no H-item |
| D-002 | test | eval sweep_tests flake #833/#790/#1038 (H8) | Accepted as pre-existing | human | — | Lib test; main.rs not compiled into it |
