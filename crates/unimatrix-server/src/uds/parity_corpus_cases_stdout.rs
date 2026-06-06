//! Parity-corpus case table, stdout half (vnc-026, ADR-001 / ADR-002):
//! fixture HookResponses → byte-authoritative `expected-stdout.bin` goldens
//! reconstructed from the verbatim hook.rs:963-1028 expressions.

use super::Case;
use super::cases_b::sas_snippet_stdin;
use serde_json::json;
use unimatrix_engine::wire::{EntryPayload, HookResponse};

/// Manifest arm keys owned by this table.
pub(crate) const ARM_KEYS_STDOUT: &[&str] = &[
    "stdout::subagent_envelope",
    "stdout::envelope_adversarial",
    "stdout::entries_empty",
    "stdout::entries_plain",
    "stdout::truncation",
    "stdout::truncation_remaining_too_small",
    "stdout::briefing_content",
    "stdout::briefing_empty",
    "stdout::subagent_non_entries_fallback",
];

fn entry(id: u64, title: &str, content: &str) -> EntryPayload {
    EntryPayload {
        id,
        title: title.to_string(),
        content: content.to_string(),
        confidence: 0.85,
        similarity: 0.92,
        category: "decision".to_string(),
    }
}

fn entries_response(items: Vec<EntryPayload>) -> HookResponse {
    HookResponse::Entries {
        total_tokens: items.len() as u32 * 50,
        items,
    }
}

fn ups_search_stdin() -> String {
    json!({
        "session_id": "sess-corpus",
        "prompt": "how should the hook client resolve config"
    })
    .to_string()
}

pub(crate) fn cases() -> Vec<Case> {
    let mut v: Vec<Case> = Vec::new();

    v.push(
        Case::new(
            "stdout-subagent-envelope",
            "SubagentStart",
            &["stdout::subagent_envelope"],
            sas_snippet_stdin(),
        )
        .with_response(entries_response(vec![
            entry(
                101,
                "ADR-001 parity corpus",
                "The Rust hook is the oracle for the corpus.",
            ),
            entry(
                102,
                "Hook exit contract",
                "The hook always exits 0 and never blocks.",
            ),
        ])),
    );

    v.push(
        Case::new(
            "stdout-subagent-envelope-adversarial",
            "SubagentStart",
            &["stdout::envelope_adversarial"],
            sas_snippet_stdin(),
        )
        .with_response(entries_response(vec![entry(
            103,
            "Adversarial \"quoted\" title",
            "line1\nhe said \"do it\" with C:\\path 😀 sep\u{2028}para\u{2029}end \u{0007}bell",
        )])),
    );

    v.push(
        Case::new(
            "stdout-subagent-empty-entries",
            "SubagentStart",
            &["stdout::entries_empty"],
            sas_snippet_stdin(),
        )
        .with_response(entries_response(vec![])),
    );

    v.push(
        Case::new(
            "stdout-plain-entries",
            "UserPromptSubmit",
            &["stdout::entries_plain"],
            ups_search_stdin(),
        )
        .with_response(entries_response(vec![
            entry(
                104,
                "Config precedence",
                "Env vars beat settings.local.json (ADR-006).",
            ),
            entry(
                105,
                "State dir layout",
                "~/.unimatrix/{hash}/hook-client per ADR-003.",
            ),
        ])),
    );

    v.push(
        Case::new(
            "stdout-entries-truncation",
            "UserPromptSubmit",
            &["stdout::truncation"],
            ups_search_stdin(),
        )
        .with_response(entries_response(vec![
            entry(106, "t1", &"a".repeat(200)),
            // 2400 bytes of emoji: the truncation cut lands inside the block
            // and must back off to a UTF-8 boundary.
            entry(107, "t2", &"😀".repeat(600)),
        ])),
    );

    v.push(
        Case::new(
            "stdout-entries-remaining-too-small",
            "UserPromptSubmit",
            &["stdout::truncation_remaining_too_small"],
            ups_search_stdin(),
        )
        .with_response(entries_response(vec![
            // First block leaves < 100 bytes of budget: the second entry is
            // dropped entirely (no truncated fragment).
            entry(108, "t1", &"a".repeat(1250)),
            entry(109, "t2", &"b".repeat(500)),
        ])),
    );

    v.push(
        Case::new(
            "stdout-briefing-content",
            "PreCompact",
            &["stdout::briefing_content"],
            json!({ "session_id": "sess-corpus", "cwd": "/work/project" }).to_string(),
        )
        .with_response(HookResponse::BriefingContent {
            content: "Restored conversation context from the server buffer.".to_string(),
            token_count: 12,
        }),
    );

    v.push(
        Case::new(
            "stdout-briefing-empty",
            "PreCompact",
            &["stdout::briefing_empty"],
            json!({ "session_id": "sess-corpus", "cwd": "/work/project" }).to_string(),
        )
        .with_response(HookResponse::BriefingContent {
            content: String::new(),
            token_count: 0,
        }),
    );

    v.push(
        Case::new(
            "stdout-subagent-non-entries-fallback",
            "SubagentStart",
            &["stdout::subagent_non_entries_fallback"],
            sas_snippet_stdin(),
        )
        .with_response(HookResponse::BriefingContent {
            content: "unexpected briefing on the SubagentStart path".to_string(),
            token_count: 8,
        }),
    );

    v
}
