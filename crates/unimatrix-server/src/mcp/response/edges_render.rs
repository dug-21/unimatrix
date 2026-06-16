//! vnc-037 serializer-seam render helpers (ADR-005 / D-08).
//!
//! Three per-format renders of an [`EdgesView`](super::edges::EdgesView), invoked **only** by
//! `format_single_entry` on the get-surface path (`Some(view)`). On `None` they are never
//! reached — the `None ⇒ key/section absent` invariant is structural (ADR-003, C-4), enforced
//! in `format_single_entry` itself, not here.
//!
//! Split off `edges.rs` (the vocabulary types) per the OQ-B pre-authorization to keep each
//! file under the 500-line limit (R-18).

use unimatrix_store::GET_EDGE_DISPLAY_LIMIT;

use super::edges::{DIRECTION_BOTH, DIRECTION_INBOUND, EdgesView};

/// Markdown glyph for a canonicalized symmetric (`"both"`) edge — no directional arrow.
const GLYPH_BOTH: &str = "\u{2194}"; // ↔
/// Markdown glyph for an asymmetric outbound (`"outbound"`) edge.
const GLYPH_OUTBOUND: &str = "\u{2192}"; // →
/// Markdown glyph for an asymmetric inbound (`"inbound"`) edge.
const GLYPH_INBOUND: &str = "\u{2190}"; // ←
/// Placeholder rendered for a dangling target (`target_title == None`) — no panic (R-15).
const UNTITLED_PLACEHOLDER: &str = "(untitled)";

/// Markdown glyph for a `direction` value. Falls back to `→` for any unexpected value so the
/// renderer never panics on bad data (the direction set is closed in practice — ADR-007).
fn direction_glyph(direction: &str) -> &'static str {
    match direction {
        DIRECTION_BOTH => GLYPH_BOTH,
        DIRECTION_INBOUND => GLYPH_INBOUND,
        _ => GLYPH_OUTBOUND,
    }
}

/// Summary/null digest appended to the entry line (ADR-005 — **LOCKED byte form**, 2026-06-16).
///
/// All-zero (every bucket 0) ⇒ `" | edges: none"` (the zero-edge sentinel, FR-12). Otherwise
/// the fixed-arity form `" | edges: {outbound}↑ {inbound}↓ ↔{both} ({K} authored)"` — every
/// count segment present even at 0; counts from the **uncapped** `EdgeTotals`; `{K}` is
/// `EdgesView::authored_total` (authored over the FULL uncapped set, never the displayed ≤cap).
pub(crate) fn render_summary_digest(view: &EdgesView) -> String {
    let t = &view.totals;
    if t.inbound == 0 && t.outbound == 0 && t.both == 0 {
        return " | edges: none".to_string();
    }
    format!(
        " | edges: {}\u{2191} {}\u{2193} \u{2194}{} ({} authored)",
        t.outbound, t.inbound, t.both, view.authored_total
    )
}

/// Markdown `### Related` section appended after the entry footer (ADR-005).
///
/// A flat ranked `≤ GET_EDGE_DISPLAY_LIMIT` list (NO author/inferred sub-split) — each line
/// `- {edge_type} {→|←|↔} #{target_id} "{target_title}"`. A dangling `None` title renders the
/// `(untitled)` placeholder (no panic, R-15). When the uncapped grand total
/// (`inbound + outbound + both`) exceeds the cap, a single `_…and N more — use context_graph_`
/// pointer with `N = total − GET_EDGE_DISPLAY_LIMIT` — both the threshold and the arithmetic
/// reference the constant, never a literal `3` (C-12/FR-18). Zero-edge ⇒ `No related entries.`.
pub(crate) fn render_markdown_related(view: &EdgesView) -> String {
    let mut out = String::from("### Related\n");
    if view.edges.is_empty() {
        out.push_str("No related entries.");
        return out;
    }
    for edge in &view.edges {
        let title = edge.target_title.as_deref().unwrap_or(UNTITLED_PLACEHOLDER);
        out.push_str(&format!(
            "- {} {} #{} \"{}\"\n",
            edge.edge_type,
            direction_glyph(edge.direction),
            edge.target_id,
            title
        ));
    }
    // Uncapped grand total = sum of ALL THREE buckets (↔ already counted once in `both`).
    let total = view.totals.inbound + view.totals.outbound + view.totals.both;
    let cap = GET_EDGE_DISPLAY_LIMIT as usize;
    if total > cap {
        let n = total - cap;
        out.push_str(&format!("_…and {n} more — use context_graph_\n"));
    }
    out
}

/// JSON `edges` array — the ranked `≤cap` set as the exact 5-field discovery-list objects
/// (ADR-002). Zero-edge ⇒ `[]` (paired with `edge_totals {0,0,0}` by the caller — FR-12).
pub(crate) fn render_json_edges(view: &EdgesView) -> serde_json::Value {
    serde_json::to_value(&view.edges).unwrap_or_else(|_| serde_json::Value::Array(Vec::new()))
}

/// JSON `edge_totals` object — the **three** uncapped buckets `{inbound, outbound, both}`
/// (`↔` once in `both`). The digest-only `authored_total` is intentionally NOT a key.
pub(crate) fn render_json_edge_totals(view: &EdgesView) -> serde_json::Value {
    serde_json::json!({
        "inbound": view.totals.inbound,
        "outbound": view.totals.outbound,
        "both": view.totals.both,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::response::edges::{DIRECTION_OUTBOUND, EdgeTotals, EdgesView, GetEdge};

    fn agent_edge() -> GetEdge {
        GetEdge::new(
            "Supports".to_string(),
            DIRECTION_OUTBOUND,
            4461,
            Some("Supersedes Exclusion".to_string()),
            "agent",
        )
    }

    fn view(
        edges: Vec<GetEdge>,
        inbound: usize,
        outbound: usize,
        both: usize,
        authored: usize,
    ) -> EdgesView {
        EdgesView {
            edges,
            totals: EdgeTotals {
                inbound,
                outbound,
                both,
            },
            authored_total: authored,
        }
    }

    /// LOCKED summary digest byte form: `" | edges: {outbound}↑ {inbound}↓ ↔{both} ({K} authored)"`.
    #[test]
    fn test_summary_digest_locked_byte_form() {
        // outbound=5, inbound=2, both=3, authored=2 ⇒ "5↑ 2↓ ↔3 (2 authored)".
        let v = view(vec![agent_edge()], 2, 5, 3, 2);
        assert_eq!(
            render_summary_digest(&v),
            " | edges: 5\u{2191} 2\u{2193} \u{2194}3 (2 authored)"
        );
    }

    /// Fixed arity: every count segment present even when its bucket is 0.
    #[test]
    fn test_summary_digest_fixed_arity_zero_segments() {
        // outbound=0, inbound=0, both=4, authored=1 ⇒ "0↑ 0↓ ↔4 (1 authored)".
        let v = view(vec![agent_edge()], 0, 0, 4, 1);
        assert_eq!(
            render_summary_digest(&v),
            " | edges: 0\u{2191} 0\u{2193} \u{2194}4 (1 authored)"
        );
    }

    /// `{K}` is `authored_total` over the FULL uncapped set — NOT re-derived from the
    /// displayed ≤cap. With one displayed (authored) edge but 7 authored over the full set,
    /// the digest reports 7.
    #[test]
    fn test_summary_digest_authored_from_full_set_not_displayed() {
        // One displayed edge, but authored_total = 7 over the full set (totals sum = 9).
        let v = view(vec![agent_edge()], 0, 0, 9, 7);
        assert_eq!(
            render_summary_digest(&v),
            " | edges: 0\u{2191} 0\u{2193} \u{2194}9 (7 authored)"
        );
    }

    /// All-three-zero sentinel ⇒ exactly `" | edges: none"` (no count terms, no tally).
    #[test]
    fn test_summary_digest_zero_edge_sentinel() {
        let v = view(Vec::new(), 0, 0, 0, 0);
        assert_eq!(render_summary_digest(&v), " | edges: none");
    }

    /// Markdown `### Related`: flat ranked list, NO author/inferred sub-headers.
    #[test]
    fn test_markdown_flat_ranked_no_subsplit() {
        let edges = vec![
            GetEdge::new(
                "Supports".to_string(),
                DIRECTION_OUTBOUND,
                4461,
                Some("Supersedes Exclusion".to_string()),
                "agent",
            ),
            GetEdge::new(
                "Prerequisite".to_string(),
                DIRECTION_INBOUND,
                4478,
                Some("EdgeRecord Type Location".to_string()),
                "agent",
            ),
            GetEdge::new(
                "Contradicts".to_string(),
                DIRECTION_BOTH,
                4502,
                Some("Loop-level exclusion".to_string()),
                "nli",
            ),
        ];
        let v = view(edges, 1, 1, 1, 2); // total == 3 == cap ⇒ no pointer
        let md = render_markdown_related(&v);
        assert!(md.starts_with("### Related\n"));
        assert!(md.contains("- Supports \u{2192} #4461 \"Supersedes Exclusion\"\n"));
        assert!(md.contains("- Prerequisite \u{2190} #4478 \"EdgeRecord Type Location\"\n"));
        assert!(md.contains("- Contradicts \u{2194} #4502 \"Loop-level exclusion\"\n"));
        // The dropped author/inferred sub-split must be ABSENT.
        assert!(
            !md.contains("Author-asserted"),
            "sub-split header must not appear"
        );
        assert!(!md.contains("Inferred"), "sub-split header must not appear");
        assert!(!md.contains("…and"), "no pointer when total <= cap");
    }

    /// `…N more` pointer references the constant; `N = total − cap`. The `both` bucket is
    /// load-bearing for crossing the threshold (a `total` that would be wrong if `both` were
    /// dropped). total = 1 + 1 + 3 = 5, cap = 3 ⇒ N = 2.
    #[test]
    fn test_markdown_capped_pointer_references_constant() {
        let edges: Vec<GetEdge> = (0..GET_EDGE_DISPLAY_LIMIT as usize)
            .map(|_| agent_edge())
            .collect();
        let v = view(edges, 1, 1, 3, 2);
        let md = render_markdown_related(&v);
        let total = 1 + 1 + 3;
        let n = total - GET_EDGE_DISPLAY_LIMIT as usize;
        assert!(md.contains(&format!("_…and {n} more — use context_graph_\n")));
        // Without `both` (=3) the total would be 2 (<= cap) and no pointer would appear —
        // proving `both` is load-bearing.
        assert_eq!(n, 2);
    }

    /// total <= cap ⇒ no pointer.
    #[test]
    fn test_markdown_no_pointer_when_total_at_or_below_cap() {
        let v = view(vec![agent_edge()], 1, 0, 0, 0); // total = 1
        assert!(!render_markdown_related(&v).contains("…and"));
    }

    /// Zero-edge markdown ⇒ `### Related\nNo related entries.`.
    #[test]
    fn test_markdown_zero_edge_empty_state() {
        let v = view(Vec::new(), 0, 0, 0, 0);
        assert_eq!(
            render_markdown_related(&v),
            "### Related\nNo related entries."
        );
    }

    /// JSON `edge_totals` is the 3-bucket object; `authored_total` is NOT a key.
    #[test]
    fn test_json_edge_totals_three_keys() {
        let v = view(vec![agent_edge()], 2, 5, 3, 2);
        let totals = render_json_edge_totals(&v);
        let obj = totals.as_object().unwrap();
        assert_eq!(obj.len(), 3);
        assert_eq!(obj["inbound"], 2);
        assert_eq!(obj["outbound"], 5);
        assert_eq!(obj["both"], 3);
        assert!(
            !obj.contains_key("authored"),
            "authored tally is digest-only, not a JSON key"
        );
        assert!(!obj.contains_key("authored_total"));
    }

    /// JSON `edges` is the array of 5-field objects; zero-edge ⇒ `[]`.
    #[test]
    fn test_json_edges_array_shape() {
        let v = view(vec![agent_edge()], 0, 1, 0, 1);
        let arr = render_json_edges(&v);
        let items = arr.as_array().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].as_object().unwrap().len(), 5);

        let empty = view(Vec::new(), 0, 0, 0, 0);
        assert!(render_json_edges(&empty).as_array().unwrap().is_empty());
    }

    /// Symmetric edge renders `↔` and emits no directional arrow; asymmetric renders its arrow.
    #[test]
    fn test_symmetric_renders_arrow_glyph_no_directional() {
        let sym = view(
            vec![GetEdge::new(
                "CoAccess".to_string(),
                DIRECTION_BOTH,
                7,
                Some("S".to_string()),
                "co_access",
            )],
            0,
            0,
            1,
            0,
        );
        let md = render_markdown_related(&sym);
        assert!(md.contains("\u{2194}"), "symmetric line carries ↔");
        assert!(
            !md.contains("\u{2192}") && !md.contains("\u{2190}"),
            "no →/← on a symmetric line"
        );

        let asym = view(
            vec![GetEdge::new(
                "Supports".to_string(),
                DIRECTION_OUTBOUND,
                7,
                Some("S".to_string()),
                "agent",
            )],
            0,
            1,
            0,
            1,
        );
        assert!(render_markdown_related(&asym).contains("\u{2192}"));
    }

    /// Dangling `target_title: None` renders the placeholder in markdown and `null` in JSON —
    /// no panic across formats (R-15/DNB-1).
    #[test]
    fn test_dangling_title_renders_across_formats() {
        let v = view(
            vec![GetEdge::new(
                "Informs".to_string(),
                DIRECTION_BOTH,
                99,
                None,
                "nli",
            )],
            0,
            0,
            1,
            0,
        );
        let md = render_markdown_related(&v);
        assert!(
            md.contains("\"(untitled)\""),
            "dangling title ⇒ placeholder, no panic"
        );
        let edges_json = render_json_edges(&v);
        assert!(edges_json.as_array().unwrap()[0]["target_title"].is_null());
    }
}
