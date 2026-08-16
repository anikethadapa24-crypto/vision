//! Answer synthesis (`docs/ROADMAP.md` M8): retrieval (`query::run`) feeds
//! a grounded prompt into the local LLM (`llm::LlmRuntime`), which streams
//! back real generated prose instead of raw ranked snippets. Retrieval
//! itself doesn't go away — it's the input to synthesis now, not what the
//! user sees directly.

use crate::engine::Engine;
use crate::error::CoreResult;
use crate::query::{self, RankedResult};

/// TinyLlama's chat template (from its GGUF repo's `chat_template` field):
/// `<|system|>\n{system}</s>\n<|user|>\n{prompt}</s>\n<|assistant|>\n`.
/// Hardcoded here rather than read from the GGUF at runtime — swapping
/// models later means swapping this template too, not a silent mismatch.
fn build_prompt(question: &str, sources: &[RankedResult]) -> String {
    let mut context = String::new();
    for (i, r) in sources.iter().enumerate() {
        context.push_str(&format!("[{}] ({})\n{}\n\n", i + 1, r.path, r.snippet));
    }

    let system = if sources.is_empty() {
        "You are Vision, a personal knowledge assistant. No indexed content matched this \
         question. Say so plainly in one short sentence — do not invent an answer."
            .to_string()
    } else {
        format!(
            "You are Vision, a personal knowledge assistant. Answer the user's question using \
             ONLY the numbered sources below — do not use outside knowledge. Cite sources \
             inline like [1]. If the sources don't answer the question, say so.\n\n{context}"
        )
    };

    format!("<|system|>\n{system}</s>\n<|user|>\n{question}</s>\n<|assistant|>\n")
}

/// Runs retrieval, then synthesis, streaming generated text fragments via
/// `on_token`. Returns the sources retrieval actually used as context, so
/// the caller can attach real citations to the final response regardless
/// of which sources the model happened to quote.
pub fn run(
    engine: &Engine,
    question: &str,
    top_k: usize,
    max_new_tokens: usize,
    on_token: impl FnMut(&str),
) -> CoreResult<Vec<RankedResult>> {
    let sources = query::run(engine, question, top_k)?;
    let prompt = build_prompt(question, &sources);
    engine.llm()?.generate(&prompt, max_new_tokens, on_token)?;
    Ok(sources)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(path: &str, snippet: &str) -> RankedResult {
        RankedResult {
            document_id: "doc-1".to_string(),
            path: path.to_string(),
            snippet: snippet.to_string(),
            score: 1.0,
            timestamp_unix_ms: 0,
        }
    }

    #[test]
    fn prompt_includes_the_question_and_every_source_snippet() {
        let sources = vec![
            result("a.md", "cats are mammals"),
            result("b.md", "cats have retractable claws"),
        ];
        let prompt = build_prompt("what are cats?", &sources);

        assert!(prompt.contains("what are cats?"));
        assert!(prompt.contains("cats are mammals"));
        assert!(prompt.contains("cats have retractable claws"));
        assert!(prompt.contains("<|system|>"));
        assert!(prompt.contains("<|assistant|>"));
    }

    #[test]
    fn prompt_with_no_sources_instructs_the_model_not_to_invent_an_answer() {
        let prompt = build_prompt("what are cats?", &[]);
        assert!(prompt.contains("No indexed content matched"));
    }
}
