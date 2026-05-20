//! graph_read_path — BFS shortest-path handler for path mode (vnc-020).
//!
//! Stub created by Wave 1 agent. Wave 2 agent replaces this with the full implementation.
//! Signature is the contract; do NOT change it.

use std::sync::{Arc, RwLock};

use unimatrix_core::Store;

use crate::services::typed_graph::TypedGraphState;

/// Handle path mode: BFS shortest outgoing-edge path from from_id to to_id over the
/// in-memory TypedRelationGraph (ADR-005, ADR-006 vnc-020).
///
/// STUB: returns INTERNAL_ERROR until Wave 2 implementation lands.
pub(super) async fn handle_path(
    _store: &Store,
    _typed_graph_state: &Arc<RwLock<TypedGraphState>>,
    _params: &super::GraphParams,
) -> Result<super::PathResponse, rmcp::model::ErrorData> {
    Err(rmcp::model::ErrorData::new(
        rmcp::model::ErrorCode::INTERNAL_ERROR,
        "path mode not yet implemented".to_string(),
        None,
    ))
}
