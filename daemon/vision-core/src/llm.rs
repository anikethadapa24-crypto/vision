//! Local LLM runtime (`docs/ROADMAP.md` M8), backed by `candle` rather than
//! literal llama.cpp bindings — see `docs/ARCHITECTURE.md` §9.1 for why
//! (pure Rust, no C++ toolchain build, same reasoning already used for the
//! SQLite/hashing-vectorizer stand-ins). Loads a quantized GGUF model and
//! runs CPU inference; the `Model Cache` directory from §5.1 is
//! `<base_data_dir>/models/`, downloaded once on first use via `curl` —
//! not a Rust HTTP client dependency, deliberately: an earlier attempt at
//! `hf-hub` pulled in `aws-lc-sys` (a large C crypto library) as an
//! unconditional dependency and produced a real, non-deterministic MSVC
//! build failure on this box. Shelling out to `curl.exe` (present on every
//! Windows 10/11 install, already relied on elsewhere in this session)
//! sidesteps that risk entirely for what is a one-time download.

use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use candle_core::quantized::gguf_file;
use candle_core::{Device, Tensor};
use candle_transformers::generation::LogitsProcessor;
use candle_transformers::models::quantized_llama::ModelWeights;
use tokenizers::Tokenizer;

use crate::error::{CoreError, CoreResult};

const MODEL_URL: &str = "https://huggingface.co/TheBloke/TinyLlama-1.1B-Chat-v1.0-GGUF/resolve/main/tinyllama-1.1b-chat-v1.0.Q4_K_M.gguf";
const TOKENIZER_URL: &str =
    "https://huggingface.co/TinyLlama/TinyLlama-1.1B-Chat-v1.0/resolve/main/tokenizer.json";
const MODEL_FILENAME: &str = "tinyllama-1.1b-chat-v1.0.Q4_K_M.gguf";
const TOKENIZER_FILENAME: &str = "tokenizer.json";

/// TinyLlama's `</s>` id in its base (non-GGUF-remapped) vocabulary — only
/// used if the GGUF file's own metadata doesn't carry
/// `tokenizer.ggml.eos_token_id`, which every GGUF conversion in practice
/// does; this is a last-resort fallback, not the primary path.
const FALLBACK_EOS_TOKEN_ID: u32 = 2;

pub struct LlmRuntime {
    model: Mutex<ModelWeights>,
    tokenizer: Tokenizer,
    device: Device,
    eos_token_id: u32,
}

impl LlmRuntime {
    /// Downloads the model/tokenizer into `models_dir` if not already
    /// present, then loads the model into memory. Blocking and slow on
    /// first run (network + ~640MB write); fast on every run after.
    pub fn load(models_dir: &Path) -> CoreResult<Self> {
        std::fs::create_dir_all(models_dir)?;
        let model_path = models_dir.join(MODEL_FILENAME);
        let tokenizer_path = models_dir.join(TOKENIZER_FILENAME);

        download_if_missing(&model_path, MODEL_URL)?;
        download_if_missing(&tokenizer_path, TOKENIZER_URL)?;

        let device = Device::Cpu;
        let mut file = BufReader::new(std::fs::File::open(&model_path)?);
        let content = gguf_file::Content::read(&mut file)
            .map_err(|e| CoreError::Extract(format!("failed to read GGUF model: {e}")))?;
        let eos_token_id = content
            .metadata
            .get("tokenizer.ggml.eos_token_id")
            .and_then(|v| v.to_u32().ok())
            .unwrap_or(FALLBACK_EOS_TOKEN_ID);
        let model = ModelWeights::from_gguf(content, &mut file, &device)
            .map_err(|e| CoreError::Extract(format!("failed to load GGUF model: {e}")))?;

        let tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| CoreError::Extract(format!("failed to load tokenizer: {e}")))?;

        Ok(Self {
            model: Mutex::new(model),
            tokenizer,
            device,
            eos_token_id,
        })
    }

    /// Autoregressively generates from `prompt`, calling `on_token` with
    /// each detokenized fragment as it's produced (real streaming, not
    /// generate-then-chunk) until EOS or `max_new_tokens`. Deterministic
    /// seed — reproducible output matters more than variety for a "does
    /// this actually work" prototype; revisit if answers feel too rigid.
    pub fn generate(
        &self,
        prompt: &str,
        max_new_tokens: usize,
        mut on_token: impl FnMut(&str),
    ) -> CoreResult<()> {
        let mut model = self
            .model
            .lock()
            .map_err(|_| CoreError::Extract("model lock poisoned".to_string()))?;

        let encoding = self
            .tokenizer
            .encode(prompt, true)
            .map_err(|e| CoreError::Extract(format!("tokenize failed: {e}")))?;
        let mut tokens = encoding.get_ids().to_vec();

        let mut logits_processor = LogitsProcessor::new(299792458, Some(0.7), Some(0.9));
        let mut index_pos = 0usize;

        // Decoding one freshly-generated token at a time (rather than the
        // growing generated-so-far sequence) loses word-boundary markers a
        // SentencePiece-style decoder relies on prior context for — every
        // call looks like "the first token of a sentence" to it, so its
        // leading space gets stripped every time and words run together
        // ("Plantsmakeenergy…"). Re-decoding the whole generated sequence
        // each step and emitting only the newly-appended suffix keeps
        // spacing correct at the cost of O(n^2) decode calls, cheap at
        // `max_new_tokens` ~200.
        let mut generated_ids: Vec<u32> = Vec::new();
        let mut decoded_len = 0usize;

        for index in 0..max_new_tokens {
            let (context_size, context_index) = if index > 0 {
                (1, index_pos)
            } else {
                (tokens.len(), 0)
            };
            let ctxt = &tokens[tokens.len() - context_size..];
            let input = Tensor::new(ctxt, &self.device)
                .and_then(|t| t.unsqueeze(0))
                .map_err(candle_err)?;
            let logits = model.forward(&input, context_index).map_err(candle_err)?;
            let logits = logits.squeeze(0).map_err(candle_err)?;
            index_pos += ctxt.len();

            let next_token = logits_processor.sample(&logits).map_err(candle_err)?;
            tokens.push(next_token);
            if next_token == self.eos_token_id {
                break;
            }
            generated_ids.push(next_token);
            if let Ok(full) = self.tokenizer.decode(&generated_ids, false) {
                if full.len() > decoded_len {
                    on_token(&full[decoded_len..]);
                    decoded_len = full.len();
                }
            }
        }

        Ok(())
    }
}

fn candle_err(e: candle_core::Error) -> CoreError {
    CoreError::Extract(format!("model inference failed: {e}"))
}

/// Model Cache download (`docs/ARCHITECTURE.md` §5.1). Downloads to a
/// `.part` sibling and renames on success, so a killed/interrupted
/// download can never leave a corrupt file at the real path that a later
/// run would mistake for complete.
fn download_if_missing(path: &Path, url: &str) -> CoreResult<()> {
    if path.exists() {
        return Ok(());
    }
    let tmp_path: PathBuf = path.with_extension("part");
    let status = std::process::Command::new("curl")
        .args(["-L", "-sS", "-o"])
        .arg(&tmp_path)
        .arg(url)
        .status()
        .map_err(|e| CoreError::Extract(format!("failed to invoke curl: {e}")))?;
    if !status.success() {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(CoreError::Extract(format!(
            "curl exited with status {status} downloading {url}"
        )));
    }
    std::fs::rename(&tmp_path, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The one test that actually proves the LLM path works end to end —
    /// real download, real GGUF load, real CPU inference. Deliberately
    /// `#[ignore]`d rather than part of the default suite: every other
    /// test that touches `Engine::llm()`/the `Query` RPC is ignored for
    /// the same reason (a real multi-hundred-MB download and real
    /// inference latency have no place in a fast default `cargo test`
    /// run) — this is the one place that risk is worth explicitly paying
    /// to verify, on demand, with `cargo test -- --ignored`.
    #[test]
    #[ignore = "downloads a real ~640MB model on first run; run explicitly with \
                `cargo test -- --ignored`"]
    fn load_and_generate_produces_real_text_from_a_real_model() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = LlmRuntime::load(dir.path()).unwrap();

        let prompt = "<|system|>\nYou are a helpful assistant.</s>\n<|user|>\nSay hello in one \
                      short sentence.</s>\n<|assistant|>\n";
        let mut output = String::new();
        runtime
            .generate(prompt, 40, |token| output.push_str(token))
            .unwrap();

        assert!(
            !output.trim().is_empty(),
            "model produced no output for a trivial prompt"
        );
    }
}
