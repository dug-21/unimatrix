# Pseudocode: server-integration

## Changes to UnimatrixServer

```
// server.rs -- add field
pub struct UnimatrixServer {
    // ... existing fields ...
    pub(crate) adapt_service: Arc<AdaptationService>,
}

// server.rs -- constructor
impl UnimatrixServer {
    pub fn new(
        // ... existing params ...
        adapt_service: Arc<AdaptationService>,
    ) -> Self:
        // ... existing init ...
        // Store adapt_service
}
```

## Write Path: context_store (tools.rs)

```
// In context_store handler, AFTER embedding, BEFORE vector insert:

// Existing step 7: Embed entry
let raw_embedding = spawn_blocking(|| adapter.embed_entry(title, content)).await?

// NEW step 7b: Adapt embedding
let category = params.category.clone()
let topic = params.topic.clone()
let adapt = Arc::clone(&self.adapt_service)
let adapted = spawn_blocking(move || {
    let mut adapted = adapt.adapt_embedding(&raw_embedding, Some(&category), Some(&topic))
    unimatrix_embed::normalize::l2_normalize(&mut adapted)
    adapted
}).await?

// Existing step 8: Insert into vector index (use adapted instead of raw)
self.vector_store.insert(entry_id, &adapted).await?
```

## Read Path: context_search (tools.rs)

```
// In context_search handler, AFTER embedding query, BEFORE vector search:

// Existing step 7: Embed query
let raw_embedding = spawn_blocking(|| adapter.embed_entry("", &query)).await?

// NEW step 7b: Adapt query embedding
let adapt = Arc::clone(&self.adapt_service)
let adapted_query = spawn_blocking(move || {
    let mut adapted = adapt.adapt_embedding(&raw_embedding, None, None)
    unimatrix_embed::normalize::l2_normalize(&mut adapted)
    adapted
}).await?

// Existing step 8: Search (use adapted_query instead of raw)
let search_results = self.vector_store.search(adapted_query, k, EF_SEARCH).await?
```

## Training Path: co-access recording

```
// In the co-access recording section (after search results are returned):
// This is the fire-and-forget block that already exists for usage recording

// Existing: record co-access pairs
let pairs = coaccess::generate_pairs(&result_ids, MAX_ENTRIES)
spawn_blocking(move || store.record_co_access_pairs(&pairs))

// NEW: feed training reservoir and attempt training step
let adapt = Arc::clone(&self.adapt_service)
let embed = Arc::clone(&self.embed_service)
let store_for_train = Arc::clone(&self.store)
tokio::task::spawn_blocking(move || {
    // Convert pairs to (id_a, id_b, count) format
    // Since these are new pairs, count=1
    let train_pairs: Vec<(u64, u64, u32)> = pairs.iter()
        .map(|&(a, b)| (a, b, 1))
        .collect();

    adapt.record_training_pairs(&train_pairs);

    // Try training step with embed callback
    adapt.try_train_step(&|entry_id| {
        // Look up entry and re-embed
        let entry = store_for_train.get(entry_id).ok()?;
        let adapter = embed.try_get_adapter()?;
        adapter.embed_entry(&entry.title, &entry.content).ok()
    });

    // Debounced save
    if adapt.should_save():
        let data_dir = store_for_train.data_dir();
        let _ = adapt.save_state(data_dir);
        adapt.reset_save_counter();
});
```

## Startup: Load Adaptation State

```
// In main.rs or server initialization, AFTER HNSW load:

let adapt_config = AdaptConfig::default();
let adapt_service = Arc::new(AdaptationService::new(adapt_config));

// Load persisted adaptation state (alongside HNSW load)
if let Err(e) = adapt_service.load_state(&data_dir):
    tracing::warn!("Failed to load adaptation state: {e}");
    // Continues with fresh identity state
```

## Shutdown: Save Adaptation State

```
// In shutdown handler, alongside HNSW dump:

// Existing: vector_index.dump(&data_dir)?
// Existing: store.compact()?

// NEW: save adaptation state
if let Err(e) = adapt_service.save_state(&data_dir):
    tracing::warn!("Failed to save adaptation state: {e}");
```

## Coherence Gate: Embedding Consistency Check

```
// In context_status handler, when check_embeddings=true:
// The existing code re-embeds entries and compares to stored embeddings.
// With adaptation, re-embedding must also apply adaptation.

// EXISTING flow (in contradiction.rs or tools.rs):
// for entry in sample:
//     let re_embedded = embed_service.embed_entry(&entry.title, &entry.content)?
//     let stored_embedding = get_stored_embedding(entry.id)?
//     if cosine_similarity(re_embedded, stored_embedding) < 0.99:
//         inconsistent_count += 1

// UPDATED flow:
// for entry in sample:
//     let raw = embed_service.embed_entry(&entry.title, &entry.content)?
//     let mut adapted = adapt_service.adapt_embedding(&raw, Some(&entry.category), Some(&entry.topic))
//     l2_normalize(&mut adapted)
//     let stored_embedding = get_stored_embedding(entry.id)?
//     if cosine_similarity(&adapted, &stored_embedding) < 0.99:
//         inconsistent_count += 1
```

## context_correct Path

```
// context_correct creates a new entry with new content.
// The embedding path is the same as context_store:
// 1. Embed new content (raw ONNX)
// 2. Adapt embedding
// 3. L2 normalize
// 4. Insert into HNSW
// No special handling needed -- the existing correction flow
// calls the same embed+insert path as store.
```
