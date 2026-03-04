//! Status change formatting (deprecate, quarantine, restore) and enrollment.
//!
//! Provides `format_status_change` (ADR-003) as the generic formatter,
//! plus backward-compatible thin wrappers for each specific status change.

use rmcp::model::{CallToolResult, Content};
use unimatrix_store::EntryRecord;

use super::{entry_to_json, ResponseFormat};
use crate::infra::registry::{Capability, EnrollResult, TrustLevel};

/// Generic status change formatter (ADR-003).
///
/// Replaces the near-identical `format_deprecate_success`, `format_quarantine_success`,
/// and `format_restore_success` functions with a single parameterized implementation.
pub fn format_status_change(
    entry: &EntryRecord,
    action: &str,
    status_key: &str,
    status_display: &str,
    reason: Option<&str>,
    format: ResponseFormat,
) -> CallToolResult {
    match format {
        ResponseFormat::Summary => {
            let text = format!("{action} #{} | {}", entry.id, entry.title);
            CallToolResult::success(vec![Content::text(text)])
        }
        ResponseFormat::Markdown => {
            let mut text = format!("## Entry {action}\n\n");
            text.push_str(&format!(
                "**Entry:** #{} - {}\n**Status:** {status_display}\n",
                entry.id, entry.title
            ));
            if let Some(r) = reason {
                text.push_str(&format!("**Reason:** {r}\n"));
            }
            CallToolResult::success(vec![Content::text(text)])
        }
        ResponseFormat::Json => {
            let obj = serde_json::json!({
                status_key: true,
                "entry": entry_to_json(entry),
                "reason": reason,
            });
            CallToolResult::success(vec![Content::text(
                serde_json::to_string_pretty(&obj).unwrap_or_default(),
            )])
        }
    }
}

/// Backward-compatible thin wrapper for deprecation.
pub fn format_deprecate_success(
    entry: &EntryRecord,
    reason: Option<&str>,
    format: ResponseFormat,
) -> CallToolResult {
    format_status_change(entry, "Deprecated", "deprecated", "deprecated", reason, format)
}

/// Backward-compatible thin wrapper for quarantine.
pub fn format_quarantine_success(
    entry: &EntryRecord,
    reason: Option<&str>,
    format: ResponseFormat,
) -> CallToolResult {
    format_status_change(entry, "Quarantined", "quarantined", "quarantined", reason, format)
}

/// Backward-compatible thin wrapper for restore.
pub fn format_restore_success(
    entry: &EntryRecord,
    reason: Option<&str>,
    format: ResponseFormat,
) -> CallToolResult {
    format_status_change(entry, "Restored", "restored", "active", reason, format)
}

pub(crate) fn trust_level_str(tl: TrustLevel) -> &'static str {
    match tl {
        TrustLevel::System => "system",
        TrustLevel::Privileged => "privileged",
        TrustLevel::Internal => "internal",
        TrustLevel::Restricted => "restricted",
    }
}

pub(crate) fn capability_str(cap: &Capability) -> &'static str {
    match cap {
        Capability::Read => "read",
        Capability::Write => "write",
        Capability::Search => "search",
        Capability::Admin => "admin",
        Capability::SessionWrite => "session_write",
    }
}

fn capabilities_str(caps: &[Capability]) -> String {
    caps.iter()
        .map(|c| capability_str(c))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Format a successful enrollment result for the given response format.
pub fn format_enroll_success(result: &EnrollResult, format: ResponseFormat) -> CallToolResult {
    let action = if result.created {
        "Enrolled"
    } else {
        "Updated"
    };
    let agent = &result.agent;
    let caps = capabilities_str(&agent.capabilities);
    let trust = trust_level_str(agent.trust_level);

    match format {
        ResponseFormat::Summary => {
            let text = format!(
                "{action} agent '{}' as {trust} with capabilities: {caps}",
                agent.agent_id
            );
            CallToolResult::success(vec![Content::text(text)])
        }
        ResponseFormat::Markdown => {
            let text = format!(
                "---BEGIN UNIMATRIX RESPONSE---\n\
                 ## Agent {action}\n\n\
                 | Field | Value |\n\
                 |-------|-------|\n\
                 | Agent ID | {} |\n\
                 | Action | {action} |\n\
                 | Trust Level | {trust} |\n\
                 | Capabilities | {caps} |\n\
                 ---END UNIMATRIX RESPONSE---",
                agent.agent_id
            );
            CallToolResult::success(vec![Content::text(text)])
        }
        ResponseFormat::Json => {
            let json = serde_json::json!({
                "action": action.to_lowercase(),
                "agent_id": agent.agent_id,
                "trust_level": trust,
                "capabilities": agent.capabilities
                    .iter()
                    .map(|c| capability_str(c))
                    .collect::<Vec<_>>(),
            });
            CallToolResult::success(vec![Content::text(
                serde_json::to_string_pretty(&json).unwrap_or_default(),
            )])
        }
    }
}
