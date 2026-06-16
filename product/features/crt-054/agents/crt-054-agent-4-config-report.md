# Agent Report — crt-054-agent-4-config (Component 9)

**Component**: 9 — `[transcript_signals]` config + `validate()` + `enabled_patterns()`.
**Status**: COMPLETE. Build green, 21/21 component tests pass.

## Files modified
- `crates/unimatrix-server/src/infra/config.rs` (production: types, Default, validate(), enabled_patterns(), ConfigError variants + Display, merge wiring)
- `crates/unimatrix-server/src/infra/transcript_signals_config_tests.rs` (new sibling test file, `#[path]`-registered)
- `product/features/crt-054/testing/CALIBRATION.md` (new — AC-10a calibration record)

## What was implemented
- `TranscriptSignal { class_name, pattern, enabled }` and `TranscriptSignalsConfig { classes }`, both `#[serde(default)]`; wired as `#[serde(default)] transcript_signals` sibling to `retention` on `UnimatrixConfig`. Also added the project-merge arm (REPLACE semantics, ADR-003 — list field).
- `Default` ships EXACTLY two classes, fixed order: index 0 `error`, index 1 `refusal`. Domain-neutral; no SDLC literals, no reread/compaction class, no `token_*` field.
- `validate(&self, path) -> Result<(), ConfigError>`: enabled-in-config-order collection; `> MAX_SIGNAL_CLASSES` (imported from `infra::transcript_activity`) → `TooManySignalClasses` (no silent truncation); duplicate enabled `class_name` → `DuplicateSignalClassName`; enabled pattern failing `regex::bytes::Regex::new` → `InvalidSignalRegex`. All loud at load.
- `enabled_patterns() -> Vec<String>` preserving config order.
- Did NOT wire `validate()` into main.rs/validate_config or build the scanner — those are a later wave per the spawn brief.

## Calibration (AC-10a)
- Patterns chosen (locked in `DEFAULT_ERROR_PATTERN` / `DEFAULT_REFUSAL_PATTERN` consts):
  - `error`: anchored bytes-domain regex over provider error TYPE tokens (`"type":"*_error"`), explicit overload phrasing, and HTTP 429/500/503/529 only in error-context. Does NOT match bare "error".
  - `refusal`: first-person refusal stem — `I ` + (cannot|can't|won't|will not|am unable to|am not able to|I'm not able to). Ignores third-person/affirmative.
- Sample source: searched ass-077/ass-078 FINDINGS, vnc-025 + engine transcript/error fixtures. NO real provider-error/refusal TRANSCRIPT sample exists in-repo (only synthetic envelopes + the JSON-RPC error frame shape). Per the fallback, patterns are anchored-by-construction/conservative, validated against hand-written positive/negative fixtures. CALIBRATION.md states explicitly that counts are DIRECTIONAL not precise (content-opacity → FP rate un-auditable post-ship).
- Artifact: `product/features/crt-054/testing/CALIBRATION.md`.

## Tests (21/21 pass)
Default error/refusal-only @0/@1; serde absent-section default; no-SDLC-literals; no-reread/compaction; index-mapping stable; default-catalog validates; error/refusal positive+low-FP matching; `MAX_SIGNAL_CLASSES==16` (referenced); over-cap rejected (no truncation, negative-mutation guard); exactly-at-cap accepted; disabled excluded from cap; invalid regex rejected; duplicate class_name rejected; duplicate allowed when other disabled; enabled_patterns order + disabled-exclusion; empty catalog valid; partial-stanza defaults enabled:true.

## Build / quality
- `cargo build -p unimatrix-server`: green (25 pre-existing warnings, none from this code).
- `cargo clippy -p unimatrix-server --all-targets`: no warnings on any new symbol.
- `rustfmt --edition 2024` applied to config.rs + the test file ONLY. No other files touched (eval/projects/integration churn files clean).
- Full-crate `cargo test config` passes on stable re-runs (RC=0); a transient RC=101 occurs only on the first run immediately after a lib rebuild — the documented workspace incremental-link flake, not a test failure (identical binaries pass on re-run; isolated lib run is 21/21 green).

## Issues / blockers
None. Left in working tree (no git add/commit) for the Delivery Leader to commit the wave.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- surfaced #4132 (new config fields require validate() checks — applied: added validate() coverage for all three new fields). Other hits were adjacent ADRs, none directly reusable.
- Stored: entry #5055 "Config validate() of a regex catalog must compile in the SAME regex domain as the consumer" via context_store (pattern) -- the bytes-domain validate/scanner lockstep gotcha (validating with regex::Regex while the scanner uses regex::bytes::Regex would let a bad pattern pass the loud gate).
