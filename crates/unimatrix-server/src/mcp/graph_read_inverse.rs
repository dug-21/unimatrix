//! graph_read_inverse — antijoin SQL handler for inverse mode (vnc-020).
//!
//! Stub created by Wave 1 agent. Wave 2 agent replaces this with the full implementation.
//! Signature is the contract; do NOT change it.

use unimatrix_core::Store;

/// Handle inverse mode: return entries of a given category that have no incoming edges
/// of ALL the specified missing_edge_types (AND semantics, ADR-003 vnc-020).
///
/// STUB: returns INTERNAL_ERROR until Wave 2 implementation lands.
pub(super) async fn handle_inverse(
    _store: &Store,
    _params: &super::GraphParams,
) -> Result<super::InverseResponse, rmcp::model::ErrorData> {
    Err(rmcp::model::ErrorData::new(
        rmcp::model::ErrorCode::INTERNAL_ERROR,
        "inverse mode not yet implemented".to_string(),
        None,
    ))
}
