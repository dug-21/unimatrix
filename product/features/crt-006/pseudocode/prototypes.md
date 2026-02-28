# Pseudocode: prototypes (Domain Prototypes)

## Structs

```
enum PrototypeKey {
    Category(String),
    Topic(String),
}

struct Prototype {
    key: PrototypeKey,
    centroid: Array1<f32>,     // Running mean, dimension d
    entry_count: u32,
    last_updated: u64,         // Unix timestamp
}

struct PrototypeManager {
    prototypes: HashMap<PrototypeKey, Prototype>,
    max_count: usize,          // default 256
    min_entries: u32,           // default 3
    pull_strength: f32,        // default 0.1
    dimension: usize,          // 384
}
```

## Construction

```
fn PrototypeManager::new(max_count: usize, min_entries: u32, pull_strength: f32, dimension: usize) -> Self:
    return PrototypeManager {
        prototypes: HashMap::new(),
        max_count,
        min_entries,
        pull_strength,
        dimension,
    }
```

## Soft Pull

```
fn PrototypeManager::apply_pull(&self, adapted: &[f32], category: Option<&str>, topic: Option<&str>) -> Vec<f32>:
    // Find best matching prototype
    let mut best_proto = None
    let mut best_sim = f32::NEG_INFINITY

    // Check category prototype
    if let Some(cat) = category:
        let key = PrototypeKey::Category(cat.to_string())
        if let Some(proto) = self.prototypes.get(&key):
            if proto.entry_count >= self.min_entries:
                let sim = cosine_similarity(adapted, proto.centroid.as_slice().unwrap())
                if sim > best_sim:
                    best_sim = sim
                    best_proto = Some(proto)

    // Check topic prototype
    if let Some(top) = topic:
        let key = PrototypeKey::Topic(top.to_string())
        if let Some(proto) = self.prototypes.get(&key):
            if proto.entry_count >= self.min_entries:
                let sim = cosine_similarity(adapted, proto.centroid.as_slice().unwrap())
                if sim > best_sim:
                    best_sim = sim
                    best_proto = Some(proto)

    // Apply pull if we found a qualifying prototype
    match best_proto:
        Some(proto):
            let alpha = self.pull_strength * best_sim.max(0.0)
            let mut result = adapted.to_vec()
            for i in 0..self.dimension:
                result[i] += alpha * (proto.centroid[i] - adapted[i])
            return result
        None:
            return adapted.to_vec()
```

## Update Centroid (Online Running Mean)

```
fn PrototypeManager::update(&mut self, adapted: &[f32], category: Option<&str>, topic: Option<&str>, now: u64):
    // Update category prototype
    if let Some(cat) = category:
        let key = PrototypeKey::Category(cat.to_string())
        self.update_single(key, adapted, now)

    // Update topic prototype
    if let Some(top) = topic:
        let key = PrototypeKey::Topic(top.to_string())
        self.update_single(key, adapted, now)

fn PrototypeManager::update_single(&mut self, key: PrototypeKey, adapted: &[f32], now: u64):
    if let Some(proto) = self.prototypes.get_mut(&key):
        // Online running mean: new_centroid = (old_centroid * n + new_value) / (n + 1)
        let n = proto.entry_count as f32
        for i in 0..self.dimension:
            proto.centroid[i] = (proto.centroid[i] * n + adapted[i]) / (n + 1.0)
        proto.entry_count += 1
        proto.last_updated = now
    else:
        // Evict if at capacity
        if self.prototypes.len() >= self.max_count:
            self.evict_lru()

        // Create new prototype
        self.prototypes.insert(key, Prototype {
            key: key.clone(),
            centroid: Array1::from(adapted.to_vec()),
            entry_count: 1,
            last_updated: now,
        })
```

## LRU Eviction

```
fn PrototypeManager::evict_lru(&mut self):
    // Find the prototype with the oldest last_updated timestamp
    let oldest_key = self.prototypes.iter()
        .min_by_key(|(_, proto)| proto.last_updated)
        .map(|(key, _)| key.clone())

    if let Some(key) = oldest_key:
        self.prototypes.remove(&key)
```

## Utility

```
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32:
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt()
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt()
    if norm_a < 1e-12 || norm_b < 1e-12:
        return 0.0
    return dot / (norm_a * norm_b)
```

## Serialization

```
struct SerializedPrototype {
    key_type: String,    // "category" or "topic"
    key_value: String,
    centroid: Vec<f32>,
    entry_count: u32,
    last_updated: u64,
}

fn PrototypeManager::to_serialized(&self) -> Vec<SerializedPrototype>:
    self.prototypes.values().map(|p| SerializedPrototype { ... }).collect()

fn PrototypeManager::from_serialized(serialized: Vec<SerializedPrototype>, config: ...) -> Self:
    // Reconstruct from deserialized data
```
