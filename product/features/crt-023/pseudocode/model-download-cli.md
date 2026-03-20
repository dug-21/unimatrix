# Model Download CLI Extension — Pseudocode

**File**: `crates/unimatrix-server/src/main.rs` (modified — `ModelDownload` command and handler)

**Purpose**: Extend the existing `unimatrix model-download` subcommand to support NLI model
download via `--nli [--nli-model minilm2|deberta]`. After download, compute and print the
SHA-256 hash of the ONNX model file so the operator can pin it in `config.toml` under
`nli_model_sha256`. Follows the existing `ensure_model` pattern via `hf-hub` (FR-07, AC-16).

**Critical constraint**: This is a synchronous CLI path (pre-tokio, like the current
`model-download` command). No tokio runtime is initialized. Progress messages go to stderr;
hash output goes to stdout so the operator can capture it.

---

## Clap Command Extension

The existing `ModelDownload` command has no subfields. Extend it to accept optional flags:

```
// Modified Command enum variant (in main.rs):
/// Download the ONNX model to cache.
///
/// With no flags: downloads the embedding model (existing behavior).
/// With --nli: downloads the NLI cross-encoder model.
/// With --nli --nli-model <name>: downloads the specified NLI model variant.
/// Outputs the SHA-256 hash of the downloaded ONNX file to stdout so the
/// operator can pin it in config.toml under nli_model_sha256.
ModelDownload {
    /// Download the NLI cross-encoder model instead of (or in addition to) the embedding model.
    /// When absent: download embedding model only (unchanged existing behavior).
    #[arg(long)]
    nli: bool,

    /// NLI model variant to download. Valid values: "minilm2", "deberta".
    /// Only valid with --nli. Defaults to "minilm2" when --nli is given without --nli-model.
    #[arg(long, requires = "nli")]
    nli_model: Option<String>,
},
```

**Backward compatibility**: The existing `ModelDownload` was a unit variant (no args). Changing
it to a struct variant with optional fields preserves backward compatibility — `unimatrix
model-download` with no flags continues to download the embedding model (nli=false).

The existing dispatch in `main()` changes from:
```
Some(Command::ModelDownload) => { return handle_model_download(); }
```
To:
```
Some(Command::ModelDownload { nli, nli_model }) => {
    return handle_model_download(nli, nli_model);
}
```

---

## `handle_model_download` Extension

The existing function downloads only the embedding model. Extended to optionally download the
NLI model.

```
/// Download ONNX model(s) to cache.
///
/// Synchronous path — NO tokio runtime. Uses ensure_model / ensure_nli_model
/// which both call hf-hub's synchronous API internally.
///
/// When nli=false (default): downloads embedding model only (unchanged behavior).
/// When nli=true: downloads embedding model AND the specified NLI model, then
///   computes and prints the SHA-256 hash of the NLI ONNX file to stdout.
///
/// Progress messages → stderr (MCP protocol uses stdout; must stay clean).
/// SHA-256 hash output → stdout (operator captures via pipe or copy-paste).
fn handle_model_download(
    nli: bool,
    nli_model_name: Option<String>,
) -> Result<(), Box<dyn std::error::Error>>

    // Step 1: Download embedding model (unchanged existing behavior).
    let embed_config = EmbedConfig::default()
    let cache_dir = embed_config.resolve_cache_dir()

    eprintln!("Downloading ONNX embedding model to {}...", cache_dir.display())
    match unimatrix_embed::ensure_model(embed_config.model, &cache_dir):
        Ok(model_dir) -> eprintln!("Embedding model ready: {}", model_dir.display())
        Err(e) ->
            eprintln!("Embedding model download failed: {e}")
            return Err(Box::new(e))

    // Step 2: If --nli flag not given, return here (existing behavior preserved).
    if !nli:
        return Ok(())

    // Step 3: Resolve the NLI model variant.
    let nli_model: NliModel = match nli_model_name.as_deref():
        None | Some("minilm2") -> NliModel::NliMiniLM2L6H768   // default for --nli
        Some("deberta")        -> NliModel::NliDebertaV3Small
        Some(unknown) ->
            eprintln!("Error: unrecognized --nli-model value '{}'; valid: minilm2, deberta", unknown)
            return Err(format!("unrecognized nli-model: {}", unknown).into())

    // Step 4: Download the NLI model via ensure_nli_model (mirrors ensure_model pattern).
    eprintln!(
        "Downloading NLI model '{}' to {}...",
        nli_model.model_id(),
        cache_dir.display()
    )

    let model_dir = match unimatrix_embed::ensure_nli_model(nli_model, &cache_dir):
        Ok(dir) ->
            eprintln!("NLI model ready: {}", dir.display())
            dir
        Err(e) ->
            eprintln!("NLI model download failed: {e}")
            return Err(Box::new(e))

    // Step 5: Compute SHA-256 hash of the ONNX file.
    let onnx_path = model_dir.join(nli_model.onnx_filename())
    eprintln!("Computing SHA-256 hash of {}...", onnx_path.display())

    let hash_hex = compute_file_sha256(&onnx_path)?

    // Step 6: Print hash to stdout (operator copies to config.toml).
    // Format: one line, lowercase hex, ready to paste.
    println!("{}", hash_hex)

    // Step 7: Print guidance to stderr (not captured by operator's pipe).
    eprintln!("")
    eprintln!("Add the following to your config.toml under [inference]:")
    eprintln!("  nli_model_sha256 = \"{}\"", hash_hex)

    Ok(())
```

---

## `compute_file_sha256` (private helper in main.rs)

```
/// Compute the SHA-256 hash of a file and return as a lowercase hex string (64 chars).
///
/// Reads the file in chunks to avoid loading the entire ~85MB model into memory at once.
fn compute_file_sha256(path: &Path) -> Result<String, Box<dyn std::error::Error>>
    use sha2::{Sha256, Digest}
    use std::io::Read

    let file = std::fs::File::open(path)
        .map_err(|e| format!("failed to open {}: {e}", path.display()))?

    let mut reader = std::io::BufReader::new(file)
    let mut hasher = Sha256::new()

    // Read in 64KB chunks — avoids loading ~85MB model into memory all at once.
    let mut buf = [0u8; 65536]
    loop:
        let n = reader.read(&mut buf)
            .map_err(|e| format!("failed to read {}: {e}", path.display()))?
        if n == 0: break
        hasher.update(&buf[..n])

    let result = hasher.finalize()
    Ok(format!("{:x}", result))  // lowercase hex, 64 chars for SHA-256
```

---

## `ensure_nli_model` (new function in `unimatrix-embed/src/download.rs`)

Follows `ensure_model` exactly — same structure, same `hf-hub` pattern:

```
/// Ensure NLI model files (ONNX model + tokenizer) exist in the cache directory.
///
/// Downloads from HuggingFace Hub via `hf-hub` if not already cached.
/// Returns the path to the model directory containing the ONNX model and tokenizer.
///
/// Pattern: identical to ensure_model; NliModel methods mirror EmbeddingModel methods.
pub fn ensure_nli_model(model: NliModel, cache_dir: &Path) -> Result<PathBuf>
    let model_dir = cache_dir.join(model.cache_subdir())
    let onnx_path = model_dir.join(model.onnx_filename())
    let tokenizer_path = model_dir.join("tokenizer.json")

    // Check if both files already exist and are non-empty.
    if onnx_path.exists()
        && tokenizer_path.exists()
        && file_size(&onnx_path) > 0
        && file_size(&tokenizer_path) > 0:
        return Ok(model_dir)

    // Create cache directory if needed.
    fs::create_dir_all(&model_dir)?

    // Download via hf-hub (synchronous API).
    let api = hf_hub::api::sync::Api::new()
        .map_err(|e| EmbedError::Download(format!("failed to create HF Hub API: {e}")))?

    let repo = api.model(model.model_id().to_string())

    // Download ONNX model file.
    let downloaded_onnx = repo.get(model.onnx_repo_path())
        .map_err(|e| EmbedError::Download(
            format!("failed to download NLI model {}: {e}", model.onnx_repo_path())
        ))?

    // Download tokenizer.
    let downloaded_tokenizer = repo.get("tokenizer.json")
        .map_err(|e| EmbedError::Download(
            format!("failed to download NLI tokenizer.json: {e}")
        ))?

    // Copy from hf-hub cache to our cache directory if paths differ.
    if downloaded_onnx != onnx_path:
        fs::copy(&downloaded_onnx, &onnx_path)?
    if downloaded_tokenizer != tokenizer_path:
        fs::copy(&downloaded_tokenizer, &tokenizer_path)?

    // Validate both files exist and are non-empty.
    if !onnx_path.exists() || file_size(&onnx_path) == 0:
        return Err(EmbedError::ModelNotFound { path: onnx_path })
    if !tokenizer_path.exists() || file_size(&tokenizer_path) == 0:
        return Err(EmbedError::ModelNotFound { path: tokenizer_path })

    Ok(model_dir)
```

---

## `unimatrix-embed/src/lib.rs` Extension

Export `ensure_nli_model` from `unimatrix-embed` (mirrors the existing `ensure_model` export):

```
// Add to unimatrix-embed/src/lib.rs:
pub use download::ensure_nli_model;
```

---

## `Cargo.toml` Dependency

The `sha2` crate is needed for SHA-256 computation. Add to `crates/unimatrix-server/Cargo.toml`:

```toml
# crt-023: SHA-256 hash computation for NLI model integrity verification.
# Used in model-download CLI (AC-16) and NliServiceHandle (NFR-09).
sha2 = "0.10"
```

Note: `sha2 = "0.10"` is already in `unimatrix-embed/Cargo.toml` if `NliServiceHandle`'s hash
verification uses it. If so, only add to `unimatrix-server/Cargo.toml`. If not already present
anywhere, add to both crates.

---

## `main_tests.rs` Extension

The existing `test_model_download_subcommand_parsed` test parses `["unimatrix", "model-download"]`
and expects `Some(Command::ModelDownload)`. After the struct variant change, the match arm changes:

```
// Updated test (in main_tests.rs):
#[test]
fn test_model_download_subcommand_parsed() {
    let cli = Cli::try_parse_from(["unimatrix", "model-download"]).unwrap()
    match cli.command:
        Some(Command::ModelDownload { nli, nli_model }) ->
            assert!(!nli)              // no --nli flag: defaults to false
            assert!(nli_model.is_none())  // no --nli-model: defaults to None
        other -> panic!("expected ModelDownload, got {other:?}")
}

// New test: --nli flag sets nli=true
#[test]
fn test_model_download_nli_flag() {
    let cli = Cli::try_parse_from(["unimatrix", "model-download", "--nli"]).unwrap()
    match cli.command:
        Some(Command::ModelDownload { nli, nli_model }) ->
            assert!(nli)
            assert!(nli_model.is_none())
        other -> panic!("expected ModelDownload, got {other:?}")
}

// New test: --nli-model requires --nli
#[test]
fn test_model_download_nli_model_requires_nli() {
    let result = Cli::try_parse_from(["unimatrix", "model-download", "--nli-model", "minilm2"])
    // clap `requires = "nli"` should reject this
    assert!(result.is_err())
}

// New test: --nli --nli-model deberta
#[test]
fn test_model_download_nli_deberta() {
    let cli = Cli::try_parse_from([
        "unimatrix", "model-download", "--nli", "--nli-model", "deberta"
    ]).unwrap()
    match cli.command:
        Some(Command::ModelDownload { nli, nli_model }) ->
            assert!(nli)
            assert_eq!(nli_model.as_deref(), Some("deberta"))
        other -> panic!("expected ModelDownload, got {other:?}")
}
```

---

## Output Format

```
stderr: Downloading ONNX embedding model to /home/user/.cache/unimatrix/models...
stderr: Embedding model ready: /home/user/.cache/unimatrix/models/sentence-transformers_all-MiniLM-L6-v2
stderr: Downloading NLI model 'cross-encoder/nli-MiniLM2-L6-H768' to /home/user/.cache/unimatrix/models...
stderr: NLI model ready: /home/user/.cache/unimatrix/models/cross-encoder_nli-MiniLM2-L6-H768
stderr: Computing SHA-256 hash of /home/user/.cache/unimatrix/models/cross-encoder_nli-MiniLM2-L6-H768/model.onnx...
stdout: a3f2b8c1d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1
stderr:
stderr: Add the following to your config.toml under [inference]:
stderr:   nli_model_sha256 = "a3f2b8c1d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1"
```

The operator runs: `unimatrix model-download --nli | head -1` to capture the hash, or
`unimatrix model-download --nli > /dev/null && unimatrix model-download --nli` to see the full
output on screen.

---

## Error Handling

| Condition | Behavior |
|-----------|----------|
| Embedding model download fails | Print error to stderr; return Err (exit non-zero) |
| Unrecognized `--nli-model` value | Print error to stderr with valid options; return Err |
| NLI model download fails | Print error to stderr; return Err (exit non-zero) |
| SHA-256 computation fails (I/O error) | Print error to stderr; return Err |
| Model file not found after download | Return `EmbedError::ModelNotFound` from `ensure_nli_model` |
| No network (hf-hub fails) | Print error from hf-hub; return Err (download is mandatory for this command) |

Note: The `--nli` flag does NOT fail if the model is already cached. `ensure_nli_model` is
idempotent — if model files exist and are non-empty, it returns immediately without downloading.
The SHA-256 hash is computed from the cached file.

---

## Key Test Scenarios

1. **AC-16 / download and hash**: Mock `ensure_nli_model` to copy a fixture ONNX file; run `handle_model_download(true, None)`; assert: (a) `ensure_model` called for embedding model; (b) `ensure_nli_model` called for NLI model; (c) SHA-256 hash printed to stdout matches pre-computed hash of fixture file.
2. **AC-16 / hash format**: Assert printed hash is exactly 64 lowercase hex characters.
3. **AC-16 / deberta variant**: Run `handle_model_download(true, Some("deberta".to_string()))`; assert `NliModel::NliDebertaV3Small` is passed to `ensure_nli_model`.
4. **AC-16 / unrecognized model**: Run `handle_model_download(true, Some("gpt4".to_string()))`; assert error returned with message containing "unrecognized" and the invalid name.
5. **AC-16 / nli=false backward compat**: Run `handle_model_download(false, None)`; assert only embedding model downloaded; no NLI logic executed; return Ok(()).
6. **AC-16 / already cached**: Create fixture model files in cache dir; run `handle_model_download(true, None)`; assert `ensure_nli_model` detects cache hit and skips download; hash computed from cached file.
7. **clap / --nli-model requires --nli**: Assert `Cli::try_parse_from(["unimatrix", "model-download", "--nli-model", "minilm2"])` returns Err (clap validation).
8. **clap / backward compat**: Assert `Cli::try_parse_from(["unimatrix", "model-download"])` parses to `ModelDownload { nli: false, nli_model: None }` without error.
9. **ensure_nli_model / download path**: Integration test (network required, skipable in CI) that downloads MiniLM2 model files; assert both `model.onnx` and `tokenizer.json` exist in cache subdir after call.
10. **compute_file_sha256**: Unit test with a known fixture file; assert computed hash matches pre-computed expected SHA-256.
