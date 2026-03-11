//! Model Registry: three-slot versioning with promote/rollback (ADR-005).

use std::collections::HashMap;
use std::path::PathBuf;

/// Model lifecycle slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ModelSlot {
    Production,
    Shadow,
    Previous,
}

impl std::fmt::Display for ModelSlot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Production => write!(f, "production"),
            Self::Shadow => write!(f, "shadow"),
            Self::Previous => write!(f, "previous"),
        }
    }
}

/// Immutable metadata for a saved model version.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ModelVersion {
    pub generation: u64,
    pub timestamp: u64,
    pub accuracy: Option<f64>,
    pub schema_version: u32,
    pub slot: ModelSlot,
}

/// Registry operation errors.
#[derive(Debug)]
pub enum RegistryError {
    NoShadowModel,
    NoProductionModel,
    NoPreviousModel,
    Io(String),
    Serialization(String),
}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoShadowModel => write!(f, "no shadow model to promote"),
            Self::NoProductionModel => write!(f, "no production model"),
            Self::NoPreviousModel => write!(f, "no previous model for rollback"),
            Self::Io(e) => write!(f, "IO error: {e}"),
            Self::Serialization(e) => write!(f, "serialization error: {e}"),
        }
    }
}

impl std::error::Error for RegistryError {}

/// Per-model slot assignments.
#[derive(serde::Serialize, serde::Deserialize, Default)]
struct ModelSlots {
    production: Option<ModelVersion>,
    shadow: Option<ModelVersion>,
    previous: Option<ModelVersion>,
}

/// Persisted registry state.
#[derive(serde::Serialize, serde::Deserialize, Default)]
struct RegistryState {
    models: HashMap<String, ModelSlots>,
}

/// Three-slot model registry with promote/rollback.
pub struct ModelRegistry {
    models_dir: PathBuf,
    state: RegistryState,
}

impl ModelRegistry {
    /// Create or load a registry from the given directory.
    pub fn new(models_dir: PathBuf) -> Self {
        let state_path = models_dir.join("registry.json");
        let state = if state_path.exists() {
            match std::fs::read_to_string(&state_path) {
                Ok(json) => serde_json::from_str(&json).unwrap_or_default(),
                Err(_) => RegistryState::default(),
            }
        } else {
            RegistryState::default()
        };
        Self { models_dir, state }
    }

    /// Get the production model version.
    pub fn get_production(&self, model_name: &str) -> Option<&ModelVersion> {
        self.state.models.get(model_name)?.production.as_ref()
    }

    /// Get the shadow model version.
    pub fn get_shadow(&self, model_name: &str) -> Option<&ModelVersion> {
        self.state.models.get(model_name)?.shadow.as_ref()
    }

    /// Get the previous model version.
    pub fn get_previous(&self, model_name: &str) -> Option<&ModelVersion> {
        self.state.models.get(model_name)?.previous.as_ref()
    }

    /// Register a new model version in the Shadow slot.
    pub fn register_shadow(
        &mut self,
        model_name: &str,
        generation: u64,
        schema_version: u32,
    ) -> Result<(), RegistryError> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let slots = self.state.models.entry(model_name.to_string()).or_default();
        slots.shadow = Some(ModelVersion {
            generation,
            timestamp: now,
            accuracy: None,
            schema_version,
            slot: ModelSlot::Shadow,
        });
        self.persist()
    }

    /// Promote Shadow -> Production (old Production -> Previous).
    pub fn promote(&mut self, model_name: &str) -> Result<(), RegistryError> {
        let slots = self
            .state
            .models
            .get_mut(model_name)
            .ok_or(RegistryError::NoShadowModel)?;
        let shadow = slots.shadow.take().ok_or(RegistryError::NoShadowModel)?;

        // Move production -> previous
        slots.previous = slots.production.take().map(|mut v| {
            v.slot = ModelSlot::Previous;
            v
        });

        // Move shadow -> production
        let mut promoted = shadow;
        promoted.slot = ModelSlot::Production;
        slots.production = Some(promoted);

        self.persist()
    }

    /// Rollback: Production -> Shadow, Previous -> Production.
    pub fn rollback(&mut self, model_name: &str) -> Result<(), RegistryError> {
        let slots = self
            .state
            .models
            .get_mut(model_name)
            .ok_or(RegistryError::NoProductionModel)?;
        let previous = slots
            .previous
            .take()
            .ok_or(RegistryError::NoPreviousModel)?;

        let current = slots
            .production
            .take()
            .ok_or(RegistryError::NoProductionModel)?;
        let mut demoted = current;
        demoted.slot = ModelSlot::Shadow;
        slots.shadow = Some(demoted);

        let mut restored = previous;
        restored.slot = ModelSlot::Production;
        slots.production = Some(restored);

        self.persist()
    }

    /// Update accuracy for a model version in a specific slot.
    pub fn update_accuracy(
        &mut self,
        model_name: &str,
        slot: ModelSlot,
        accuracy: f64,
    ) -> Result<(), RegistryError> {
        let slots = self
            .state
            .models
            .get_mut(model_name)
            .ok_or(RegistryError::NoProductionModel)?;
        let version = match slot {
            ModelSlot::Production => slots.production.as_mut(),
            ModelSlot::Shadow => slots.shadow.as_mut(),
            ModelSlot::Previous => slots.previous.as_mut(),
        };
        if let Some(v) = version {
            v.accuracy = Some(accuracy);
        }
        self.persist()
    }

    /// Model file path for a given model name and slot.
    pub fn model_path(&self, model_name: &str, slot: ModelSlot) -> PathBuf {
        self.models_dir.join(model_name).join(format!("{slot}.bin"))
    }

    /// Save a model's bytes to the appropriate slot path.
    pub fn save_model(
        &self,
        model_name: &str,
        slot: ModelSlot,
        data: &[u8],
    ) -> Result<(), RegistryError> {
        let path = self.model_path(model_name, slot);
        let parent = path
            .parent()
            .ok_or_else(|| RegistryError::Io("no parent dir".to_string()))?;
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| RegistryError::Io("invalid filename".to_string()))?;
        crate::save_atomic(data, parent, filename).map_err(RegistryError::Io)
    }

    /// Load a model's bytes from the appropriate slot path.
    pub fn load_model(
        &self,
        model_name: &str,
        slot: ModelSlot,
    ) -> Result<Option<Vec<u8>>, RegistryError> {
        let path = self.model_path(model_name, slot);
        let parent = path
            .parent()
            .ok_or_else(|| RegistryError::Io("no parent dir".to_string()))?;
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| RegistryError::Io("invalid filename".to_string()))?;
        crate::load_file(parent, filename).map_err(RegistryError::Io)
    }

    fn persist(&self) -> Result<(), RegistryError> {
        let json = serde_json::to_string_pretty(&self.state)
            .map_err(|e| RegistryError::Serialization(e.to_string()))?;
        std::fs::create_dir_all(&self.models_dir).map_err(|e| RegistryError::Io(e.to_string()))?;
        let path = self.models_dir.join("registry.json");
        std::fs::write(&path, json.as_bytes()).map_err(|e| RegistryError::Io(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir_registry() -> (tempfile::TempDir, ModelRegistry) {
        let dir = tempfile::TempDir::new().expect("tmpdir");
        let reg = ModelRegistry::new(dir.path().join("models"));
        (dir, reg)
    }

    // T-RG-01: Registry new creates empty state
    #[test]
    fn new_empty_state() {
        let (_dir, reg) = tmpdir_registry();
        assert!(reg.get_production("test").is_none());
        assert!(reg.get_shadow("test").is_none());
        assert!(reg.get_previous("test").is_none());
    }

    // T-RG-02: Register shadow + promote
    #[test]
    fn register_shadow_and_promote() {
        let (_dir, mut reg) = tmpdir_registry();
        reg.register_shadow("classifier", 1, 1).expect("register");
        assert!(reg.get_shadow("classifier").is_some());
        assert_eq!(reg.get_shadow("classifier").unwrap().generation, 1);

        reg.promote("classifier").expect("promote");
        assert!(reg.get_production("classifier").is_some());
        assert_eq!(reg.get_production("classifier").unwrap().generation, 1);
        assert!(reg.get_shadow("classifier").is_none());
    }

    // T-RG-03: Promote with existing production moves to previous
    #[test]
    fn promote_moves_production_to_previous() {
        let (_dir, mut reg) = tmpdir_registry();
        reg.register_shadow("classifier", 1, 1).expect("r1");
        reg.promote("classifier").expect("p1");

        reg.register_shadow("classifier", 2, 1).expect("r2");
        reg.promote("classifier").expect("p2");

        assert_eq!(reg.get_production("classifier").unwrap().generation, 2);
        assert_eq!(reg.get_previous("classifier").unwrap().generation, 1);
    }

    // T-RG-04: Rollback restores previous
    #[test]
    fn rollback_restores_previous() {
        let (_dir, mut reg) = tmpdir_registry();
        reg.register_shadow("classifier", 1, 1).expect("r1");
        reg.promote("classifier").expect("p1");
        reg.register_shadow("classifier", 2, 1).expect("r2");
        reg.promote("classifier").expect("p2");

        // Now: production=gen2, previous=gen1
        reg.rollback("classifier").expect("rollback");
        assert_eq!(reg.get_production("classifier").unwrap().generation, 1);
        assert_eq!(reg.get_shadow("classifier").unwrap().generation, 2);
    }

    // T-RG-05: Rollback with no previous fails
    #[test]
    fn rollback_no_previous_fails() {
        let (_dir, mut reg) = tmpdir_registry();
        reg.register_shadow("classifier", 1, 1).expect("register");
        reg.promote("classifier").expect("promote");

        let result = reg.rollback("classifier");
        assert!(result.is_err());
        // Production unchanged
        assert_eq!(reg.get_production("classifier").unwrap().generation, 1);
    }

    // T-RG-06: Save and load model roundtrip
    #[test]
    fn save_load_model_roundtrip() {
        let (_dir, reg) = tmpdir_registry();
        let data = b"model weights data";
        reg.save_model("classifier", ModelSlot::Production, data)
            .expect("save");
        let loaded = reg
            .load_model("classifier", ModelSlot::Production)
            .expect("load");
        assert_eq!(loaded, Some(data.to_vec()));
    }

    // T-RG-07: Registry state persists across instances
    #[test]
    fn state_persists_across_instances() {
        let dir = tempfile::TempDir::new().expect("tmpdir");
        let models_dir = dir.path().join("models");

        {
            let mut reg = ModelRegistry::new(models_dir.clone());
            reg.register_shadow("classifier", 1, 1).expect("register");
            reg.promote("classifier").expect("promote");
        }

        let reg2 = ModelRegistry::new(models_dir);
        assert!(reg2.get_production("classifier").is_some());
        assert_eq!(reg2.get_production("classifier").unwrap().generation, 1);
    }
}
