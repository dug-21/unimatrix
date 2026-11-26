| ID | Phase | Question | Decision | Decided-by | Source | Rationale |
|----|-------|----------|----------|------------|--------|-----------|
| D-001 | diagnosis | Approve 5 mechanical lint fixes | Approved | uni-zero-bugfix | #1027 design review; reviewer comment 7194641536 | Reproduced by clippy run; behavior-preserving; no H-item |
| D-002 | diagnosis | File follow-up for CI toolchain alignment + clippy step | File issue, do not touch ci.yml | uni-zero-bugfix | memory: ci.yml changes via issue | Out of scope here |
| D-003 | diagnosis | Close duplicates in fix PR | Closes #973 #1017 #935 only if verified identical; #756/#688 reported, not closed | uni-zero-bugfix | reviewer comment 7194641536 | Closing unverified stale issues is beyond fix scope |
| D-004 | test | #757 cert_provisioner flake (H8) | Accepted as pre-existing | human | — | Passes 3/3 isolated; untouched by diff |
