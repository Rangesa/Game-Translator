use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

static DEBUG_LOG_ENABLED: AtomicBool = AtomicBool::new(false);

pub fn set_debug_log(enabled: bool) {
    DEBUG_LOG_ENABLED.store(enabled, Ordering::SeqCst);
}

pub fn is_debug_log() -> bool {
    DEBUG_LOG_ENABLED.load(Ordering::SeqCst)
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum TranslationEngine {
    DeepL,
    LocalLLM,
    Groq,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub translation_engine: TranslationEngine,
    pub deepl_api_key: String,
    pub local_llm_endpoint: String,
    pub local_llm_model: String,
    pub groq_api_key: String,
    pub groq_model: String,
    pub source_lang: String,
    pub target_lang: String,
    pub overlay_text_color: [f32; 4],
    pub overlay_bg_color: [f32; 4],
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            translation_engine: TranslationEngine::DeepL,
            deepl_api_key: String::new(),
            local_llm_endpoint: "http://localhost:5000".to_string(),
            local_llm_model: "default".to_string(),
            groq_api_key: String::new(),
            groq_model: "llama-3.3-70b-versatile".to_string(),
            source_lang: "EN".to_string(),
            target_lang: "JA".to_string(),
            overlay_text_color: [1.0, 1.0, 0.0, 1.0], // Yellow
            overlay_bg_color: [0.0, 0.0, 0.0, 0.85],   // Semi-transparent black
        }
    }
}

impl AppConfig {
    fn config_path() -> PathBuf {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."));
        exe_dir.join("config.toml")
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(config) = toml::from_str(&content) {
                    return config;
                }
            }
        }
        // Try loading API key from .env for migration
        let mut config = Self::default();
        if config.deepl_api_key.is_empty() {
            if let Ok(key) = std::env::var("DEEPL_API_KEY") {
                if !key.is_empty() {
                    config.deepl_api_key = key;
                }
            }
            if config.deepl_api_key.is_empty() {
                let env_path = std::path::Path::new(".env");
                if env_path.exists() {
                    if let Ok(content) = std::fs::read_to_string(env_path) {
                        for line in content.lines() {
                            if let Some(val) = line.strip_prefix("DEEPL_API_KEY=") {
                                let val = val.trim().trim_matches('"');
                                if !val.is_empty() {
                                    config.deepl_api_key = val.to_string();
                                }
                            }
                        }
                    }
                }
            }
        }
        config
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path();
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }
}
