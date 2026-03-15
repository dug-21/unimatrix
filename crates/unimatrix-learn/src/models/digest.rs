//! SignalDigest: fixed-width 32-slot feature vector (ADR-003).

/// Fixed-width 32-slot feature vector for neural extraction models.
///
/// Slots 0-6 are populated from ProposedEntry fields.
/// Slots 7-31 are reserved for future signals (crt-008/009).
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct SignalDigest {
    pub features: [f32; 32],
}

/// Category ordinal encoding for slot 3.
fn category_ordinal(category: &str) -> f32 {
    match category {
        "convention" => 0.0,
        "pattern" => 0.2,
        "lesson-learned" => 0.4,
        "gap" => 0.6,
        "decision" => 0.8,
        _ => 1.0,
    }
}

/// Rule ordinal encoding for slot 4.
fn rule_ordinal(rule: &str) -> f32 {
    match rule {
        "knowledge-gap" => 0.0,
        "implicit-convention" => 0.2,
        "dead-knowledge" => 0.4,
        "recurring-friction" => 0.6,
        "file-dependency" => 0.8,
        _ => 1.0,
    }
}

impl SignalDigest {
    /// Construct digest from extraction ProposedEntry field values.
    ///
    /// Uses raw field values to avoid cross-crate dependency on unimatrix-observe.
    /// The observe crate's NeuralEnhancer bridges the gap.
    pub fn from_fields(
        extraction_confidence: f64,
        source_feature_count: usize,
        content_length: usize,
        category: &str,
        source_rule: &str,
        title_length: usize,
        tag_count: usize,
    ) -> Self {
        let mut features = [0.0_f32; 32];
        features[0] = extraction_confidence as f32;
        features[1] = (source_feature_count as f32 / 10.0).min(1.0);
        features[2] = (content_length as f32 / 1000.0).min(1.0);
        features[3] = category_ordinal(category);
        features[4] = rule_ordinal(source_rule);
        features[5] = (title_length as f32 / 200.0).min(1.0);
        features[6] = (tag_count as f32 / 10.0).min(1.0);
        // slots 7-31: reserved, zero-initialized
        Self { features }
    }

    /// All-zero digest (useful for baseline testing).
    pub fn zeros() -> Self {
        Self {
            features: [0.0; 32],
        }
    }

    /// Return features as a slice.
    pub fn as_slice(&self) -> &[f32] {
        &self.features
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-MT-01: SignalDigest from_fields slot assignment
    #[test]
    fn from_fields_slot_assignment() {
        let d = SignalDigest::from_fields(0.7, 3, 500, "convention", "knowledge-gap", 50, 2);
        assert!((d.features[0] - 0.7).abs() < 1e-6); // extraction_confidence
        assert!((d.features[1] - 0.3).abs() < 1e-6); // 3/10
        assert!((d.features[2] - 0.5).abs() < 1e-6); // 500/1000
        assert!((d.features[3] - 0.0).abs() < 1e-6); // convention ordinal
        assert!((d.features[4] - 0.0).abs() < 1e-6); // knowledge-gap ordinal
        assert!((d.features[5] - 0.25).abs() < 1e-6); // 50/200
        assert!((d.features[6] - 0.2).abs() < 1e-6); // 2/10
        // slots 7-31 all zero
        for i in 7..32 {
            assert_eq!(d.features[i], 0.0, "slot {i} should be zero");
        }
    }

    // T-MT-02: SignalDigest normalization clamping
    #[test]
    fn normalization_clamping() {
        let d = SignalDigest::from_fields(0.9, 20, 5000, "pattern", "dead-knowledge", 500, 30);
        assert_eq!(d.features[1], 1.0); // 20/10 clamped
        assert_eq!(d.features[2], 1.0); // 5000/1000 clamped
        assert_eq!(d.features[5], 1.0); // 500/200 clamped
        assert_eq!(d.features[6], 1.0); // 30/10 clamped
        // All values in [0.0, 1.0]
        for (i, &v) in d.features.iter().enumerate() {
            assert!((0.0..=1.0).contains(&v), "slot {i} = {v} out of [0,1]");
        }
    }

    // T-MT-03: SignalDigest zeros
    #[test]
    fn zeros_all_zero() {
        let d = SignalDigest::zeros();
        for (i, &v) in d.features.iter().enumerate() {
            assert_eq!(v, 0.0, "slot {i} should be zero");
        }
    }
}
