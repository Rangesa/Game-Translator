use eframe::egui;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use crate::capture::list_windows;
use crate::config::{AppConfig, TranslationEngine};
use crate::overlay::OverlayConfig;
use crate::translate::Translator;

/// Status message displayed in the GUI
#[derive(Clone)]
enum AppStatus {
    Idle,
    Running,
    Stopping,
    Error(String),
}

pub struct GameTranslatorApp {
    config: AppConfig,
    /// List of (hwnd_raw, title)
    window_list: Vec<(isize, String)>,
    selected_window_index: Option<usize>,
    status: AppStatus,
    /// Stop signal for the capture thread
    stop_signal: Arc<AtomicBool>,
    /// Handle to the overlay thread
    overlay_thread: Option<JoinHandle<()>>,
    /// Overlay HWND for sending WM_DESTROY
    overlay_hwnd_raw: Arc<std::sync::atomic::AtomicIsize>,
    /// API test result (None = not tested / in progress, Some = result message)
    api_test_result: Arc<Mutex<Option<String>>>,
    api_testing: Arc<AtomicBool>,
    debug_log: bool,
}

impl GameTranslatorApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Load Japanese font
        let mut fonts = egui::FontDefinitions::default();
        let font_data = include_bytes!("../makinas4/Makinas-4-Square.otf");
        fonts.font_data.insert(
            "Makinas4".to_owned(),
            std::sync::Arc::new(egui::FontData::from_static(font_data)),
        );
        // Put Japanese font first for proportional text
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "Makinas4".to_owned());
        fonts
            .families
            .entry(egui::FontFamily::Monospace)
            .or_default()
            .push("Makinas4".to_owned());
        cc.egui_ctx.set_fonts(fonts);

        let config = AppConfig::load();
        let mut app = Self {
            config,
            window_list: Vec::new(),
            selected_window_index: None,
            status: AppStatus::Idle,
            stop_signal: Arc::new(AtomicBool::new(false)),
            overlay_thread: None,
            overlay_hwnd_raw: Arc::new(std::sync::atomic::AtomicIsize::new(0)),
            api_test_result: Arc::new(Mutex::new(None)),
            api_testing: Arc::new(AtomicBool::new(false)),
            debug_log: false,
        };
        app.refresh_windows();
        app
    }

    fn refresh_windows(&mut self) {
        self.window_list = list_windows();
        self.selected_window_index = None;
    }

    fn start(&mut self) {
        // Validate config
        match self.config.translation_engine {
            TranslationEngine::DeepL => {
                if self.config.deepl_api_key.trim().is_empty() {
                    self.status = AppStatus::Error("DeepL APIキーが未設定です".to_string());
                    return;
                }
            }
            TranslationEngine::LocalLLM => {
                if self.config.local_llm_endpoint.trim().is_empty() {
                    self.status = AppStatus::Error("LLMエンドポイントが未設定です".to_string());
                    return;
                }
            }
            TranslationEngine::Groq => {
                if self.config.groq_api_key.trim().is_empty() {
                    self.status = AppStatus::Error("Groq APIキーが未設定です".to_string());
                    return;
                }
            }
        }

        let target_hwnd_raw = match self.selected_window_index {
            Some(idx) if idx < self.window_list.len() => self.window_list[idx].0,
            _ => {
                self.status = AppStatus::Error("ウィンドウを選択してください".to_string());
                return;
            }
        };

        // Save config
        if let Err(e) = self.config.save() {
            crate::log_always(&format!("Failed to save config: {}", e));
        }

        // Reset stop signal
        self.stop_signal.store(false, Ordering::SeqCst);
        let stop_signal = self.stop_signal.clone();
        let overlay_hwnd_arc = self.overlay_hwnd_raw.clone();

        let overlay_config = OverlayConfig {
            text_color: self.config.overlay_text_color,
            bg_color: self.config.overlay_bg_color,
        };

        let config = self.config.clone();

        let handle = std::thread::spawn(move || {
            if let Err(e) = crate::run_overlay_thread(
                target_hwnd_raw,
                config,
                overlay_config,
                stop_signal,
                overlay_hwnd_arc,
            ) {
                crate::log_always(&format!("Overlay thread error: {}", e));
            }
        });

        self.overlay_thread = Some(handle);
        self.status = AppStatus::Running;
    }

    fn stop(&mut self) {
        self.stop_signal.store(true, Ordering::SeqCst);

        // Send WM_CLOSE to overlay window to break the message loop
        let hwnd_raw = self.overlay_hwnd_raw.load(Ordering::SeqCst);
        if hwnd_raw != 0 {
            unsafe {
                use windows::Win32::Foundation::*;
                use windows::Win32::UI::WindowsAndMessaging::*;
                let hwnd = HWND(hwnd_raw as *mut _);
                let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        }

        self.status = AppStatus::Stopping;
    }

    /// Check if the overlay thread has finished and clean up.
    fn poll_thread_completion(&mut self) {
        if let Some(handle) = &self.overlay_thread {
            if handle.is_finished() {
                if let Some(handle) = self.overlay_thread.take() {
                    let _ = handle.join();
                }
                self.overlay_hwnd_raw.store(0, Ordering::SeqCst);
                self.status = AppStatus::Idle;
            }
        }
    }

    fn is_running(&self) -> bool {
        matches!(self.status, AppStatus::Running | AppStatus::Stopping)
    }

    fn start_api_test(&self) {
        if self.api_testing.load(Ordering::SeqCst) {
            return;
        }
        self.api_testing.store(true, Ordering::SeqCst);
        *self.api_test_result.lock().unwrap() = None;

        let translator = match self.config.translation_engine {
            TranslationEngine::DeepL => Translator::new_deepl(self.config.deepl_api_key.clone()),
            TranslationEngine::LocalLLM => Translator::new_local(
                self.config.local_llm_endpoint.clone(),
                self.config.local_llm_model.clone(),
            ),
            TranslationEngine::Groq => Translator::new_groq(
                self.config.groq_api_key.clone(),
                self.config.groq_model.clone(),
            ),
        };

        let source = self.config.source_lang.clone();
        let target = self.config.target_lang.clone();
        let result = self.api_test_result.clone();
        let testing = self.api_testing.clone();

        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let start = std::time::Instant::now();
            let res = rt.block_on(translator.translate_batch(
                vec!["Hello".to_string()],
                &source,
                &target,
            ));
            let elapsed = start.elapsed();

            let msg = match res {
                Ok(translations) => {
                    let translated = translations.first()
                        .and_then(|t| t.clone())
                        .unwrap_or_else(|| "(empty)".to_string());
                    format!("OK: \"{}\" ({:.0}ms)", translated, elapsed.as_millis())
                }
                Err(e) => format!("NG: {}", e),
            };

            *result.lock().unwrap() = Some(msg);
            testing.store(false, Ordering::SeqCst);
        });
    }
}

impl eframe::App for GameTranslatorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Poll overlay thread completion without blocking
        if matches!(self.status, AppStatus::Stopping) {
            self.poll_thread_completion();
            ctx.request_repaint();
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Game Translator");
            ui.separator();

            // === Window Selection ===
            ui.group(|ui| {
                ui.label("対象ウィンドウ");
                ui.horizontal(|ui| {
                    if ui.button("更新").clicked() {
                        self.refresh_windows();
                    }
                    let selected_label = self
                        .selected_window_index
                        .and_then(|idx| self.window_list.get(idx))
                        .map(|(_, title)| title.as_str())
                        .unwrap_or("-- 選択してください --");

                    egui::ComboBox::from_id_salt("window_select")
                        .selected_text(selected_label)
                        .width(400.0)
                        .show_ui(ui, |ui| {
                            for (i, (_, title)) in self.window_list.iter().enumerate() {
                                ui.selectable_value(
                                    &mut self.selected_window_index,
                                    Some(i),
                                    title,
                                );
                            }
                        });
                });
            });

            ui.add_space(8.0);

            // === Translation Settings ===
            ui.group(|ui| {
                ui.label("翻訳設定");

                ui.horizontal(|ui| {
                    ui.radio_value(
                        &mut self.config.translation_engine,
                        TranslationEngine::DeepL,
                        "DeepL",
                    );
                    ui.radio_value(
                        &mut self.config.translation_engine,
                        TranslationEngine::LocalLLM,
                        "Local LLM",
                    );
                    ui.radio_value(
                        &mut self.config.translation_engine,
                        TranslationEngine::Groq,
                        "Groq",
                    );
                });

                match self.config.translation_engine {
                    TranslationEngine::DeepL => {
                        ui.horizontal(|ui| {
                            ui.label("APIキー:");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.config.deepl_api_key)
                                    .password(true)
                                    .desired_width(300.0),
                            );
                        });
                    }
                    TranslationEngine::LocalLLM => {
                        ui.horizontal(|ui| {
                            ui.label("エンドポイント:");
                            ui.text_edit_singleline(&mut self.config.local_llm_endpoint);
                        });
                        ui.horizontal(|ui| {
                            ui.label("モデル:");
                            ui.text_edit_singleline(&mut self.config.local_llm_model);
                        });
                    }
                    TranslationEngine::Groq => {
                        ui.horizontal(|ui| {
                            ui.label("APIキー:");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.config.groq_api_key)
                                    .password(true)
                                    .desired_width(300.0),
                            );
                        });
                        ui.horizontal(|ui| {
                            ui.label("モデル:");
                            ui.text_edit_singleline(&mut self.config.groq_model);
                        });
                    }
                }

                ui.horizontal(|ui| {
                    ui.label("ソース言語:");
                    ui.text_edit_singleline(&mut self.config.source_lang);
                    ui.label("ターゲット言語:");
                    ui.text_edit_singleline(&mut self.config.target_lang);
                });

                ui.horizontal(|ui| {
                    let testing = self.api_testing.load(Ordering::SeqCst);
                    if testing {
                        ui.add_enabled(false, egui::Button::new("テスト中..."));
                        ui.ctx().request_repaint();
                    } else if ui.button("接続テスト").clicked() {
                        self.start_api_test();
                    }

                    if let Some(msg) = self.api_test_result.lock().unwrap().as_ref() {
                        if msg.starts_with("OK") {
                            ui.colored_label(egui::Color32::GREEN, msg);
                        } else {
                            ui.colored_label(egui::Color32::RED, msg);
                        }
                    }
                });
            });

            ui.add_space(8.0);

            // === Overlay Appearance ===
            ui.group(|ui| {
                ui.label("オーバーレイ外観");
                ui.horizontal(|ui| {
                    ui.label("テキスト色:");
                    ui.color_edit_button_rgba_unmultiplied(&mut self.config.overlay_text_color);
                    ui.label("背景色:");
                    ui.color_edit_button_rgba_unmultiplied(&mut self.config.overlay_bg_color);
                });
            });

            ui.add_space(12.0);

            // === Controls ===
            ui.horizontal(|ui| {
                match &self.status {
                    AppStatus::Idle | AppStatus::Error(_) => {
                        if ui
                            .add_sized([120.0, 30.0], egui::Button::new("開始"))
                            .clicked()
                        {
                            self.start();
                        }
                    }
                    AppStatus::Running => {
                        if ui
                            .add_sized([120.0, 30.0], egui::Button::new("停止"))
                            .clicked()
                        {
                            self.stop();
                        }
                    }
                    AppStatus::Stopping => {
                        ui.add_enabled(false, egui::Button::new("停止中...").min_size(egui::vec2(120.0, 30.0)));
                    }
                }

                ui.add_space(16.0);

                if ui.checkbox(&mut self.debug_log, "Debug Log").changed() {
                    crate::config::set_debug_log(self.debug_log);
                }

                ui.add_space(16.0);

                match &self.status {
                    AppStatus::Idle => {
                        ui.label("待機中");
                    }
                    AppStatus::Running => {
                        ui.colored_label(egui::Color32::GREEN, "実行中");
                    }
                    AppStatus::Stopping => {
                        ui.colored_label(egui::Color32::YELLOW, "停止中...");
                    }
                    AppStatus::Error(msg) => {
                        ui.colored_label(egui::Color32::RED, msg.as_str());
                    }
                }
            });
        });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if self.is_running() {
            self.stop();
            // Block on exit to ensure clean shutdown
            if let Some(handle) = self.overlay_thread.take() {
                let _ = handle.join();
            }
        }
    }
}
