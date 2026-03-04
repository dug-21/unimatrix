# Pseudocode: response-split

## Purpose
Split the 2541-line `response.rs` into `mcp/response/` sub-module with 5 files. Create generic `format_status_change`.

## Files Created
- `src/mcp/response/mod.rs`
- `src/mcp/response/entries.rs`
- `src/mcp/response/mutations.rs`
- `src/mcp/response/status.rs`
- `src/mcp/response/briefing.rs`

## Files Deleted
- `src/response.rs` (content distributed across 5 new files)

## Files Modified
- `src/lib.rs` (remove `pub mod response;`, add re-export through mcp)

## Pseudocode

### src/mcp/response/mod.rs (~80 lines)

```
// Shared response helpers and re-exports.

use rmcp::model::{CallToolResult, Content};
use unimatrix_store::EntryRecord;

use crate::error::ServerError;

mod entries;
mod mutations;
mod status;
mod briefing;

// Re-export public API
pub use entries::{
    format_single_entry, format_search_results, format_lookup_results,
    format_store_success, format_store_success_with_note,
    format_correct_success, format_duplicate_found,
};
pub use mutations::{format_status_change, format_enroll_success};
// Preserve backward compatibility: thin wrappers
pub use mutations::{format_deprecate_success, format_quarantine_success, format_restore_success};
pub use status::{format_status_report, StatusReport, CoAccessClusterEntry};

#[cfg(feature = "mcp-briefing")]
pub use briefing::{format_briefing, format_retrospective_report, Briefing};

// Shared types
pub enum ResponseFormat { Summary, Markdown, Json }

pub fn parse_format(format: &Option<String>) -> Result<ResponseFormat, ServerError> {
    // Same logic as current response.rs
}

// Shared helpers (pub(super) for sub-modules)
pub(super) fn format_timestamp(ts: u64) -> String { /* existing logic */ }
pub(super) fn status_str(status: Status) -> &'static str { /* existing logic */ }
pub(super) fn tags_str(tags: &[String]) -> String { /* existing logic */ }
pub(super) fn entry_to_json(entry: &EntryRecord) -> serde_json::Value { /* existing logic */ }
```

### src/mcp/response/entries.rs (~700 lines)

Move from response.rs:
- `format_single_entry()`
- `format_search_results()`
- `format_lookup_results()`
- `format_store_success()`
- `format_store_success_with_note()`
- `format_correct_success()`
- `format_duplicate_found()`
- All associated tests

Imports:
```
use super::{ResponseFormat, format_timestamp, status_str, tags_str, entry_to_json};
use rmcp::model::{CallToolResult, Content};
use unimatrix_store::EntryRecord;
```

### src/mcp/response/mutations.rs (~250 lines)

New generic function + thin wrappers + enroll:

```
use super::{ResponseFormat, entry_to_json};
use rmcp::model::{CallToolResult, Content};
use unimatrix_store::EntryRecord;
use crate::infra::registry::{Capability, EnrollResult, TrustLevel};

/// Generic status change formatter (ADR-003).
/// Replaces format_deprecate_success, format_quarantine_success, format_restore_success.
pub(crate) fn format_status_change(
    entry: &EntryRecord,
    action: &str,         // "Deprecated", "Quarantined", "Restored"
    status_key: &str,     // "deprecated", "quarantined", "restored"
    status_display: &str, // "deprecated", "quarantined", "active"
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
pub fn format_deprecate_success(entry: &EntryRecord, reason: Option<&str>, format: ResponseFormat) -> CallToolResult {
    format_status_change(entry, "Deprecated", "deprecated", "deprecated", reason, format)
}

/// Backward-compatible thin wrapper for quarantine.
pub fn format_quarantine_success(entry: &EntryRecord, reason: Option<&str>, format: ResponseFormat) -> CallToolResult {
    format_status_change(entry, "Quarantined", "quarantined", "quarantined", reason, format)
}

/// Backward-compatible thin wrapper for restore.
pub fn format_restore_success(entry: &EntryRecord, reason: Option<&str>, format: ResponseFormat) -> CallToolResult {
    format_status_change(entry, "Restored", "restored", "active", reason, format)
}

/// Format enrollment success.
pub fn format_enroll_success(result: &EnrollResult, format: ResponseFormat) -> CallToolResult {
    // Move existing format_enroll_success from response.rs
}
```

Tests: 18 test cases verifying generic matches original for all 3 variants x 3 formats x 2 reason states.

### src/mcp/response/status.rs (~350 lines)

Move from response.rs:
- `StatusReport` struct (with all fields)
- `CoAccessClusterEntry` struct
- `format_status_report()` function
- All associated tests

### src/mcp/response/briefing.rs (~150 lines)

Move from response.rs (behind `#[cfg(feature = "mcp-briefing")]`):
- `Briefing` struct
- `format_briefing()`
- `format_retrospective_report()`
- All associated tests

## Import Updates

All consumers of `crate::response::*` update to `crate::mcp::response::*`. Temporary re-export `pub use mcp::response;` in lib.rs.

## Compilation Gate

After this step: `cargo check --workspace` must succeed. All formatting tests pass.
