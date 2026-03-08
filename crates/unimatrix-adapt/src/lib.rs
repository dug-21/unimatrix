#![forbid(unsafe_code)]

//! Adaptive embedding pipeline for Unimatrix.
//!
//! Provides MicroLoRA-based adaptation of frozen ONNX embeddings using
//! contrastive learning from co-access pair signals. The pipeline sits between
//! the ONNX embedding output and the HNSW vector index, transforming raw
//! 384d f32 vectors into domain-adapted 384d f32 vectors.
//!
//! Components:
//! - `lora`: MicroLoRA engine (forward/backward/update)
//! - `training`: InfoNCE loss, reservoir sampling, batch training
//! - `regularization`: EWC++ online Fisher regularization
//! - `prototypes`: Domain prototype centroids with soft pull
//! - `persistence`: Versioned binary state save/load
//! - `service`: Public API orchestrating all components
//! - `config`: Configuration with sensible defaults

pub mod config;
pub mod lora;
pub mod persistence;
pub mod prototypes;
pub mod regularization;
pub mod service;
pub mod training;

pub use config::AdaptConfig;
pub use service::AdaptationService;
