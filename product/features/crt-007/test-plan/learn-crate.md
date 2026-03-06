# Test Plan: learn-crate

## Risks Covered: R-01, R-02

### T-LC-01: Generic reservoir basic add (R-01)
- Create `TrainingReservoir<(u64, u64)>` with capacity 10
- Add 5 items
- Assert len() == 5, total_seen() == 5

### T-LC-02: Generic reservoir capacity bound (R-01)
- Create reservoir with capacity 10
- Add 100 items
- Assert len() == 10, total_seen() == 100

### T-LC-03: Generic reservoir sample_batch (R-01)
- Create reservoir, add 50 items
- sample_batch(32) returns 32 items
- sample_batch(100) returns 50 items (capped at len)

### T-LC-04: EwcState update_from_flat known values (R-02)
- Create EwcState(4, 0.95, 0.5)
- First update_from_flat with params=[1,0,0,0], grad_squared=[1,0,0,0]
- Assert fisher=[1,0,0,0] (initialized directly)
- Second update_from_flat with params=[0,0,1,0], grad_squared=[0,0,1,0]
- Assert fisher=[0.95, 0, 0.05, 0] (alpha-blended)

### T-LC-05: EwcState update_from_flat penalty matches update() (R-02)
- Create two EwcState instances with same params
- Update one via update(params, grad_a, grad_b)
- Update other via update_from_flat(params, grad_squared) where grad_squared
  is computed the same way (chain of grad_a^2 + grad_b^2)
- Assert penalty() returns same value for both

### T-LC-06: save_atomic and load_file roundtrip
- save_atomic(b"test data", tmpdir, "test.bin")
- load_file(tmpdir, "test.bin") == Some(b"test data")
- No .tmp file remaining

### T-LC-07: load_file missing file returns None
- load_file(tmpdir, "nonexistent.bin") == None

### T-LC-08: Adapt test suite regression (R-01)
- `cargo test -p unimatrix-adapt` -- all existing tests pass
- This is a compile-and-run validation, not a new test
