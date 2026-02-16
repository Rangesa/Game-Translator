use anyhow::Result;
use std::mem;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Storage::Xps::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::BOOL;

/// 対象ウィンドウのPrintWindowキャプチャ
pub struct WindowCapture {
    target_hwnd: HWND,
    width: u32,
    height: u32,
}

impl WindowCapture {
    pub fn new(target_hwnd: HWND) -> Result<Self> {
        Ok(Self {
            target_hwnd,
            width: 0,
            height: 0,
        })
    }

    pub fn capture_frame(&mut self) -> Result<Option<Vec<u8>>> {
        unsafe {
            // 対象ウィンドウのクライアント領域サイズを取得
            let mut rect = RECT::default();
            GetClientRect(self.target_hwnd, &mut rect)?;

            let width = (rect.right - rect.left) as u32;
            let height = (rect.bottom - rect.top) as u32;

            if width == 0 || height == 0 {
                return Ok(None);
            }

            self.width = width;
            self.height = height;

            // メモリDCとビットマップを作成
            let window_dc = GetDC(Some(self.target_hwnd));
            if window_dc.is_invalid() {
                anyhow::bail!("GetDC failed for target window");
            }
            let memory_dc = CreateCompatibleDC(Some(window_dc));
            if memory_dc.is_invalid() {
                ReleaseDC(Some(self.target_hwnd), window_dc);
                anyhow::bail!("CreateCompatibleDC failed");
            }

            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width as i32,
                    biHeight: -(height as i32), // top-down
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };

            let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
            let bitmap = CreateDIBSection(
                Some(memory_dc),
                &bmi,
                DIB_RGB_COLORS,
                &mut bits,
                None,
                0,
            )?;

            let old_bitmap = SelectObject(memory_dc, HGDIOBJ(bitmap.0));

            // PrintWindowでウィンドウ内容をキャプチャ（DXゲーム対応）
            let pw_result = PrintWindow(self.target_hwnd, memory_dc, PRINT_WINDOW_FLAGS(2)); // PW_RENDERFULLCONTENT = 2

            if !pw_result.as_bool() {
                // PrintWindow失敗時はBitBltにフォールバック
                let _ = BitBlt(memory_dc, 0, 0, width as i32, height as i32, Some(window_dc), 0, 0, SRCCOPY);
            }

            // ピクセルデータをコピー
            let data_size = (width * height * 4) as usize;
            let mut pixel_data = vec![0u8; data_size];
            std::ptr::copy_nonoverlapping(bits as *const u8, pixel_data.as_mut_ptr(), data_size);

            // クリーンアップ
            SelectObject(memory_dc, old_bitmap);
            let _ = DeleteObject(HGDIOBJ(bitmap.0));
            let _ = DeleteDC(memory_dc);
            ReleaseDC(Some(self.target_hwnd), window_dc);

            Ok(Some(pixel_data))
        }
    }

    pub fn get_dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// 対象ウィンドウのスクリーン上の位置を取得
    pub fn get_window_position(&self) -> (i32, i32) {
        unsafe {
            let mut rect = RECT::default();
            if GetWindowRect(self.target_hwnd, &mut rect).is_ok() {
                // クライアント領域の左上をスクリーン座標に変換
                let mut pt = POINT { x: 0, y: 0 };
                let _ = ClientToScreen(self.target_hwnd, &mut pt);
                (pt.x, pt.y)
            } else {
                (0, 0)
            }
        }
    }
}

/// 実行中のウィンドウ一覧を取得
pub fn list_windows() -> Vec<(isize, String)> {
    let mut windows: Vec<(isize, String)> = Vec::new();

    unsafe {
        let _ = EnumWindows(
            Some(enum_windows_callback),
            LPARAM(&mut windows as *mut Vec<(isize, String)> as isize),
        );
    }

    windows
}

unsafe extern "system" fn enum_windows_callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let windows = &mut *(lparam.0 as *mut Vec<(isize, String)>);

    // 表示されてるウィンドウのみ
    if !IsWindowVisible(hwnd).as_bool() {
        return TRUE;
    }

    // タイトルがあるウィンドウのみ
    let mut title = [0u16; 256];
    let len = GetWindowTextW(hwnd, &mut title);
    if len == 0 {
        return TRUE;
    }

    let title = String::from_utf16_lossy(&title[..len as usize]);

    // 自分自身のオーバーレイは除外
    if title == "Game Translator Overlay" {
        return TRUE;
    }

    // 空タイトルやシステム系を除外
    if !title.trim().is_empty() {
        windows.push((hwnd.0 as isize, title));
    }

    TRUE
}
