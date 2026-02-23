# C6: Persistence Module -- Test Plan

## AC-09: Dump Produces Index Files

```
test_dump_creates_files:
    vi = TestVectorIndex::new()
    ids = seed_vectors(vi.vi(), vi.store(), 50)
    dump_dir = vi.dir().join("index")
    vi.vi().dump(&dump_dir).unwrap()

    // Verify files exist
    ASSERT dump_dir.join("unimatrix.hnsw.graph").exists()
    ASSERT dump_dir.join("unimatrix.hnsw.data").exists()
    ASSERT dump_dir.join("unimatrix-vector.meta").exists()

test_dump_metadata_content:
    vi = TestVectorIndex::new()
    seed_vectors(vi.vi(), vi.store(), 10)
    dump_dir = vi.dir().join("index")
    vi.vi().dump(&dump_dir).unwrap()

    meta = fs::read_to_string(dump_dir.join("unimatrix-vector.meta")).unwrap()
    ASSERT meta.contains("basename=unimatrix")
    ASSERT meta.contains("point_count=10")
    ASSERT meta.contains("dimension=384")
    ASSERT meta.contains("next_data_id=10")

test_dump_empty_index:
    vi = TestVectorIndex::new()
    dump_dir = vi.dir().join("index")
    vi.vi().dump(&dump_dir).unwrap()
    ASSERT dump_dir.join("unimatrix-vector.meta").exists()
```

## AC-10: Load Restores Index

```
test_load_round_trip:
    vi = TestVectorIndex::new()
    // Insert and record search results
    ids = seed_vectors(vi.vi(), vi.store(), 50)

    // Run 5 search queries and save results
    queries = (0..5).map(|_| random_normalized_embedding(384)).collect()
    original_results = queries.iter()
        .map(|q| vi.vi().search(q, 10, 32).unwrap())
        .collect()

    // Dump
    dump_dir = vi.dir().join("index")
    vi.vi().dump(&dump_dir).unwrap()

    // Load into new VectorIndex
    loaded = VectorIndex::load(vi.store().clone(), VectorConfig::default(), &dump_dir).unwrap()

    // Verify same search results
    for (query, original) in queries.iter().zip(original_results.iter()):
        loaded_results = loaded.search(query, 10, 32).unwrap()
        ASSERT loaded_results.len() == original.len()
        for (o, l) in original.iter().zip(loaded_results.iter()):
            ASSERT o.entry_id == l.entry_id
            ASSERT (o.similarity - l.similarity).abs() < 0.01

test_load_point_count_matches:
    vi = TestVectorIndex::new()
    seed_vectors(vi.vi(), vi.store(), 100)
    original_count = vi.vi().point_count()

    dump_dir = vi.dir().join("index")
    vi.vi().dump(&dump_dir).unwrap()

    loaded = VectorIndex::load(vi.store().clone(), VectorConfig::default(), &dump_dir).unwrap()
    ASSERT loaded.point_count() == original_count

test_load_idmap_consistent:
    vi = TestVectorIndex::new()
    ids = seed_vectors(vi.vi(), vi.store(), 100)
    dump_dir = vi.dir().join("index")
    vi.vi().dump(&dump_dir).unwrap()

    loaded = VectorIndex::load(vi.store().clone(), VectorConfig::default(), &dump_dir).unwrap()

    for id in ids:
        ASSERT loaded.contains(id)
        // Verify IdMap matches VECTOR_MAP
        ASSERT vi.store().get_vector_mapping(id).unwrap().is_some()
```

## R-04: Persistence Round-Trip (additional scenarios)

```
test_load_missing_graph_file:
    vi = TestVectorIndex::new()
    seed_vectors(vi.vi(), vi.store(), 10)
    dump_dir = vi.dir().join("index")
    vi.vi().dump(&dump_dir).unwrap()

    // Delete graph file
    fs::remove_file(dump_dir.join("unimatrix.hnsw.graph")).unwrap()

    result = VectorIndex::load(vi.store().clone(), VectorConfig::default(), &dump_dir)
    ASSERT matches!(result, Err(VectorError::Persistence(_)))

test_load_missing_data_file:
    vi = TestVectorIndex::new()
    seed_vectors(vi.vi(), vi.store(), 10)
    dump_dir = vi.dir().join("index")
    vi.vi().dump(&dump_dir).unwrap()

    fs::remove_file(dump_dir.join("unimatrix.hnsw.data")).unwrap()

    result = VectorIndex::load(vi.store().clone(), VectorConfig::default(), &dump_dir)
    ASSERT matches!(result, Err(VectorError::Persistence(_)))

test_load_missing_meta_file:
    vi = TestVectorIndex::new()
    seed_vectors(vi.vi(), vi.store(), 10)
    dump_dir = vi.dir().join("index")
    vi.vi().dump(&dump_dir).unwrap()

    fs::remove_file(dump_dir.join("unimatrix-vector.meta")).unwrap()

    result = VectorIndex::load(vi.store().clone(), VectorConfig::default(), &dump_dir)
    ASSERT matches!(result, Err(VectorError::Persistence(_)))

test_load_empty_directory:
    vi = TestVectorIndex::new()
    dump_dir = vi.dir().join("empty_index")
    fs::create_dir_all(&dump_dir).unwrap()

    result = VectorIndex::load(vi.store().clone(), VectorConfig::default(), &dump_dir)
    ASSERT matches!(result, Err(VectorError::Persistence(_)))

test_load_nonexistent_directory:
    vi = TestVectorIndex::new()
    dump_dir = vi.dir().join("does_not_exist")

    result = VectorIndex::load(vi.store().clone(), VectorConfig::default(), &dump_dir)
    ASSERT matches!(result, Err(VectorError::Persistence(_)))

test_multi_cycle_dump_load:
    vi = TestVectorIndex::new()
    // Cycle 1: insert, dump, load
    seed_vectors(vi.vi(), vi.store(), 10)
    dump_dir = vi.dir().join("index")
    vi.vi().dump(&dump_dir).unwrap()
    loaded = VectorIndex::load(vi.store().clone(), VectorConfig::default(), &dump_dir).unwrap()

    // Cycle 2: insert more into loaded index, dump again
    // Note: need store entries for new vectors
    for i in 0..10:
        entry = TestEntry::new("test", "cycle2").with_title(&format!("Cycle2 {i}")).build()
        eid = vi.store().insert(entry).unwrap()
        loaded.insert(eid, &random_normalized_embedding(384)).unwrap()

    loaded.dump(&dump_dir).unwrap()
    loaded2 = VectorIndex::load(vi.store().clone(), VectorConfig::default(), &dump_dir).unwrap()

    ASSERT loaded2.point_count() == 20

test_load_dimension_mismatch:
    vi = TestVectorIndex::new()
    seed_vectors(vi.vi(), vi.store(), 5)
    dump_dir = vi.dir().join("index")
    vi.vi().dump(&dump_dir).unwrap()

    // Load with wrong dimension config
    wrong_config = VectorConfig { dimension: 768, ..VectorConfig::default() }
    result = VectorIndex::load(vi.store().clone(), wrong_config, &dump_dir)
    ASSERT matches!(result, Err(VectorError::Persistence(_)))
```

## AC-18: IdMap Consistent After Insert, Dump, Load

```
test_idmap_consistency_full_lifecycle:
    vi = TestVectorIndex::new()
    ids = seed_vectors(vi.vi(), vi.store(), 100)

    // Verify before dump
    for id in ids:
        ASSERT vi.vi().contains(id)
        ASSERT vi.store().get_vector_mapping(id).unwrap().is_some()

    // Dump and load
    dump_dir = vi.dir().join("index")
    vi.vi().dump(&dump_dir).unwrap()
    loaded = VectorIndex::load(vi.store().clone(), VectorConfig::default(), &dump_dir).unwrap()

    // Verify after load
    for id in ids:
        ASSERT loaded.contains(id)

    // Re-embed 10 entries
    for i in 0..10:
        loaded.insert(ids[i], &random_normalized_embedding(384)).unwrap()

    // Verify after re-embed
    for id in ids:
        ASSERT loaded.contains(id)
```

## IR-03: New Index with Existing VECTOR_MAP

```
test_new_index_with_existing_vector_map:
    vi = TestVectorIndex::new()
    seed_vectors(vi.vi(), vi.store(), 10)
    // VECTOR_MAP has 10 entries

    // Create a brand new index with the same store
    new_vi = VectorIndex::new(vi.store().clone(), VectorConfig::default()).unwrap()
    ASSERT new_vi.point_count() == 0   // new index is empty
    ASSERT new_vi.contains(1) == false  // not aware of old mappings

    // But VECTOR_MAP still has old entries
    ASSERT vi.store().get_vector_mapping(1).unwrap().is_some()
```

## Risks Covered

| Risk | Test Count | Coverage |
|------|-----------|----------|
| R-04 (Persistence) | 10 | Round-trip, missing files, multi-cycle, dim mismatch |
| R-01 (IdMap desync) | 2 | Consistency after load, full lifecycle |
| R-11 (Load failures) | 5 | Missing graph/data/meta, empty dir, nonexistent dir |
| IR-03 (Lifecycle) | 1 | New index with existing VECTOR_MAP |
