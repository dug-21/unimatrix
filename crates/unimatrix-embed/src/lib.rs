#![forbid(unsafe_code)]

mod config;
pub mod cross_encoder;
mod download;
mod error;
mod model;
mod normalize;
mod onnx;
mod pooling;
mod provider;
mod text;

#[cfg(any(test, feature = "test-support"))]
pub mod test_helpers;

pub use config::EmbedConfig;
pub use cross_encoder::{CrossEncoderProvider, NliProvider, NliScores};
pub use error::{EmbedError, Result};
pub use model::{EmbeddingModel, NliModel};
pub use normalize::{l2_normalize, l2_normalized};
pub use onnx::OnnxProvider;
pub use provider::EmbeddingProvider;
pub use text::{embed_entries, embed_entry, prepare_text};

// Re-export ensure_model for CLI model-download subcommand (nan-004 C8).
pub use download::ensure_model;

// Re-export ensure_nli_model for CLI model-download subcommand (crt-023).
pub use download::ensure_nli_model;
