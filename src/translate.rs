use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Truncate a string to at most `max_chars` characters (safe for multi-byte UTF-8).
fn truncate_str(s: &str, max_chars: usize) -> &str {
    match s.char_indices().nth(max_chars) {
        Some((idx, _)) => &s[..idx],
        None => s,
    }
}

fn tlog(msg: &str) {
    crate::log(msg);
}

// === DeepL API ===

#[derive(Debug, Serialize)]
struct DeepLRequest {
    text: Vec<String>,
    target_lang: String,
    source_lang: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DeepLResponse {
    translations: Vec<DeepLTranslation>,
}

#[derive(Debug, Deserialize)]
struct DeepLTranslation {
    text: String,
}

// === OpenAI Chat Completions API (Groq等) ===

#[derive(Debug, Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f32,
    max_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatCompletionChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionChoice {
    message: ChatMessageResponse,
}

#[derive(Debug, Deserialize)]
struct ChatMessageResponse {
    content: String,
}

// === OpenAI互換API (ExLlama3等) ===

#[derive(Debug, Serialize)]
struct CompletionRequest {
    model: String,
    prompt: String,
    temperature: f32,
    max_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct CompletionResponse {
    choices: Vec<CompletionChoice>,
}

#[derive(Debug, Deserialize)]
struct CompletionChoice {
    text: String,
}

// === 番号付きレスポンス解析 ===

/// "N. テキスト" 形式のレスポンスを解析。
/// 複数行にまたがる翻訳にも対応（次の番号行が来るまで結合）。
fn parse_numbered_response(raw: &str, count: usize) -> Vec<Option<String>> {
    let mut results: Vec<Option<String>> = vec![None; count];
    let mut current_num: Option<usize> = None;
    let mut current_text = String::new();

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() { continue; }

        // "N. テキスト" パターンをチェック
        let mut matched_num = None;
        if let Some(dot_pos) = trimmed.find(". ") {
            if let Ok(num) = trimmed[..dot_pos].trim().parse::<usize>() {
                if num >= 1 && num <= count {
                    matched_num = Some((num, trimmed[dot_pos + 2..].to_string()));
                }
            }
        }

        if let Some((num, text)) = matched_num {
            // 前の番号のテキストを確定
            if let Some(prev) = current_num {
                if prev >= 1 && prev <= count {
                    results[prev - 1] = Some(current_text.trim().to_string());
                }
            }
            current_num = Some(num);
            current_text = text;
        } else if current_num.is_some() {
            // 続きの行を結合
            current_text.push(' ');
            current_text.push_str(trimmed);
        }
    }

    // 最後の番号のテキストを確定
    if let Some(prev) = current_num {
        if prev >= 1 && prev <= count {
            results[prev - 1] = Some(current_text.trim().to_string());
        }
    }

    // 1件だけで番号なしの場合のフォールバック
    if count == 1 && results[0].is_none() && !raw.trim().is_empty() {
        results[0] = Some(raw.trim().to_string());
    }

    results
}

// === Translator ===

#[allow(dead_code)]
pub enum TranslatorBackend {
    DeepL { api_key: String },
    LocalLLM { endpoint: String, model: String },
    Groq { api_key: String, model: String },
}

pub struct Translator {
    client: Client,
    backend: TranslatorBackend,
}

impl Translator {
    pub fn new_deepl(api_key: String) -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .unwrap_or_else(|_| Client::new()),
            backend: TranslatorBackend::DeepL { api_key },
        }
    }

    #[allow(dead_code)]
    pub fn new_local(endpoint: String, model: String) -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .unwrap_or_else(|_| Client::new()),
            backend: TranslatorBackend::LocalLLM { endpoint, model },
        }
    }

    pub fn new_groq(api_key: String, model: String) -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .unwrap_or_else(|_| Client::new()),
            backend: TranslatorBackend::Groq { api_key, model },
        }
    }

    pub async fn translate_batch(&self, texts: Vec<String>, from: &str, to: &str) -> Result<Vec<Option<String>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        // Track which original indices have non-empty text
        let non_empty_indices: Vec<usize> = texts.iter()
            .enumerate()
            .filter(|(_, t)| !t.trim().is_empty())
            .map(|(i, _)| i)
            .collect();

        if non_empty_indices.is_empty() {
            return Ok(vec![None; texts.len()]);
        }

        let non_empty_texts: Vec<String> = non_empty_indices.iter()
            .map(|&i| texts[i].clone())
            .collect();

        let translated = match &self.backend {
            TranslatorBackend::DeepL { api_key } => {
                self.translate_deepl(&non_empty_texts, from, to, api_key).await?
            }
            TranslatorBackend::LocalLLM { endpoint, model } => {
                self.translate_local(&non_empty_texts, from, to, endpoint, model).await?
            }
            TranslatorBackend::Groq { api_key, model } => {
                self.translate_groq(&non_empty_texts, from, to, api_key, model).await?
            }
        };

        // Map results back to original indices
        let mut results = vec![None; texts.len()];
        for (translated_idx, &original_idx) in non_empty_indices.iter().enumerate() {
            if translated_idx < translated.len() {
                results[original_idx] = translated[translated_idx].clone();
            }
        }

        Ok(results)
    }

    async fn translate_deepl(&self, texts: &[String], from: &str, to: &str, api_key: &str) -> Result<Vec<Option<String>>> {
        let request = DeepLRequest {
            text: texts.to_vec(),
            target_lang: to.to_uppercase(),
            source_lang: Some(from.to_uppercase()),
        };

        // Free API keys end with ":fx", Pro keys don't
        let base_url = if api_key.ends_with(":fx") {
            "https://api-free.deepl.com/v2/translate"
        } else {
            "https://api.deepl.com/v2/translate"
        };

        let response = self.client
            .post(base_url)
            .header("Authorization", format!("DeepL-Auth-Key {}", api_key))
            .json(&request)
            .send()
            .await
            .context("Failed to send DeepL request")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("DeepL API error: {} - {}", status, body);
        }

        let resp: DeepLResponse = response.json().await
            .context("Failed to parse DeepL response")?;

        Ok(resp.translations.iter().map(|t| Some(t.text.clone())).collect())
    }

    async fn translate_local(&self, texts: &[String], _from: &str, _to: &str, endpoint: &str, model: &str) -> Result<Vec<Option<String>>> {
        let url = format!("{}/v1/completions", endpoint.trim_end_matches('/'));

        // 全テキストを1リクエストにバッチ化（速度重視）
        let numbered: Vec<String> = texts.iter().enumerate()
            .map(|(i, t)| format!("{}. {}", i + 1, t))
            .collect();
        let input_block = numbered.join("\n");

        let prompt = format!(
            "<start_of_turn>user\nTranslate each numbered line from English to Japanese. Output ONLY the translations, one per line, keeping the same numbering.\n\n{}<end_of_turn>\n<start_of_turn>model\n",
            input_block
        );

        let max_tokens = (texts.len() as u32 * 64).min(1024);

        let request = CompletionRequest {
            model: model.to_string(),
            prompt,
            temperature: 0.1,
            max_tokens,
        };

        let response = self.client
            .post(&url)
            .json(&request)
            .send()
            .await
            .context("Failed to send request to local LLM")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Local LLM error: {} - {}", status, body);
        }

        let resp: CompletionResponse = response.json().await
            .context("Failed to parse local LLM response")?;

        let raw = resp.choices.first()
            .map(|c| c.text.trim().to_string())
            .unwrap_or_default();

        Ok(parse_numbered_response(&raw, texts.len()))
    }

    async fn translate_groq(&self, texts: &[String], from: &str, to: &str, api_key: &str, model: &str) -> Result<Vec<Option<String>>> {
        let numbered: Vec<String> = texts.iter().enumerate()
            .map(|(i, t)| format!("{}. {}", i + 1, t))
            .collect();
        let input_block = numbered.join("\n");

        let lang_pair = format!("{} to {}", from, to);

        let request = ChatCompletionRequest {
            model: model.to_string(),
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: format!(
                        "You are a translator. Translate each numbered line from {}. Output ONLY the translations, one per line, keeping the same numbering. No explanations.",
                        lang_pair
                    ),
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: input_block,
                },
            ],
            temperature: 0.3,
            max_tokens: (texts.len() as u32 * 128).min(2048),
        };

        let response = self.client
            .post("https://api.groq.com/openai/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&request)
            .send()
            .await
            .context("Failed to send Groq request")?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            tlog(&format!("[GROQ ERR] HTTP {} - {}", status, truncate_str(&body, 200)));
            anyhow::bail!("Groq API error: {} - {}", status, body);
        }

        let body_text = response.text().await
            .context("Failed to read Groq response body")?;

        let resp: ChatCompletionResponse = serde_json::from_str(&body_text)
            .context("Failed to parse Groq response JSON")?;

        let raw = resp.choices.first()
            .map(|c| c.message.content.trim().to_string())
            .unwrap_or_default();

        tlog(&format!("[GROQ RAW] count={} raw={}", texts.len(), truncate_str(&raw, 300)));

        let results = parse_numbered_response(&raw, texts.len());
        let fail_count = results.iter().filter(|r| r.is_none()).count();
        if fail_count > 0 {
            tlog(&format!("[GROQ PARSE] {} fails out of {}. Full raw: {}", fail_count, texts.len(), truncate_str(&raw, 500)));
        }

        Ok(results)
    }
}
