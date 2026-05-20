//! graph_read_filter — correlated subquery handler for filter mode (vnc-020).
//!
//! Stub created by Wave 1 agent. Wave 2 agent replaces this with the full implementation.
//! Signature is the contract; do NOT change it.

use unimatrix_core::Store;

/// Handle filter mode: return entries matching category + optional property + edge-count
/// constraints via parameterized correlated subquery SQL (ADR-007 vnc-020).
///
/// STUB: returns INTERNAL_ERROR until Wave 2 implementation lands.
pub(super) async fn handle_filter(
    _store: &Store,
    _params: &super::GraphParams,
) -> Result<super::FilterResponse, rmcp::model::ErrorData> {
    Err(rmcp::model::ErrorData::new(
        rmcp::model::ErrorCode::INTERNAL_ERROR,
        "filter mode not yet implemented".to_string(),
        None,
    ))
}
