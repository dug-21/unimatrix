//! Domain prototypes: online running-mean centroids with soft pull.
//!
//! Maintains per-category and per-topic prototype centroids that gently
//! pull adapted embeddings toward their domain cluster center.

use std::collections::HashMap;

use ndarray::Array1;
use serde::{Deserialize, Serialize};

/// Key for a prototype: either a category or a topic.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PrototypeKey {
    Category(String),
    Topic(String),
}

/// A single domain prototype with running-mean centroid.
struct Prototype {
    centroid: Array1<f32>,
    entry_count: u32,
    last_updated: u64,
}

/// Manages bounded domain prototypes with LRU eviction.
pub struct PrototypeManager {
    prototypes: HashMap<PrototypeKey, Prototype>,
    max_count: usize,
    min_entries: u32,
    pull_strength: f32,
    dimension: usize,
}

/// Serialized form of a prototype for persistence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedPrototype {
    pub key_type: String,
    pub key_value: String,
    pub centroid: Vec<f32>,
    pub entry_count: u32,
    pub last_updated: u64,
}

impl PrototypeManager {
    /// Create a new prototype manager.
    pub fn new(
        max_count: usize,
        min_entries: u32,
        pull_strength: f32,
        dimension: usize,
    ) -> Self {
        Self {
            prototypes: HashMap::new(),
            max_count,
            min_entries,
            pull_strength,
            dimension,
        }
    }

    /// Apply soft pull toward the nearest qualifying prototype.
    ///
    /// Returns the pulled embedding (or a copy of input if no prototype qualifies).
    /// A prototype qualifies if its entry_count >= min_entries.
    pub fn apply_pull(
        &self,
        adapted: &[f32],
        category: Option<&str>,
        topic: Option<&str>,
    ) -> Vec<f32> {
        let mut best_proto: Option<&Prototype> = None;
        let mut best_sim = f32::NEG_INFINITY;

        // Check category prototype
        if let Some(cat) = category {
            let key = PrototypeKey::Category(cat.to_string());
            if let Some(proto) = self.prototypes.get(&key).filter(|p| p.entry_count >= self.min_entries) {
                let sim = cosine_similarity(adapted, proto.centroid.as_slice().unwrap());
                if sim > best_sim {
                    best_sim = sim;
                    best_proto = Some(proto);
                }
            }
        }

        // Check topic prototype
        if let Some(top) = topic {
            let key = PrototypeKey::Topic(top.to_string());
            if let Some(proto) = self.prototypes.get(&key).filter(|p| p.entry_count >= self.min_entries) {
                let sim = cosine_similarity(adapted, proto.centroid.as_slice().unwrap());
                if sim > best_sim {
                    best_sim = sim;
                    best_proto = Some(proto);
                }
            }
        }

        match best_proto {
            Some(proto) => {
                let alpha = self.pull_strength * best_sim.max(0.0);
                let mut result = adapted.to_vec();
                for i in 0..self.dimension {
                    result[i] += alpha * (proto.centroid[i] - adapted[i]);
                }
                result
            }
            None => adapted.to_vec(),
        }
    }

    /// Update prototypes with a new adapted embedding.
    pub fn update(
        &mut self,
        adapted: &[f32],
        category: Option<&str>,
        topic: Option<&str>,
        now: u64,
    ) {
        if let Some(cat) = category {
            let key = PrototypeKey::Category(cat.to_string());
            self.update_single(key, adapted, now);
        }
        if let Some(top) = topic {
            let key = PrototypeKey::Topic(top.to_string());
            self.update_single(key, adapted, now);
        }
    }

    /// Update a single prototype with the online running-mean formula.
    fn update_single(&mut self, key: PrototypeKey, adapted: &[f32], now: u64) {
        if let Some(proto) = self.prototypes.get_mut(&key) {
            // Online running mean: new = (old * n + new_value) / (n + 1)
            let n = proto.entry_count as f32;
            for (i, &val) in adapted.iter().enumerate().take(self.dimension) {
                proto.centroid[i] = (proto.centroid[i] * n + val) / (n + 1.0);
            }
            proto.entry_count += 1;
            proto.last_updated = now;
        } else {
            // Evict if at capacity
            if self.prototypes.len() >= self.max_count {
                self.evict_lru();
            }

            self.prototypes.insert(
                key,
                Prototype {
                    centroid: Array1::from(adapted.to_vec()),
                    entry_count: 1,
                    last_updated: now,
                },
            );
        }
    }

    /// Evict the least-recently-updated prototype.
    fn evict_lru(&mut self) {
        let oldest_key = self
            .prototypes
            .iter()
            .min_by_key(|(_, proto)| proto.last_updated)
            .map(|(key, _)| key.clone());

        if let Some(key) = oldest_key {
            self.prototypes.remove(&key);
        }
    }

    /// Number of prototypes currently stored.
    pub fn len(&self) -> usize {
        self.prototypes.len()
    }

    /// Whether no prototypes are stored.
    pub fn is_empty(&self) -> bool {
        self.prototypes.is_empty()
    }

    /// Serialize all prototypes for persistence.
    pub fn to_serialized(&self) -> Vec<SerializedPrototype> {
        self.prototypes
            .iter()
            .map(|(key, proto)| {
                let (key_type, key_value) = match key {
                    PrototypeKey::Category(s) => ("category".to_string(), s.clone()),
                    PrototypeKey::Topic(s) => ("topic".to_string(), s.clone()),
                };
                SerializedPrototype {
                    key_type,
                    key_value,
                    centroid: proto.centroid.to_vec(),
                    entry_count: proto.entry_count,
                    last_updated: proto.last_updated,
                }
            })
            .collect()
    }

    /// Restore from serialized prototypes.
    pub fn from_serialized(
        serialized: Vec<SerializedPrototype>,
        max_count: usize,
        min_entries: u32,
        pull_strength: f32,
        dimension: usize,
    ) -> Self {
        let mut prototypes = HashMap::new();
        for sp in serialized {
            let key = match sp.key_type.as_str() {
                "category" => PrototypeKey::Category(sp.key_value),
                "topic" => PrototypeKey::Topic(sp.key_value),
                _ => continue,
            };
            if sp.centroid.len() == dimension {
                prototypes.insert(
                    key,
                    Prototype {
                        centroid: Array1::from(sp.centroid),
                        entry_count: sp.entry_count,
                        last_updated: sp.last_updated,
                    },
                );
            }
        }
        Self {
            prototypes,
            max_count,
            min_entries,
            pull_strength,
            dimension,
        }
    }
}

/// Cosine similarity between two f32 slices.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a < 1e-12 || norm_b < 1e-12 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-PRO-01: Construction and empty state
    #[test]
    fn construction_empty() {
        let pm = PrototypeManager::new(256, 3, 0.1, 384);
        assert_eq!(pm.len(), 0);

        let input = vec![0.1_f32; 384];
        let result = pm.apply_pull(&input, None, None);
        assert_eq!(result, input);

        let result2 = pm.apply_pull(&input, Some("cat"), None);
        assert_eq!(result2, input);
    }

    // T-PRO-02: Prototype creation on first update
    #[test]
    fn prototype_creation() {
        let mut pm = PrototypeManager::new(256, 3, 0.1, 4);
        let v = vec![1.0, 0.0, 0.0, 0.0];
        pm.update(&v, Some("decision"), None, 100);
        assert_eq!(pm.len(), 1);
    }

    // T-PRO-03: Running-mean centroid update
    #[test]
    fn running_mean_update() {
        let mut pm = PrototypeManager::new(256, 1, 0.1, 4);
        pm.update(&[1.0, 0.0, 0.0, 0.0], Some("test"), None, 100);
        pm.update(&[0.0, 1.0, 0.0, 0.0], Some("test"), None, 200);
        pm.update(&[0.0, 0.0, 1.0, 0.0], Some("test"), None, 300);

        // After 3 updates: centroid should be [1/3, 1/3, 1/3, 0]
        let key = PrototypeKey::Category("test".to_string());
        let proto = pm.prototypes.get(&key).unwrap();
        assert!((proto.centroid[0] - 1.0 / 3.0).abs() < 1e-5);
        assert!((proto.centroid[1] - 1.0 / 3.0).abs() < 1e-5);
        assert!((proto.centroid[2] - 1.0 / 3.0).abs() < 1e-5);
        assert!((proto.centroid[3] - 0.0).abs() < 1e-5);
        assert_eq!(proto.entry_count, 3);
    }

    // T-PRO-04: Soft pull below minimum entries threshold
    #[test]
    fn pull_below_threshold() {
        let mut pm = PrototypeManager::new(256, 3, 0.1, 4);
        // Only 2 entries, below min_entries=3
        pm.update(&[1.0, 0.0, 0.0, 0.0], Some("test"), None, 100);
        pm.update(&[1.0, 0.0, 0.0, 0.0], Some("test"), None, 200);

        let input = vec![0.5, 0.5, 0.0, 0.0];
        let result = pm.apply_pull(&input, Some("test"), None);
        assert_eq!(result, input, "should not pull below min_entries");
    }

    // T-PRO-05: Soft pull above minimum entries threshold
    #[test]
    fn pull_above_threshold() {
        let mut pm = PrototypeManager::new(256, 3, 0.1, 4);
        // 5 entries, above min_entries=3
        for _ in 0..5 {
            pm.update(&[1.0, 0.0, 0.0, 0.0], Some("test"), None, 100);
        }

        let input = vec![0.8, 0.2, 0.0, 0.0];
        let result = pm.apply_pull(&input, Some("test"), None);
        // Should be pulled toward [1, 0, 0, 0]
        assert!(result[0] > input[0], "first element should increase toward prototype");
        assert!(result[1] < input[1], "second element should decrease toward prototype");
    }

    // T-PRO-06: Prototype stability under rapid updates
    #[test]
    fn stability_rapid_updates() {
        let mut pm = PrototypeManager::new(256, 1, 0.1, 4);
        let v1 = vec![1.0, 0.0, 0.0, 0.0];
        let v2 = vec![0.0, 1.0, 0.0, 0.0];

        for i in 0..100 {
            if i % 2 == 0 {
                pm.update(&v1, Some("test"), None, i);
            } else {
                pm.update(&v2, Some("test"), None, i);
            }
        }

        // Centroid should converge to mean of v1 and v2: [0.5, 0.5, 0, 0]
        let key = PrototypeKey::Category("test".to_string());
        let proto = pm.prototypes.get(&key).unwrap();
        assert!((proto.centroid[0] - 0.5).abs() < 0.1, "centroid[0]={}", proto.centroid[0]);
        assert!((proto.centroid[1] - 0.5).abs() < 0.1, "centroid[1]={}", proto.centroid[1]);
        assert_eq!(proto.entry_count, 100);
    }

    // T-PRO-08: LRU eviction at capacity
    #[test]
    fn lru_eviction() {
        let mut pm = PrototypeManager::new(3, 1, 0.1, 4);
        pm.update(&[1.0, 0.0, 0.0, 0.0], Some("a"), None, 100);
        pm.update(&[0.0, 1.0, 0.0, 0.0], Some("b"), None, 200);
        pm.update(&[0.0, 0.0, 1.0, 0.0], Some("c"), None, 300);
        assert_eq!(pm.len(), 3);

        // Adding "d" should evict "a" (oldest timestamp=100)
        pm.update(&[0.0, 0.0, 0.0, 1.0], Some("d"), None, 400);
        assert_eq!(pm.len(), 3);
        assert!(!pm.prototypes.contains_key(&PrototypeKey::Category("a".to_string())));
        assert!(pm.prototypes.contains_key(&PrototypeKey::Category("b".to_string())));
        assert!(pm.prototypes.contains_key(&PrototypeKey::Category("c".to_string())));
        assert!(pm.prototypes.contains_key(&PrototypeKey::Category("d".to_string())));
    }

    // T-PRO-09: Category and topic prototypes independent
    #[test]
    fn category_topic_independent() {
        let mut pm = PrototypeManager::new(256, 1, 0.1, 4);
        pm.update(&[1.0, 0.0, 0.0, 0.0], None, Some("arch"), 100);
        pm.update(&[0.0, 1.0, 0.0, 0.0], Some("decision"), None, 100);

        assert_eq!(pm.len(), 2);
        let cat_key = PrototypeKey::Category("decision".to_string());
        let top_key = PrototypeKey::Topic("arch".to_string());
        assert!(pm.prototypes.contains_key(&cat_key));
        assert!(pm.prototypes.contains_key(&top_key));

        // Centroids should be different
        let cat_centroid = &pm.prototypes[&cat_key].centroid;
        let top_centroid = &pm.prototypes[&top_key].centroid;
        assert_ne!(cat_centroid.as_slice().unwrap(), top_centroid.as_slice().unwrap());
    }

    // T-PRO-11: Serialization round-trip
    #[test]
    fn serialization_roundtrip() {
        let mut pm = PrototypeManager::new(256, 3, 0.1, 4);
        for i in 0..5 {
            pm.update(&[1.0, 0.0, 0.0, 0.0], Some(&format!("cat{i}")), None, i as u64 * 100);
            pm.update(&[0.0, 1.0, 0.0, 0.0], None, Some(&format!("top{i}")), i as u64 * 100);
        }

        let serialized = pm.to_serialized();
        let restored = PrototypeManager::from_serialized(serialized, 256, 3, 0.1, 4);

        assert_eq!(restored.len(), pm.len());
    }

    // T-PRO-12: apply_pull with None category and None topic
    #[test]
    fn pull_with_no_keys() {
        let mut pm = PrototypeManager::new(256, 1, 0.1, 4);
        for _ in 0..5 {
            pm.update(&[1.0, 0.0, 0.0, 0.0], Some("test"), None, 100);
        }

        let input = vec![0.5, 0.5, 0.0, 0.0];
        let result = pm.apply_pull(&input, None, None);
        assert_eq!(result, input, "should not pull without category/topic");
    }
}
