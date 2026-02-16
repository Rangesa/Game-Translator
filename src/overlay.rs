use anyhow::Result;
use std::collections::HashMap;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::Com::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::*;
use std::mem;

pub struct TranslatedText {
    pub translated_text: String,
    pub x: f32,
    pub y: f32,
    pub max_width: f32,
    pub font_size: f32,
}

/// Configuration for overlay appearance
#[derive(Clone)]
pub struct OverlayConfig {
    pub text_color: [f32; 4],  // RGBA
    pub bg_color: [f32; 4],    // RGBA
}

impl Default for OverlayConfig {
    fn default() -> Self {
        Self {
            text_color: [1.0, 1.0, 0.0, 1.0],
            bg_color: [0.0, 0.0, 0.0, 0.85],
        }
    }
}

pub struct Overlay {
    factory: ID2D1Factory,
    dc_render_target: Option<ID2D1DCRenderTarget>,
    write_factory: IDWriteFactory,
    memory_dc: HDC,
    bitmap: HBITMAP,
    old_bitmap: HGDIOBJ,
    bg_brush: Option<ID2D1SolidColorBrush>,
    text_brush: Option<ID2D1SolidColorBrush>,
    /// Font size (quantized to integer) -> cached IDWriteTextFormat
    text_format_cache: HashMap<u32, IDWriteTextFormat>,
    width: u32,
    height: u32,
    origin_x: i32,
    origin_y: i32,
    config: OverlayConfig,
}

impl Overlay {
    pub fn new(config: OverlayConfig) -> Result<Self> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

            let factory: ID2D1Factory = D2D1CreateFactory(
                D2D1_FACTORY_TYPE_SINGLE_THREADED,
                None,
            )?;

            let write_factory: IDWriteFactory = DWriteCreateFactory(
                DWRITE_FACTORY_TYPE_SHARED,
            )?;

            Ok(Self {
                factory,
                dc_render_target: None,
                write_factory,
                memory_dc: HDC::default(),
                bitmap: HBITMAP::default(),
                old_bitmap: HGDIOBJ::default(),
                bg_brush: None,
                text_brush: None,
                text_format_cache: HashMap::new(),
                width: 0,
                height: 0,
                origin_x: 0,
                origin_y: 0,
                config,
            })
        }
    }

    fn get_or_create_text_format(&mut self, font_size: f32) -> Result<IDWriteTextFormat> {
        let key = font_size.max(8.0) as u32;
        if let Some(fmt) = self.text_format_cache.get(&key) {
            return Ok(fmt.clone());
        }
        unsafe {
            let fmt = self.write_factory.CreateTextFormat(
                w!("Arial"),
                None,
                DWRITE_FONT_WEIGHT_BOLD,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                key as f32,
                w!("ja-JP"),
            )?;
            fmt.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING)?;
            fmt.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_NEAR)?;
            self.text_format_cache.insert(key, fmt.clone());
            Ok(fmt)
        }
    }

    fn recreate_render_resources(&mut self) -> Result<()> {
        // Drop old D2D resources
        self.bg_brush = None;
        self.text_brush = None;
        self.text_format_cache.clear();
        self.dc_render_target = None;

        unsafe {
            let render_props = D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: 0.0,
                dpiY: 0.0,
                usage: D2D1_RENDER_TARGET_USAGE_NONE,
                minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
            };

            let dc_render_target = self.factory.CreateDCRenderTarget(&render_props)?;

            let rect = RECT {
                left: 0,
                top: 0,
                right: self.width as i32,
                bottom: self.height as i32,
            };
            dc_render_target.BindDC(self.memory_dc, &rect)?;

            let base_target: ID2D1RenderTarget = dc_render_target.cast()?;
            let bg = &self.config.bg_color;
            self.bg_brush = Some(base_target.CreateSolidColorBrush(
                &D2D1_COLOR_F { r: bg[0], g: bg[1], b: bg[2], a: bg[3] },
                None,
            )?);
            let tc = &self.config.text_color;
            self.text_brush = Some(base_target.CreateSolidColorBrush(
                &D2D1_COLOR_F { r: tc[0], g: tc[1], b: tc[2], a: tc[3] },
                None,
            )?);

            self.dc_render_target = Some(dc_render_target);
        }
        Ok(())
    }

    pub fn create_render_target(&mut self, _hwnd: HWND, width: u32, height: u32, origin_x: i32, origin_y: i32) -> Result<()> {
        unsafe {
            self.width = width;
            self.height = height;
            self.origin_x = origin_x;
            self.origin_y = origin_y;

            // Create memory DC with error checking
            let screen_dc = GetDC(None);
            if screen_dc.is_invalid() {
                anyhow::bail!("GetDC failed");
            }
            let memory_dc = CreateCompatibleDC(Some(screen_dc));
            ReleaseDC(None, screen_dc);
            if memory_dc.is_invalid() {
                anyhow::bail!("CreateCompatibleDC failed");
            }

            // Create persistent DIBSection
            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width as i32,
                    biHeight: -(height as i32),
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };

            let mut _bits: *mut core::ffi::c_void = std::ptr::null_mut();
            let bitmap = CreateDIBSection(
                Some(memory_dc),
                &bmi,
                DIB_RGB_COLORS,
                &mut _bits,
                None,
                0,
            )?;

            let old_bitmap = SelectObject(memory_dc, HGDIOBJ(bitmap.0));

            // Create DC Render Target
            let render_props = D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: 0.0,
                dpiY: 0.0,
                usage: D2D1_RENDER_TARGET_USAGE_NONE,
                minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
            };

            let dc_render_target = self.factory.CreateDCRenderTarget(&render_props)?;

            // Bind DC once to create brushes
            let rect = RECT {
                left: 0,
                top: 0,
                right: width as i32,
                bottom: height as i32,
            };
            dc_render_target.BindDC(memory_dc, &rect)?;

            // Create brushes (requires bound DC)
            let base_target: ID2D1RenderTarget = dc_render_target.cast()?;
            let bg = &self.config.bg_color;
            let bg_brush = base_target.CreateSolidColorBrush(
                &D2D1_COLOR_F { r: bg[0], g: bg[1], b: bg[2], a: bg[3] },
                None,
            )?;
            let tc = &self.config.text_color;
            let text_brush = base_target.CreateSolidColorBrush(
                &D2D1_COLOR_F { r: tc[0], g: tc[1], b: tc[2], a: tc[3] },
                None,
            )?;

            self.dc_render_target = Some(dc_render_target);
            self.memory_dc = memory_dc;
            self.bitmap = bitmap;
            self.old_bitmap = old_bitmap;
            self.bg_brush = Some(bg_brush);
            self.text_brush = Some(text_brush);

            Ok(())
        }
    }

    pub fn render(&mut self, texts: &[TranslatedText], hwnd: HWND) -> Result<()> {
        if self.dc_render_target.is_none() || self.memory_dc.is_invalid() {
            return Ok(());
        }

        match self.render_inner(texts, hwnd) {
            Ok(()) => Ok(()),
            Err(e) => {
                // D2DERR_RECREATE_TARGET = 0x8899000C
                let is_recreate = e.downcast_ref::<windows::core::Error>()
                    .is_some_and(|we| we.code() == HRESULT(0x8899000Cu32 as i32));
                if is_recreate {
                    eprintln!("[D2D] Render target lost, recreating...");
                    self.recreate_render_resources()?;
                    self.render_inner(texts, hwnd)
                } else {
                    Err(e)
                }
            }
        }
    }

    fn render_inner(&mut self, texts: &[TranslatedText], hwnd: HWND) -> Result<()> {
        // Resolve cached text formats before borrowing D2D resources
        let mut formats: Vec<IDWriteTextFormat> = Vec::with_capacity(texts.len());
        for text in texts {
            formats.push(self.get_or_create_text_format(text.font_size)?);
        }

        unsafe {
            let target = self.dc_render_target.as_ref().unwrap();
            let bg_brush = match &self.bg_brush {
                Some(b) => b,
                None => return Ok(()),
            };
            let text_brush = match &self.text_brush {
                Some(b) => b,
                None => return Ok(()),
            };

            let rect = RECT {
                left: 0,
                top: 0,
                right: self.width as i32,
                bottom: self.height as i32,
            };
            target.BindDC(self.memory_dc, &rect)?;

            target.BeginDraw();

            target.Clear(Some(&D2D1_COLOR_F {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.0,
            }));

            let ox = self.origin_x as f32;
            let oy = self.origin_y as f32;

            for (text, text_format) in texts.iter().zip(formats.iter()) {
                let text_w: Vec<u16> = text.translated_text
                    .encode_utf16()
                    .chain(Some(0))
                    .collect();

                let wrap_width = text.max_width.max(150.0);
                let local_x = text.x - ox;
                let local_y = text.y - oy;

                let text_layout = self.write_factory.CreateTextLayout(
                    &text_w[..text_w.len()-1],
                    text_format,
                    wrap_width,
                    self.height as f32,
                )?;

                let mut metrics = DWRITE_TEXT_METRICS::default();
                text_layout.GetMetrics(&mut metrics)?;

                let padding = 4.0;
                let box_width = metrics.width + padding * 2.0;
                let box_height = metrics.height + padding * 2.0;

                let bg_rect = D2D_RECT_F {
                    left: local_x - padding,
                    top: local_y - padding,
                    right: local_x + box_width - padding,
                    bottom: local_y + box_height - padding,
                };

                target.FillRectangle(&bg_rect, bg_brush);

                let text_rect = D2D_RECT_F {
                    left: local_x,
                    top: local_y,
                    right: local_x + box_width,
                    bottom: local_y + box_height,
                };

                target.DrawText(
                    &text_w[..text_w.len()-1],
                    text_format,
                    &text_rect,
                    text_brush,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
            }

            target.EndDraw(None, None)?;

            let window_pos = POINT { x: self.origin_x, y: self.origin_y };
            let window_size = SIZE {
                cx: self.width as i32,
                cy: self.height as i32,
            };
            let source_pos = POINT { x: 0, y: 0 };

            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };

            let screen_dc = GetDC(None);
            if screen_dc.is_invalid() {
                anyhow::bail!("GetDC failed in render");
            }
            UpdateLayeredWindow(
                hwnd,
                Some(screen_dc),
                Some(&window_pos),
                Some(&window_size),
                Some(self.memory_dc),
                Some(&source_pos),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            )?;
            ReleaseDC(None, screen_dc);

            Ok(())
        }
    }

    pub fn clear(&mut self, hwnd: HWND) -> Result<()> {
        if self.dc_render_target.is_none() || self.memory_dc.is_invalid() {
            return Ok(());
        }

        match self.clear_inner(hwnd) {
            Ok(()) => Ok(()),
            Err(e) => {
                let is_recreate = e.downcast_ref::<windows::core::Error>()
                    .is_some_and(|we| we.code() == HRESULT(0x8899000Cu32 as i32));
                if is_recreate {
                    eprintln!("[D2D] Render target lost in clear, recreating...");
                    self.recreate_render_resources()?;
                    self.clear_inner(hwnd)
                } else {
                    Err(e)
                }
            }
        }
    }

    fn clear_inner(&self, hwnd: HWND) -> Result<()> {
        unsafe {
            let target = self.dc_render_target.as_ref().unwrap();

            let rect = RECT {
                left: 0,
                top: 0,
                right: self.width as i32,
                bottom: self.height as i32,
            };
            target.BindDC(self.memory_dc, &rect)?;

            target.BeginDraw();
            target.Clear(Some(&D2D1_COLOR_F {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.0,
            }));
            target.EndDraw(None, None)?;

            let window_pos = POINT { x: self.origin_x, y: self.origin_y };
            let window_size = SIZE {
                cx: self.width as i32,
                cy: self.height as i32,
            };
            let source_pos = POINT { x: 0, y: 0 };
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 0,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };

            let screen_dc = GetDC(None);
            if screen_dc.is_invalid() {
                anyhow::bail!("GetDC failed in clear");
            }
            UpdateLayeredWindow(
                hwnd,
                Some(screen_dc),
                Some(&window_pos),
                Some(&window_size),
                Some(self.memory_dc),
                Some(&source_pos),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            )?;
            ReleaseDC(None, screen_dc);

            Ok(())
        }
    }
}

impl Drop for Overlay {
    fn drop(&mut self) {
        unsafe {
            // Release D2D/DWrite resources before render target
            self.bg_brush = None;
            self.text_brush = None;
            self.text_format_cache.clear();
            self.dc_render_target = None;

            if !self.memory_dc.is_invalid() {
                SelectObject(self.memory_dc, self.old_bitmap);
                let _ = DeleteObject(HGDIOBJ(self.bitmap.0));
                let _ = DeleteDC(self.memory_dc);
            }
            CoUninitialize();
        }
    }
}
