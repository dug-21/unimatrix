//! vnc-037 — the thin **discovery-list** edge vocabulary surfaced on `context_get`.
//!
//! `GetEdge` / `EdgeTotals` / `EdgesView` are a deliberate *projection* of
//! `context_graph`'s `EdgeRecord` — fields are **dropped**, two reader-facing fields are
//! **added**, and **no enrichment field may be introduced** (ADR-002 guardrail, FR-4/FR-15,
//! C-5). The per-edge payload is **EXACTLY** `{edge_type, direction, target_id,
//! target_title, authored}` — never `source_id`, `depth`, `metadata`, the raw `source`
//! string, `weight`, or `target_confidence`.
//!
//! These are plain data structs (no behavior). They are built by get-edge-assembly
//! (`mcp/get_edges.rs`) from the ranked `RawEdgeRow`s + the title map + the
//! `EdgeCountSplit`. The 3-format render helpers (`↔` glyph, `…N more` pointer) live in the
//! serializer-seam component (next wave), also in this file.
//!
//! `GetEdge::edges.len()` is bounded by `GET_EDGE_DISPLAY_LIMIT`; the totals on `EdgeTotals`
//! are **uncapped** (a `↔` symmetric edge counted once) and never reference the cap.

use serde::Serialize;

// The canonical `direction` values (D-02 fix / ADR-007). Consumed by get-edge-assembly
// (`mcp/get_edges.rs`) for projection and by serializer-seam for glyph selection — both
// land in the next wave, hence the `allow(dead_code)` until they wire up.
/// Canonicalized symmetric edge (`Contradicts`/`CoAccess`/`Informs`); renders `↔`.
#[allow(dead_code)]
pub(crate) const DIRECTION_BOTH: &str = "both";
/// Asymmetric edge anchored at `source_id`; renders `→`.
#[allow(dead_code)]
pub(crate) const DIRECTION_OUTBOUND: &str = "outbound";
/// Asymmetric edge anchored at `target_id`; renders `←`.
#[allow(dead_code)]
pub(crate) const DIRECTION_INBOUND: &str = "inbound";

/// A single discovery-list edge — exactly enough for a reader to decide whether to go read
/// a related entry (ADR-002). Serializes to **exactly** these 5 keys; any added field is a
/// boundary violation requiring a new ADR.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GetEdge {
    /// `= RawEdgeRow.relation_type` (renamed for the get vocabulary).
    pub edge_type: String,
    /// `"inbound"` | `"outbound"` | `"both"`. `"both"` (the get-only ADR-007 addition)
    /// renders `↔`; `"inbound"`/`"outbound"` render `←`/`→`. `&'static str` keeps the JSON
    /// serialization trivial and aligned with `EdgeRecord`'s string directions.
    pub direction: &'static str,
    /// The OTHER endpoint — the entry point back into `context_graph`.
    pub target_id: u64,
    /// Human label; `None` (JSON `null`) when the target is unresolved (dangling — retained,
    /// DNB-1). Null is signal.
    pub target_title: Option<String>,
    /// `source == "agent"` (ADR-004 / D-03). The raw `source` string is intentionally NOT
    /// surfaced; it is collapsed to this boolean.
    pub authored: bool,
    // NO source_id, depth, metadata, source string, weight, or target_confidence —
    // enrichment forbidden (ADR-002 guardrail). target_confidence is consumed by the SQL
    // ORDER BY and dropped at projection.
}

impl GetEdge {
    /// Build a `GetEdge` from a projected ranked row.
    ///
    /// `authored` is computed here from the raw `source` string against
    /// [`EDGE_SOURCE_AGENT`](crate::mcp::edge_write::EDGE_SOURCE_AGENT) — an **exact** match,
    /// no case/whitespace fuzz (R-09). This predicate MUST stay identical to the SQL
    /// `(source = 'agent')` rank term (store-ranked-query): a divergence corrupts both the
    /// trust split and authored-first ranking.
    pub fn new(
        edge_type: String,
        direction: &'static str,
        target_id: u64,
        target_title: Option<String>,
        source: &str,
    ) -> Self {
        Self {
            edge_type,
            direction,
            target_id,
            target_title,
            authored: source == crate::mcp::edge_write::EDGE_SOURCE_AGENT,
        }
    }
}

/// Honest, **uncapped** inbound/outbound edge totals — a `↔` symmetric edge counted **once**
/// (post-canonicalization; `= EdgeCountSplit` projected). The nested-object shape matches the
/// `co_access`/`correction_chains`/`security` JSON house style (OQ-01 / ADR-005). The cap
/// (`GET_EDGE_DISPLAY_LIMIT`) never touches these counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct EdgeTotals {
    pub inbound: usize,
    pub outbound: usize,
}

/// The surfaced edge view: the ranked, capped (`≤ GET_EDGE_DISPLAY_LIMIT`) display set plus
/// the honest uncapped totals. Passed by reference (`Option<&EdgesView>`) into
/// `format_single_entry`; `None ⇒ no edges key/section` (ADR-003 structural invariant).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EdgesView {
    /// The ranked display set, `len() ≤ GET_EDGE_DISPLAY_LIMIT`.
    pub edges: Vec<GetEdge>,
    pub totals: EdgeTotals,
}

#[cfg(test)]
mod tests {
    use super::*;
    use unimatrix_store::GET_EDGE_DISPLAY_LIMIT;

    fn agent_edge() -> GetEdge {
        GetEdge::new(
            "Supports".to_string(),
            DIRECTION_OUTBOUND,
            4461,
            Some("Supersedes Exclusion".to_string()),
            "agent",
        )
    }

    // -- Discovery-list shape (ADR-002 guardrail, FR-4/AC-02) --

    /// `GetEdge` serializes to **exactly** the 5 discovery-list keys and nothing more — no
    /// `source_id`, `depth`, `metadata`, raw `source`, `weight`, or `target_confidence`.
    #[test]
    fn test_get_edge_exact_five_fields() {
        let edge = agent_edge();
        let value = serde_json::to_value(&edge).unwrap();
        let obj = value.as_object().expect("GetEdge serializes to an object");

        let mut keys: Vec<&str> = obj.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec![
                "authored",
                "direction",
                "edge_type",
                "target_id",
                "target_title"
            ],
            "GetEdge must carry exactly the 5 discovery-list fields, got: {keys:?}"
        );
        assert_eq!(obj.len(), 5, "no extra (enrichment) fields permitted");
    }

    /// `EdgesView.edges.len() <= GET_EDGE_DISPLAY_LIMIT` — reference the constant, not `3`.
    #[test]
    fn test_edges_view_caps_at_limit() {
        let cap = GET_EDGE_DISPLAY_LIMIT as usize;
        let edges: Vec<GetEdge> = (0..cap).map(|_| agent_edge()).collect();
        let view = EdgesView {
            edges,
            totals: EdgeTotals {
                inbound: 9,
                outbound: 4,
            },
        };
        assert!(
            view.edges.len() <= GET_EDGE_DISPLAY_LIMIT as usize,
            "display set must not exceed the cap"
        );
    }

    // -- R-09: `authored` boolean exactness (High) --

    /// `authored == (source == "agent")`: true **only** for `agent`, false for every live
    /// inferred source (AC-03).
    #[test]
    fn test_authored_true_only_for_agent() {
        let agent = GetEdge::new("Informs".to_string(), DIRECTION_BOTH, 1, None, "agent");
        assert!(agent.authored, "agent source ⇒ authored");

        for source in ["co_access", "cosine", "behavioral", "S8", "nli", "S1", "S2"] {
            let edge = GetEdge::new("Informs".to_string(), DIRECTION_BOTH, 1, None, source);
            assert!(
                !edge.authored,
                "inferred source {source:?} must not be authored"
            );
        }
    }

    /// Near-miss strings stay `authored=false` — exact match, no case/whitespace fuzz. This
    /// predicate MUST match the SQL `(source='agent')` rank term (store-ranked-query).
    #[test]
    fn test_authored_exact_match_no_near_miss() {
        for near_miss in ["Agent", " agent", "agent ", "AGENT", "agentic"] {
            let edge = GetEdge::new(
                "Supports".to_string(),
                DIRECTION_OUTBOUND,
                1,
                None,
                near_miss,
            );
            assert!(
                !edge.authored,
                "near-miss source {near_miss:?} must NOT flip authored true"
            );
        }
    }

    // -- R-10 / FR-15: projection fidelity to EdgeRecord --

    /// `direction` ∈ {`"inbound"`, `"outbound"`, `"both"`}; `"both"` is the get-only
    /// canonical-symmetric value (renders `↔`) and is distinct from the asymmetric values.
    #[test]
    fn test_direction_strings_inbound_outbound_both() {
        assert_eq!(DIRECTION_INBOUND, "inbound");
        assert_eq!(DIRECTION_OUTBOUND, "outbound");
        assert_eq!(DIRECTION_BOTH, "both");
        assert_ne!(DIRECTION_BOTH, DIRECTION_INBOUND);
        assert_ne!(DIRECTION_BOTH, DIRECTION_OUTBOUND);

        let symmetric = GetEdge::new("Contradicts".to_string(), DIRECTION_BOTH, 4502, None, "nli");
        assert_eq!(
            symmetric.direction, "both",
            "canonicalized symmetric edge carries the get-only \"both\" value"
        );
    }

    /// FR-15: the projected `edge_type`/`target_id`/`direction` align with `EdgeRecord`'s
    /// `relation_type`/`target_id`/(`incoming`/`outgoing`) vocabulary. The projection drops
    /// `source_id`/`depth`/`metadata`, adds `target_title`/`authored`, and adds the get-only
    /// `"both"` (`↔`) value which MUST NOT exist in the neighbors (`EdgeRecord`) vocabulary.
    #[test]
    fn test_projection_matches_edgerecord_mapping() {
        // Asymmetric outbound: anchor is source_id; target_id is the other endpoint.
        let outbound = GetEdge::new(
            "Prerequisite".to_string(),
            DIRECTION_OUTBOUND,
            4478,
            Some("EdgeRecord Type Location".to_string()),
            "agent",
        );
        assert_eq!(outbound.edge_type, "Prerequisite"); // = EdgeRecord.relation_type
        assert_eq!(outbound.target_id, 4478); // = the other endpoint
        assert!(matches!(
            outbound.direction,
            "outbound" | "inbound" | "both"
        ));

        // EdgeRecord (neighbors) directions are only "incoming"/"outgoing" — never "both".
        // "both" is the documented get-only addition (ADR-007); assert it is distinct from
        // the EdgeRecord direction vocabulary.
        for edgerecord_direction in ["incoming", "outgoing"] {
            assert_ne!(
                DIRECTION_BOTH, edgerecord_direction,
                "the get-only ↔ value must not collide with EdgeRecord directions"
            );
        }
    }

    // -- EdgeTotals shape (OQ-01 / ADR-005) --

    /// `EdgeTotals { inbound, outbound }` — the nested object carries both fields, uncapped.
    #[test]
    fn test_edge_totals_inbound_outbound_object() {
        let totals = EdgeTotals {
            inbound: 5,
            outbound: 2,
        };
        let value = serde_json::to_value(totals).unwrap();
        let obj = value
            .as_object()
            .expect("EdgeTotals serializes to an object");
        assert_eq!(obj.len(), 2, "exactly inbound + outbound");
        assert_eq!(obj["inbound"], 5);
        assert_eq!(obj["outbound"], 2);
    }

    // -- R-20: target_confidence never surfaced (ADR-002/ADR-006) --

    /// The ranked row's `target_confidence` (the inferred tiebreak input) is **absent** from
    /// `GetEdge` — never a field, never serialized.
    #[test]
    fn test_target_confidence_not_in_get_edge() {
        let edge = agent_edge();
        let value = serde_json::to_value(&edge).unwrap();
        let obj = value.as_object().unwrap();
        assert!(
            !obj.contains_key("target_confidence"),
            "target_confidence is consumed by the SQL ORDER BY and must never be surfaced"
        );
        // Also assert the other dropped/forbidden fields are absent.
        for forbidden in ["source_id", "depth", "metadata", "source", "weight"] {
            assert!(
                !obj.contains_key(forbidden),
                "forbidden enrichment field {forbidden:?} must not be on GetEdge"
            );
        }
    }

    // -- Edge cases --

    /// `target_title: None` serializes as JSON `null`.
    #[test]
    fn test_target_title_none_serializes_null() {
        let edge = GetEdge::new(
            "CoAccess".to_string(),
            DIRECTION_BOTH,
            99,
            None,
            "co_access",
        );
        let value = serde_json::to_value(&edge).unwrap();
        assert!(value["target_title"].is_null());
    }
}
