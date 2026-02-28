# Test Plan: persistence (Adaptation State)

## Component Under Test

`crates/unimatrix-adapt/src/persistence.rs` -- AdaptationState, save_state, load_state, snapshot_state, restore_state.

## Risks Covered

- **R-04** (High): Adaptation state deserialization failure on version upgrade

## Test Cases

### T-PER-01: Save and load round-trip

**Purpose**: Verify state survives serialization round-trip.
**Setup**: Create AdaptationState with known values (non-trivial weights, Fisher, prototypes).
**Method**: Call save_state to a temp directory, then load_state from same directory.
**Assertions**:
- Loaded state matches saved state field-by-field:
  - version, rank, dimension, scale
  - weights_a, weights_b (element-wise)
  - fisher_diagonal, reference_params (element-wise)
  - prototypes (count, keys, centroids)
  - training_generation, total_training_steps

### T-PER-02: Version upgrade compatibility (R-04)

**Purpose**: Verify loading state with missing fields (future version upgrade scenario).
**Setup**: Save a state with CURRENT_VERSION. Manually add an unknown field to the struct (or rely on serde(default) behavior for future fields).
**Method**: Load the saved state.
**Assertions**:
- Load succeeds (no deserialization error)
- Known fields have correct values
- This validates the `#[serde(default)]` pattern for forward compatibility

### T-PER-03: Corrupt file graceful fallback (R-04)

**Purpose**: Verify corrupt state file produces None, not an error.
**Setup**: Write random bytes to the state file path.
**Method**: Call load_state.
**Assertions**:
- Returns Ok(None) (graceful fallback)
- A file named "adaptation.state.corrupt" exists (renamed for debugging)

### T-PER-04: Zero-byte file graceful fallback (R-04)

**Purpose**: Verify empty state file produces None.
**Setup**: Create empty file at state file path.
**Method**: Call load_state.
**Assertions**:
- Returns Ok(None)

### T-PER-05: Dimension mismatch detection (R-04, EC-07)

**Purpose**: Verify loading state with wrong dimension is rejected.
**Setup**: Save state with dimension=768.
**Method**: Load with service configured for dimension=384.
**Assertions**:
- Load falls back to identity (warning logged)
- Service operates with fresh weights

### T-PER-06: Rank mismatch detection (EC-07)

**Purpose**: Verify loading state with wrong rank is rejected.
**Setup**: Save state with rank=8.
**Method**: Load with service configured for rank=4.
**Assertions**:
- Load falls back to identity (warning logged)

### T-PER-07: Missing state file

**Purpose**: Verify missing state file returns None (fresh start).
**Setup**: Point load_state at directory with no adaptation.state file.
**Assertions**:
- Returns Ok(None)
- No error logged (this is the expected cold-start case)

### T-PER-08: Atomic write safety

**Purpose**: Verify save uses temp file + rename pattern.
**Setup**: Save state to a directory.
**Method**: Verify no "adaptation.state.tmp" remains after successful save.
**Assertions**:
- "adaptation.state" exists
- "adaptation.state.tmp" does not exist
- File content is valid (loadable)

### T-PER-09: snapshot_state captures current values

**Purpose**: Verify snapshot captures live state correctly.
**Setup**: Create MicroLoRA with known weights, EwcState with known fisher/reference, PrototypeManager with known prototypes.
**Method**: Call snapshot_state.
**Assertions**:
- State.weights_a matches MicroLoRA A matrix flattened
- State.weights_b matches MicroLoRA B matrix flattened
- State.fisher_diagonal matches EwcState fisher
- State.reference_params matches EwcState reference
- State.prototypes has correct count and keys

### T-PER-10: restore_state applies to live components

**Purpose**: Verify restore correctly updates MicroLoRA, EwcState, PrototypeManager.
**Setup**: Create fresh components. Save a state with non-trivial values. Call restore_state.
**Assertions**:
- MicroLoRA weights match saved state
- EwcState fisher and reference match saved state
- PrototypeManager prototypes match saved state
- Generation and total_steps match saved state

### T-PER-11: Version too new is rejected

**Purpose**: Verify state with version > CURRENT_VERSION produces None.
**Setup**: Save state, manually set version to CURRENT_VERSION + 1 (modify bytes or use serde).
**Method**: Call load_state.
**Assertions**:
- Returns Ok(None)
- Warning logged about version mismatch

## Edge Cases

| Case | Test | Expected |
|------|------|----------|
| EC-01 Empty KB | T-PER-07 | No state file, fresh start |
| EC-07 Rank change | T-PER-06 | State rejected, fresh start |

## Total: 11 unit tests
