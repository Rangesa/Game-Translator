#![windows_subsystem = "windows"]

mod capture;
mod config;
mod gui;
mod ocr;
mod overlay;
mod translate;

use anyhow::Result;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{mpsc, Arc};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::*;

use crate::capture::WindowCapture;
use crate::config::{AppConfig, TranslationEngine};
use crate::ocr::OCREngine;
use crate::overlay::{Overlay, OverlayConfig, TranslatedText};
use crate::translate::Translator;
use eframe::egui;

const WM_RENDER: u32 = WM_USER + 1;

/// Render command sent from background thread to overlay thread
enum RenderCommand {
    Draw(Vec<TranslatedText>),
    Clear,
}

/// Store receiver in window's user data
struct WndState {
    overlay: Overlay,
    overlay_hwnd: HWND,
    rx: mpsc::Receiver<RenderCommand>,
}

// Global pointer for wndproc access (set once before message loop)
static mut WND_STATE: *mut WndState = std::ptr::null_mut();

unsafe extern "system" fn wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let _hdc = BeginPaint(hwnd, &mut ps);
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_RENDER => {
            // Process all pending render commands
            if !WND_STATE.is_null() {
                let state = &mut *WND_STATE;
                while let Ok(cmd) = state.rx.try_recv() {
                    match cmd {
                        RenderCommand::Draw(texts) => {
                            if let Err(e) = state.overlay.render(&texts, state.overlay_hwnd) {
                                eprintln!("Render error: {:?}", e);
                            }
                        }
                        RenderCommand::Clear => {
                            let _ = state.overlay.clear(state.overlay_hwnd);
                        }
                    }
                }
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn create_transparent_window() -> Result<HWND> {
    unsafe {
        let instance = GetModuleHandleW(None)?;

        let class_name = w!("GameTranslatorOverlay");

        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wndproc),
            hInstance: HINSTANCE(instance.0),
            lpszClassName: class_name,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            hbrBackground: HBRUSH(GetStockObject(NULL_BRUSH).0),
            ..Default::default()
        };

        RegisterClassW(&wc);

        let virtual_x = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let virtual_y = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let virtual_width = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let virtual_height = GetSystemMetrics(SM_CYVIRTUALSCREEN);

        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST,
            class_name,
            w!("Game Translator Overlay"),
            WS_POPUP,
            virtual_x,
            virtual_y,
            virtual_width,
            virtual_height,
            None,
            None,
            Some(HINSTANCE(instance.0)),
            None,
        )?;

        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = UpdateWindow(hwnd);

        Ok(hwnd)
    }
}

const CACHE_FILE: &str = "translation_cache.json";

fn load_cache() -> HashMap<String, String> {
    let path = std::path::Path::new(CACHE_FILE);
    if path.exists() {
        if let Ok(data) = std::fs::read_to_string(path) {
            if let Ok(map) = serde_json::from_str(&data) {
                return map;
            }
        }
    }
    HashMap::new()
}

fn save_cache(cache: &HashMap<String, String>) {
    if let Ok(json) = serde_json::to_string(cache) {
        let _ = std::fs::write(CACHE_FILE, json);
    }
}

fn texts_changed(current: &[String], previous: &[String]) -> bool {
    if current.len() != previous.len() {
        return true;
    }
    current.iter().zip(previous.iter()).any(|(a, b)| a != b)
}

async fn capture_and_translate_loop(
    translator: Arc<Translator>,
    tx: mpsc::Sender<RenderCommand>,
    overlay_hwnd: HWND,
    target_hwnd: HWND,
    stop_signal: Arc<AtomicBool>,
    source_lang: String,
    target_lang: String,
) -> Result<()> {
    let mut capture = WindowCapture::new(target_hwnd)?;
    let ocr = OCREngine::new()?;

    let mut translation_cache = load_cache();
    println!("  キャッシュ読み込み: {}件", translation_cache.len());
    let mut prev_texts: Vec<String> = Vec::new();
    let mut no_change_count: u32 = 0;

    println!("Starting capture loop...");

    loop {
        // Check stop signal
        if stop_signal.load(Ordering::SeqCst) {
            println!("[EXIT] 停止シグナル受信");
            unsafe {
                let _ = PostMessageW(Some(overlay_hwnd), WM_DESTROY, WPARAM(0), LPARAM(0));
            }
            break;
        }

        let interval = if no_change_count > 10 {
            2000
        } else if no_change_count > 5 {
            1000
        } else {
            200
        };

        // 対象ウィンドウが閉じられたかチェック
        if !unsafe { IsWindow(Some(target_hwnd)) }.as_bool() {
            println!("[EXIT] 対象ウィンドウが閉じられました");
            unsafe {
                let _ = PostMessageW(Some(overlay_hwnd), WM_DESTROY, WPARAM(0), LPARAM(0));
            }
            break;
        }

        if let Some(frame_data) = capture.capture_frame()? {
            let (width, height) = capture.get_dimensions();
            let (win_x, win_y) = capture.get_window_position();

            let text_regions = ocr.detect_text(&frame_data, width, height).await?;

            if !text_regions.is_empty() {
                let current_texts: Vec<String> =
                    text_regions.iter().map(|r| r.text.clone()).collect();

                if texts_changed(&current_texts, &prev_texts) {
                    no_change_count = 0;

                    let uncached: Vec<String> = current_texts
                        .iter()
                        .filter(|t| !translation_cache.contains_key(*t))
                        .cloned()
                        .collect();

                    if !uncached.is_empty() {
                        println!(
                            "\n[NEW] {}個の未翻訳テキスト (キャッシュ: {}件)",
                            uncached.len(),
                            translation_cache.len()
                        );
                        for text in &uncached {
                            println!("  - {}", text);
                        }

                        let translations = translator
                            .translate_batch(uncached.clone(), &source_lang, &target_lang)
                            .await?;

                        let mut new_entries = false;
                        for (orig, trans) in uncached.iter().zip(translations.iter()) {
                            if let Some(t) = trans {
                                println!("  -> {}", t);
                                translation_cache.insert(orig.clone(), t.clone());
                                new_entries = true;
                            } else {
                                println!("  -> [翻訳失敗] {}", orig);
                            }
                        }
                        if new_entries {
                            save_cache(&translation_cache);
                        }
                    } else {
                        println!(
                            "\n[CACHE HIT] {}個すべてキャッシュ済み",
                            current_texts.len()
                        );
                    }

                    let mut translated_texts = Vec::new();
                    for region in &text_regions {
                        if let Some(translation) = translation_cache.get(&region.text) {
                            translated_texts.push(TranslatedText {
                                translated_text: translation.clone(),
                                x: region.x as f32 + win_x as f32,
                                y: region.y as f32 + win_y as f32,
                                max_width: region.width as f32 * 1.3,
                                font_size: region.height as f32,
                            });
                        }
                    }

                    // Send render command to overlay thread
                    let _ = tx.send(RenderCommand::Draw(translated_texts));
                    unsafe {
                        let _ = PostMessageW(
                            Some(overlay_hwnd),
                            WM_RENDER,
                            WPARAM(0),
                            LPARAM(0),
                        );
                    }

                    prev_texts = current_texts;
                } else {
                    no_change_count += 1;
                    if no_change_count == 1 {
                        println!("[NO CHANGE] 翻訳スキップ (間隔: {}ms)", interval);
                    }
                }
            } else {
                if !prev_texts.is_empty() {
                    let _ = tx.send(RenderCommand::Clear);
                    unsafe {
                        let _ = PostMessageW(
                            Some(overlay_hwnd),
                            WM_RENDER,
                            WPARAM(0),
                            LPARAM(0),
                        );
                    }
                    prev_texts.clear();
                    println!("[CLEAR] テキスト未検出 - オーバーレイクリア");
                }
                no_change_count += 1;
            }
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(interval as u64)).await;
    }

    Ok(())
}

/// Run overlay window + capture loop on a dedicated thread.
/// Called from the GUI's Start button.
pub fn run_overlay_thread(
    target_hwnd_raw: isize,
    config: AppConfig,
    overlay_config: OverlayConfig,
    stop_signal: Arc<AtomicBool>,
    overlay_hwnd_arc: Arc<AtomicIsize>,
) -> Result<()> {
    // DPI awareness
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }

    // Create translator based on config
    let translator = Arc::new(match config.translation_engine {
        TranslationEngine::DeepL => Translator::new_deepl(config.deepl_api_key.clone()),
        TranslationEngine::LocalLLM => {
            Translator::new_local(config.local_llm_endpoint.clone(), config.local_llm_model.clone())
        }
    });

    let source_lang = config.source_lang.clone();
    let target_lang = config.target_lang.clone();

    // Create overlay window
    let overlay_hwnd = create_transparent_window()?;
    overlay_hwnd_arc.store(overlay_hwnd.0 as isize, Ordering::SeqCst);
    println!("  Overlay window created");

    let mut overlay = Overlay::new(overlay_config)?;
    println!("  Overlay renderer initialized");

    unsafe {
        let mut rect = RECT::default();
        GetClientRect(overlay_hwnd, &mut rect)?;
        let virtual_x = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let virtual_y = GetSystemMetrics(SM_YVIRTUALSCREEN);
        overlay.create_render_target(
            overlay_hwnd,
            (rect.right - rect.left) as u32,
            (rect.bottom - rect.top) as u32,
            virtual_x,
            virtual_y,
        )?;
    }
    println!("  Render target created");

    // Clear initial state (prevent black screen)
    overlay.clear(overlay_hwnd)?;

    // Channel for render commands
    let (tx, rx) = mpsc::channel::<RenderCommand>();

    // Set up window state for wndproc
    let mut wnd_state = WndState {
        overlay,
        overlay_hwnd,
        rx,
    };
    unsafe {
        WND_STATE = &mut wnd_state as *mut WndState;
    }

    println!("Starting translation service...");

    let overlay_hwnd_raw = overlay_hwnd.0 as isize;

    // Spawn capture thread
    let capture_stop = stop_signal.clone();
    std::thread::spawn(move || {
        let overlay_hwnd = HWND(overlay_hwnd_raw as *mut _);
        let target_hwnd = HWND(target_hwnd_raw as *mut _);
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            if let Err(e) = capture_and_translate_loop(
                translator,
                tx,
                overlay_hwnd,
                target_hwnd,
                capture_stop,
                source_lang,
                target_lang,
            )
            .await
            {
                eprintln!("Error in capture loop: {}", e);
            }
        });
    });

    // Windows message loop (overlay runs on this thread)
    unsafe {
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        // Clean up global pointer
        WND_STATE = std::ptr::null_mut();
    }

    overlay_hwnd_arc.store(0, Ordering::SeqCst);
    Ok(())
}

fn main() -> eframe::Result {
    // DPI awareness (set early)
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([560.0, 400.0])
            .with_min_inner_size([400.0, 300.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Game Translator",
        options,
        Box::new(|cc| Ok(Box::new(gui::GameTranslatorApp::new(cc)))),
    )
}
