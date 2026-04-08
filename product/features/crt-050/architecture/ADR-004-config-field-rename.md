## ADR-004: Rename query_log_lookback_days to phase_freq_lookback_days with Serde Alias

### Context

`InferenceConfig::query_log_lookback_days` (col-031 ADR-002, Unimatrix #3686)
governs the time window for `PhaseFreqTable::rebuild()`. Once `query_log` is
no longer the signal source, the field name is actively misleading: operators
reading `query_log_lookback_days = 30` in a TOML config file will assume it
controls query log retention, not the observations window.

Three options were identified in the SCOPE:

**Option A (retain old name):** Keep `query_log_lookback_days` as the field
name. The meaning shifts silently. Operators with existing configs are
unaffected but the name is wrong.

**Option B (rename with serde alias):** Rename to `phase_freq_lookback_days`.
Add `#[serde(alias = "query_log_lookback_days")]` for backward compatibility
with existing TOML config files. Rust struct literal sites (tests) must be
updated.

**Option C (new field + deprecate old):** Add
`explicit_read_lookback_days: u32` as a new field with its own default, keep
`query_log_lookback_days` as a dead field. Two config fields with overlapping
semantics creates new confusion.

The SCOPE.md Proposed Approach (Step 4) mandates Option B. The risk assessment
SR-04 flags that struct literal constructions in tests will fail to compile
after the rename — not a serde alias concern, a Rust syntax concern.

### Decision

Implement Option B.

- Rename `InferenceConfig::query_log_lookback_days` → `phase_freq_lookback_days`.
- Annotate with `#[serde(alias = "query_log_lookback_days")]` so all existing
  TOML config files that use the old name continue to deserialize correctly.
- Rename the default function `default_query_log_lookback_days()` →
  `default_phase_freq_lookback_days()` (internal; rename is cosmetic but
  consistent).
- Update all Rust struct literal constructions in test code (SR-04 surface):
  search for `query_log_lookback_days:` in test modules and update to
  `phase_freq_lookback_days:`. The serde alias does not cover Rust struct syntax.
- Update all field-name string references in validation error messages and
  config merge logic.
- Update `background.rs` line 622 (confirmed single site):
  `inference_config.query_log_lookback_days` → `inference_config.phase_freq_lookback_days`.
- Update the crt-036 diagnostic in `status.rs` to reference `phase_freq_lookback_days`
  and update the doc comment to note that the field now governs the `observations`
  window.
- Update existing tests in `config.rs` that assert the field name string
  (e.g., `NliFieldOutOfRange { field: "query_log_lookback_days", ... }`) to use
  `"phase_freq_lookback_days"`.

The serde alias approach is backward-compatible for all config file readers.
The Rust struct literal update is a compile-time change — tests will not
silently pass with the wrong field; the compiler will reject them.

### Consequences

- Existing TOML config files with `query_log_lookback_days = N` continue to
  work transparently.
- New configs written after this change use `phase_freq_lookback_days = N`.
- Operators upgrading who read the config field name now see a semantically
  correct name.
- Test code must be updated — the compiler enforces this; no silent regressions.
- The validation error message field name changes from `"query_log_lookback_days"`
  to `"phase_freq_lookback_days"` — any external tooling parsing that error
  string would need updating (low risk: this is an operational diagnostic, not
  an API contract).
