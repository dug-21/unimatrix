| ID | Phase | Question | Decision | Decided-by | Source | Rationale |
|----|-------|----------|----------|------------|--------|-----------|
| D-001 | diagnosis | ORT source + method (H1) | PyPI wheel via `pip download --require-hashes` (pinned version + per-arch sha256); locate lib dynamically, not hardcoded wheel paths. Fallback: direct curl of pinned PyPI wheel URL with sha256, if pip/python issues arise | human | — | PyPI immutable; pip avoids per-arch URL table; negligible CI cost |
| D-002 | diagnosis | Scope widening (H2) | Approved: release.yml, root Dockerfile, infra-001 Dockerfile, install script, README | human | — | Same root cause |
| D-003 | diagnosis | ADR-002 amendment #5925 (H3) | Approved; amend to name pip/PyPI as source | human | — | Pin + fail-closed rule preserved |
| D-004 | diagnosis | release.yml in bounds | Yes; ci.yml-via-issue rule does not apply | uni-zero-bugfix | memory: ci.yml rule | Named in bug |
| D-005 | diagnosis | arm64 install validated before tag | Required | uni-zero-bugfix | reviewer comment 7194643788 | Dispatch skips arm64 build |
