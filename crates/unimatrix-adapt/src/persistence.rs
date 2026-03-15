//! Adaptation state persistence: versioned binary save/load.
//!
//! Uses bincode v2 with serde for serialization. The state file is written
//! atomically (temp + rename) and gracefully falls back to identity on
//! missing or corrupt files.

use std::fs;
use std::path::Path;

use ndarray::Array2;
use serde::{Deserialize, Serialize};

use crate::config::AdaptConfig;
use crate::lora::MicroLoRA;
use crate::prototypes::{PrototypeManager, SerializedPrototype};
use crate::regularization::EwcState;

/// Current state file version. Increment when adding fields.
const CURRENT_VERSION: u32 = 1;

/// State file name in the data directory.
const STATE_FILENAME: &str = "adaptation.state";

/// Serializable snapshot of all adaptation state.
#[derive(Serialize, Deserialize)]
pub struct AdaptationState {
    pub version: u32,
    pub rank: u8,
    pub dimension: u16,
    pub scale: f32,
    pub weights_a: Vec<f32>,
    pub weights_b: Vec<f32>,
    pub fisher_diagonal: Vec<f32>,
    pub reference_params: Vec<f32>,
    pub prototypes: Vec<SerializedPrototype>,
    pub training_generation: u64,
    pub total_training_steps: u64,
    #[serde(default)]
    pub config: AdaptConfig,
}

/// Save adaptation state to disk atomically.
pub fn save_state(state: &AdaptationState, dir: &Path) -> Result<(), String> {
    let path = dir.join(STATE_FILENAME);
    let tmp_path = dir.join("adaptation.state.tmp");

    let bytes = bincode::serde::encode_to_vec(state, bincode::config::standard())
        .map_err(|e| format!("serialization failed: {e}"))?;

    fs::write(&tmp_path, &bytes).map_err(|e| format!("write failed: {e}"))?;
    fs::rename(&tmp_path, &path).map_err(|e| format!("rename failed: {e}"))?;

    Ok(())
}

/// Load adaptation state from disk. Returns None if file is missing or corrupt.
pub fn load_state(dir: &Path) -> Result<Option<AdaptationState>, String> {
    let path = dir.join(STATE_FILENAME);

    if !path.exists() {
        return Ok(None);
    }

    let bytes = match fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!("Failed to read adaptation state: {e}");
            return Ok(None);
        }
    };

    if bytes.is_empty() {
        tracing::warn!("Adaptation state file is empty, starting fresh");
        return Ok(None);
    }

    let state: AdaptationState =
        match bincode::serde::decode_from_slice(&bytes, bincode::config::standard()) {
            Ok((s, _)) => s,
            Err(e) => {
                tracing::warn!("Failed to deserialize adaptation state: {e}, starting fresh");
                let corrupt_path = dir.join("adaptation.state.corrupt");
                let _ = fs::rename(&path, &corrupt_path);
                return Ok(None);
            }
        };

    if state.version > CURRENT_VERSION {
        tracing::warn!(
            "Adaptation state version {} > current {}, starting fresh",
            state.version,
            CURRENT_VERSION
        );
        return Ok(None);
    }

    Ok(Some(state))
}

/// Create a snapshot of live state for persistence.
pub fn snapshot_state(
    lora: &MicroLoRA,
    ewc: &EwcState,
    prototypes: &PrototypeManager,
    config: &AdaptConfig,
    generation: u64,
    total_steps: u64,
) -> AdaptationState {
    let a = lora.weights_a();
    let b = lora.weights_b();
    let (fisher, reference) = ewc.to_vecs();

    AdaptationState {
        version: CURRENT_VERSION,
        rank: config.rank,
        dimension: config.dimension,
        scale: config.scale,
        weights_a: a.iter().cloned().collect(),
        weights_b: b.iter().cloned().collect(),
        fisher_diagonal: fisher,
        reference_params: reference,
        prototypes: prototypes.to_serialized(),
        training_generation: generation,
        total_training_steps: total_steps,
        config: config.clone(),
    }
}

/// Restore live state from a loaded snapshot.
///
/// Returns (training_generation, total_training_steps).
pub fn restore_state(
    state: AdaptationState,
    lora: &MicroLoRA,
    ewc: &mut EwcState,
    prototypes: &mut PrototypeManager,
    config: &AdaptConfig,
) -> Result<(u64, u64), String> {
    let d = state.dimension as usize;
    let r = state.rank as usize;

    // Restore LoRA weights
    let a = Array2::from_shape_vec((d, r), state.weights_a)
        .map_err(|e| format!("weight_a shape mismatch: {e}"))?;
    let b = Array2::from_shape_vec((r, d), state.weights_b)
        .map_err(|e| format!("weight_b shape mismatch: {e}"))?;
    lora.set_weights(a, b);

    // Restore EWC state
    *ewc = EwcState::from_vecs(
        state.fisher_diagonal,
        state.reference_params,
        config.ewc_alpha,
        config.ewc_lambda,
    );

    // Restore prototypes
    *prototypes = PrototypeManager::from_serialized(
        state.prototypes,
        config.max_prototypes,
        config.min_prototype_entries,
        config.pull_strength,
        d,
    );

    Ok((state.training_generation, state.total_training_steps))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lora::LoraConfig;

    fn make_test_state() -> AdaptationState {
        let d = 16_usize;
        let r = 4_usize;
        AdaptationState {
            version: CURRENT_VERSION,
            rank: 4,
            dimension: 16,
            scale: 1.0,
            weights_a: vec![0.1; d * r],
            weights_b: vec![0.2; r * d],
            fisher_diagonal: vec![0.01; 2 * d * r],
            reference_params: vec![0.05; 2 * d * r],
            prototypes: vec![SerializedPrototype {
                key_type: "category".to_string(),
                key_value: "test".to_string(),
                centroid: vec![0.5; d],
                entry_count: 5,
                last_updated: 1000,
            }],
            training_generation: 42,
            total_training_steps: 100,
            config: AdaptConfig {
                rank: 4,
                dimension: 16,
                ..AdaptConfig::default()
            },
        }
    }

    // T-PER-01: Save and load round-trip
    #[test]
    fn save_load_roundtrip() {
        let dir = tempfile::TempDir::new().unwrap();
        let state = make_test_state();

        save_state(&state, dir.path()).unwrap();
        let loaded = load_state(dir.path()).unwrap().unwrap();

        assert_eq!(loaded.version, state.version);
        assert_eq!(loaded.rank, state.rank);
        assert_eq!(loaded.dimension, state.dimension);
        assert_eq!(loaded.weights_a, state.weights_a);
        assert_eq!(loaded.weights_b, state.weights_b);
        assert_eq!(loaded.fisher_diagonal, state.fisher_diagonal);
        assert_eq!(loaded.reference_params, state.reference_params);
        assert_eq!(loaded.prototypes.len(), state.prototypes.len());
        assert_eq!(loaded.training_generation, 42);
        assert_eq!(loaded.total_training_steps, 100);
    }

    // T-PER-03: Corrupt file graceful fallback
    #[test]
    fn corrupt_file_fallback() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join(STATE_FILENAME);
        fs::write(&path, b"this is not bincode data").unwrap();

        let result = load_state(dir.path()).unwrap();
        assert!(result.is_none(), "corrupt file should return None");

        // Should have renamed to .corrupt
        assert!(dir.path().join("adaptation.state.corrupt").exists());
    }

    // T-PER-04: Zero-byte file graceful fallback
    #[test]
    fn empty_file_fallback() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join(STATE_FILENAME);
        fs::write(&path, b"").unwrap();

        let result = load_state(dir.path()).unwrap();
        assert!(result.is_none());
    }

    // T-PER-07: Missing state file
    #[test]
    fn missing_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let result = load_state(dir.path()).unwrap();
        assert!(result.is_none());
    }

    // T-PER-08: Atomic write safety
    #[test]
    fn atomic_write() {
        let dir = tempfile::TempDir::new().unwrap();
        let state = make_test_state();
        save_state(&state, dir.path()).unwrap();

        assert!(dir.path().join(STATE_FILENAME).exists());
        assert!(!dir.path().join("adaptation.state.tmp").exists());

        // Verify loadable
        let loaded = load_state(dir.path()).unwrap();
        assert!(loaded.is_some());
    }

    // T-PER-09: snapshot_state captures current values
    #[test]
    fn snapshot_captures_state() {
        let config = AdaptConfig {
            rank: 4,
            dimension: 16,
            ..AdaptConfig::default()
        };
        let lora = MicroLoRA::new(LoraConfig {
            rank: config.rank,
            dimension: config.dimension,
            scale: config.scale,
        });
        let ewc = EwcState::new(2 * 16 * 4, config.ewc_alpha, config.ewc_lambda);
        let prototypes = PrototypeManager::new(
            config.max_prototypes,
            config.min_prototype_entries,
            config.pull_strength,
            16,
        );

        let state = snapshot_state(&lora, &ewc, &prototypes, &config, 5, 10);
        assert_eq!(state.version, CURRENT_VERSION);
        assert_eq!(state.rank, 4);
        assert_eq!(state.dimension, 16);
        assert_eq!(state.weights_a.len(), 16 * 4);
        assert_eq!(state.weights_b.len(), 4 * 16);
        assert_eq!(state.training_generation, 5);
        assert_eq!(state.total_training_steps, 10);
    }

    // T-PER-10: restore_state applies to live components
    #[test]
    fn restore_state_applies() {
        let config = AdaptConfig {
            rank: 4,
            dimension: 16,
            ..AdaptConfig::default()
        };
        let lora = MicroLoRA::new(LoraConfig {
            rank: config.rank,
            dimension: config.dimension,
            scale: config.scale,
        });
        let mut ewc = EwcState::new(2 * 16 * 4, config.ewc_alpha, config.ewc_lambda);
        let mut prototypes = PrototypeManager::new(
            config.max_prototypes,
            config.min_prototype_entries,
            config.pull_strength,
            16,
        );

        let state = make_test_state();
        let (restored_gen, steps) =
            restore_state(state, &lora, &mut ewc, &mut prototypes, &config).unwrap();

        assert_eq!(restored_gen, 42);
        assert_eq!(steps, 100);

        // Verify weights were restored
        let a = lora.weights_a();
        assert!((a[[0, 0]] - 0.1).abs() < 1e-6);

        // Verify prototypes restored
        assert_eq!(prototypes.len(), 1);
    }

    // T-PER-11: Version too new is rejected
    #[test]
    fn version_too_new() {
        let dir = tempfile::TempDir::new().unwrap();
        let mut state = make_test_state();
        state.version = CURRENT_VERSION + 1;
        save_state(&state, dir.path()).unwrap();

        let result = load_state(dir.path()).unwrap();
        assert!(result.is_none(), "future version should return None");
    }
}
