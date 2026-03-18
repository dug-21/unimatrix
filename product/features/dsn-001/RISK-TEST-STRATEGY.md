# Risk-Based Test Strategy: dsn-001 — Config Externalization (W0-3)

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Rename checklist miss: `context_retrospective` → `context_cycle_review` is partial — non-Rust files (Python integration tests, protocol files, skill files, README) are not checked by the compiler and can be missed | High | High | Critical |
| R-02 | `ConfidenceParams` migration incomplete: `compute_confidence` / `freshness_score` used in 13 files across 2 crates; any call site that still passes positional args or reads `FRESHNESS_HALF_LIFE_HOURS` directly silently uses stale values at runtime | High | High | Critical |
| R-03 | Merge false-negative: ADR-003 merge uses `PartialEq` with `Default` to detect "field was explicitly set"; a per-project config that explicitly sets a field to its default value (e.g., `freshness_half_life_hours = 168.0`) will not override the global value — silent no-op | High | Med | High |
| R-04 | `ContentScanner::global()` warm-up ordering: `OnceLock::get_or_init` initializes on first call; if `validate_config()` is the first call site and the scanner is not yet warmed, the call still works — but tests that bypass `load_config` and call `validate_config` directly may never exercise the warm-up path, leaving the ordering constraint untested | Med | Med | High |
| R-05 | Forward-compat stubs accept unknown keys silently: `ConfidenceConfig {}` and `CycleConfig {}` parse any TOML content into empty structs with no error — a user who typo-writes `[confidence] freshes = 24.0` gets no feedback | Med | Med | High |
| R-06 | File size enforcement bypass: size check reads file metadata before content; a TOML parser that streams could be passed a growing file; the spec requires reading at most 64 KB before passing to parser — implementation must read to buffer, check length, then pass buffer (not pass file handle) | Med | Med | High |
| R-07 | `boosted_categories` config replacement leaves in-flight search stale: boosted set is captured in `SearchService` at construction; a restart is required for changes to take effect — but no error or warning tells the operator their config change has no effect without restart | Med | Low | Med |
| R-08 | `AgentRegistry::new()` signature change without `session_caps` propagation: new `agent_resolve_or_enroll(id, permissive, session_caps)` signature — existing call sites that pass `None` get old behavior, but the `resolve_or_enroll` wrapper on the server side must pass the config-derived `session_caps`; missing the wrapper update means capabilities are never applied from config | High | Med | High |
| R-09 | `dirs::home_dir()` returns `None` in CI / containers: server must degrade gracefully with a warning, not panic; untested in current CI because CI always has `HOME` set | Med | Med | Med |
| R-10 | Permission check platform-gating: `#[cfg(unix)]` wrapping is required; a Windows build that accidentally compiles the permission check will fail to compile; a Unix build that accidentally omits `#[cfg(unix)]` will compile but break on non-Unix | Med | Low | Med |
| R-11 | `validate_config()` `ContentScanner` dependency makes it untestable in isolation: the architecture doc says "the `validate_config` function should be independently testable (no tokio, no store, no scanner singleton)"; if the implementation calls `ContentScanner::global()` directly, unit tests cannot inject a failure case without triggering the real scanner | Med | Med | Med |
| R-12 | `[server].instructions` length check must run before `scan_title()`: if the implementation runs scan before the length check, a 9 KB instructions string will be passed to the regex engine before the cheap guard fires | Low | Med | Low |
| R-13 | `freshness_half_life_hours` boundary at exactly `87600.0` must be accepted (AC-17 specifies `87600.0 → Ok`, `87600.1 → Err`); off-by-epsilon comparisons using `>` vs `>=` produce an incorrect boundary | Low | Med | Low |

---

## Risk-to-Scenario Mapping

### R-01: Rename checklist miss (context_retrospective)

**Severity**: High
**Likelihood**: High
**Impact**: Protocol and skill callers fail silently at runtime. Integration test suite still references the old tool name and passes on the pre-rename binary. A client calling `context_retrospective` after the rename receives an MCP "tool not found" error — a silent protocol breakage that does not fail the Rust build.

**Test Scenarios**:
1. Run `grep -r "context_retrospective" . --include="*.rs" --include="*.py" --include="*.md" --include="*.toml"` post-merge; assert zero matches (excluding archival feature docs).
2. Integration test `test_protocol.py` asserts `context_cycle_review` is in the server's tool list and `context_retrospective` is absent.
3. `harness/client.py` `context_cycle_review()` method calls succeed against a live server; the old `context_retrospective` call path is removed.
4. `session_metrics.rs` test `classify_tool("context_cycle_review")` uses the new name.

**Coverage Requirement**: Zero-match grep is mandatory before PR merge. Integration test in `test_protocol.py` must assert both presence of new name and absence of old name in the tool list. All 11 `server.context_retrospective(...)` call sites in `test_tools.py` must be updated.

---

### R-02: ConfidenceParams migration incomplete

**Severity**: High
**Likelihood**: High
**Impact**: `freshness_score()` uses `FRESHNESS_HALF_LIFE_HOURS` (168.0) at runtime regardless of config. All confidence scoring silently ignores the operator's configured half-life. No compile error — the constant is still exported; callers that were not updated compile fine against the old signature.

**Test Scenarios**:
1. Unit test: call `freshness_score()` via a `ConfidenceParams` with `freshness_half_life_hours = 336.0`; assert a 336-hour-old entry scores 0.5 (AC-04).
2. Unit test: call `compute_confidence()` with a custom `ConfidenceParams`; assert the returned value differs from the `Default` params result for the same entry.
3. Grep for `FRESHNESS_HALF_LIFE_HOURS` appearing in a non-comment, non-`Default` context across all crates — assert zero matches.
4. All 13 identified call sites (`pipeline_regression.rs`, `pipeline_calibration.rs`, `test_scenarios_unit.rs`, `test_scenarios.rs`, `coherence.rs`, `response/mod.rs`, `response/status.rs`, `tools.rs`, `server.rs`, `services/confidence.rs`, `services/usage.rs`, `services/status.rs`, and `confidence.rs` itself) compile using `&ConfidenceParams`.

**Coverage Requirement**: Every file that previously called `compute_confidence` with positional `alpha0, beta0` args must be updated. Tests that exercise non-default half-life values must use `ConfidenceParams { freshness_half_life_hours: X, ..Default::default() }` struct-update syntax.

---

### R-03: Merge false-negative for explicitly-set default values

**Severity**: High
**Likelihood**: Med
**Impact**: An operator who deliberately sets `freshness_half_life_hours = 168.0` in a per-project config to document their intent (matching global default) will find the global config's value is used rather than the per-project value. While the effective value is the same, the merge semantics are incorrect and will confuse debugging. Worse: if the global config later changes, the per-project "pinning to 168.0" is silently ignored.

**Test Scenarios**:
1. Unit test: global config `freshness_half_life_hours = 500.0`; per-project config `freshness_half_life_hours = 168.0` (the compiled default); assert merged config reads `168.0` (per-project explicit intent wins, not the global).
2. Unit test: global `categories = ["a", "b"]`; per-project `categories = ["a", "b"]` (same as global); assert merged `categories = ["a", "b"]` and no fallthrough to `Default`.
3. Unit test (the happy path from AC-07): global `freshness_half_life_hours = 500.0`, per-project `freshness_half_life_hours = 24.0`; merged reads `24.0`.

**Coverage Requirement**: The merge function must be tested with the boundary case where the per-project value is identical to the compiled default. The `PartialEq`-with-`Default` detection strategy must document this edge case explicitly in code comments. The alternative — using `Option<T>` intermediate deserialization — avoids this problem entirely and is specified in SPECIFICATION.md §Load order and merge algorithm.

---

### R-04: ContentScanner warm-up not covered by validate_config unit tests

**Severity**: Med
**Likelihood**: Med
**Impact**: If `validate_config()` unit tests construct `UnimatrixConfig` directly and call `validate_config()` without first calling `ContentScanner::global()`, the tests never exercise the warm-up-then-scan path. The ordering invariant documented in ARCHITECTURE.md §ContentScanner ordering is asserted in comments but never tested.

**Test Scenarios**:
1. Unit test: call `validate_config()` with injection-pattern instructions on a cold process (first `global()` call); assert `Err` is returned — confirming scanner initializes on first call even in validate path.
2. Integration test: server startup with an injection-pattern `instructions` value aborts before any tool is registered.
3. The `let _scanner = ContentScanner::global();` warm-up call at the top of `load_config()` must be verified present in code review (not just in tests).

**Coverage Requirement**: At least one unit test must call `validate_config()` as the first scanner interaction in its test process (achievable by isolation or explicit scan-init call before test). Architecture doc constraint must be backed by a code comment and code review checklist item.

---

### R-05: Forward-compat stubs silently accept unknown/misspelled fields

**Severity**: Med
**Likelihood**: Med
**Impact**: A user who writes `[confidence]\nfreshness = 24.0` (misspelling `freshness_half_life_hours` under the wrong section) gets no error and no effect. This creates a debugging trap where config changes appear to have no effect.

**Test Scenarios**:
1. Unit test: parse a TOML with `[confidence]\nunknown_field = 42` — assert `load_config()` returns `Ok` (no error on unknown field, by design) and the `ConfidenceConfig` is the empty default.
2. Documentation test: the code comment on `ConfidenceConfig` must explain which fields are planned for W3-1 so operators know they're writing a stub. Code review checks this.

**Coverage Requirement**: Verify serde does not use `deny_unknown_fields` on stub structs (which would break W3-1 field additions). One test asserting silent acceptance of an unrecognized `[confidence]` key is sufficient. No additional runtime behavior is required — this is an operator-experience risk.

---

### R-06: File size enforcement — buffer-read vs. file-handle pass

**Severity**: Med
**Likelihood**: Med
**Impact**: If the implementation passes a `File` handle directly to `toml::from_reader()` without a size check on the pre-read buffer, the size cap is bypassed. A crafted 100 MB config file would be parsed in full, potentially causing memory exhaustion. The spec requires reading at most 64 KB before any TOML parsing.

**Test Scenarios**:
1. Unit test (AC-15): write a temp file of 65537 bytes; assert `load_config()` returns `Err` containing the file path before any TOML parse (verify by making the oversized file invalid TOML — if TOML parse errors appear, the size check did not fire first).
2. Unit test: file of exactly 65536 bytes of valid TOML (even if only whitespace); assert `load_config()` proceeds to TOML parse step.
3. Code review: confirm implementation reads to `Vec<u8>`, checks `len() > 65536`, then calls `toml::from_str(std::str::from_utf8(&buf)?)` — not `toml::from_reader(file)`.

**Coverage Requirement**: Both the "exactly at cap" (Ok) and "one byte over" (Err) cases must be unit-tested. The error message must identify the file path and observed size.

---

### R-07: No restart-required signal for boosted_categories config change

**Severity**: Med
**Likelihood**: Low
**Impact**: Operators who change `boosted_categories` and expect immediate effect (without restart) receive no feedback that the change is deferred until restart. The config is load-once; the `SearchService` `HashSet` is immutable post-construction. This is a correct design but an operability gap.

**Test Scenarios**:
1. Unit test: construct `SearchService` with `boosted_categories = ["custom"]`; assert that a search result from category `"custom"` receives `PROVENANCE_BOOST` and one from `"lesson-learned"` does not (AC-03).
2. Unit test: construct `SearchService` with `boosted_categories = []`; assert no category receives the boost.

**Coverage Requirement**: Confirm no literal `"lesson-learned"` string comparison remains in `search.rs` (grep assertion). Two constructor scenarios (custom set, empty set) tested for boost application.

---

### R-08: AgentRegistry session_caps not propagated through resolve_or_enroll wrapper

**Severity**: High
**Likelihood**: Med
**Impact**: `AgentRegistry::resolve_or_enroll()` in `infra/registry.rs` currently calls `store.agent_resolve_or_enroll(agent_id, PERMISSIVE_AUTO_ENROLL)`. After the refactor, this wrapper must pass `session_caps` from config. If the wrapper is updated to pass the config `permissive` bool but not the `session_caps`, the new third parameter is never used — auto-enrolled agents always get the default capability set regardless of config. No compile error (passing `None` is valid per the architecture).

**Test Scenarios**:
1. Unit test (AC-06 strict): configure `default_trust = "strict"`; call `agent_resolve_or_enroll` for an unknown agent; assert capabilities are `[Read, Search]` and not `[Write]`.
2. Unit test (AC-06 custom caps): configure `session_capabilities = ["Read"]`; enroll an unknown agent; assert capabilities are exactly `[Read]`.
3. Unit test: configure `session_capabilities = ["Read", "Write", "Search"]` (permissive default); enroll; assert all three caps present.
4. Ensure the test exercises the full call path through `AgentRegistry::resolve_or_enroll()` → `store.agent_resolve_or_enroll()`, not just the store method directly.

**Coverage Requirement**: End-to-end enrollment path (server infra → store) must be covered for both strict and custom caps cases. Not just store-layer unit tests.

---

### R-09: dirs::home_dir() None in containers

**Severity**: Med
**Likelihood**: Med
**Impact**: If `dirs::home_dir()` returns `None` and the implementation panics or returns `Err`, CI containers without a set `HOME` will fail server startup. The architecture and spec both require graceful degradation to defaults with a `warn!`.

**Test Scenarios**:
1. Unit test: call `load_config` with a `home_dir: Option<&Path>` of `None`; assert `Ok(UnimatrixConfig::default())` is returned.
2. Verify a `tracing::warn!` is emitted (via `tracing-test` or equivalent) when `home_dir` is `None`.
3. No `panic!` or `process::exit` on the `None` path — code review confirms.

**Coverage Requirement**: The `None` home_dir path must have a unit test. The function signature for `load_config` should accept `Option<&Path>` for home_dir to make this testable without mocking `dirs::home_dir()`.

---

## Integration Risks

**Config load to CategoryAllowlist wiring**: `CategoryAllowlist::from_categories()` is called at startup with a `Vec<String>` from config. If the config validation passes categories that contain characters accepted by the TOML parser but rejected by `from_categories()` (e.g., empty string `""`), the allowlist may silently drop the entry or panic. Validation of the regex `^[a-z0-9_-]+$` must prevent empty strings explicitly (empty string does not match `+` quantifier — but verify this in a test).

**ConfidenceParams construction site in background tick**: The server constructs `ConfidenceParams` "per-call from the Arc-loaded config" per ADR-001. The background tick path (`services/confidence.rs`, `services/usage.rs`) currently calls `compute_confidence` with positional args. After migration, `ConfidenceParams` must be built from `Arc<UnimatrixConfig>` at each tick invocation. If the tick service holds a stale copy of `alpha0, beta0` without routing through `ConfidenceParams`, the half-life config is applied but `alpha0/beta0` are not.

**`agent_resolve_or_enroll` third-parameter None-defaulting at call sites**: The architecture specifies existing call sites pass `None` to preserve current behavior. Any call site that is not updated and passes two args (not three) will fail to compile — this is the safe failure mode. But call sites in test code that construct `AgentRegistry` directly may test the old two-arg form and fail to exercise the new capability-override path.

---

## Edge Cases

**Freshness bounds — exact boundary values**:
- `freshness_half_life_hours = 87600.0` must be accepted (≤ constraint is inclusive).
- `freshness_half_life_hours = 87600.1` must be rejected.
- `freshness_half_life_hours = -0.0` (IEEE negative zero): `> 0.0` is false for `-0.0` — must be rejected.
- `freshness_half_life_hours = f64::MIN_POSITIVE` (smallest positive f64): must be accepted as `> 0.0` even though it produces extreme decay.

**Category allowlist edge cases**:
- Empty list `categories = []`: validation must either reject (no categories accepted ever) or accept as a degenerate but valid config. The spec does not explicitly address an empty categories list — the constraint is `≤ 64` but no minimum. Spec should clarify; test both outcomes.
- Single-char category `"a"`: must pass `[a-z0-9_-]` with `len ≤ 64` (valid).
- Category with exactly 64 characters: must pass. 65 characters: must fail.
- Duplicate categories in `categories` list: spec does not address deduplication. If a category appears twice, the `HashSet` in `CategoryAllowlist` deduplicates silently — but the 64-count limit should apply to the unique set, not the raw list.

**Per-project config with only one section**: Per-project config that specifies `[knowledge]` but omits `[server]` and `[agents]`; merged config must use global values for `server` and `agents`.

**Both config files absent**: `load_config()` must return `UnimatrixConfig::default()` with no I/O errors.

**Config file is a symlink to a world-writable target**: `std::fs::metadata()` follows symlinks — the target's permissions are checked, not the symlink's. Spec confirms this is correct behavior. Test: create a symlink to a world-writable file; assert startup aborts.

**TOML with `[confidence]` section containing unknown fields**: must parse to `ConfidenceConfig {}` without error (serde default, no `deny_unknown_fields`).

---

## Security Risks

**`[server].instructions` — direct system-prompt injection surface**: Instructions are passed verbatim to every connecting AI agent during the MCP initialize handshake. `ContentScanner::scan_title()` runs 26 injection regexes. Attack surface: an operator whose config is writable by another user (not world-writable but group-writable) can have their instructions silently replaced with injected commands. File permission enforcement (world-writable abort, group-writable warn) is the primary defence; `ContentScanner` is a secondary layer that catches known patterns only.

- What untrusted input: any operator-controlled string up to 8 KB, validated but not sanitized.
- Damage from malformed input: the server would relay injection payloads to every client agent during handshake. A crafted instructions string that passes `scan_title()` (bypassing the 26 patterns via obfuscation) could prime connected agents with adversarial context.
- Blast radius: all agents connecting to this server instance receive the injected instructions.
- Test: one known injection-pattern string (from `ContentScanner` test corpus) → `Err`; one benign multi-line instructions string → `Ok`.

**`[knowledge].categories` — knowledge base schema gate**: A misconfigured or adversarially-crafted category list controls what enters the knowledge base post-restart. Categories are validated for character set and length but not for semantic validity (any `[a-z0-9_-]` string is accepted). An attacker with config write access could clear the category list to `[]`, effectively preventing any new knowledge from being stored.

- What untrusted input: operator-supplied strings, validated against `^[a-z0-9_-]+$`.
- Damage: empty or minimal category list blocks all `context_store` calls that use any category not in the list. Existing data is not affected; only new stores fail.
- Blast radius: limited to entries stored after restart; no DB corruption.
- Test: `categories = []` — test whether store calls that specify any category fail with category-not-allowed error.

**`[agents].session_capabilities` — privilege escalation via `Admin` exclusion**: The spec explicitly excludes `Admin` from valid session_capabilities values. An operator who somehow sets `session_capabilities = ["Admin"]` must get a startup error, not a server that auto-enrolls all agents as Admin.

- What untrusted input: capability strings, validated against a closed allowlist.
- Damage: if `Admin` were accepted, every connecting agent would gain Admin-level access, bypassing the `alc-002` protected-agent logic.
- Blast radius: full server compromise for all agents.
- Test (AC-19): `session_capabilities = ["Read", "Admin"]` → startup abort with error.

**File permission enforcement race (TOCTOU)**: The permission check reads `metadata()` at startup; the file could be made world-writable between the permission check and the read. This is an inherent TOCTOU limitation of filesystem-based checks.

- Mitigation: read the file immediately after the permission check (no sleep between check and read). The implementation should check permissions, then immediately read to buffer — not check permissions, do other work, then read.
- Test: verify the implementation opens and reads the file in the same `try_load_file()` call, not in separate steps.

---

## Failure Modes

**Config file malformed TOML**: `load_config()` returns `Err(ConfigError::Parse { path, detail })`; server logs the path and parse error; exits with non-zero code. Expected behavior per FR-021. No partial config state must remain.

**Config file world-writable**: `load_config()` returns `Err(ConfigError::Permission { path })`; startup aborts. No config is applied, not even defaults — the server must not start with a compromised config path present.

**`dirs::home_dir()` returns `None`**: Server starts with `UnimatrixConfig::default()`; emits `tracing::warn!` identifying the cause. No abort. Per FR-020.

**Validation failure (any field)**: Server exits with a descriptive error identifying the violating field, the invalid value, and the constraint. Error message must be actionable — not "validation error" but "knowledge.categories[3]: value 'My Category' contains invalid characters; allowed: [a-z0-9_-]".

**Per-project config absent, global config valid**: Merge uses global config values for all fields. No error. Per FR-002.

**Both configs absent**: `UnimatrixConfig::default()` — identical to pre-dsn-001 behavior for all subsystems. All 2169+ existing tests must pass unchanged. Per AC-01 and NFR-003.

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (toml crate dependency) | — | ADR: `toml = "0.8"` pinned to `unimatrix-server` only. Run `cargo tree` post-add. No active architecture risk; verify in implementation. |
| SR-02 (ConfidenceParams API change, 15+ call sites) | R-02 | ADR-001: `ConfidenceParams` struct introduced. All 13 identified call-site files require mechanical migration. Risk remains high due to volume and cross-crate scope. |
| SR-03 (ContentScanner initialization ordering) | R-04 | ADR: explicit `let _scanner = ContentScanner::global();` at top of `load_config`. `OnceLock::get_or_init` is safe on first call — but warm-up path is not covered by `validate_config` unit tests. |
| SR-04 (forward-compat stubs) | R-05 | ADR-004: empty `ConfidenceConfig` and `CycleConfig` stubs. Risk shifts from format-break to silent-accept of misspelled fields. |
| SR-05 (context_retrospective rename blast radius) | R-01 | Architecture confirms 22+ non-Rust locations. SPECIFICATION.md §SR-05 provides exhaustive checklist. Zero-match grep is the gate. |
| SR-06 (two-level merge semantics) | R-03 | ADR-003: replace semantics. Risk is the merge false-negative for explicitly-set default values — `Option<T>` intermediate approach (specified in SPECIFICATION.md) avoids it if implemented correctly. |
| SR-07 (CategoryAllowlist constructor split) | — | ADR-002: `new()` delegates to `from_categories(INITIAL_CATEGORIES)`. Resolved at architecture level. Verify in code review. |
| SR-08 (crate boundary / Arc<UnimatrixConfig>) | R-08 | ADR-002: plain parameter crossing only. Risk remains that the `resolve_or_enroll` wrapper is updated for `permissive` bool but `session_caps` is left as `None`. |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 2 (R-01, R-02) | 8 scenarios (rename zero-match grep + tool list test + 4 migration verifications; half-life parameterization unit tests + call-site audit) |
| High | 4 (R-03, R-05, R-06, R-08) | 11 scenarios (merge edge cases; stub acceptance; size cap boundary; enrollment end-to-end) |
| Med | 5 (R-04, R-07, R-09, R-10, R-11) | 7 scenarios (scanner warm-up; boost set replacement; home_dir None; platform-gating code review; testability isolation) |
| Low | 2 (R-12, R-13) | 3 scenarios (length-before-scan ordering; boundary value at exactly 87600.0; off-by-epsilon) |

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "lesson-learned failures gate rejection" — returned 5 entries; entry #1910 (Tokio+fork ordering recurring risk in daemon features) is tangentially relevant to SR-03. Entry #364 (Retain-and-Rename pattern) confirms the rename risk category is known.
- Queried: `/uni-knowledge-search` for "risk pattern" (category: pattern) — no directly applicable patterns returned for config externalization or validation ordering.
- Queried: `/uni-knowledge-search` for "API signature change cross-crate test migration" — entry #364 (Retain-and-Rename), #645 (Cross-Crate Type Deduplication), #747 (Cross-Crate Test Infrastructure) all point to the same recurring risk: cross-crate API migrations require explicit call-site audits, not just build verification.
- Queried: `/uni-knowledge-search` for "security validation config injection prompt" — entry #146 (OWASP awareness) confirms the injection validation pattern is a tracked convention.
- Stored: nothing novel to store — the rename blast-radius risk and cross-crate migration risk are already represented in entries #364 and #747. The merge false-negative risk (R-03) is dsn-001-specific and does not generalize until a second config-merging feature is implemented.
