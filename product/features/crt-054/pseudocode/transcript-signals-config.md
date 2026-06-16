# Component 9 — `[transcript_signals]` config + `validate()`

**File**: `crates/unimatrix-server/src/infra/config.rs` (modify) — `UnimatrixConfig` (`:71`), sibling to `retention` (`:87`). Validation wired into the startup validate path (mirror how `InferenceConfig::validate()` at `:1263` is called from `main.rs`).
**ADRs**: ADR-002 (one shared `RegexSet`, `[transcript_signals]` config, `validate()`-bounded, `MAX_SIGNAL_CLASSES = 16` pinned, v1 = error/refusal only, calibrated in delivery).

## Purpose

Define the `[transcript_signals]` config table, ship the domain-neutral `error`/`refusal` default set, and `validate()`-enforce the bounds (≤ `MAX_SIGNAL_CLASSES` enabled classes, every pattern compiles, no duplicate `class_name`) LOUDLY at load — no silent fallback. The validated, enabled patterns (in config order) feed `SignatureScanner::compile` (Component 2).

## Types

```
// Sibling to RetentionConfig on UnimatrixConfig, #[serde(default)].
#[serde(default)]
struct TranscriptSignalsConfig {
    classes: Vec<TranscriptSignal>,
}

#[serde(default)]   // each field defaults so partial TOML entries are tolerated
struct TranscriptSignal {
    class_name: String,
    pattern:    String,
    enabled:    bool,        // default true (a configured class is on unless disabled)
}
```

Wire into `UnimatrixConfig`:
```
struct UnimatrixConfig {
    ...
    #[serde(default)]
    retention: RetentionConfig,        // existing :87
    #[serde(default)]
    transcript_signals: TranscriptSignalsConfig,   // NEW — sibling
    ...
}
```

## Default set (v1) — ADR-002, FR-C2, AC-10

```
impl Default for TranscriptSignalsConfig
    fn default() -> Self
        // EXACTLY two classes, fixed order → fixed indices: 0 = error, 1 = refusal.
        // Domain-neutral behavioral signatures only — NO SDLC literals, NO reread/compaction class.
        // High-precision, anchored. CALIBRATED against real transcripts during delivery
        // before locking (FR-C2a, AC-10a) — the patterns below are placeholders to be
        // finalized at calibration; keep minimal (under-catalog; domains extend via config).
        return TranscriptSignalsConfig {
            classes: vec![
                TranscriptSignal { class_name: "error",   pattern: <calibrated provider hard/overload error pattern>, enabled: true },
                TranscriptSignal { class_name: "refusal", pattern: <calibrated model refusal phrasing pattern>,        enabled: true },
            ],
        }
```

**Calibration handoff (AC-10a, coordination item 3)**: the two default patterns are NOT finalized here. Delivery calibrates them against a real transcript sample, records the precision/false-positive observations in the delivery artifact, and locks them before merge. The counts are surfaced as DIRECTIONAL, not precise. The pseudocode pins the *shape* (two classes, indices 0/1, anchored bytes-domain regex); delivery pins the *literal patterns*.

## `validate()` — loud at load (FR-C3, NFR-6, AC-11; R-10)

```
impl TranscriptSignalsConfig
    fn validate(&self, path: &Path) -> Result<(), ConfigError>
        // 1. Collect enabled classes in config order.
        let enabled: Vec<&TranscriptSignal> = self.classes.iter().filter(|c| c.enabled).collect()

        // 2. Bound — number of ENABLED classes <= MAX_SIGNAL_CLASSES (== 16, AC-11).
        //    Loud reject; NO silent truncation to 16.
        if enabled.len() > MAX_SIGNAL_CLASSES:
            return Err(ConfigError::TooManySignalClasses {
                found: enabled.len(), max: MAX_SIGNAL_CLASSES, path
            })

        // 3. Duplicate class_name (among enabled) → loud reject (indices must be unambiguous).
        let mut seen = Set::new()
        for c in &enabled:
            if not seen.insert(c.class_name):
                return Err(ConfigError::DuplicateSignalClassName { name: c.class_name, path })

        // 4. Every enabled pattern compiles (bytes-domain RegexSet semantics) → loud reject.
        //    Compile-check here; the actual shared RegexSet is built once at startup (Component 2)
        //    from these same validated patterns.
        for c in &enabled:
            regex::bytes::Regex::new(&c.pattern)
                .map_err(|e| ConfigError::InvalidSignalRegex { name: c.class_name, source: e, path })?

        Ok(())
```

New `ConfigError` variants: `TooManySignalClasses`, `DuplicateSignalClassName`, `InvalidSignalRegex` (mirror the existing `ConfigError::*OutOfRange`/`*Invariant*` style at `:330`+).

## Startup wiring

- Call `config.transcript_signals.validate(path)?` in the same startup validate sequence as `RetentionConfig`/`InferenceConfig::validate()` (`main.rs`). On `Err`, fail startup loudly (no degrade).
- After validate passes, build the shared scanner once: collect `enabled` patterns in config order → `SignatureScanner::compile(&patterns)` (Component 2) → `Arc<SignatureScanner>` carried to the buffer construction sites (Component 3).

## Helper — enabled patterns in order (for the scanner)

```
impl TranscriptSignalsConfig
    fn enabled_patterns(&self) -> Vec<String>
        return self.classes.iter().filter(|c| c.enabled).map(|c| c.pattern.clone()).collect()
        // config order preserved → RegexSet index == class index == class_counts index (FR-C4, AC-10)
```

## Constraints restated

- `MAX_SIGNAL_CLASSES == 16` exactly (NFR-6, AC-11) — referenced from Component 2's module, not redefined.
- No `reread`/`compaction` class; no `token_*` field anywhere (R-12, AC-15).
- `#[serde(default)]` so a config omitting `[transcript_signals]` yields the v1 default set (AC-10), and a config with no enabled classes yields an empty scanner (Component 2 `empty()`) — bytes/deltas still folded.

## Error handling

- All bound/regex/duplicate failures → `ConfigError` returned from `validate()`, fail-loud at startup. No runtime fallback (R-10).

## Key test scenarios (hints)

- Default config (no `[transcript_signals]`) → exactly two classes, `error→0`, `refusal→1`; no SDLC literal; no `reread`/`compaction` class (AC-10).
- `> MAX_SIGNAL_CLASSES` enabled classes → `validate()` Err, clear message, no silent truncation (AC-11, R-10).
- Unparseable regex → `validate()` Err loudly at load, no runtime fallback (AC-11, R-10).
- Duplicate `class_name` among enabled → `validate()` Err.
- `MAX_SIGNAL_CLASSES == 16` constant assertion (AC-11).
- `enabled_patterns()` preserves config order (index stability, FR-C4).
- A class with `enabled: false` is excluded from the count and the scanner.
