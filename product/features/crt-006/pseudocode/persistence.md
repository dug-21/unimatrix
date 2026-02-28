# Pseudocode: persistence (Adaptation State)

## Structs

```
const CURRENT_VERSION: u32 = 1;
const STATE_FILENAME: &str = "adaptation.state";

#[derive(Serialize, Deserialize)]
struct AdaptationState {
    version: u32,
    rank: u8,
    dimension: u16,
    scale: f32,
    weights_a: Vec<f32>,        // d * r flattened
    weights_b: Vec<f32>,        // r * d flattened
    fisher_diagonal: Vec<f32>,  // 2 * d * r flattened
    reference_params: Vec<f32>, // 2 * d * r flattened
    prototypes: Vec<SerializedPrototype>,
    training_generation: u64,
    total_training_steps: u64,
    #[serde(default)]
    config: AdaptConfig,
}
```

## Save

```
fn save_state(state: &AdaptationState, dir: &Path) -> Result<()>:
    let path = dir.join(STATE_FILENAME)

    // Serialize with bincode v2
    let bytes = bincode::serde::encode_to_vec(state, bincode::config::standard())?

    // Write atomically: write to temp file, then rename
    let tmp_path = dir.join("adaptation.state.tmp")
    fs::write(&tmp_path, &bytes)?
    fs::rename(&tmp_path, &path)?

    return Ok(())
```

## Load

```
fn load_state(dir: &Path) -> Result<Option<AdaptationState>>:
    let path = dir.join(STATE_FILENAME)

    if !path.exists():
        return Ok(None)  // Fresh start, no state file

    let bytes = match fs::read(&path):
        Ok(b) -> b
        Err(e):
            log_warning("Failed to read adaptation state: {e}")
            return Ok(None)  // Graceful fallback

    let state: AdaptationState = match bincode::serde::decode_from_slice(&bytes, bincode::config::standard()):
        Ok((s, _)) -> s
        Err(e):
            log_warning("Failed to deserialize adaptation state: {e}, starting fresh")
            // Rename corrupt file for debugging
            let corrupt_path = dir.join("adaptation.state.corrupt")
            let _ = fs::rename(&path, &corrupt_path)
            return Ok(None)

    // Validate version
    if state.version > CURRENT_VERSION:
        log_warning("Adaptation state version {} > current {}, starting fresh", state.version, CURRENT_VERSION)
        return Ok(None)

    return Ok(Some(state))
```

## Snapshot from Live State

```
fn snapshot_state(
    lora: &MicroLoRA,
    ewc: &EwcState,
    prototypes: &PrototypeManager,
    config: &AdaptConfig,
    generation: u64,
    total_steps: u64,
) -> AdaptationState:
    let weights = lora.weights.read()
    let (fisher, reference) = ewc.to_vecs()

    return AdaptationState {
        version: CURRENT_VERSION,
        rank: config.rank,
        dimension: config.dimension,
        scale: config.scale,
        weights_a: weights.a.iter().cloned().collect(),
        weights_b: weights.b.iter().cloned().collect(),
        fisher_diagonal: fisher,
        reference_params: reference,
        prototypes: prototypes.to_serialized(),
        training_generation: generation,
        total_training_steps: total_steps,
        config: config.clone(),
    }
```

## Restore to Live State

```
fn restore_state(
    state: AdaptationState,
    lora: &MicroLoRA,
    ewc: &mut EwcState,
    prototypes: &mut PrototypeManager,
) -> (u64, u64):
    // Restore LoRA weights
    let d = state.dimension as usize
    let r = state.rank as usize
    let a = Array2::from_shape_vec((d, r), state.weights_a).expect("weight_a shape mismatch")
    let b = Array2::from_shape_vec((r, d), state.weights_b).expect("weight_b shape mismatch")
    let mut weights = lora.weights.write()
    weights.a = a
    weights.b = b
    drop(weights)

    // Restore EWC state
    *ewc = EwcState::from_vecs(state.fisher_diagonal, state.reference_params, ewc.alpha, ewc.lambda)

    // Restore prototypes
    *prototypes = PrototypeManager::from_serialized(state.prototypes, ...)

    return (state.training_generation, state.total_training_steps)
```
