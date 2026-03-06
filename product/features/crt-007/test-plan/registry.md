# Test Plan: registry

## Risks Covered: R-05, R-06, R-10

### T-RG-01: Registry new creates empty state
- Create ModelRegistry in tmpdir
- get_production("test") returns None
- get_shadow("test") returns None

### T-RG-02: Register shadow + promote (R-05, AC-11)
- register_shadow("classifier", gen=1, schema=1)
- get_shadow("classifier") returns Some with generation=1
- promote("classifier") succeeds
- get_production("classifier") returns Some with generation=1
- get_shadow("classifier") returns None

### T-RG-03: Promote with existing production moves to previous (R-05, AC-11)
- register_shadow("classifier", gen=1, schema=1) + promote
- register_shadow("classifier", gen=2, schema=1) + promote
- get_production -> generation=2
- get_previous -> generation=1

### T-RG-04: Rollback restores previous (R-05, AC-12)
- Set up: production=gen2, previous=gen1
- rollback("classifier")
- get_production -> generation=1
- get_shadow -> generation=2 (demoted production)

### T-RG-05: Rollback with no previous fails (R-05)
- Set up: production=gen1, no previous
- rollback("classifier") returns Err(NoPreviousModel)
- Production unchanged

### T-RG-06: Save and load model roundtrip (R-06, AC-13)
- save_model("classifier", Production, b"model data")
- load_model("classifier", Production) == Some(b"model data")
- File exists at models_dir/classifier/production.bin

### T-RG-07: Registry state persists across instances (R-06)
- Create registry, register shadow, promote
- Drop registry
- Create new registry at same dir
- get_production returns the promoted model
