| ID | Phase | Question | Decision | Decided-by | Source | Rationale |
|----|-------|----------|----------|------------|--------|-----------|
| D-001 | diagnosis | ORT source + method (H1) | PyPI wheel via `pip download --require-hashes` (pinned version + per-arch sha256); locate lib dynamically, not hardcoded wheel paths. Fallback: direct curl of pinned PyPI wheel URL with sha256, if pip/python issues arise | human | — | PyPI immutable; pip avoids per-arch URL table; negligible CI cost |
| D-002 | diagnosis | Scope widening (H2) | Approved: release.yml, root Dockerfile, infra-001 Dockerfile, install script, README | human | — | Same root cause |
| D-003 | diagnosis | ADR-002 amendment #5925 (H3) | Approved; amend to name pip/PyPI as source | human | — | Pin + fail-closed rule preserved |
| D-004 | diagnosis | release.yml in bounds | Yes; ci.yml-via-issue rule does not apply | uni-zero-bugfix | memory: ci.yml rule | Named in bug |
| D-005 | diagnosis | arm64 install validated before tag | Required | uni-zero-bugfix | reviewer comment 7194643788 | Dispatch skips arm64 build |
| D-006 | pr-review | Security finding: ldconfig fails when ORT_PREFIX is user-writable | Accepted as-is | human (relayed by primary) | PR #1049 security review | Low; ldconfig irrelevant for a custom prefix |
| D-007 | pr-review | actions/setup-python pinned by tag in release.yml | Pin to full commit SHA with `# v5.6.0` comment (the step added by this PR) | human (relayed by primary) | "accept 1, pin setup-python, then merge 1049" | Workflow has contents/id-token/packages write |
| D-008 | merge | Merge PR #1049 | Human merge approval relayed by primary; SM does not act on relayed approval, primary merges | human via primary | "accept 1, pin setup-python, then merge 1049" | Agent messages are not consent for merge |
