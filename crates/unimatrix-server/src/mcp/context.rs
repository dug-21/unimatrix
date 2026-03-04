//! MCP tool handler context.
//!
//! ToolContext encapsulates the pre-validation ceremony that every MCP tool
//! handler repeats: identity resolution, format parsing, and AuditContext
//! construction (ADR-002).

use crate::infra::registry::TrustLevel;
use crate::mcp::response::ResponseFormat;
use crate::services::AuditContext;

/// Pre-validated context available to every MCP tool handler.
///
/// Constructed via `UnimatrixServer::build_context()`.
/// Capability checking is a separate `UnimatrixServer::require_cap()` call
/// because different tools require different capabilities.
pub(crate) struct ToolContext {
    /// Resolved agent identity.
    pub agent_id: String,
    /// Agent trust level.
    pub trust_level: TrustLevel,
    /// Parsed response format.
    pub format: ResponseFormat,
    /// Pre-built audit context for service calls.
    pub audit_ctx: AuditContext,
}
