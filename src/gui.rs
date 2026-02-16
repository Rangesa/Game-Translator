use eframe::egui;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

use crate::capture::list_windows;
use crate::config::{AppConfig, TranslationEngine};
use crate::overlay::OverlayConfig;

/// Status message displayed in the GUI
#[derive(Clone)]
enum AppStatus {
    Idle,
    Running,
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
            eprintln!("Failed to save config: {}", e);
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
                eprintln!("Overlay thread error: {}", e);
            }
        });

        self.overlay_thread = Some(handle);
        self.status = AppStatus::Running;
    }

    fn stop(&mut self) {
        self.stop_signal.store(true, Ordering::SeqCst);

        // Send WM_DESTROY to overlay window to break the message loop
        let hwnd_raw = self.overlay_hwnd_raw.load(Ordering::SeqCst);
        if hwnd_raw != 0 {
            unsafe {
                use windows::Win32::Foundation::*;
                use windows::Win32::UI::WindowsAndMessaging::*;
                let hwnd = HWND(hwnd_raw as *mut _);
                let _ = PostMessageW(Some(hwnd), WM_DESTROY, WPARAM(0), LPARAM(0));
            }
        }

        // Wait for overlay thread to finish
        if let Some(handle) = self.overlay_thread.take() {
            let _ = handle.join();
        }

        self.overlay_hwnd_raw.store(0, Ordering::SeqCst);
        self.status = AppStatus::Idle;
    }

    fn is_running(&self) -> bool {
        matches!(self.status, AppStatus::Running)
    }
}

impl eframe::App for GameTranslatorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
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
                }

                ui.horizontal(|ui| {
                    ui.label("ソース言語:");
                    ui.text_edit_singleline(&mut self.config.source_lang);
                    ui.label("ターゲット言語:");
                    ui.text_edit_singleline(&mut self.config.target_lang);
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
                let running = self.is_running();
                if !running {
                    if ui
                        .add_sized([120.0, 30.0], egui::Button::new("開始"))
                        .clicked()
                    {
                        self.start();
                    }
                } else {
                    if ui
                        .add_sized([120.0, 30.0], egui::Button::new("停止"))
                        .clicked()
                    {
                        self.stop();
                    }
                }

                ui.add_space(16.0);

                match &self.status {
                    AppStatus::Idle => {
                        ui.label("待機中");
                    }
                    AppStatus::Running => {
                        ui.colored_label(egui::Color32::GREEN, "実行中");
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
        }
    }
}
