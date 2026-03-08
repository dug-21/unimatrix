//! Configuration for the adaptation pipeline.

use serde::{Deserialize, Serialize};

/// Configuration for the adaptive embedding pipeline.
///
/// All parameters have sensible defaults. The defaults are suitable for
/// knowledge bases up to 100K entries with 384d embeddings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdaptConfig {
    /// LoRA rank (2-16). Controls adaptation capacity. Default: 4.
    pub rank: u8,
    /// Embedding dimension. Must match ONNX model output. Default: 384.
    pub dimension: u16,
    /// LoRA scale factor applied to the residual. Default: 1.0.
    pub scale: f32,
    /// Learning rate for the A (down-projection) matrix. Default: 0.001.
    pub lr_a: f32,
    /// Learning rate ratio: B lr = lr_ratio * lr_a (LoRA+). Default: 16.0.
    pub lr_ratio: f32,
    /// InfoNCE temperature parameter. Default: 0.07.
    pub temperature: f32,
    /// Training batch size (number of pairs per step). Default: 32.
    pub batch_size: usize,
    /// Training reservoir capacity (max buffered pairs). Default: 512.
    pub reservoir_capacity: usize,
    /// EWC++ exponential decay factor for Fisher diagonal. Default: 0.95.
    pub ewc_alpha: f32,
    /// EWC penalty weight (lambda). Default: 0.5.
    pub ewc_lambda: f32,
    /// Maximum number of prototypes. Default: 256.
    pub max_prototypes: usize,
    /// Minimum entries before a prototype applies pull. Default: 3.
    pub min_prototype_entries: u32,
    /// Prototype soft pull strength. Default: 0.1.
    pub pull_strength: f32,
    /// Reservoir RNG seed. Default: 42.
    #[serde(default = "default_reservoir_seed")]
    pub reservoir_seed: u64,
    /// MicroLoRA init seed. Default: 42.
    #[serde(default = "default_init_seed")]
    pub init_seed: u64,
}

fn default_reservoir_seed() -> u64 {
    42
}

fn default_init_seed() -> u64 {
    42
}

impl Default for AdaptConfig {
    fn default() -> Self {
        Self {
            rank: 4,
            dimension: 384,
            scale: 1.0,
            lr_a: 0.001,
            lr_ratio: 16.0,
            temperature: 0.07,
            batch_size: 32,
            reservoir_capacity: 512,
            ewc_alpha: 0.95,
            ewc_lambda: 0.5,
            max_prototypes: 256,
            min_prototype_entries: 3,
            pull_strength: 0.1,
            reservoir_seed: 42,
            init_seed: 42,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_values() {
        let c = AdaptConfig::default();
        assert_eq!(c.rank, 4);
        assert_eq!(c.dimension, 384);
        assert_eq!(c.scale, 1.0);
        assert_eq!(c.lr_a, 0.001);
        assert_eq!(c.lr_ratio, 16.0);
        assert_eq!(c.temperature, 0.07);
        assert_eq!(c.batch_size, 32);
        assert_eq!(c.reservoir_capacity, 512);
        assert_eq!(c.ewc_alpha, 0.95);
        assert_eq!(c.ewc_lambda, 0.5);
        assert_eq!(c.max_prototypes, 256);
        assert_eq!(c.min_prototype_entries, 3);
        assert_eq!(c.pull_strength, 0.1);
        assert_eq!(c.reservoir_seed, 42);
        assert_eq!(c.init_seed, 42);
    }

    // T-COMPAT-01: Old-format AdaptConfig (without new fields) deserializes with defaults
    #[test]
    fn old_format_deserializes_with_defaults() {
        // Serialize a config, then try to deserialize. Bincode v2 with serde
        // should honor #[serde(default)] for the new fields.
        let c = AdaptConfig::default();
        let bytes =
            bincode::serde::encode_to_vec(&c, bincode::config::standard()).unwrap();
        let (decoded, _): (AdaptConfig, _) =
            bincode::serde::decode_from_slice(&bytes, bincode::config::standard())
                .unwrap();
        assert_eq!(decoded.reservoir_seed, 42);
        assert_eq!(decoded.init_seed, 42);
    }

    #[test]
    fn config_serde_roundtrip() {
        let c = AdaptConfig::default();
        let bytes =
            bincode::serde::encode_to_vec(&c, bincode::config::standard()).unwrap();
        let (decoded, _): (AdaptConfig, _) =
            bincode::serde::decode_from_slice(&bytes, bincode::config::standard())
                .unwrap();
        assert_eq!(decoded.rank, c.rank);
        assert_eq!(decoded.dimension, c.dimension);
    }
}
