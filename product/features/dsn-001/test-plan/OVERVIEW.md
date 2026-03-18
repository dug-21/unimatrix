# dsn-001 Test Plan — Overview

## Overall Test Strategy

This feature introduces the config externalization system (W0-3) and touches eight
distinct components across three crates. The test strategy is layered:

1. **Unit tests** — validate each component in isolation. `validate_config()` must
   be independently testable (no tokio, no store). This is the primary vehicle for
   risk coverage: ConfigError variants, preset weights, freshness precedence chain,
   security validation, merge semantics.

2. **Integration tests (infra-001 harness)** — validate behavior visible through the
   MCP JSON-RPC interface. Covers: tool rename propagation, server instructions in
   `initialize` response, agent enrollment with configured capabilities, two-level
   merge effect on CategoryAllowlist, boosted_categories in search behavior.

3. **Static gates (grep)** — two mandatory non-test checks:
   - `grep -r "context_retrospective" .` returns zero matches outside excluded dirs.
   - `grep '"lesson-learned"' crates/unimatrix-server/src/services/search.rs` returns
     zero matches.

4. **Full test suite regression** — `cargo test --workspace 2>&1 | tail -30` with
   `ConfidenceParams::default()` at all migrated call sites. Zero failures required.

---

## Risk-to-Test Mapping

| Risk ID | Priority | Component(s) | Test Section(s) |
|---------|----------|-------------|-----------------|
| R-01 | Critical | confidence-params | R-01: weight fields load-bearing; `empirical` w_fresh test |
| R-02 | Critical | confidence-params | SR-10 mandatory test with exact comment |
| R-03 | Critical | confidence-params, config-loader | All four preset rows sum to 0.92; `0.95` boundary case |
| R-04 | Critical | tool-rename | grep static gate; integration test_protocol.py; context_cycle_review call |
| R-05 | Critical | config-loader | Four-case truth table for custom preset missing fields |
| R-06 | Critical | config-loader | Four AC-25 freshness precedence unit tests (named) |
| R-07 | High | config-loader | Instructions injection + length check ordering |
| R-08 | High | config-loader | Named preset immune to [confidence] weights |
| R-09 | High | config-loader | Sum-invariant 0.95 rejection (vs wrong `<= 1.0`) |
| R-10 | High | config-loader | Cross-level custom preset weight inheritance prohibited |
| R-11 | High | config-loader | Admin exclusion from session_capabilities allowlist |
| R-12 | High | config-loader | NaN, Infinity, 0.0, -0.0, 87600.001 for half_life |
| R-13 | Med | startup-wiring | ContentScanner::global() warm call code review gate |
| R-14 | High | agent-registry | session_caps propagation integration test |
| R-15 | Med | startup-wiring | dirs::home_dir() None degrades gracefully (unit test) |
| R-16 | Med | config-loader | 65536 vs 65537 byte file size cap boundary |
| R-17 | Med | category-allowlist | new() still returns compiled defaults after dsn-001 |
| R-18 | Med | config-loader | from_preset(Custom) panic by design — audit gate |
| R-19 | Med | config-loader | BoostedCategoryNotInAllowlist validation |
| R-20 | Med | startup-wiring | Hook/bridge path excluded from config load |
| R-21 | Med | config-loader | Error message identifies file path in ConfigError Display |
| R-22 | Med | config-loader | Merge false-negative (Option<f64> type prevents it) |
| IR-01 | Integration | confidence-params | cargo test --workspace zero failures |
| IR-02 | Integration | agent-registry | session_caps non-None flows to AgentRecord |
| IR-03 | Integration | search-service | grep + integration with custom boosted_categories |
| IR-04 | Integration | startup-wiring | Background tick receives resolved ConfidenceParams |
| IR-05 | Integration | category-allowlist | new() vs from_categories(INITIAL) identity |
| EC-01 | Edge | config-loader | Empty categories list behavior documented and tested |
| EC-02 | Edge | search-service | Empty boosted_categories HashSet no panic |
| EC-03 | Edge | config-loader | Zero weight in custom (valid, sum invariant still applies) |
| EC-04 | Edge | config-loader | IEEE boundary values for half_life |
| EC-05 | Edge | config-loader | Empty per-project config file (zero bytes) = defaults |
| EC-06 | Edge | config-loader | 65536-byte boundary inclusive |
| EC-07 | Edge | config-loader | Symlink to world-writable target aborts startup |
| EC-08 | Edge | agent-registry | Duplicate session_capabilities behavior documented |
| SR-SEC-01 | Security | config-loader | Instructions injection; length-before-scan ordering |
| SR-SEC-02 | Security | config-loader | Admin exclusion explicit allowlist |
| SR-SEC-03 | Security | config-loader | Category character/length/count validation |
| SR-SEC-04 | Security | config-loader | metadata() not symlink_metadata() code review |
| SR-SEC-05 | Security | config-loader | 64 KB read-to-buffer before toml::from_str |

---

## Acceptance Criteria Coverage

| AC-ID | Component | Test Plan Section |
|-------|----------|-------------------|
| AC-01 | startup-wiring | No-config backward compatibility |
| AC-02 | category-allowlist | from_categories custom list replaces defaults |
| AC-03 | search-service | grep gate + integration with custom boosted list |
| AC-04 | confidence-params | freshness_score with configurable half_life |
| AC-05 | server-instructions | MCP initialize integration test |
| AC-06 | agent-registry | Strict trust + session_caps integration test |
| AC-07 | config-loader | Two-level merge replace semantics integration test |
| AC-08 | config-loader | World-writable abort (unix) |
| AC-09 | config-loader | Group-writable warn (unix) |
| AC-10 | config-loader | Category char/length/count validation |
| AC-11 | config-loader | BoostedCategoryNotInAllowlist |
| AC-12 | config-loader | InstructionsInjection |
| AC-13 | tool-rename | grep gate + test_protocol.py + live call |
| AC-14 | tool-rename | Manual doc comment read |
| AC-15 | config-loader | FileTooLarge 65537 boundary |
| AC-16 | config-loader | InvalidHalfLifeValue (0.0, -1.0, NaN, Infinity, -0.0) |
| AC-17 | config-loader | HalfLifeOutOfRange 87600.001 |
| AC-18 | config-loader | InvalidDefaultTrust |
| AC-19 | config-loader | InvalidSessionCapability (Admin) |
| AC-20 | config-loader | InstructionsTooLong before scan |
| AC-21 | confidence-params | SR-10 test present with exact comment |
| AC-22 | confidence-params | resolve_confidence_params(default config) == ConfidenceParams::default() |
| AC-23 | confidence-params | Named preset exact field values; [confidence] ignored |
| AC-24 | config-loader | custom missing weights abort |
| AC-25 | config-loader | Four freshness precedence unit tests (named) |
| AC-26 | config-loader | Unrecognised preset serde error |
| AC-27 | confidence-params | Nine-field struct; all fields non-zero for all presets |

---

## Integration Harness Plan (infra-001)

### Suites to Run

This feature touches: server tool logic (rename), server instructions, agent
enrollment behavior, search boosted_categories, and a new config startup path.

| Suite | Rationale |
|-------|-----------|
| `smoke` | Mandatory minimum gate. Validates core paths not broken by startup changes. |
| `protocol` | Tool list must contain `context_cycle_review`; must not contain `context_retrospective`. |
| `tools` | All `context_retrospective` call sites renamed in test_tools.py (14 sites); renamed tool must respond correctly. |
| `security` | ContentScanner warm-up ordering; capability enforcement after config-driven session_caps. |
| `lifecycle` | Restart persistence; ConfidenceParams loaded at startup persists through background tick. |

Suites **not** required: `confidence` (confidence formula logic unchanged; only
params delivery changes), `contradiction`, `volume` (no schema changes), `edge_cases`
(no new edge-case behavior visible through MCP).

### New Integration Tests Needed

The following behavior is only visible through the MCP interface and is not covered
by existing suite tests. These tests must be added to the appropriate suites in
Stage 3b/3c:

#### 1. `suites/test_tools.py` — context_cycle_review call succeeds (AC-13)

```python
def test_cycle_review_renamed_tool_responds(server):
    """AC-13: context_cycle_review is callable and returns structured output."""
    resp = server.context_cycle_review(feature_cycle="col-022")
    assert resp["status"] == "ok" or "report" in resp
```

Fixture: `server`. Tool renamed from `context_retrospective`.

#### 2. `suites/test_protocol.py` — tool list membership (AC-13)

Update existing test at line 55 to assert:
- `"context_cycle_review"` in tool names
- `"context_retrospective"` not in tool names

#### 3. `suites/test_tools.py` — server instructions in initialize (AC-05)

```python
def test_server_instructions_from_config(server):
    """AC-05: [server] instructions appears in ServerInfo.instructions."""
    # Requires a server fixture launched with a config specifying instructions.
    # Currently the harness uses a fixed binary with no config — needs fixture
    # extension or env var injection to pass a config path.
```

Note: This test requires a harness fixture that starts the server with a config
file. If this is not feasible without significant harness changes, file a GH Issue
and test AC-05 at the unit level through validate_config + ServerConfig struct alone.
The integration path (MCP initialize response) should be documented as a gap.

#### 4. `suites/test_tools.py` — agent enrollment with session_caps (AC-06)

```python
def test_agent_enrollment_strict_session_caps(admin_server):
    """AC-06: strict trust + session_capabilities = ["Read","Search"] is enforced."""
    # Requires a server started with configured session_capabilities.
    # Same harness fixture constraint as AC-05 above.
```

Same constraint: config injection into harness fixture needed for AC-05/AC-06/AC-07.

### Harness Fixture Gap: Config-Aware Server

Tests for AC-05, AC-06, and AC-07 require starting the server with a specific
config file. The current harness does not have a config-injection fixture. Options:

1. **Add a `config_server` fixture** in `conftest.py` that writes a temp config
   file and sets `UNIMATRIX_CONFIG_PATH` (if that env var is added) or places the
   file at the expected global path within the test environment. This is the cleanest
   approach.
2. **Use a unit test** for AC-05/AC-06 at the struct level; accept the MCP-level
   verification as partial (config struct + unit test suffice for acceptance).
3. **File a GH Issue** for `infra-001` config-injection fixture support; scope it
   as a harness enhancement separate from dsn-001.

**Recommendation**: Deliver unit tests for AC-05/AC-06/AC-07 in Stage 3b. Attempt
the integration path in Stage 3c. If harness changes are too large for this PR,
file a GH Issue and document the gap in RISK-COVERAGE-REPORT.md.

### Tests to Update in Existing Suites

| File | What to Update |
|------|---------------|
| `suites/test_protocol.py` line 55 | `"context_retrospective"` → `"context_cycle_review"` |
| `suites/test_tools.py` ~14 sites | All `context_retrospective` → `context_cycle_review` (per SR-05 checklist) |
| `harness/client.py` | `context_retrospective` method → `context_cycle_review` |

These are not new tests — they are required updates from the SR-05 rename blast
radius. They must be complete before Stage 3c runs.

---

## Mandatory Pre-PR Static Gates

Run these before opening the PR; document results in RISK-COVERAGE-REPORT.md:

```bash
# Gate 1: SR-10 test presence (search for comment text)
grep -r "fix the weight table, not the test" crates/

# Gate 2: context_retrospective eradication
grep -r "context_retrospective" . \
  --exclude-dir=product/features/col-002 \
  --exclude-dir=product/features/col-002b \
  ... (all excluded dirs per SPECIFICATION.md §SR-05)

# Gate 3: lesson-learned literal removed from search.rs
grep '"lesson-learned"' crates/unimatrix-server/src/services/search.rs

# Gate 4: weight sum invariant
grep 'sum <= 1.0' crates/unimatrix-server/src/infra/config.rs
# Must return zero results.
```

---

## Cross-Component Test Dependencies

| Test | Requires |
|------|---------|
| SR-10 test | ConfidenceParams::default() + confidence_params_from_preset(Collaborative) |
| AC-25 freshness precedence | resolve_confidence_params() + UnimatrixConfig built inline |
| AC-03 boosted_categories grep | search.rs implementation complete |
| AC-13 tool rename grep | All 14 non-Rust files updated |
| Integration tool call | Binary built with `cargo build --release` |
| config-loader unit tests | validate_config() independently testable (no tokio) |
