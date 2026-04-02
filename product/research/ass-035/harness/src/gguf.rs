//! ASS-036: GGUF inference provider for LLM-based relationship detection.
//!
//! Tests whether a local GGUF model can reliably classify prescriptive knowledge
//! relationships (Supports edges) between Unimatrix entry pairs.
//!
//! Two task formulations from the SCOPE:
//!   Formulation A — direct relationship query: full body text (≤800 chars/side)
//!   Formulation B — prescriptive framing: topic fields + category relationship
//!
//! Answers parsed as YES / NO / UNSURE from the first token(s) of model output.
//!
//! FFI: wraps llama-cpp-2 which compiles llama.cpp from source. The backend
//! initialises ggml global state. Context is created ONCE per model load and
//! reused (with KV cache cleared between inferences) to avoid per-inference
//! KV allocation overhead (~768MB for Phi-3 mini at 2048 ctx).
//!
//! Stability test runs N consecutive inferences reusing the same context to
//! detect memory growth from KV accumulation or compute buffer leaks.

use std::path::Path;
use std::num::NonZeroU32;
use std::time::Instant;

use llama_cpp_2::{
    context::{LlamaContext, params::LlamaContextParams},
    llama_backend::LlamaBackend,
    llama_batch::LlamaBatch,
    model::{params::LlamaModelParams, AddBos, LlamaModel, Special},
    sampling::LlamaSampler,
};
// LLAMA_FLASH_ATTN_TYPE_DISABLED = 0 (from llama.h enum llama_flash_attn_type).
// Used to disable auto-flash-attention on ARM which causes ggml_abort on aarch64.
const FLASH_ATTN_DISABLED: llama_cpp_sys_2::llama_flash_attn_type = 0;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Maximum new tokens to generate per inference call.
/// YES/NO/UNSURE + one-sentence explanation fits comfortably in 60 tokens.
const MAX_NEW_TOKENS: usize = 60;

/// Context window size (tokens). Formulation A at 800 chars/side ≈ 400-500 tokens + template + output.
pub const N_CTX: u32 = 2048;

// ---------------------------------------------------------------------------

/// Entry fields passed to GGUF prompt formatters.
pub struct EntryForGguf<'a> {
    pub title: &'a str,
    pub topic: &'a str,
    pub content: &'a str,
    pub category: &'a str,
}

/// Parsed answer from model output.
#[derive(Debug, Clone, PartialEq)]
pub enum GgufAnswer {
    Yes,
    No,
    Unsure,
    ParseError(String),
}

impl GgufAnswer {
    /// Whether this answer is "correct" for the given ground-truth label.
    /// YES = true, NO = false, UNSURE = borderline.
    pub fn correct_for(&self, label: &str) -> bool {
        matches!(
            (self, label),
            (GgufAnswer::Yes, "true")
                | (GgufAnswer::No, "false")
                | (GgufAnswer::Unsure, "borderline")
        )
    }

    pub fn as_str(&self) -> &str {
        match self {
            GgufAnswer::Yes => "YES",
            GgufAnswer::No => "NO ",
            GgufAnswer::Unsure => "UNS",
            GgufAnswer::ParseError(_) => "ERR",
        }
    }
}

/// Scores for one pair under both formulations.
pub struct GgufPairResult {
    pub form_a: GgufAnswer,
    pub form_a_raw: String,
    pub form_a_ms: u128,
    pub form_b: GgufAnswer,
    pub form_b_raw: String,
    pub form_b_ms: u128,
}

/// Results from the N-inference stability test.
pub struct StabilityResult {
    pub n_run: usize,
    pub n_error: usize,
    pub latencies_ms: Vec<u128>,
    pub rss_start_kb: u64,
    pub rss_end_kb: u64,
}

impl StabilityResult {
    pub fn mean_ms(&self) -> f64 {
        if self.latencies_ms.is_empty() {
            return 0.0;
        }
        self.latencies_ms.iter().sum::<u128>() as f64 / self.latencies_ms.len() as f64
    }
    pub fn max_ms(&self) -> u128 {
        self.latencies_ms.iter().cloned().max().unwrap_or(0)
    }
    pub fn min_ms(&self) -> u128 {
        self.latencies_ms.iter().cloned().min().unwrap_or(0)
    }
    pub fn p95_ms(&self) -> u128 {
        let mut sorted = self.latencies_ms.clone();
        sorted.sort_unstable();
        let idx = ((sorted.len() as f64 * 0.95) as usize).min(sorted.len().saturating_sub(1));
        sorted[idx]
    }
    pub fn rss_delta_mb(&self) -> i64 {
        (self.rss_end_kb as i64 - self.rss_start_kb as i64) / 1024
    }
}

// ---------------------------------------------------------------------------

/// GGUF inference provider.
///
/// Holds the llama.cpp backend and model. Context is created ONCE via
/// `new_ctx()` and passed into `score_pair` / `infer` to be reused across
/// all inference calls (KV cache cleared between each). This avoids the
/// per-inference KV allocation (~768MB for Phi-3 mini at 2048 ctx).
///
/// Field order matters for drop: `model` is dropped before `_backend` because
/// Rust drops fields in declaration order (top-to-bottom). The backend must
/// outlive the model to avoid use-after-free in llama.cpp global state.
pub struct GgufProvider {
    model: LlamaModel,
    _backend: LlamaBackend,
    pub model_name: String,
    pub model_family: String, // "phi3" | "llama"
}

impl GgufProvider {
    pub fn load(model_path: &Path, model_name: &str, model_family: &str) -> Result<Self> {
        eprintln!("Initializing llama.cpp backend...");
        let backend = LlamaBackend::init()?;

        let model_params = LlamaModelParams::default();
        eprintln!("Loading GGUF model: {}", model_path.display());
        let model = LlamaModel::load_from_file(&backend, model_path, &model_params)
            .map_err(|e| format!("GGUF model load: {e}"))?;

        eprintln!("Model loaded OK.");
        Ok(Self {
            model,
            _backend: backend,
            model_name: model_name.to_string(),
            model_family: model_family.to_string(),
        })
    }

    /// Create a single inference context. Caller should reuse this across
    /// all inferences (call `ctx.clear_kv_cache()` between runs).
    ///
    /// Flash attention is explicitly DISABLED: auto-enable on ARM (aarch64)
    /// causes a ggml_abort in decode during token generation for Phi-3 mini.
    pub fn new_ctx(&self) -> Result<LlamaContext<'_>> {
        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(Some(NonZeroU32::new(N_CTX).unwrap()))
            .with_n_batch(N_CTX)
            .with_flash_attention_policy(FLASH_ATTN_DISABLED);
        self.model
            .new_context(&self._backend, ctx_params)
            .map_err(|e| format!("GGUF context create: {e}").into())
    }

    /// Score one pair under both prompt formulations.
    /// The context is reused: KV is cleared before each formulation.
    pub fn score_pair<'m>(
        &'m self,
        ctx: &mut LlamaContext<'m>,
        ea: &EntryForGguf<'_>,
        eb: &EntryForGguf<'_>,
    ) -> Result<GgufPairResult> {
        let prompt_a = self.apply_template(&formulation_a(ea, eb));
        let prompt_b = self.apply_template(&formulation_b(ea, eb));

        ctx.clear_kv_cache();
        let (ans_a, raw_a, ms_a) = self.infer(ctx, &prompt_a, MAX_NEW_TOKENS)?;

        ctx.clear_kv_cache();
        let (ans_b, raw_b, ms_b) = self.infer(ctx, &prompt_b, MAX_NEW_TOKENS)?;

        Ok(GgufPairResult {
            form_a: ans_a,
            form_a_raw: raw_a,
            form_a_ms: ms_a,
            form_b: ans_b,
            form_b_raw: raw_b,
            form_b_ms: ms_b,
        })
    }

    /// Q3 stability test: N consecutive inferences reusing the same context.
    /// Uses `clear_kv_cache()` between each run. Measures latency and RSS growth.
    pub fn run_stability_test<'m>(
        &'m self,
        ctx: &mut LlamaContext<'m>,
        ea: &EntryForGguf<'_>,
        eb: &EntryForGguf<'_>,
        n_iter: usize,
    ) -> StabilityResult {
        let prompt = self.apply_template(&formulation_a(ea, eb));
        let rss_start = read_rss_kb();
        let mut latencies = Vec::with_capacity(n_iter);
        let mut n_error = 0usize;

        for i in 0..n_iter {
            if i % 10 == 0 {
                let rss = read_rss_kb();
                eprintln!(
                    "  stability [{:>3}/{}] RSS: {}MB",
                    i,
                    n_iter,
                    rss / 1024
                );
            }
            ctx.clear_kv_cache();
            match self.infer(ctx, &prompt, MAX_NEW_TOKENS) {
                Ok((_, _, ms)) => latencies.push(ms),
                Err(e) => {
                    eprintln!("  stability error at iter {i}: {e}");
                    n_error += 1;
                }
            }
        }

        let rss_end = read_rss_kb();
        StabilityResult {
            n_run: n_iter,
            n_error,
            latencies_ms: latencies,
            rss_start_kb: rss_start,
            rss_end_kb: rss_end,
        }
    }

    /// Core inference: decode prompt + generate up to `max_new` tokens.
    /// Assumes the KV cache was already cleared by the caller.
    /// Returns (answer, raw_output_trimmed, latency_ms).
    fn infer<'m>(
        &'m self,
        ctx: &mut LlamaContext<'m>,
        prompt: &str,
        max_new: usize,
    ) -> Result<(GgufAnswer, String, u128)> {
        let tokens = self
            .model
            .str_to_token(prompt, AddBos::Always)
            .map_err(|e| format!("GGUF tokenize: {e}"))?;

        if tokens.len() + max_new >= N_CTX as usize {
            return Err(format!(
                "prompt too long: {} tokens (limit {})",
                tokens.len(),
                N_CTX
            )
            .into());
        }

        let n_prompt = tokens.len();
        let mut batch = LlamaBatch::new(n_prompt.max(1), 1);
        for (i, &tok) in tokens.iter().enumerate() {
            batch
                .add(tok, i as i32, &[0], i == n_prompt - 1)
                .map_err(|e| format!("batch add prompt: {e}"))?;
        }

        let start = Instant::now();
        ctx.decode(&mut batch)
            .map_err(|e| format!("GGUF decode prompt: {e}"))?;

        let mut output = String::new();
        let mut n_cur = n_prompt as i32;
        let mut sampler = LlamaSampler::greedy();

        for _ in 0..max_new {
            let token = sampler.sample(ctx, -1);
            sampler.accept(token);

            if self.model.is_eog_token(token) {
                break;
            }

            let piece = self
                .model
                .token_to_str(token, Special::Tokenize)
                .unwrap_or_default();
            output.push_str(&piece);

            // Stop after first non-empty line — YES/NO/UNSURE + one-sentence explanation.
            if piece.contains('\n') && !output.trim().is_empty() {
                break;
            }

            batch.clear();
            batch
                .add(token, n_cur, &[0], true)
                .map_err(|e| format!("batch add token: {e}"))?;
            ctx.decode(&mut batch)
                .map_err(|e| format!("GGUF decode token: {e}"))?;
            n_cur += 1;
        }

        let ms = start.elapsed().as_millis();
        let answer = parse_gguf_answer(&output);
        Ok((answer, output.trim().to_string(), ms))
    }

    /// Apply model-family chat template to the user message.
    fn apply_template(&self, user_msg: &str) -> String {
        match self.model_family.as_str() {
            "phi3" => format!("<|user|>\n{user_msg}<|end|>\n<|assistant|>\n"),
            "llama" => format!(
                "<|begin_of_text|><|start_header_id|>user<|end_header_id|>\n\n\
                 {user_msg}<|eot_id|><|start_header_id|>assistant<|end_header_id|>\n\n"
            ),
            _ => user_msg.to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Prompt formulations

/// Formulation A: direct relationship query using full body text (≤800 chars/side).
/// Tests whether the model can reason from content alone.
pub fn formulation_a(ea: &EntryForGguf<'_>, eb: &EntryForGguf<'_>) -> String {
    let body_a = trunc(ea.content, 800);
    let body_b = trunc(eb.content, 800);
    format!(
        "Entry A [category: {}]:\n{body_a}\n\n\
         Entry B [category: {}]:\n{body_b}\n\n\
         Does knowing Entry A help you correctly apply Entry B?\n\
         Answer YES, NO, or UNSURE with a one-sentence explanation.",
        ea.category, eb.category
    )
}

/// Formulation B: prescriptive framing using topic fields + category relationship.
/// Tests whether the model can reason from compressed topic signals.
pub fn formulation_b(ea: &EntryForGguf<'_>, eb: &EntryForGguf<'_>) -> String {
    format!(
        "Entry A describes: {}\n\
         Entry B describes: {}\n\n\
         Category relationship: {} → {}\n\n\
         Does Entry A contain knowledge that informs, motivates, or prevents \
         misapplication of Entry B? Answer YES, NO, or UNSURE.",
        ea.topic, eb.topic, ea.category, eb.category
    )
}

// ---------------------------------------------------------------------------

/// Parse YES / NO / UNSURE from the first non-whitespace word of model output.
fn parse_gguf_answer(text: &str) -> GgufAnswer {
    let upper = text.trim().to_uppercase();
    if upper.starts_with("YES") {
        GgufAnswer::Yes
    } else if upper.starts_with("NO") {
        GgufAnswer::No
    } else if upper.starts_with("UNSURE")
        || upper.starts_with("UNCERTAIN")
        || upper.starts_with("MAYBE")
    {
        GgufAnswer::Unsure
    } else {
        let snippet: String = text.trim().chars().take(40).collect();
        GgufAnswer::ParseError(snippet)
    }
}

/// Read VmRSS from /proc/self/status (Linux only). Returns 0 on error.
pub fn read_rss_kb() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("VmRSS:"))
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|n| n.parse().ok())
        })
        .unwrap_or(0)
}

fn trunc(text: &str, limit: usize) -> &str {
    if text.len() <= limit {
        return text;
    }
    let mut end = limit.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}
