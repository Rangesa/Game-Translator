use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};

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

// === Translator ===

#[allow(dead_code)]
pub enum TranslatorBackend {
    DeepL { api_key: String },
    LocalLLM { endpoint: String, model: String },
}

pub struct Translator {
    client: Client,
    backend: TranslatorBackend,
}

impl Translator {
    pub fn new_deepl(api_key: String) -> Self {
        Self {
            client: Client::new(),
            backend: TranslatorBackend::DeepL { api_key },
        }
    }

    #[allow(dead_code)]
    pub fn new_local(endpoint: String, model: String) -> Self {
        Self {
            client: Client::new(),
            backend: TranslatorBackend::LocalLLM { endpoint, model },
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

        let max_tokens = (texts.len() as u32 * 32).min(512);

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

        // 番号付き行からテキストを抽出
        let mut results: Vec<Option<String>> = vec![None; texts.len()];
        for line in raw.lines() {
            let line = line.trim();
            if line.is_empty() { continue; }
            // "1. 翻訳テキスト" パターンを探す
            if let Some(dot_pos) = line.find(". ") {
                if let Ok(num) = line[..dot_pos].trim().parse::<usize>() {
                    if num >= 1 && num <= texts.len() {
                        results[num - 1] = Some(line[dot_pos + 2..].to_string());
                    }
                }
            }
        }

        // パースできなかった場合はログ出力
        for (i, result) in results.iter().enumerate() {
            if result.is_none() {
                eprintln!("Translation missing for '{}' (raw: {})", texts[i], &raw[..raw.len().min(100)]);
            }
        }

        Ok(results)
    }
}
