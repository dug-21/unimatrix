//! MCP tool handler context.
//!
//! ToolContext encapsulates the pre-validation ceremony that every MCP tool
//! handler repeats: identity resolution, format parsing, and AuditContext
//! construction (ADR-002).

use crate::infra::registry::TrustLevel;
use crate::mcp::response::ResponseFormat;
use crate::services::{AuditContext, CallerId};

/// Pre-validated context available to every MCP tool handler.
///
/// Constructed via `UnimatrixServer::build_context_with_external_identity()`.
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
    /// Typed caller identity for rate limiting.
    pub caller_id: CallerId,
    /// Transport-attested client name from MCP initialize handshake.
    ///
    /// Populated from `client_type_map` keyed on the rmcp session ID.
    /// `None` when no entry exists (no `initialize` called, or stdio with no
    /// registered client name).
    ///
    /// Used to populate `AuditEvent.agent_attribution` and `AuditEvent.metadata`.
    /// MUST NOT be confused with `agent_id` (which is agent-declared, spoofable).
    pub client_type: Option<String>,
}
