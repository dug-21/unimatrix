//! OpenCode event-name canonicalization + carrier validation (vnc-049 C2).
//!
//! ADR-004 (provider arm) / ADR-005 (new-module-with-thin-wiring): the substantive
//! OpenCode normalization logic lives here so the over-cap `hook.rs` gains only a const
//! entry plus a single delegating call. This module is the Rust anchor that the JS mirror
//! (`normalize.js`, C3) and the parity corpus (`parity_corpus_uds.rs`, C8) assert against
//! (col-022 split-brain, #5737) and the home for `is_valid_model_id` re-export + any future
//! OpenCode-specific event aliases.
//!
//! The C1 plugin shim already emits the seven *canonical* event names (SessionStart,
//! UserPromptSubmit, PreToolUse, PostToolUse, SubagentStart, PreCompact, Stop) and always
//! passes `--provider opencode` (the hint path), so [`canonicalize`] is currently identity
//! for those names. The module exists as the single-source contract (#5302) and to keep
//! provider inference honest for any future OpenCode-unique alias.

/// The seven canonical Unimatrix event names OpenCode maps onto (via the C1 shim).
///
/// OpenCode source → canonical (see ARCHITECTURE.md §OpenCode → canonical event map):
/// - `session.created` (+`session.updated`)            → `SessionStart` (bus-derived, degraded)
/// - `chat.message`                                     → `UserPromptSubmit`
/// - `tool.execute.before`                              → `PreToolUse`
/// - `tool.execute.after`                               → `PostToolUse`
/// - child `session.created`+`parentID`+`session.agent` → `SubagentStart` (observe-only)
/// - `experimental.session.compacting`                  → `PreCompact` (experimental, ADR-009)
/// - `session.idle`                                     → `Stop` (bus-derived, over-count guard)
pub const OPENCODE_CANONICAL_EVENTS: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "SubagentStart",
    "PreCompact",
    "Stop",
];

/// The provider string stamped for OpenCode-origin events.
pub const OPENCODE_PROVIDER: &str = "opencode";

/// Sentinel returned for an unrecognized event name.
///
/// Mirrors the `hook.rs` convention: the caller detects `"__unknown__"` and substitutes the
/// raw event string so the unrecognized name is preserved in the DB `hook` column (NFR-01).
pub const UNKNOWN_EVENT: &str = "__unknown__";

/// Map an OpenCode event name to its canonical Unimatrix name.
///
/// The C1 shim emits already-canonical names, so this is identity for the seven canonical
/// names plus any OpenCode-unique alias (none today). Returns [`UNKNOWN_EVENT`] for an
/// unrecognized name (caller preserves the raw string).
pub fn canonicalize(event: &str) -> &'static str {
    match event {
        "SessionStart" => "SessionStart",
        "UserPromptSubmit" => "UserPromptSubmit",
        "PreToolUse" => "PreToolUse",
        "PostToolUse" => "PostToolUse",
        "SubagentStart" => "SubagentStart",
        "PreCompact" => "PreCompact",
        "Stop" => "Stop",
        // No OpenCode-unique aliases known today; this table is the parity/symmetry anchor
        // and the seam for future OpenCode-specific remaps.
        _ => UNKNOWN_EVENT,
    }
}

/// Canonicalize an OpenCode event and stamp the OpenCode provider in one step.
///
/// Returns `(canonical_name, provider)` matching the `normalize_event_name` return contract
/// (#5302: single-source the whole contract, not just the name). `provider` is always
/// [`OPENCODE_PROVIDER`]; an unrecognized event still stamps `opencode` (the caller knows the
/// origin) while the name falls back to [`UNKNOWN_EVENT`].
pub fn normalize(event: &str) -> (&'static str, &'static str) {
    (canonicalize(event), OPENCODE_PROVIDER)
}

/// True if `event` is an OpenCode-specific alias that must be remapped to a different
/// canonical name on the *inference* path (`--provider` absent).
///
/// Currently always `false`: OpenCode emits canonical names and always supplies
/// `--provider opencode` (the hint path). It MUST stay `false` for the seven shared
/// canonical names, otherwise the delegating guard in `normalize_event_name` would hijack
/// claude-code's inference for those names. It exists as the honest, safe seam for a future
/// OpenCode-unique alias.
pub fn is_opencode_event_alias(_event: &str) -> bool {
    false
}

/// `model_id` carrier validation (`^[a-z0-9._/-]{1,128}$`).
///
/// Re-exported from the C4 wire contract so this module remains the single Rust anchor the
/// JS mirror (C3) and parity corpus (C8) reference. The value shape is
/// `"<providerID>/<modelID>"` (e.g. `"ollama/qwen3-coder"`), which contains `/` and so is
/// intentionally broader than the `^[a-z0-9_-]{1,64}$` `source_domain` contract.
pub use unimatrix_engine::wire::is_valid_model_id;

#[cfg(test)]
mod tests {
    use super::*;

    // -- Event-name canonicalization (FR-01, AC-02): assert the full (name, provider)
    //    contract for each of the seven canonical events, not just the name (#5302). --

    #[test]
    fn test_opencode_canonicalize_sessionstart() {
        assert_eq!(normalize("SessionStart"), ("SessionStart", "opencode"));
    }

    #[test]
    fn test_opencode_canonicalize_userpromptsubmit() {
        assert_eq!(
            normalize("UserPromptSubmit"),
            ("UserPromptSubmit", "opencode")
        );
    }

    #[test]
    fn test_opencode_canonicalize_pretooluse() {
        assert_eq!(normalize("PreToolUse"), ("PreToolUse", "opencode"));
    }

    #[test]
    fn test_opencode_canonicalize_posttooluse() {
        assert_eq!(normalize("PostToolUse"), ("PostToolUse", "opencode"));
    }

    #[test]
    fn test_opencode_canonicalize_subagentstart() {
        assert_eq!(normalize("SubagentStart"), ("SubagentStart", "opencode"));
    }

    #[test]
    fn test_opencode_canonicalize_precompact() {
        assert_eq!(normalize("PreCompact"), ("PreCompact", "opencode"));
    }

    #[test]
    fn test_opencode_canonicalize_stop() {
        assert_eq!(normalize("Stop"), ("Stop", "opencode"));
    }

    /// All seven canonical events round-trip through `canonicalize` unchanged.
    #[test]
    fn test_opencode_canonicalize_all_seven_identity() {
        for event in OPENCODE_CANONICAL_EVENTS {
            assert_eq!(
                canonicalize(event),
                *event,
                "canonicalize({event}) must be identity for canonical names"
            );
        }
    }

    /// Unrecognized event → sentinel (caller substitutes the raw string), not silently coerced.
    #[test]
    fn test_opencode_canonicalize_unknown_returns_sentinel() {
        assert_eq!(canonicalize("session.created"), UNKNOWN_EVENT);
        assert_eq!(canonicalize("CompletelyUnknownEvent"), UNKNOWN_EVENT);
        assert_eq!(normalize("nope"), (UNKNOWN_EVENT, "opencode"));
    }

    /// The alias guard MUST NOT claim any shared canonical name, or the inference-path
    /// delegating guard in `hook.rs` would hijack claude-code inference for those names.
    #[test]
    fn test_opencode_alias_does_not_hijack_canonical_names() {
        for event in OPENCODE_CANONICAL_EVENTS {
            assert!(
                !is_opencode_event_alias(event),
                "{event} is a shared canonical name; is_opencode_event_alias must be false"
            );
        }
        assert!(!is_opencode_event_alias("BeforeTool"));
        assert!(!is_opencode_event_alias("session.idle"));
    }

    /// `is_valid_model_id` is reachable through the module anchor (C4 re-export).
    #[test]
    fn test_opencode_model_id_validation_reexport() {
        assert!(is_valid_model_id("ollama/qwen3-coder"));
        assert!(!is_valid_model_id("model with space"));
    }
}
