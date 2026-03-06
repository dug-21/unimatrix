# Test Plan: integration

## Risks Covered: confidence system integration

### T-IN-01: trust_score "neural" returns 0.40 (AC-15)
- Call trust_score("neural")
- Assert returns 0.40

### T-IN-02: trust_score preserves existing values
- trust_score("human") == 1.0
- trust_score("system") == 0.7
- trust_score("agent") == 0.5
- trust_score("auto") == 0.35
- trust_score("unknown") == 0.3

### T-IN-03: Confidence computation with neural trust_source
- Create EntryRecord with trust_source = "neural"
- compute_confidence() uses 0.40 weight for trust factor
- Verify result is in valid [0.0, 1.0] range

### T-IN-04: init_neural_enhancer graceful fallback
- Call init with non-existent models_dir
- Returns (enhancer, evaluator) with baseline models
- enhancer.enhance(entry) works without error
