# Decision Register — #1060

| ID | Phase | Question | Decision | Decided-by | Source | Rationale |
|----|-------|----------|----------|------------|--------|-----------|
| D-001 | diagnosis | Approve F1 (remote.json-marker shared helper) + F2 (daemon_server stderr readiness wait)? | Approved, architect-revised design | uni-zero-bugfix | #5126 (vnc-039 ADR-003), #5293 (nan-021 ADR-003) | Root causes evidenced, architect concurs, test-infra only, no H-item |
| D-002 | diagnosis | File follow-ups (error wording; PR-CI init-layout coverage)? | Deferred to human | — | H2 | Filing widens scope |
| D-003 | review | D-002 follow-ups | File both as separate issues: (a) misleading "failed to load" error — bug, goal:personal-cloud; (b) PR-CI init+smoke coverage (higher priority, absorbs dry-run question) — enhancement, goal:personal-cloud, goal:platform | human | D-002 | Each changes product/CI behaviour, outside #1060 |
| D-004 | review | release.yml workflow_dispatch dry-run before v0.12.1 tag | Run it | human | — | :latest-<arch> overwrite low cost; avoids second failed release over unconfirmed rc=1 |
| D-005 | review | Security F1 (readiness wait before try/finally leaks store dir) | Apply: move require_embed_ready into try | human | H1 | — |
| D-006 | review | Security F2 (16-hex check in read_back_project_hash) | Decline | human | H1 | Fixtures use non-hex names |
| D-007 | merge | Gap: Gate 6 real path has no direct logic-test seam (covered via helper tests) | Accept | human | H4 | Dry-run exercised Gate 6 on both arches |
| D-008 | merge | Gap: test_daemon_readiness.py ran locally only with --noconftest (stale local binary) | Accept | human | H4 | Dry-run parity job exercised fixture |
| D-009 | merge | Gap: Gate 10 absent from dry-run log, unconfirmed | Accept | human | H4 | Not a regression vs v0.12.0 |
| D-010 | merge | Merge PR #1061 | Approved | human | H6 | — |
