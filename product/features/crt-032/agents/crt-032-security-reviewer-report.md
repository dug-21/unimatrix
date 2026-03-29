# Security Review Report: crt-032

> Agent: crt-032-security-reviewer
> Date: 2026-03-29
> PR: #449
> Risk Level: LOW
> Blocking findings: None

## Change Summary (Cold Read)

PR changes 25 lines across two files:
- `crates/unimatrix-server/src/infra/config.rs`: 5 production sites (2 value changes, 3 doc comment updates), 4 test sites (1 helper value, 1 assertion, 2 comments + 1 sum assertion test)
- `crates/unimatrix-server/src/services/search.rs`: 1 field comment update, 1 test sum assertion update

All changes are numeric constant updates (`0.10` → `0.0` or `0.0`, and cascading `0.95` → `0.85`, `1.02` → `0.92`) and corresponding doc/comment updates.

## Security Checks

### Injection (OWASP A03)

**PASS** — No injection risk. Changes are floating-point literals in Rust source. No string interpolation, SQL, or shell commands are introduced or modified.

### Access Control (OWASP A01)

**PASS** — No access control changes. The `w_coac` field remains present in `InferenceConfig` with its serde attribute, validate() range check [0.0, 1.0], and the field itself. Operators who have set `w_coac` explicitly in their config file are unaffected (backward-compatible default only). No privilege paths modified.

### Deserialization (OWASP A08)

**PASS** — `default_w_coac()` now returns `0.0` instead of `0.10`. The value `0.0` is within the valid range [0.0, 1.0] enforced by `validate()`. The serde deserialization path is unchanged in structure; only the default value changes. No deserialization safety regression.

### Input Validation (OWASP A03)

**PASS** — `validate()` at config.rs ~line 920–933 is unchanged. It enforces:
- Per-field range [0.0, 1.0] for all six weights including `w_coac`
- Sum ≤ 1.0 for the six-weight combination

With defaults `0.85 ≤ 1.0`: validation passes. A malicious TOML with `w_coac = 0.0` was already valid. No new rejection bypass or overflow condition possible.

### Blast Radius

**LOW** — Worst case: search result ordering changes for operators using default config when PPR is disabled. Per ADR-001 crt-032, Phase 1 measurement showed the direct co-access term contributes zero signal not already present in PPR's graph edge representation. Impact is equivalent ranking; no functional regression expected. No data corruption, no auth bypass, no privilege escalation, no crash path.

### Regression Risk

**LOW** —
- All 15 `FusionWeights { w_coac: 0.10 }` scoring-math fixtures unchanged
- `test_inference_config_validate_accepts_sum_exactly_one` unchanged (intentional fixture)
- Unit test suite: 2379 passed
- Integration smoke: 20/20 passed
- No regressions observed

### Dependencies

**PASS** — No new dependencies introduced. No cargo.toml changes. `cargo audit` not run (no dependency changes to audit).

### Secrets

**PASS** — No hardcoded credentials, API keys, tokens, or secrets. The values `0.0`, `0.10`, `0.85`, `0.92` are scoring weight parameters, not credentials.

## Verdict

Risk level: **LOW**

No blocking findings. No security regressions. The change is a numeric constant update with clear ADR backing (Unimatrix entry #3785).

## Knowledge Stewardship

- Queried: nothing (fresh cold read; no Unimatrix queries needed for a constant-change review)
- Stored: nothing novel to store — pure constant change review follows standard patterns
