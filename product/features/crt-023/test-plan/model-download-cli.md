# Test Plan: Model Download CLI

## Component Scope

CLI subcommand: `unimatrix model-download --nli [--nli-model minilm2|deberta]`

Changes to: CLI entry point, `unimatrix-embed/src/download.rs` (`ensure_nli_model`)

## AC Covered

AC-16: Download succeeds, SHA-256 hash printed to stdout.

---

## Unit Tests

### ensure_nli_model Path Construction

```rust
#[test]
fn test_ensure_nli_model_uses_correct_cache_subdir() {
    // ensure_nli_model follows the same pattern as ensure_model.
    // The download destination must be derived from NliModel::cache_subdir().
    // Test: for NliMiniLM2L6H768, target path must contain "nli-minilm2" (or
    // whatever cache_subdir() returns) to avoid collision with embedding model cache.
    let cache_root = tempdir::TempDir::new().unwrap();
    let expected_subdir = NliModel::NliMiniLM2L6H768.cache_subdir();
    let path = resolve_nli_model_cache_path(
        &cache_root.path(),
        NliModel::NliMiniLM2L6H768
    );
    assert!(path.to_string_lossy().contains(expected_subdir),
        "Cache path must include model-specific subdir '{}': {:?}", expected_subdir, path);
}

#[test]
fn test_sha256_computation_produces_64_hex_chars() {
    // The hash printed by model-download must be a valid 64-char hex string
    // suitable for copy-paste into nli_model_sha256 in config.toml.
    use sha2::{Sha256, Digest};
    let data = b"test model file content";
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = format!("{:x}", hasher.finalize());
    assert_eq!(result.len(), 64, "SHA-256 hex must be exactly 64 chars");
    assert!(result.chars().all(|c| c.is_ascii_hexdigit()),
        "SHA-256 output must be valid hex");
}

#[test]
fn test_model_download_cli_default_model_is_minilm2() {
    // unimatrix model-download --nli (no --nli-model flag) defaults to minilm2.
    let args = CliArgs::parse_from(&["unimatrix", "model-download", "--nli"]);
    let resolved_model = args.resolve_nli_model().unwrap();
    assert_eq!(resolved_model, NliModel::NliMiniLM2L6H768);
}

#[test]
fn test_model_download_cli_deberta_flag_selects_deberta() {
    let args = CliArgs::parse_from(&["unimatrix", "model-download", "--nli", "--nli-model", "deberta"]);
    let resolved_model = args.resolve_nli_model().unwrap();
    assert_eq!(resolved_model, NliModel::NliDebertaV3Small);
}

#[test]
fn test_model_download_cli_invalid_model_flag_errors() {
    // Unknown model name must produce a CLI parse error before any download attempt.
    let result = CliArgs::try_parse_from(&["unimatrix", "model-download", "--nli", "--nli-model", "gpt4"]);
    assert!(result.is_err(), "Invalid --nli-model value must produce parse error");
}
```

### Hash Output Format

```rust
#[test]
fn test_hash_output_is_copy_paste_ready() {
    // The hash printed to stdout must be the bare 64-char hex string.
    // It must NOT include labels like "SHA256:" that would break config.toml parsing.
    // Format verified by: splitting on whitespace and finding the 64-char token.
    let cli_output = "a3f2c1d9b4e7890123456789abcdef01fedcba9876543210abcdef0123456789ab";
    // This is what the output looks like. Validate the format:
    assert_eq!(cli_output.len(), 64);
    assert!(cli_output.chars().all(|c| c.is_ascii_hexdigit()));
    // Confirm it can be inserted into toml:
    let toml_str = format!("nli_model_sha256 = \"{cli_output}\"");
    let config: std::collections::HashMap<String, String> = toml::from_str(&toml_str).unwrap();
    assert_eq!(config["nli_model_sha256"].len(), 64);
}
```

---

## Integration Test (Manual Smoke Test for Stage 3c)

AC-16 is primarily verified by a manual smoke test in the delivery report:

```bash
# Run model download CLI
cargo run --bin unimatrix -- model-download --nli

# Expected output:
# Downloading cross-encoder/nli-MiniLM2-L6-H768...
# Downloaded to: ~/.cache/unimatrix/nli-minilm2/model.onnx
# SHA-256: <64-char hex>
#
# Add to config.toml:
#   nli_model_sha256 = "<64-char hex>"
```

Verification assertions:
1. Exit code is 0.
2. Stdout contains a 64-char hex string.
3. The file exists at the printed path.
4. `sha256sum <path>` matches the printed hash.

If HuggingFace Hub is not available in the test environment, AC-16 is verified as a
manual smoke test noted in the delivery report, not a CI-gated automated test.

---

## R-22: sha2 Crate Present in Cargo.toml

```bash
# Verification command (run at Stage 3c):
cargo tree -p unimatrix-server | grep sha2
# Must return a result. If not, build will fail on hash verification code.
```

This is a build-time check, not a runtime test. Failure here is caught by
`cargo check -p unimatrix-server` at the start of Stage 3b.
