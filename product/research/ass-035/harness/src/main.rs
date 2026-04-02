//! ASS-035 / ASS-036: NLI + Cosine + GGUF Relationship Detection Harness
//!
//! Scores the 20 labeled entry pairs from PAIRS.md under three extraction strategies:
//!   A — entry.topic only (~5-15 words, matches original model eval distribution)
//!   B — first semantically dense paragraph (Decision: block for ADRs, Takeaway: for
//!       lessons, first paragraph otherwise) — approximates 1-2 sentence eval regime
//!   C — full entry.content truncated at 2000 chars (current production baseline)
//!
//! Also computes cosine similarity using the production embedding model
//! (sentence-transformers/all-MiniLM-L6-v2) with the production text preparation:
//!   embed_text = "{title}: {content}" (same as prepare_text in unimatrix-embed/text.rs)
//!
//! Usage:
//!   cargo run --release
//!   cargo run --release -- --model deberta-q8
//!   cargo run --release -- --cosine-only        (skip NLI, cosine table only)
//!   cargo run --release -- --db /path/to/unimatrix.db
//!
//! Output: NLI score table + cosine table + threshold summary to stdout.
//!
//! ISOLATION: reads DB read-only, loads ONNX models from cached files.
//! Does NOT connect to, signal, or otherwise interact with the running unimatrix server.

mod gguf;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use ort::session::Session;
use ort::value::Tensor;
use tokenizers::Tokenizer;
use rusqlite::OptionalExtension as _;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

// ---------------------------------------------------------------------------
// NLI label indices — cross-encoder/nli-MiniLM2-L6-H768 config.json id2label:
//   "0": "contradiction", "1": "entailment", "2": "neutral"
// Assumed same for nli-deberta-v3-small (MNLI standard order).
const LOGIT_IDX_CONTRADICTION: usize = 0;
const LOGIT_IDX_ENTAILMENT: usize = 1;
const LOGIT_IDX_NEUTRAL: usize = 2;

// MS-MARCO cross-encoders output a single logit. sigmoid(logit) → relevance ∈ (0,1).
// No label ordering applies.

/// Per-side character limit — matches production PER_SIDE_CHAR_LIMIT in cross_encoder.rs.
const PER_SIDE_CHAR_LIMIT: usize = 2000;

/// Strategy B max chars — short-text regime (~1-2 sentences).
const STRATEGY_B_CHAR_LIMIT: usize = 400;

/// Embedding model dimension (all-MiniLM-L6-v2).
const EMBED_DIM: usize = 384;

/// Embedding model max sequence length.
const EMBED_MAX_SEQ: usize = 256;

// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct NliScores {
    entailment: f32,
    neutral: f32,
    contradiction: f32,
}

/// Scores from one cross-encoder pair evaluation.
/// NLI models produce 3-class output; MS-MARCO produces a single relevance score.
#[derive(Debug, Clone)]
enum PairScore {
    Nli(NliScores),
    Relevance(f32), // sigmoid(logit) for MS-MARCO rerankers
}

impl PairScore {
    /// The primary "does A support B?" signal, normalized to [0,1].
    fn signal(&self) -> f32 {
        match self {
            PairScore::Nli(s) => s.entailment,
            PairScore::Relevance(r) => *r,
        }
    }
}

struct NliProvider {
    session: Mutex<Session>,
    tokenizer: Tokenizer,
    model_name: String,
}

impl NliProvider {
    /// `tokenizer_dir` — directory containing tokenizer.json (may differ from ONNX dir).
    /// `onnx_path` — full path to the .onnx file.
    fn load(tokenizer_dir: &Path, onnx_path: &Path, model_name: &str) -> Result<Self> {
        let tokenizer_path = tokenizer_dir.join("tokenizer.json");
        let mut tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| format!("NLI tokenizer load: {e}"))?;

        tokenizer
            .with_truncation(Some(tokenizers::TruncationParams {
                max_length: 512,
                strategy: tokenizers::TruncationStrategy::LongestFirst,
                ..Default::default()
            }))
            .map_err(|e| format!("NLI truncation config: {e}"))?;

        tokenizer.with_padding(Some(tokenizers::PaddingParams {
            strategy: tokenizers::PaddingStrategy::BatchLongest,
            ..Default::default()
        }));

        eprintln!("Loading cross-encoder: {}", onnx_path.display());
        let session = Session::builder()?
            .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level3)?
            .commit_from_file(onnx_path)?;

        Ok(Self { session: Mutex::new(session), tokenizer, model_name: model_name.to_string() })
    }

    fn score_pair(&self, a: &str, b: &str) -> Result<PairScore> {
        let a_t = truncate_chars(a, PER_SIDE_CHAR_LIMIT);
        let b_t = truncate_chars(b, PER_SIDE_CHAR_LIMIT);

        let encode_inputs = vec![tokenizers::EncodeInput::Dual(
            tokenizers::InputSequence::from(a_t),
            tokenizers::InputSequence::from(b_t),
        )];

        let encodings = self
            .tokenizer
            .encode_batch(encode_inputs, true)
            .map_err(|e| format!("tokenize: {e}"))?;

        let seq_len = encodings[0].get_ids().len();
        let mut input_ids: Vec<i64> = Vec::with_capacity(seq_len);
        let mut attn_mask: Vec<i64> = Vec::with_capacity(seq_len);
        let mut type_ids: Vec<i64> = Vec::with_capacity(seq_len);
        for &id in encodings[0].get_ids() { input_ids.push(id as i64); }
        for &m in encodings[0].get_attention_mask() { attn_mask.push(m as i64); }
        for &t in encodings[0].get_type_ids() { type_ids.push(t as i64); }

        let shape = vec![1i64, seq_len as i64];
        let ids_tensor = Tensor::from_array((shape.clone(), input_ids))?;
        let mask_tensor = Tensor::from_array((shape.clone(), attn_mask))?;
        let type_tensor = Tensor::from_array((shape, type_ids))?;

        let inputs = ort::inputs![
            "input_ids"      => ids_tensor,
            "attention_mask" => mask_tensor,
            "token_type_ids" => type_tensor,
        ]?;

        let logits: Vec<f32> = {
            let session = self.session.lock().map_err(|_| "NLI session mutex poisoned")?;
            let outputs = session.run(inputs)?;
            let (_shape, data) = outputs[0].try_extract_raw_tensor::<f32>()?;
            data.to_vec()
        };

        // Detect output shape: 3 logits → NLI (softmax), 1 logit → reranker (sigmoid)
        if logits.len() == 1 {
            let score = 1.0 / (1.0 + (-logits[0]).exp()); // sigmoid
            Ok(PairScore::Relevance(score))
        } else {
            Ok(PairScore::Nli(softmax_3class(&logits)))
        }
    }
}

// ---------------------------------------------------------------------------
// Embedding provider — replicates production pipeline from unimatrix-embed/onnx.rs
//
// Text preparation: prepare_text(title, content, ": ") from unimatrix-embed/text.rs
//   = "{title}: {content}" (both non-empty)
//   = "{content}" (title empty)
//   = "{title}" (content empty)
//
// Pipeline: tokenize → ORT inference → mean_pool (attention mask weighted) → L2 normalize
// Cosine similarity = dot product (both vectors are L2-normalized unit vectors)

struct EmbeddingProvider {
    session: Mutex<Session>,
    tokenizer: Tokenizer,
}

impl EmbeddingProvider {
    fn load(model_dir: &Path) -> Result<Self> {
        let tokenizer_path = model_dir.join("tokenizer.json");
        let mut tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| format!("embed tokenizer load: {e}"))?;

        tokenizer
            .with_truncation(Some(tokenizers::TruncationParams {
                max_length: EMBED_MAX_SEQ,
                strategy: tokenizers::TruncationStrategy::LongestFirst,
                ..Default::default()
            }))
            .map_err(|e| format!("embed truncation config: {e}"))?;

        tokenizer.with_padding(Some(tokenizers::PaddingParams {
            strategy: tokenizers::PaddingStrategy::BatchLongest,
            ..Default::default()
        }));

        let onnx_path = model_dir.join("model.onnx");
        eprintln!("Loading embedding model: {}", onnx_path.display());
        let session = Session::builder()?
            .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level3)?
            .commit_from_file(&onnx_path)?;

        Ok(Self { session: Mutex::new(session), tokenizer })
    }

    /// Embed `"{title}: {content}"` and return L2-normalized 384-dim vector.
    fn embed(&self, title: &str, content: &str) -> Result<Vec<f32>> {
        let text = prepare_embed_text(title, content);
        let encoding = self.tokenizer
            .encode(text.as_str(), true)
            .map_err(|e| format!("embed encode: {e}"))?;

        let seq_len = encoding.get_ids().len();
        let input_ids: Vec<i64> = encoding.get_ids().iter().map(|&v| v as i64).collect();
        let attention_mask: Vec<i64> = encoding.get_attention_mask().iter().map(|&v| v as i64).collect();
        let token_type_ids: Vec<i64> = encoding.get_type_ids().iter().map(|&v| v as i64).collect();

        let shape = vec![1_i64, seq_len as i64];
        let ids_tensor = Tensor::from_array((shape.clone(), input_ids.clone()))?;
        let mask_tensor = Tensor::from_array((shape.clone(), attention_mask.clone()))?;
        let type_tensor = Tensor::from_array((shape, token_type_ids))?;

        let inputs = ort::inputs![
            "input_ids"      => ids_tensor,
            "attention_mask" => mask_tensor,
            "token_type_ids" => type_tensor,
        ]?;

        let (output_flat, actual_seq_len) = {
            let session = self.session.lock().map_err(|_| "embed session mutex poisoned")?;
            let outputs = session.run(inputs)?;
            let (shape, data) = outputs[0].try_extract_raw_tensor::<f32>()?;
            if shape.len() != 3 || shape[0] != 1 || shape[2] as usize != EMBED_DIM {
                return Err(format!(
                    "embed output shape mismatch: {:?}, expected [1, seq_len, {EMBED_DIM}]",
                    shape
                ).into());
            }
            (data.to_vec(), shape[1] as usize)
        };

        // Mean pool: sum(token_embed * mask) / sum(mask)
        let mut pooled = vec![0.0_f32; EMBED_DIM];
        let mut mask_sum = 0.0_f32;
        for t in 0..actual_seq_len {
            let mask_val = attention_mask[t] as f32;
            if mask_val > 0.0 {
                for d in 0..EMBED_DIM {
                    pooled[d] += output_flat[t * EMBED_DIM + d] * mask_val;
                }
                mask_sum += mask_val;
            }
        }
        if mask_sum < 1e-9 { mask_sum = 1e-9; }
        for v in pooled.iter_mut() { *v /= mask_sum; }

        // L2 normalize
        let norm_sq: f32 = pooled.iter().map(|v| v * v).sum();
        let norm = norm_sq.sqrt();
        if norm >= 1e-12 {
            for v in pooled.iter_mut() { *v /= norm; }
        }

        Ok(pooled)
    }
}

/// Replicates prepare_text(title, content, ": ") from unimatrix-embed/text.rs.
fn prepare_embed_text(title: &str, content: &str) -> String {
    match (title.is_empty(), content.is_empty()) {
        (true, true) => String::new(),
        (true, false) => content.to_string(),
        (false, true) => title.to_string(),
        (false, false) => format!("{title}: {content}"),
    }
}

/// Cosine similarity of two L2-normalized vectors = their dot product.
fn cosine_dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

// ---------------------------------------------------------------------------

fn truncate_chars(text: &str, limit: usize) -> &str {
    if text.len() <= limit {
        return text;
    }
    let mut end = limit.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

fn cap_chars(s: &str, limit: usize) -> &str {
    truncate_chars(s, limit)
}

fn softmax_3class(logits: &[f32]) -> NliScores {
    let max_l = logits[0].max(logits[1]).max(logits[2]);
    let exps = [
        (logits[LOGIT_IDX_CONTRADICTION] - max_l).exp(),
        (logits[LOGIT_IDX_ENTAILMENT] - max_l).exp(),
        (logits[LOGIT_IDX_NEUTRAL] - max_l).exp(),
    ];
    let sum = exps[0] + exps[1] + exps[2];
    if sum == 0.0 || sum.is_nan() {
        return NliScores { entailment: 1.0 / 3.0, neutral: 1.0 / 3.0, contradiction: 1.0 / 3.0 };
    }
    NliScores {
        entailment: exps[LOGIT_IDX_ENTAILMENT] / sum,
        neutral: exps[LOGIT_IDX_NEUTRAL] / sum,
        contradiction: exps[LOGIT_IDX_CONTRADICTION] / sum,
    }
}

// ---------------------------------------------------------------------------
// Extraction strategies

fn extract_a(topic: &str) -> &str {
    topic.trim()
}

/// Strategy B: first semantically dense claim.
fn extract_b(content: &str) -> String {
    let text = content.trim();

    for header in ["## Decision\n", "### Decision\n", "**Decision**\n"] {
        if let Some(pos) = text.find(header) {
            let start = pos + header.len();
            let rest = &text[start..];
            let end = rest.find("\n\n").unwrap_or(rest.len());
            let extracted = rest[..end].trim();
            if !extracted.is_empty() {
                return cap_chars(extracted, STRATEGY_B_CHAR_LIMIT).to_string();
            }
        }
    }

    if let Some(pos) = text.find("\n\nDecision:") {
        let rest = &text[pos + 2..];
        let end = rest.find("\n\n").unwrap_or(rest.len());
        let extracted = rest[..end].trim();
        if !extracted.is_empty() {
            return cap_chars(extracted, STRATEGY_B_CHAR_LIMIT).to_string();
        }
    }

    if let Some(pos) = text.find("Takeaway:") {
        let rest = &text[pos..];
        let end = rest.find("\n\n").unwrap_or(rest.len());
        let extracted = rest[..end].trim();
        if !extracted.is_empty() {
            return cap_chars(extracted, STRATEGY_B_CHAR_LIMIT).to_string();
        }
    }

    let end = text.find("\n\n").unwrap_or(text.len());
    cap_chars(text[..end].trim(), STRATEGY_B_CHAR_LIMIT).to_string()
}

fn extract_c(content: &str) -> &str {
    truncate_chars(content.trim(), PER_SIDE_CHAR_LIMIT)
}

// ---------------------------------------------------------------------------
// Pair definitions

#[derive(Debug)]
struct Pair {
    id: &'static str,
    a: u64,
    b: u64,
    label: &'static str, // "true" | "borderline" | "false"
    group: &'static str, // "A" | "B" | "C"
}

fn pairs() -> Vec<Pair> {
    vec![
        // Group A: same-feature positive controls
        Pair { id: "P01", a: 376,  b: 375,  label: "true",       group: "A" },
        Pair { id: "P02", a: 2798, b: 2809, label: "true",       group: "A" },
        Pair { id: "P03", a: 665,  b: 667,  label: "true",       group: "A" },
        Pair { id: "P04", a: 3353, b: 3354, label: "true",       group: "A" },
        Pair { id: "P05", a: 1688, b: 1369, label: "true",       group: "A" },
        Pair { id: "P06", a: 3744, b: 3750, label: "true",       group: "A" },
        Pair { id: "P07", a: 374,  b: 375,  label: "true",       group: "A" },
        Pair { id: "P08", a: 2571, b: 2728, label: "true",       group: "A" },
        // Group B: cross-feature, semantically related
        Pair { id: "P09", a: 376,  b: 2060, label: "borderline", group: "B" },
        Pair { id: "P10", a: 735,  b: 1369, label: "borderline", group: "B" },
        Pair { id: "P11", a: 3353, b: 3660, label: "true",       group: "B" },
        Pair { id: "P12", a: 378,  b: 238,  label: "borderline", group: "B" },
        Pair { id: "P13", a: 1628, b: 1367, label: "borderline", group: "B" },
        Pair { id: "P14", a: 2571, b: 3741, label: "borderline", group: "B" },
        Pair { id: "P15", a: 667,  b: 245,  label: "borderline", group: "B" },
        // Group C: incompatible-category negative controls
        Pair { id: "P16", a: 376,  b: 2701, label: "false",      group: "C" },
        Pair { id: "P17", a: 64,   b: 735,  label: "false",      group: "C" },
        Pair { id: "P18", a: 63,   b: 1688, label: "false",      group: "C" },
        Pair { id: "P19", a: 239,  b: 3732, label: "false",      group: "C" },
        Pair { id: "P20", a: 2393, b: 65,   label: "false",      group: "C" },
        // Group D: compatible-category cross-feature negative controls
        // Purpose: test whether cosine ≥ 0.65 alone produces false positives without
        // the same_feature_cycle filter. All pairs are in informs_category_pairs but
        // have no actual Supports relationship.
        Pair { id: "P21", a: 665,  b: 2701, label: "false",      group: "D" }, // lesson→decision: flock TOCTOU → NLI sort
        Pair { id: "P22", a: 1628, b: 64,   label: "false",      group: "D" }, // lesson→decision: spawn_blocking mutex → DistDot metric
        Pair { id: "P23", a: 3353, b: 245,  label: "false",      group: "D" }, // lesson→decision: rayon panic → socket lifecycle
        Pair { id: "P24", a: 667,  b: 2701, label: "false",      group: "D" }, // pattern→decision: flock PID → NLI sort
        Pair { id: "P25", a: 2571, b: 238,  label: "false",      group: "D" }, // pattern→convention: rayon-tokio bridge → testing infra
    ]
}

// ---------------------------------------------------------------------------
// DB

#[derive(Debug)]
struct EntryFields {
    title: String,
    topic: String,
    content: String,
    category: String,
}

fn load_entries(db_path: &Path, ids: &[u64]) -> Result<HashMap<u64, EntryFields>> {
    let conn = rusqlite::Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;

    let mut map = HashMap::new();
    for &id in ids {
        let row = conn
            .query_row(
                "SELECT title, topic, content, category FROM entries WHERE id = ?1",
                rusqlite::params![id as i64],
                |row| Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                )),
            )
            .optional()?;

        match row {
            Some((title, topic, content, category)) => {
                map.insert(id, EntryFields { title, topic, content, category });
            }
            None => eprintln!("WARNING: entry {id} not found"),
        }
    }
    Ok(map)
}

// ---------------------------------------------------------------------------
// Scored result (computed once, reported multiple times)

struct ScoredPair<'a> {
    pair: &'a Pair,
    cosine: f32,
    // NLI scores (present only when NLI is not skipped)
    nli: Option<NliResult>,
    // GGUF scores (present only in GGUF mode)
    gguf: Option<gguf::GgufPairResult>,
    b_text_a: String,
    b_text_b: String,
}

struct NliResult {
    score_a: PairScore,
    score_b: PairScore,
    score_c: PairScore,
}

// ---------------------------------------------------------------------------
// Main

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let model_arg = arg_value(&args, "--model").unwrap_or_else(|| "minilm2-q8".to_string());
    let cosine_only = args.contains(&"--cosine-only".to_string());
    let stability_test = args.contains(&"--stability-test".to_string());
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/vscode".to_string());
    let db_path = PathBuf::from(
        arg_value(&args, "--db")
            .unwrap_or_else(|| format!("{home}/.unimatrix/0d62f3bf1bf46a0a/unimatrix.db")),
    );
    let model_cache_base = PathBuf::from(format!("{home}/.cache/unimatrix/models"));
    let embed_model_dir = model_cache_base.join("sentence-transformers_all-MiniLM-L6-v2");
    let gguf_dir = model_cache_base.join("gguf");

    // Detect GGUF mode: any model arg that maps to a GGUF file name.
    let is_gguf = is_gguf_model(&model_arg);

    eprintln!("=== ASS-035/ASS-036: NLI + Cosine + GGUF Harness ===");
    eprintln!("DB    : {}", db_path.display());
    if cosine_only {
        eprintln!("Mode  : cosine only (NLI/GGUF skipped)");
    } else if is_gguf {
        eprintln!("Mode  : GGUF   model={model_arg}");
    } else {
        eprintln!("Model : {model_arg}");
    }
    eprintln!();

    // Load embedding model (always — cosine baseline is always computed)
    let embedder = EmbeddingProvider::load(&embed_model_dir)?;

    // Load cross-encoder (unless --cosine-only or GGUF mode).
    let nli_provider = if !cosine_only && !is_gguf {
        let (model_dir, onnx_filename, display_name) = resolve_nli_model(&model_arg, &model_cache_base)?;
        let is_ms_marco = model_arg.starts_with("ms-marco");
        let onnx_path = if is_ms_marco {
            model_dir.join("onnx").join(onnx_filename)
        } else {
            model_dir.join(onnx_filename)
        };
        eprintln!("Cross-encoder: {display_name}");
        Some((NliProvider::load(&model_dir, &onnx_path, &display_name)?, display_name))
    } else {
        None
    };

    // Load GGUF model and create a single reusable context (only in GGUF mode).
    // Context is created once and reused for all inferences (KV cleared between calls).
    let gguf_provider = if is_gguf && !cosine_only {
        let (filename, family, display) = resolve_gguf_model(&model_arg)?;
        let path = gguf_dir.join(filename);
        if !path.exists() {
            return Err(format!(
                "GGUF model not found: {}\nDownload it first (see ASS-036 SCOPE.md).",
                path.display()
            ).into());
        }
        Some((gguf::GgufProvider::load(&path, &display, family)?, display))
    } else {
        None
    };
    // Create context after provider (borrows provider; must be dropped before provider).
    let mut gguf_ctx_holder = if let Some((ref provider, _)) = gguf_provider {
        Some(provider.new_ctx()?)
    } else {
        None
    };

    eprintln!("Models loaded OK. Scoring {} pairs...\n", pairs().len());

    let pair_list = pairs();
    let mut ids: Vec<u64> = pair_list.iter().flat_map(|p| [p.a, p.b]).collect();
    ids.sort_unstable();
    ids.dedup();
    let entries = load_entries(&db_path, &ids)?;

    // --- Score all pairs (single pass) ---
    let mut results: Vec<ScoredPair> = Vec::with_capacity(pair_list.len());

    for pair in &pair_list {
        let ea = match entries.get(&pair.a) {
            Some(e) => e,
            None => { eprintln!("SKIP {}: entry {} not found", pair.id, pair.a); continue; }
        };
        let eb = match entries.get(&pair.b) {
            Some(e) => e,
            None => { eprintln!("SKIP {}: entry {} not found", pair.id, pair.b); continue; }
        };

        // Cosine similarity using production embedding text preparation
        let emb_a = embedder.embed(&ea.title, &ea.content)?;
        let emb_b = embedder.embed(&eb.title, &eb.content)?;
        let cosine = cosine_dot(&emb_a, &emb_b);

        // NLI scores (optional)
        let b_text_a = extract_b(&ea.content);
        let b_text_b = extract_b(&eb.content);

        let nli = if let Some((ref provider, _)) = nli_provider {
            let score_a = provider.score_pair(extract_a(&ea.topic), extract_a(&eb.topic))?;
            let score_b = provider.score_pair(&b_text_a, &b_text_b)?;
            let score_c = provider.score_pair(extract_c(&ea.content), extract_c(&eb.content))?;
            Some(NliResult { score_a, score_b, score_c })
        } else {
            None
        };

        let gguf_result = if let (Some((provider, _)), Some(ctx)) =
            (gguf_provider.as_ref(), gguf_ctx_holder.as_mut())
        {
            eprintln!("  GGUF scoring {} ...", pair.id);
            let ea_gguf = gguf::EntryForGguf {
                title: &ea.title, topic: &ea.topic,
                content: &ea.content, category: &ea.category,
            };
            let eb_gguf = gguf::EntryForGguf {
                title: &eb.title, topic: &eb.topic,
                content: &eb.content, category: &eb.category,
            };
            Some(provider.score_pair(ctx, &ea_gguf, &eb_gguf)?)
        } else {
            None
        };

        results.push(ScoredPair { pair, cosine, nli, gguf: gguf_result, b_text_a, b_text_b });
    }

    // ==========================================================================
    // Table 1: Cosine similarity
    // ==========================================================================
    let nli_display_name = nli_provider.as_ref().map(|(_, n)| n.as_str()).unwrap_or("(skipped)");

    println!("# ASS-035 Cosine Similarity — sentence-transformers/all-MiniLM-L6-v2");
    println!("# Text: prepare_text(title, content, \": \") — production embedding input");
    println!("# Thresholds tested: 0.70, 0.75, 0.80, 0.85, 0.90");
    println!();
    println!("{:<4} {:<5} {:<10}  {:>7}  {:>5} {:>5} {:>5} {:>5} {:>5}  A-id   B-id",
        "Pair", "Grp", "Label", "Cosine",
        "≥.70", "≥.75", "≥.80", "≥.85", "≥.90",
    );
    println!("{}", "-".repeat(80));

    for r in &results {
        println!(
            "{:<4} {:<5} {:<10}  {:>7.4}  {:>5} {:>5} {:>5} {:>5} {:>5}  {}  {}",
            r.pair.id, r.pair.group, r.pair.label,
            r.cosine,
            yn(r.cosine, 0.70), yn(r.cosine, 0.75), yn(r.cosine, 0.80),
            yn(r.cosine, 0.85), yn(r.cosine, 0.90),
            r.pair.a, r.pair.b,
        );
    }

    // Cosine group summary
    println!();
    println!("# Cosine group summary");
    println!("{:<10} {:<11}  {:>8} {:>8} {:>7} {:>7} {:>7}",
        "Group", "Label", "max", "mean", "n≥.70", "n≥.80", "n≥.90");
    println!("{}", "-".repeat(65));

    for group in ["A", "B", "C", "D"] {
        for label in ["true", "borderline", "false"] {
            let subset: Vec<&ScoredPair> = results.iter()
                .filter(|r| r.pair.group == group && r.pair.label == label)
                .collect();
            if subset.is_empty() { continue; }
            let n = subset.len() as f32;
            let max_c = subset.iter().map(|r| r.cosine).fold(f32::NEG_INFINITY, f32::max);
            let min_c = subset.iter().map(|r| r.cosine).fold(f32::INFINITY, f32::min);
            let mean_c: f32 = subset.iter().map(|r| r.cosine).sum::<f32>() / n;
            let cnt_70 = subset.iter().filter(|r| r.cosine >= 0.70).count();
            let cnt_80 = subset.iter().filter(|r| r.cosine >= 0.80).count();
            let cnt_90 = subset.iter().filter(|r| r.cosine >= 0.90).count();
            println!(
                "{:<10} {:<11}  {:>8.4} {:>8.4} {:>7} {:>7} {:>7}  (min {:.4})",
                group, label, max_c, mean_c, cnt_70, cnt_80, cnt_90, min_c,
            );
        }
    }

    // ==========================================================================
    // Table 2: Cross-encoder results (NLI or reranker)
    // ==========================================================================
    if nli_provider.is_some() {
        // Detect whether we're in NLI mode or reranker mode from the first result
        let is_reranker = results.iter()
            .find_map(|r| r.nli.as_ref())
            .map(|n| matches!(n.score_b, PairScore::Relevance(_)))
            .unwrap_or(false);

        if is_reranker {
            // ---- Reranker (MS-MARCO) output: single relevance score per strategy ----
            println!();
            println!("# Cross-encoder Relevance — {nli_display_name}");
            println!("# Score = sigmoid(logit). Framing: A=query (lesson/pattern), B=passage (pattern/decision).");
            println!("# Thresholds: 0.5, 0.6, 0.7 — no entailment semantics; higher = more relevant.");
            println!();
            println!("{:<4} {:<5} {:<10}  {:>7} {:>7} {:>7}  {:>5} {:>5} {:>5}  A-id  B-id",
                "Pair", "Grp", "Label",
                "Rel_A", "Rel_B", "Rel_C",
                "B≥.5", "B≥.6", "B≥.7",
            );
            println!("{}", "-".repeat(90));

            for r in &results {
                if let Some(ref nli) = r.nli {
                    let ra = nli.score_a.signal();
                    let rb = nli.score_b.signal();
                    let rc = nli.score_c.signal();
                    println!(
                        "{:<4} {:<5} {:<10}  {:>7.4} {:>7.4} {:>7.4}  {:>5} {:>5} {:>5}  {}  {}",
                        r.pair.id, r.pair.group, r.pair.label,
                        ra, rb, rc,
                        yn(rb, 0.5), yn(rb, 0.6), yn(rb, 0.7),
                        r.pair.a, r.pair.b,
                    );
                }
            }

            // Reranker group summary (Strategy B — operative claim, most meaningful)
            println!();
            println!("# Relevance group summary (Strategy B — operative claim)");
            println!("{:<10} {:<11}  {:>8} {:>8} {:>6} {:>6} {:>6}",
                "Group", "Label", "max_B", "mean_B", "n≥.5", "n≥.6", "n≥.7");
            println!("{}", "-".repeat(65));

            for group in ["A", "B", "C"] {
                for label in ["true", "borderline", "false"] {
                    let subset: Vec<&ScoredPair> = results.iter()
                        .filter(|r| r.pair.group == group && r.pair.label == label)
                        .collect();
                    if subset.is_empty() { continue; }
                    let scores_b: Vec<f32> = subset.iter()
                        .filter_map(|r| r.nli.as_ref())
                        .map(|n| n.score_b.signal())
                        .collect();
                    if scores_b.is_empty() { continue; }
                    let n = scores_b.len() as f32;
                    let max_b = scores_b.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
                    let min_b = scores_b.iter().cloned().fold(f32::INFINITY, f32::min);
                    let mean_b: f32 = scores_b.iter().sum::<f32>() / n;
                    let cnt_5 = scores_b.iter().filter(|&&s| s >= 0.5).count();
                    let cnt_6 = scores_b.iter().filter(|&&s| s >= 0.6).count();
                    let cnt_7 = scores_b.iter().filter(|&&s| s >= 0.7).count();
                    println!(
                        "{:<10} {:<11}  {:>8.4} {:>8.4} {:>6} {:>6} {:>6}  (min {:.4})",
                        group, label, max_b, mean_b, cnt_5, cnt_6, cnt_7, min_b,
                    );
                }
            }
        } else {
            // ---- NLI (3-class) output ----
            println!();
            println!("# NLI Entailment — {nli_display_name}");
            println!();
            println!("{:<4} {:<5} {:<10}  {:>6} {:>6} {:>6}  {:>6} {:>6} {:>6}  {:>6} {:>6} {:>6}  A-id  B-id",
                "Pair", "Grp", "Label",
                "Ent_A", "Neu_A", "Con_A",
                "Ent_B", "Neu_B", "Con_B",
                "Ent_C", "Neu_C", "Con_C",
            );
            println!("{}", "-".repeat(115));

            for r in &results {
                if let Some(ref nli) = r.nli {
                    let (ea, na, ca) = nli_triple(&nli.score_a);
                    let (eb, nb, cb) = nli_triple(&nli.score_b);
                    let (ec, nc, cc) = nli_triple(&nli.score_c);
                    println!(
                        "{:<4} {:<5} {:<10}  {:>6.3} {:>6.3} {:>6.3}  {:>6.3} {:>6.3} {:>6.3}  {:>6.3} {:>6.3} {:>6.3}  {}  {}",
                        r.pair.id, r.pair.group, r.pair.label,
                        ea, na, ca, eb, nb, cb, ec, nc, cc,
                        r.pair.a, r.pair.b,
                    );
                }
            }

            // NLI threshold crossings
            println!();
            println!("{:<4} {:<10}  {:>6} {:>6} {:>6}  {:>6} {:>6} {:>6}  {:>6} {:>6} {:>6}",
                "Pair", "Label",
                "A≥.45", "A≥.50", "A≥.60",
                "B≥.45", "B≥.50", "B≥.60",
                "C≥.45", "C≥.50", "C≥.60",
            );
            println!("{}", "-".repeat(90));
            for r in &results {
                if let Some(ref nli) = r.nli {
                    let sa = nli.score_a.signal();
                    let sb = nli.score_b.signal();
                    let sc = nli.score_c.signal();
                    println!(
                        "{:<4} {:<10}  {:>6} {:>6} {:>6}  {:>6} {:>6} {:>6}  {:>6} {:>6} {:>6}",
                        r.pair.id, r.pair.label,
                        yn(sa, 0.45), yn(sa, 0.50), yn(sa, 0.60),
                        yn(sb, 0.45), yn(sb, 0.50), yn(sb, 0.60),
                        yn(sc, 0.45), yn(sc, 0.50), yn(sc, 0.60),
                    );
                }
            }

            // NLI group summary
            println!();
            println!("# NLI group summary (entailment)");
            println!("{:<10} {:<5}  {:>8} {:>8} {:>8}  {:>8} {:>8} {:>8}  {:>8} {:>8} {:>8}",
                "Group", "Label", "max_A", "mean_A", "n≥.45_A", "max_B", "mean_B", "n≥.45_B", "max_C", "mean_C", "n≥.45_C");
            println!("{}", "-".repeat(100));

            for group in ["A", "B", "C"] {
                for label in ["true", "borderline", "false"] {
                    let subset: Vec<&ScoredPair> = results.iter()
                        .filter(|r| r.pair.group == group && r.pair.label == label)
                        .collect();
                    if subset.is_empty() { continue; }
                    let nli_results: Vec<&NliResult> = subset.iter().filter_map(|r| r.nli.as_ref()).collect();
                    if nli_results.is_empty() { continue; }
                    let n = nli_results.len() as f32;
                    let max_a = nli_results.iter().map(|r| r.score_a.signal()).fold(f32::NEG_INFINITY, f32::max);
                    let max_b = nli_results.iter().map(|r| r.score_b.signal()).fold(f32::NEG_INFINITY, f32::max);
                    let max_c = nli_results.iter().map(|r| r.score_c.signal()).fold(f32::NEG_INFINITY, f32::max);
                    let mean_a: f32 = nli_results.iter().map(|r| r.score_a.signal()).sum::<f32>() / n;
                    let mean_b: f32 = nli_results.iter().map(|r| r.score_b.signal()).sum::<f32>() / n;
                    let mean_c: f32 = nli_results.iter().map(|r| r.score_c.signal()).sum::<f32>() / n;
                    let cnt_a = nli_results.iter().filter(|r| r.score_a.signal() >= 0.45).count();
                    let cnt_b = nli_results.iter().filter(|r| r.score_b.signal() >= 0.45).count();
                    let cnt_c = nli_results.iter().filter(|r| r.score_c.signal() >= 0.45).count();
                    println!(
                        "{:<10} {:<5}  {:>8.3} {:>8.3} {:>8}  {:>8.3} {:>8.3} {:>8}  {:>8.3} {:>8.3} {:>8}",
                        group, label, max_a, mean_a, cnt_a, max_b, mean_b, cnt_b, max_c, mean_c, cnt_c,
                    );
                }
            }
        }

        // Strategy B extraction preview (both modes)
        println!();
        println!("# Strategy B extraction preview");
        for r in &results {
            let preview_a = &r.b_text_a[..r.b_text_a.len().min(100)];
            let preview_b = &r.b_text_b[..r.b_text_b.len().min(100)];
            println!("  {} A#{}: {:?}", r.pair.id, r.pair.a, preview_a);
            println!("     B#{}: {:?}", r.pair.b, preview_b);
        }
    }

    // ==========================================================================
    // Table 3: GGUF results (ASS-036)
    // ==========================================================================
    if let Some((ref provider, ref display)) = gguf_provider {
        println!();
        println!("# GGUF Relationship Classification — {display}");
        println!("# Form-A: full body text (≤800 chars/side)  Form-B: topic + category framing");
        println!("# Correct: YES=true, NO=false, UNSURE=borderline");
        println!();
        println!(
            "{:<4} {:<5} {:<10}  {:>5} {:>7}  {:>5} {:>7}  {:>8} {:>8}  A-id  B-id",
            "Pair", "Grp", "Label", "Frm-A", "ms-A", "Frm-B", "ms-B", "A-corr", "B-corr",
        );
        println!("{}", "-".repeat(90));

        let mut lat_a: Vec<u128> = Vec::new();
        let mut lat_b: Vec<u128> = Vec::new();
        let mut correct_a = 0usize;
        let mut correct_b = 0usize;
        let mut fp_a = 0usize; // false positives: false label + YES answer
        let mut fp_b = 0usize;
        let mut n_false = 0usize;

        for r in &results {
            if let Some(ref g) = r.gguf {
                let ok_a = g.form_a.correct_for(r.pair.label);
                let ok_b = g.form_b.correct_for(r.pair.label);
                if ok_a { correct_a += 1; }
                if ok_b { correct_b += 1; }
                if r.pair.label == "false" {
                    n_false += 1;
                    if matches!(g.form_a, gguf::GgufAnswer::Yes) { fp_a += 1; }
                    if matches!(g.form_b, gguf::GgufAnswer::Yes) { fp_b += 1; }
                }
                lat_a.push(g.form_a_ms);
                lat_b.push(g.form_b_ms);
                println!(
                    "{:<4} {:<5} {:<10}  {:>5} {:>7}  {:>5} {:>7}  {:>8} {:>8}  {}  {}",
                    r.pair.id, r.pair.group, r.pair.label,
                    g.form_a.as_str(), g.form_a_ms,
                    g.form_b.as_str(), g.form_b_ms,
                    if ok_a { "Y" } else { "n" },
                    if ok_b { "Y" } else { "n" },
                    r.pair.a, r.pair.b,
                );
            }
        }

        let n_scored = lat_a.len();
        let fp_rate_a = if n_false > 0 { fp_a as f64 / n_false as f64 * 100.0 } else { 0.0 };
        let fp_rate_b = if n_false > 0 { fp_b as f64 / n_false as f64 * 100.0 } else { 0.0 };

        println!();
        println!("# GGUF accuracy summary");
        println!(
            "  Form-A correct: {correct_a}/{n_scored}  FP rate: {fp_a}/{n_false} ({fp_rate_a:.0}%)"
        );
        println!(
            "  Form-B correct: {correct_b}/{n_scored}  FP rate: {fp_b}/{n_false} ({fp_rate_b:.0}%)"
        );

        // Q1: pass criterion = ≥16/20 AND FP rate < 20%
        let q1_pass_a = correct_a >= 16 && fp_rate_a < 20.0;
        let q1_pass_b = correct_b >= 16 && fp_rate_b < 20.0;
        println!("  Q1 pass (≥16/{n_scored}, FP<20%): Form-A={} Form-B={}",
            if q1_pass_a { "PASS" } else { "FAIL" },
            if q1_pass_b { "PASS" } else { "FAIL" },
        );

        // Q2: latency analysis
        println!();
        println!("# GGUF latency (ms)");
        if !lat_a.is_empty() {
            let lat_mean_a: f64 = lat_a.iter().sum::<u128>() as f64 / lat_a.len() as f64;
            let lat_mean_b: f64 = lat_b.iter().sum::<u128>() as f64 / lat_b.len() as f64;
            let lat_max_a = *lat_a.iter().max().unwrap_or(&0);
            let lat_max_b = *lat_b.iter().max().unwrap_or(&0);
            let lat_min_a = *lat_a.iter().min().unwrap_or(&0);
            let lat_min_b = *lat_b.iter().min().unwrap_or(&0);
            let mut sorted_a = lat_a.clone(); sorted_a.sort_unstable();
            let mut sorted_b = lat_b.clone(); sorted_b.sort_unstable();
            let p95_a = sorted_a[((sorted_a.len() as f64 * 0.95) as usize).min(sorted_a.len()-1)];
            let p95_b = sorted_b[((sorted_b.len() as f64 * 0.95) as usize).min(sorted_b.len()-1)];
            println!("         {:>8} {:>8} {:>8} {:>8}", "min", "mean", "p95", "max");
            println!("  Form-A {:>8} {:>8.0} {:>8} {:>8}", lat_min_a, lat_mean_a, p95_a, lat_max_a);
            println!("  Form-B {:>8} {:>8.0} {:>8} {:>8}", lat_min_b, lat_mean_b, p95_b, lat_max_b);
            let q2_threshold_ms = 2000u128;
            println!("  Q2 (≤{}ms/pair): Form-A={} Form-B={}",
                q2_threshold_ms,
                if lat_max_a <= q2_threshold_ms { "pass" } else { "marginal" },
                if lat_max_b <= q2_threshold_ms { "pass" } else { "marginal" },
            );
        }

        // Raw output preview (for debugging parse errors)
        println!();
        println!("# GGUF raw output preview");
        for r in &results {
            if let Some(ref g) = r.gguf {
                let raw_a: String = g.form_a_raw.chars().take(80).collect();
                let raw_b: String = g.form_b_raw.chars().take(60).collect();
                println!("  {} A: {:?}", r.pair.id, raw_a);
                println!("     B: {:?}", raw_b);
            }
        }

        // Q3: stability test (100 inferences on P04 — the hardest NLI case)
        if stability_test {
            println!();
            println!("# GGUF stability test — 100 consecutive inferences on P04");
            eprintln!("\n=== STABILITY TEST (100 inferences on P04) ===");
            let p04 = pair_list.iter().find(|p| p.id == "P04");
            if let (Some(p04), Some(ctx)) = (p04, gguf_ctx_holder.as_mut()) {
                if let (Some(ea_e), Some(eb_e)) = (entries.get(&p04.a), entries.get(&p04.b)) {
                    let ea_gguf = gguf::EntryForGguf {
                        title: &ea_e.title, topic: &ea_e.topic,
                        content: &ea_e.content, category: &ea_e.category,
                    };
                    let eb_gguf = gguf::EntryForGguf {
                        title: &eb_e.title, topic: &eb_e.topic,
                        content: &eb_e.content, category: &eb_e.category,
                    };
                    let stab = provider.run_stability_test(ctx, &ea_gguf, &eb_gguf, 100);
                    println!("  n_run: {}  n_error: {}", stab.n_run, stab.n_error);
                    println!("  latency (ms): min={}  mean={:.0}  p95={}  max={}",
                        stab.min_ms(), stab.mean_ms(), stab.p95_ms(), stab.max_ms());
                    println!("  RSS start: {}MB  end: {}MB  delta: {}MB",
                        stab.rss_start_kb / 1024, stab.rss_end_kb / 1024, stab.rss_delta_mb());
                    let q3_pass = stab.n_error == 0 && stab.rss_delta_mb().abs() < 200;
                    println!("  Q3 (0 errors, RSS delta <200MB): {}", if q3_pass { "PASS" } else { "FAIL" });
                }
            }
        }
    }

    eprintln!("\nDone.");
    Ok(())
}

// ---------------------------------------------------------------------------

fn yn(score: f32, threshold: f32) -> &'static str {
    if score >= threshold { "Y" } else { "n" }
}

/// Unpack a PairScore into (entailment, neutral, contradiction) for NLI display.
/// For reranker scores this is never called; returns (signal, 0, 0) as fallback.
fn nli_triple(score: &PairScore) -> (f32, f32, f32) {
    match score {
        PairScore::Nli(s) => (s.entailment, s.neutral, s.contradiction),
        PairScore::Relevance(r) => (*r, 0.0, 0.0),
    }
}

fn arg_value(args: &[String], flag: &str) -> Option<String> {
    args.windows(2).find(|w| w[0] == flag).map(|w| w[1].clone())
}

fn resolve_nli_model<'a>(
    name: &str,
    base: &Path,
) -> Result<(PathBuf, &'a str, String)> {
    match name {
        "minilm2-q8" => Ok((
            base.join("nli-minilm2-l6-h768"),
            "model_qint8_avx512.onnx",
            "cross-encoder/nli-MiniLM2-L6-H768 Q8".to_string(),
        )),
        "minilm2" => Ok((
            base.join("nli-minilm2-l6-h768"),
            "model.onnx",
            "cross-encoder/nli-MiniLM2-L6-H768 FP32".to_string(),
        )),
        "deberta-q8" => {
            eprintln!("NOTE: DeBERTa label order assumed MNLI standard (0=contradiction, 1=entailment, 2=neutral).");
            Ok((
                base.join("nli-deberta-v3-small"),
                "model_qint8_avx512.onnx",
                "cross-encoder/nli-deberta-v3-small Q8".to_string(),
            ))
        }
        "deberta" => {
            eprintln!("NOTE: DeBERTa label order assumed MNLI standard.");
            Ok((
                base.join("nli-deberta-v3-small"),
                "model.onnx",
                "cross-encoder/nli-deberta-v3-small FP32".to_string(),
            ))
        }
        // MS-MARCO reranker: single-logit output, sigmoid → relevance score [0,1].
        // Tokenizer is at model root; ONNX is in the onnx/ subdirectory.
        // We return the model root as dir and let the caller patch the ONNX path.
        "ms-marco" | "ms-marco-q8" => Ok((
            base.join("ms-marco-MiniLM-L6-v2"),
            "model_qint8_avx512.onnx",
            "cross-encoder/ms-marco-MiniLM-L6-v2 Q8".to_string(),
        )),
        other => Err(format!(
            "unknown model '{other}'. NLI: minilm2-q8, minilm2, deberta-q8, deberta, ms-marco. \
             GGUF: llama1b-q4, llama1b-q8, phi3-q4, phi3-q8"
        ).into()),
    }
}

/// True if the model arg names a GGUF model (not an ONNX cross-encoder).
fn is_gguf_model(name: &str) -> bool {
    matches!(name, "llama1b-q4" | "llama1b-q8" | "phi3-q4" | "phi3-q8")
}

/// Returns (filename, family, display_name) for a GGUF model arg.
fn resolve_gguf_model(name: &str) -> Result<(&'static str, &'static str, String)> {
    match name {
        "llama1b-q4" => Ok((
            "llama-3.2-1b-q4_k_m.gguf",
            "llama",
            "Llama-3.2-1B-Instruct Q4_K_M".to_string(),
        )),
        "llama1b-q8" => Ok((
            "llama-3.2-1b-q8_0.gguf",
            "llama",
            "Llama-3.2-1B-Instruct Q8_0".to_string(),
        )),
        "phi3-q4" => Ok((
            "phi-3-mini-4k-q4_k_m.gguf",
            "phi3",
            "Phi-3-mini-4k-instruct Q4_K_M".to_string(),
        )),
        "phi3-q8" => Ok((
            "phi-3-mini-4k-q8_0.gguf",
            "phi3",
            "Phi-3-mini-4k-instruct Q8_0".to_string(),
        )),
        other => Err(format!(
            "unknown GGUF model '{other}'. Use: llama1b-q4, llama1b-q8, phi3-q4, phi3-q8"
        ).into()),
    }
}
