use anyhow::Result;
use windows::core::Interface;
use windows::Graphics::Imaging::*;
use windows::Media::Ocr::*;
use windows::Win32::System::WinRT::IMemoryBufferByteAccess;

/// OCRの生の行データ
struct RawLine {
    text: String,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

/// 段落グループ化済みのテキスト領域
pub struct TextRegion {
    pub text: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

pub struct OCREngine {
    engine: OcrEngine,
}

impl OCREngine {
    pub fn new() -> Result<Self> {
        let available_languages = OcrEngine::AvailableRecognizerLanguages()
            .map_err(|e| anyhow::anyhow!("Failed to get available OCR languages: {:?}", e))?;

        let count = available_languages.Size()
            .map_err(|e| anyhow::anyhow!("Failed to get language count: {:?}", e))?;

        if count == 0 {
            anyhow::bail!("No OCR languages available.");
        }

        let mut engine_opt = None;
        for i in 0..count {
            let lang = available_languages.GetAt(i)
                .map_err(|e| anyhow::anyhow!("Failed to get language at index {}: {:?}", i, e))?;
            let tag = lang.LanguageTag()
                .map_err(|e| anyhow::anyhow!("Failed to get language tag: {:?}", e))?;

            println!("  Available OCR language: {}", tag);

            if tag.to_string().to_lowercase().starts_with("en") {
                engine_opt = OcrEngine::TryCreateFromLanguage(&lang).ok();
                if engine_opt.is_some() {
                    println!("  Using English OCR");
                    break;
                }
            }
        }

        if engine_opt.is_none() {
            let lang = available_languages.GetAt(0)?;
            let tag = lang.LanguageTag()?;
            println!("  English not found, using: {}", tag);
            engine_opt = OcrEngine::TryCreateFromLanguage(&lang).ok();
        }

        let engine = engine_opt.ok_or_else(|| {
            anyhow::anyhow!("Failed to create OCR engine from any available language")
        })?;

        Ok(Self { engine })
    }

    /// 近い行を段落としてグループ化
    fn group_into_paragraphs(lines: Vec<RawLine>) -> Vec<TextRegion> {
        if lines.is_empty() {
            return Vec::new();
        }

        let mut paragraphs: Vec<TextRegion> = Vec::new();
        let mut current_text = lines[0].text.clone();
        let mut current_x = lines[0].x;
        let mut current_y = lines[0].y;
        let mut current_max_width = lines[0].width;
        let mut current_max_height = lines[0].height;
        let mut prev_y = lines[0].y;
        let mut prev_height = lines[0].height;
        let mut prev_x = lines[0].x;

        for line in &lines[1..] {
            let gap = line.y - (prev_y + prev_height);
            let x_diff = (line.x - prev_x).abs();

            let threshold = (prev_height as f32 * 0.8) as i32;
            if gap >= 0 && gap < threshold && x_diff < prev_height * 2 {
                current_text.push(' ');
                current_text.push_str(&line.text);
                if line.width > current_max_width {
                    current_max_width = line.width;
                }
                if line.height > current_max_height {
                    current_max_height = line.height;
                }
            } else {
                paragraphs.push(TextRegion {
                    text: current_text,
                    x: current_x,
                    y: current_y,
                    width: current_max_width,
                    height: current_max_height,
                });
                current_text = line.text.clone();
                current_x = line.x;
                current_y = line.y;
                current_max_width = line.width;
                current_max_height = line.height;
            }

            prev_y = line.y;
            prev_height = line.height;
            prev_x = line.x;
        }

        paragraphs.push(TextRegion {
            text: current_text,
            x: current_x,
            y: current_y,
            width: current_max_width,
            height: current_max_height,
        });

        paragraphs
    }

    pub async fn detect_text(&self, image_data: &[u8], width: u32, height: u32) -> Result<Vec<TextRegion>> {
        let bitmap = SoftwareBitmap::CreateWithAlpha(
            BitmapPixelFormat::Bgra8,
            width as i32,
            height as i32,
            BitmapAlphaMode::Premultiplied,
        )?;

        {
            let buffer = bitmap.LockBuffer(BitmapBufferAccessMode::Write)?;
            let reference = buffer.CreateReference()?;

            let interop: IMemoryBufferByteAccess = reference.cast()?;
            let mut data_ptr: *mut u8 = std::ptr::null_mut();
            let mut capacity: u32 = 0;
            unsafe {
                interop.GetBuffer(&mut data_ptr, &mut capacity)?;
                let dest = std::slice::from_raw_parts_mut(data_ptr, capacity as usize);
                let copy_len = dest.len().min(image_data.len());
                dest[..copy_len].copy_from_slice(&image_data[..copy_len]);
            }
        }

        let result = self.engine.RecognizeAsync(&bitmap)?.await?;

        // まず生の行データを収集
        let mut raw_lines = Vec::new();
        let lines = result.Lines()?;
        let line_count = lines.Size()?;

        for i in 0..line_count {
            let line = lines.GetAt(i)?;
            let text = line.Text()?.to_string();

            if !text.trim().is_empty() {
                let words = line.Words()?;
                if words.Size()? > 0 {
                    let first_word = words.GetAt(0)?;
                    let first_rect = first_word.BoundingRect()?;

                    // 行全体の高さと幅を取得
                    let mut max_height = first_rect.Height;
                    let mut right_edge = first_rect.X + first_rect.Width;
                    for w in 0..words.Size()? {
                        let word = words.GetAt(w)?;
                        let rect = word.BoundingRect()?;
                        if rect.Height > max_height {
                            max_height = rect.Height;
                        }
                        let word_right = rect.X + rect.Width;
                        if word_right > right_edge {
                            right_edge = word_right;
                        }
                    }

                    raw_lines.push(RawLine {
                        text,
                        x: first_rect.X as i32,
                        y: first_rect.Y as i32,
                        width: (right_edge - first_rect.X) as i32,
                        height: max_height as i32,
                    });
                }
            }
        }

        // 段落グループ化して返す
        Ok(Self::group_into_paragraphs(raw_lines))
    }
}
