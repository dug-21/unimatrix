# Security Review: 684-685-security-reviewer

## Risk Level: low

## Summary
Test-expectation update (#684) plus binary-rename doc/Docker fixes and a session-scoped preflight guard (#685). No production runtime code changes. No new trust boundaries, no injection surface, no new dependencies, no secrets. No blocking findings.

## Findings

### 1. Schema union does not loosen input validation (#684)
- **Severity**: none (verified non-issue)
- **Location**: crates/unimatrix-server/src/server.rs:3246-3320 (test-only)
- **Description**: The `["integer","null"]` union on 5 optional fields is truthful — `deserialize_opt_i64_or_string`/`deserialize_opt_usize_or_string` (`mcp/serde_util.rs`) have accepted JSON `null` since vnc-012 (`visit_none`/`visit_unit` → `None`). The 4 required fields' `I64OrStringVisitor` has no `visit_none`, so null is still rejected (-32602). Advertised schema now matches actual acceptance. AC-10's empty-schema `{}` guard survives (fails both assertion branches).
- **Blocking**: no

### 2. Preflight subprocess execution
- **Severity**: low (informational)
- **Location**: product/test/infra-001/harness/conftest.py:74-127
- **Description**: `subprocess.run([binary, "version"], ...)` — list args, no shell, no injection. `UNIMATRIX_BINARY` is operator-controlled test config the harness already executes via `Popen` in client.py; no new trust boundary. Verified `handle_version` (main.rs:1461) is side-effect-free without `--project-dir` (DB-creation branch not reached). 30s timeout bounds hangs.
- **Blocking**: no

### 3. stderr tail in ServerDied exception messages
- **Severity**: low (informational)
- **Location**: product/test/infra-001/harness/client.py:51-62
- **Description**: Server `RUST_LOG=info` output now appears in pytest error lines / CI logs. Bounded to 3 lines; full stderr was already retained on `self.stderr_output`. Acceptable for a local harness with synthetic data; revisit if the harness ever runs against real data in shared CI.
- **Blocking**: no

### 4. tomllib usage
- **Severity**: none
- **Location**: product/test/infra-001/harness/conftest.py:54-70
- **Description**: Python 3.11+ stdlib (harness requires 3.12+) — no new dependency, no CVE surface. Parse failures (`OSError`/`TOMLDecodeError`/`KeyError`) degrade to skipping the version-match check; malformed Cargo.toml cannot crash collection.
- **Blocking**: no

### 5. Preflight false-positive friction
- **Severity**: low
- **Location**: product/test/infra-001/harness/conftest.py:117-126
- **Description**: Strict version equality aborts sessions when an older binary is intentionally tested via `UNIMATRIX_BINARY`. Fail-closed and self-explanatory — correct trade-off. An env-var escape hatch can be added later if needed.
- **Blocking**: no

## Blast Radius Assessment
- #684 worst case: miscategorized field → loud test failure; zero production effect.
- #685 worst case: preflight bug → test-session abort (test-infra DoS only, never production). False-negative: same-version stale rebuild passes — still strictly better than pre-fix.
- Dockerfile: COPY rename fails the build immediately if wrong (fail-closed). `--package unimatrix-server` correctly left unchanged (package vs bin-target distinction — design-review trap avoided).
- Docker runtime image lacks workspace Cargo.toml → version-match skipped, execute/exit-code/format checks still run. Graceful degradation confirmed.

## Regression Risk
- Autouse session fixture runs in every harness invocation including Docker; verified safe degradation there.
- `_WORKSPACE_ROOT` (5 `.parent` hops from harness/conftest.py) resolves correctly to repo root.
- Exact-equality assertion on `["integer","null"]` couples the test to rmcp ≥1.7 emission order; investigator confirmed schemars emission deterministic. Test-only exposure.
- suites/conftest.py re-export ensures fixture discovery — preserved.

## PR Comments
- Posted 1 review comment on PR #687 (gh pr review --comment)
- Blocking findings: no

## Knowledge Stewardship
- Stored: nothing novel to store — lessons #4730/#4731 (rmcp nullable-union, stale bin-rename artifact) already stored by investigators; no recurring security anti-pattern observed across PRs.
