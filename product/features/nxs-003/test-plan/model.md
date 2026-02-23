# C3: Model Module -- Test Plan

## Tests

```
test_default_model:
    model = EmbeddingModel::default()
    ASSERT model == EmbeddingModel::AllMiniLmL6V2

test_all_models_have_model_id:
    FOR model IN [AllMiniLmL6V2, AllMiniLmL12V2, MultiQaMiniLmL6,
                  ParaphraseMiniLmL6V2, BgeSmallEnV15, E5SmallV2, GteSmall]:
        ASSERT NOT model.model_id().is_empty()
        ASSERT model.model_id().contains('/')  // HF format: org/name

test_all_models_dimension_384:
    FOR model IN all_variants:
        ASSERT model.dimension() == 384

test_max_seq_length_values:
    ASSERT AllMiniLmL6V2.max_seq_length() == 256
    ASSERT AllMiniLmL12V2.max_seq_length() == 256
    ASSERT MultiQaMiniLmL6.max_seq_length() == 256
    ASSERT ParaphraseMiniLmL6V2.max_seq_length() == 256
    ASSERT BgeSmallEnV15.max_seq_length() == 512
    ASSERT E5SmallV2.max_seq_length() == 512
    ASSERT GteSmall.max_seq_length() == 512

test_onnx_filename:
    FOR model IN all_variants:
        ASSERT model.onnx_filename() == "model.onnx"

test_cache_subdir_sanitization:
    model = EmbeddingModel::AllMiniLmL6V2
    subdir = model.cache_subdir()
    ASSERT subdir == "sentence-transformers_all-MiniLM-L6-v2"
    ASSERT NOT subdir.contains('/')

    model = EmbeddingModel::BgeSmallEnV15
    subdir = model.cache_subdir()
    ASSERT subdir == "BAAI_bge-small-en-v1.5"

test_model_id_known_values:
    ASSERT AllMiniLmL6V2.model_id() == "sentence-transformers/all-MiniLM-L6-v2"
    ASSERT AllMiniLmL12V2.model_id() == "sentence-transformers/all-MiniLM-L12-v2"
    ASSERT MultiQaMiniLmL6.model_id() == "sentence-transformers/multi-qa-MiniLM-L6-cos-v1"
    ASSERT ParaphraseMiniLmL6V2.model_id() == "sentence-transformers/paraphrase-MiniLM-L6-v2"
    ASSERT BgeSmallEnV15.model_id() == "BAAI/bge-small-en-v1.5"
    ASSERT E5SmallV2.model_id() == "intfloat/e5-small-v2"
    ASSERT GteSmall.model_id() == "thenlper/gte-small"

test_model_enum_derives:
    model = EmbeddingModel::AllMiniLmL6V2
    cloned = model.clone()
    ASSERT model == cloned
    debug = format!("{:?}", model)
    ASSERT debug contains "AllMiniLmL6V2"
    copied = model  // Copy
    ASSERT copied == model

test_seven_variants_exist:
    variants = [AllMiniLmL6V2, AllMiniLmL12V2, MultiQaMiniLmL6,
                ParaphraseMiniLmL6V2, BgeSmallEnV15, E5SmallV2, GteSmall]
    ASSERT variants.len() == 7
```

## Risks Covered

- R-13: Model catalog dimension mismatch -- all 7 variants verified as 384-d.
- AC-17: All models have HF ID, dimension, max seq length, ONNX filename.
- AC-16: dimension() returns 384 for all models.
