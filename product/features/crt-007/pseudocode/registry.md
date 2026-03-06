# Pseudocode: registry (Model Registry + Versioning)

## Pattern: JSON-persisted slot manager with promote/rollback

Three slots per model name: Production, Shadow, Previous.
Registry state stored as JSON in the models directory.

## Files

### crates/unimatrix-learn/src/registry.rs

```pseudo
use std::path::PathBuf;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ModelSlot {
    Production,
    Shadow,
    Previous,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ModelVersion {
    pub generation: u64,
    pub timestamp: u64,
    pub accuracy: Option<f64>,
    pub schema_version: u32,
    pub slot: ModelSlot,
}

#[derive(Debug)]
pub enum RegistryError {
    NoShadowModel,
    NoProductionModel,
    NoPreviousModel,
    InsufficientEvaluations { have: u32, need: u32 },
    AccuracyBelowThreshold { shadow: f64, production: f64 },
    CategoryRegression { category: String },
    Io(String),
    Serialization(String),
}

impl std::fmt::Display for RegistryError { ... }
impl std::error::Error for RegistryError {}

/// Registry state stored in JSON
#[derive(serde::Serialize, serde::Deserialize, Default)]
struct RegistryState {
    models: HashMap<String, ModelSlots>,
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct ModelSlots {
    production: Option<ModelVersion>,
    shadow: Option<ModelVersion>,
    previous: Option<ModelVersion>,
}

pub struct ModelRegistry {
    models_dir: PathBuf,
    state: RegistryState,
}

impl ModelRegistry {
    pub fn new(models_dir: PathBuf) -> Self {
        // Load registry.json if exists, else default
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

    pub fn get_production(&self, model_name: &str) -> Option<&ModelVersion> {
        self.state.models.get(model_name)?.production.as_ref()
    }

    pub fn get_shadow(&self, model_name: &str) -> Option<&ModelVersion> {
        self.state.models.get(model_name)?.shadow.as_ref()
    }

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
        let slots = self.state.models.entry(model_name.to_string())
            .or_insert_with(ModelSlots::default);
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
    /// Requires: shadow exists, accuracy >= production accuracy, min evaluations met.
    pub fn promote(&mut self, model_name: &str) -> Result<(), RegistryError> {
        let slots = self.state.models.get_mut(model_name)
            .ok_or(RegistryError::NoShadowModel)?;
        let shadow = slots.shadow.take()
            .ok_or(RegistryError::NoShadowModel)?;

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
        let slots = self.state.models.get_mut(model_name)
            .ok_or(RegistryError::NoProductionModel)?;
        let previous = slots.previous.take()
            .ok_or(RegistryError::NoPreviousModel)?;

        // Current production -> shadow
        let current = slots.production.take()
            .ok_or(RegistryError::NoProductionModel)?;
        let mut demoted = current;
        demoted.slot = ModelSlot::Shadow;
        slots.shadow = Some(demoted);

        // Previous -> production
        let mut restored = previous;
        restored.slot = ModelSlot::Production;
        slots.production = Some(restored);

        self.persist()
    }

    /// Update accuracy for a model version.
    pub fn update_accuracy(
        &mut self,
        model_name: &str,
        slot: ModelSlot,
        accuracy: f64,
    ) -> Result<(), RegistryError> {
        let slots = self.state.models.get_mut(model_name)
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
        let slot_str = match slot {
            ModelSlot::Production => "production",
            ModelSlot::Shadow => "shadow",
            ModelSlot::Previous => "previous",
        };
        self.models_dir.join(model_name).join(format!("{slot_str}.bin"))
    }

    /// Save a model's bytes to the appropriate slot path.
    pub fn save_model(
        &self,
        model_name: &str,
        slot: ModelSlot,
        data: &[u8],
    ) -> Result<(), RegistryError> {
        let path = self.model_path(model_name, slot);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| RegistryError::Io(e.to_string()))?;
        }
        crate::save_atomic(data, path.parent().unwrap(), path.file_name().unwrap().to_str().unwrap())
            .map_err(|e| RegistryError::Io(e))
    }

    /// Load a model's bytes from the appropriate slot path.
    pub fn load_model(
        &self,
        model_name: &str,
        slot: ModelSlot,
    ) -> Result<Option<Vec<u8>>, RegistryError> {
        let path = self.model_path(model_name, slot);
        if let Some(parent) = path.parent() {
            crate::load_file(parent, path.file_name().unwrap().to_str().unwrap())
                .map_err(|e| RegistryError::Io(e))
        } else {
            Ok(None)
        }
    }

    fn persist(&self) -> Result<(), RegistryError> {
        let json = serde_json::to_string_pretty(&self.state)
            .map_err(|e| RegistryError::Serialization(e.to_string()))?;
        let path = self.models_dir.join("registry.json");
        std::fs::create_dir_all(&self.models_dir)
            .map_err(|e| RegistryError::Io(e.to_string()))?;
        std::fs::write(&path, json.as_bytes())
            .map_err(|e| RegistryError::Io(e.to_string()))
    }
}
```

## Key Design Decisions

- Registry state in JSON for human readability and debugging
- Model binaries in separate .bin files per slot (not embedded in JSON)
- Atomic save via save_atomic for model binaries
- Promotion/rollback are pure slot transitions -- the calling code decides
  whether promotion criteria are met (ShadowEvaluator handles accuracy checks)
- Schema version in ModelVersion enables future deserialization compat checks
