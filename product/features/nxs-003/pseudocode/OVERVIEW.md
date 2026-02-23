# nxs-003: Embedding Pipeline -- Pseudocode Overview

## Component Interaction Map

```
                  lib.rs (C11)
                    |
        re-exports all public types
                    |
    +-------+-------+-------+-------+-------+
    |       |       |       |       |       |
  config  error  provider  model   text   test-helpers
  (C2)    (C1)   (C7)      (C3)   (C6)    (C10)
    |       |       |       |       |       |
    |       |  EmbeddingProvider   |   MockProvider
    |       |   trait    |  |      |   cosine_sim
    |       |       |    |  |      |   assertions
    |       |       |    +--+------+
    |       |       |    |
    |       +---+---+----+
    |           |
    |     onnx (C9)
    |     OnnxProvider
    |     Session + Tokenizer
    |           |
    |     +-----+-----+
    |     |           |
    |  pooling     normalize
    |  (C5)        (C4)
    |  mean_pool   l2_normalize
    |              l2_normalized
    |
    +------+
           |
     download (C8)
     ensure_model
     hf-hub + dirs
```

## Data Flow: Single Embed

```
caller.embed(text)
  |
  +--> tokenizer.encode(text, add_special_tokens=true)
  |      -> Encoding { input_ids, attention_mask, token_type_ids }
  |
  +--> build tensors: input_ids[1, seq_len], attention_mask[1, seq_len], token_type_ids[1, seq_len]
  |
  +--> LOCK session (Mutex)
  |      session.run(inputs)
  |        -> output tensor [1, seq_len, 384]
  |    UNLOCK session
  |
  +--> mean_pool(output, attention_mask, 1, seq_len, 384)
  |      -> Vec<Vec<f32>> with one entry [384]
  |
  +--> l2_normalize(&mut embedding)
  |      -> [384] unit norm
  |
  +--> return Ok(embedding)
```

## Data Flow: Batch Embed

```
caller.embed_batch(texts)
  |
  +--> chunk texts into groups of config.batch_size
  |
  +--> for each chunk:
  |      |
  |      +--> tokenizer.encode_batch(chunk, add_special_tokens=true)
  |      |      -> Vec<Encoding>
  |      |      -> flatten to: input_ids[batch, seq_len], attention_mask[batch, seq_len]
  |      |
  |      +--> build tensors: [batch_size, max_seq_len]
  |      |
  |      +--> LOCK session
  |      |      session.run(inputs)
  |      |        -> output [batch_size, seq_len, 384]
  |      |    UNLOCK session
  |      |
  |      +--> mean_pool(output, attention_mask, batch_size, seq_len, 384)
  |      |      -> Vec<Vec<f32>> with batch_size entries
  |      |
  |      +--> for each embedding: l2_normalize(&mut embedding)
  |      |
  |      +--> collect into results
  |
  +--> return Ok(all_embeddings)
```

## Data Flow: Convenience Functions

```
embed_entry(provider, title, content)
  |
  +--> text = prepare_text(title, content, ": ")
  +--> provider.embed(&text)
  +--> return result

embed_entries(provider, entries)
  |
  +--> texts: Vec<String> = entries.map(|(t,c)| prepare_text(t, c, ": "))
  +--> refs: Vec<&str> = texts.iter().map(|s| s.as_str())
  +--> provider.embed_batch(&refs)
  +--> return result
```

## Shared Types

All components use these shared types from `error.rs`:
- `EmbedError` -- error enum with thiserror 2.0 derives
- `Result<T>` -- alias for `std::result::Result<T, EmbedError>`

Key data types from other modules:
- `EmbeddingModel` (model.rs) -- enum with 7 variants + metadata methods
- `EmbedConfig` (config.rs) -- configuration struct with defaults
- `EmbeddingProvider` (provider.rs) -- trait for embedding abstraction
- `OnnxProvider` (onnx.rs) -- concrete implementation

## Implementation Order

1. C1: error -- no deps; everything else uses Result/EmbedError
2. C2: config + C3: model -- depend on error only
3. C4: normalize -- standalone arithmetic
4. C5: pooling -- standalone arithmetic with attention mask
5. C7: provider -- trait definition; depends on error
6. C6: text -- depends on provider trait (interface only)
7. C8: download -- depends on model, config, error; uses hf-hub + dirs
8. C9: onnx -- depends on all above
9. C10: test-helpers -- depends on provider, normalize; gated behind test-support
10. C11: lib -- crate root; re-exports
