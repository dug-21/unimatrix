# Test Plan: model-trait (NeuralModel + SignalDigest)

## Risks Covered: R-06 (partial), R-09 (partial)

### T-MT-01: SignalDigest from_fields slot assignment
- Call from_fields with known values:
  extraction_confidence=0.7, source_feature_count=3, content_length=500,
  category="convention", source_rule="knowledge-gap", title_length=50, tag_count=2
- Assert features[0] == 0.7
- Assert features[1] == 0.3 (3/10)
- Assert features[2] == 0.5 (500/1000)
- Assert features[3] == 0.0 (convention ordinal)
- Assert features[4] == 0.0 (knowledge-gap ordinal)
- Assert features[5] == 0.25 (50/200)
- Assert features[6] == 0.2 (2/10)
- Assert features[7..32] all zero

### T-MT-02: SignalDigest normalization clamping
- content_length=5000 -> features[2] == 1.0 (clamped)
- source_feature_count=20 -> features[1] == 1.0 (clamped)
- All values in [0.0, 1.0]

### T-MT-03: SignalDigest zeros()
- All 32 elements == 0.0

### T-MT-04: NeuralModel flat_parameters + set_parameters roundtrip (R-06)
- Create classifier with baseline weights
- Get flat_parameters() -> params1
- Create new classifier, set_parameters(params1)
- Get flat_parameters() -> params2
- Assert params1 == params2

### T-MT-05: NeuralModel serialize + deserialize roundtrip (R-06)
- Create classifier with baseline weights
- serialize() -> bytes
- deserialize(bytes) -> classifier2
- Forward pass on both with same input -> same output
