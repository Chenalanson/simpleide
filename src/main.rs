#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod arraygen;
mod brackets;
mod build;
mod column;
mod column_select;
mod completion;
mod config;
mod debugger;
mod editor;
mod file_tree;
mod flash;
mod formatting;
mod git;
mod hexview;
mod highlight;
mod hover;
mod ignore;
mod indent;
mod log;
mod macro_check;
mod makefile;
mod memory;
mod memory_check;
mod minimap;
mod navigation;
mod project_meta;
mod registers;
mod search;
mod support_pack;
mod symbols;
mod theme;
mod todo;
mod toolchain;
mod util;

use std::io::Cursor;
use std::path::PathBuf;
#[cfg(windows)]
use std::sync::atomic::{AtomicIsize, AtomicU8, Ordering};
use std::sync::Arc;
use std::sync::OnceLock;

use eframe::egui;

/// Window handle captured at startup; the icon override runs from the first UI
/// frames, after eframe has applied its own icon.
#[cfg(windows)]
static PENDING_HWND: AtomicIsize = AtomicIsize::new(0);
#[cfg(windows)]
static ICON_FRAMES: AtomicU8 = AtomicU8::new(0);
#[cfg(windows)]
static ICON_FILE: OnceLock<Option<PathBuf>> = OnceLock::new();

fn main() -> eframe::Result<()> {
    let mut args = std::env::args_os().skip(1);
    let first = args.next().map(PathBuf::from).filter(|path| path.exists());
    let second = args.next().map(PathBuf::from).filter(|path| path.is_file());
    let (initial_project, initial_file) = match first {
        Some(path) if path.is_dir() || (path.is_file() && config::is_workspace_file(&path)) => {
            (Some(path), second)
        }
        Some(path) if path.is_file() => (None, Some(path)),
        _ => (None, second),
    };

    let viewport = egui::ViewportBuilder::default()
        .with_inner_size([1360.0, 850.0])
        .with_min_inner_size([1000.0, 640.0])
        .with_title("Simple embedded · 作者 smallYYY")
        .with_icon(Arc::new(window_icon()));

    let native_options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };

    eframe::run_native(
        "Simple embedded",
        native_options,
        Box::new(move |cc| {
            configure_fonts(&cc.egui_ctx);
            theme::install(&cc.egui_ctx);
            install_native_icons(cc);
            Ok(Box::new(app::App::new(
                cc,
                initial_project.clone(),
                initial_file.clone(),
            )))
        }),
    )
}

fn configure_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    let candidates = [
        r"C:\Windows\Fonts\msyh.ttc",
        r"C:\Windows\Fonts\Deng.ttf",
        r"C:\Windows\Fonts\NotoSansSC-VF.ttf",
        r"C:\Windows\Fonts\simhei.ttf",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/opentype/noto/NotoSansCJKsc-Regular.otf",
        "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
        "/System/Library/Fonts/PingFang.ttc",
    ];

    if let Some(bytes) = candidates.iter().find_map(|path| std::fs::read(path).ok()) {
        let mut data = egui::FontData::from_owned(bytes);
        data.index = 0;
        fonts
            .font_data
            .insert("cjk_fallback".to_owned(), data.into());
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            fonts
                .families
                .entry(family)
                .or_default()
                .push("cjk_fallback".to_owned());
        }
    }

    ctx.set_fonts(fonts);
}

/// Captures the native window handle at startup. `apply_pending_icons` uses it
/// to install separate title-bar and taskbar artwork after eframe applies its
/// own single-size icon.
fn install_native_icons(cc: &eframe::CreationContext<'_>) {
    #[cfg(windows)]
    {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        let Ok(handle) = cc.window_handle() else {
            return;
        };
        let RawWindowHandle::Win32(win) = handle.as_raw() else {
            return;
        };
        // eframe applies its own icon in `pre_update` of the first frames, so
        // wait until the UI runs and then keep overriding for a few frames in
        // case eframe had to retry.
        PENDING_HWND.store(win.hwnd.get(), Ordering::Relaxed);
        ICON_FRAMES.store(8, Ordering::Relaxed);
    }
    #[cfg(not(windows))]
    let _ = cc;
}

/// Called from the first few UI frames. eframe sets both native icons from the
/// single 48 px wordmark before `App::ui`; this swaps the small title-bar slot
/// for the 16 px monogram while leaving the taskbar on the wordmark.
pub(crate) fn apply_pending_icons() {
    #[cfg(windows)]
    {
        let hwnd = PENDING_HWND.load(Ordering::Relaxed);
        if hwnd == 0 {
            return;
        }
        let frames = ICON_FRAMES.load(Ordering::Relaxed);
        if frames == 0 {
            PENDING_HWND.store(0, Ordering::Relaxed);
            return;
        }
        ICON_FRAMES.store(frames - 1, Ordering::Relaxed);
        let path = ICON_FILE.get_or_init(|| write_app_icon_ico().ok());
        if let Some(path) = path {
            native_icon::install(hwnd, path);
        }
    }
}

#[cfg(windows)]
mod native_icon {
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "user32")]
    extern "system" {
        fn LoadImageW(
            instance: *mut c_void,
            name: *const u16,
            kind: u32,
            cx: i32,
            cy: i32,
            flags: u32,
        ) -> isize;
        fn SendMessageW(hwnd: isize, msg: u32, wparam: usize, lparam: isize) -> isize;
        fn GetSystemMetrics(index: i32) -> i32;
    }

    const IMAGE_ICON: u32 = 1;
    const LR_LOADFROMFILE: u32 = 0x0010;
    const WM_SETICON: u32 = 0x0080;
    const ICON_SMALL: usize = 0;
    const ICON_BIG: usize = 1;
    const SM_CXICON: i32 = 11;
    const SM_CYICON: i32 = 12;
    const SM_CXSMICON: i32 = 49;
    const SM_CYSMICON: i32 = 50;

    pub(super) fn install(hwnd: isize, ico: &std::path::Path) {
        let wide: Vec<u16> = ico
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        // The ICO contains 16/24 px monograms and 32 px+ wordmarks, so loading
        // the DPI-appropriate system sizes picks the right artwork for each
        // slot. The startup frames replace these handles a few times; Windows
        // releases the old ones when the process exits.
        unsafe {
            let small = LoadImageW(
                std::ptr::null_mut(),
                wide.as_ptr(),
                IMAGE_ICON,
                GetSystemMetrics(SM_CXSMICON),
                GetSystemMetrics(SM_CYSMICON),
                LR_LOADFROMFILE,
            );
            if small != 0 {
                SendMessageW(hwnd, WM_SETICON, ICON_SMALL, small);
            }
            let big = LoadImageW(
                std::ptr::null_mut(),
                wide.as_ptr(),
                IMAGE_ICON,
                GetSystemMetrics(SM_CXICON),
                GetSystemMetrics(SM_CYICON),
                LR_LOADFROMFILE,
            );
            if big != 0 {
                SendMessageW(hwnd, WM_SETICON, ICON_BIG, big);
            }
        }
    }
}

/// Embedded artwork. Large sizes use the `Simple` wordmark; 16/24 px get a
/// condensed `S` monogram so the icon stays recognizable instead of turning
/// the six letters into a white smear.
fn embedded_icon(wordmark: bool) -> Option<&'static image::RgbaImage> {
    static WORD: OnceLock<Option<image::RgbaImage>> = OnceLock::new();
    static MARK: OnceLock<Option<image::RgbaImage>> = OnceLock::new();
    let cell = if wordmark { &WORD } else { &MARK };
    cell.get_or_init(|| {
        let bytes = if wordmark {
            include_bytes!("../assets/icon-word.png").as_slice()
        } else {
            include_bytes!("../assets/icon-mark.png").as_slice()
        };
        image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
            .ok()
            .map(|image| image.to_rgba8())
    })
    .as_ref()
}

fn icon_from_asset(size: u32, wordmark: bool) -> egui::IconData {
    let size = size.max(1);
    if let Some(source) = embedded_icon(wordmark) {
        let resized =
            image::imageops::resize(source, size, size, image::imageops::FilterType::Lanczos3);
        return egui::IconData {
            rgba: resized.into_raw(),
            width: size,
            height: size,
        };
    }

    // Embedded assets are compiled in; this fallback only guards against a
    // malformed PNG in a future change.
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    for pixel in rgba.as_chunks_mut::<4>().0 {
        pixel[0] = 4;
        pixel[1] = 5;
        pixel[2] = 7;
        pixel[3] = 255;
    }
    egui::IconData {
        rgba,
        width: size,
        height: size,
    }
}

pub(crate) fn app_icon(size: u32) -> egui::IconData {
    icon_from_asset(size, size >= 32)
}

/// Initial window icon applied by winit/eframe. It is the 48 px wordmark; the
/// taskbar keeps that artwork while `apply_pending_icons` gives the 16 px
/// title-bar slot the monogram.
pub(crate) fn window_icon() -> egui::IconData {
    icon_from_asset(48, true)
}

/// Build a multi-size Windows ICO file: wordmark for the large entries and the
/// monogram for 16/24 px. Sizes up to 128 px use 32-bit BMP entries because
/// `LoadImageW` (used for the taskbar icon) does not reliably read
/// PNG-compressed entries below 256 px; only 256 px stays PNG.
pub(crate) fn app_icon_ico() -> Result<Vec<u8>, String> {
    let sizes = [16u32, 24, 32, 48, 64, 128, 256];
    let mut entries = Vec::new();
    for size in sizes {
        let icon = app_icon(size);
        let data = if size >= 256 {
            let image = image::RgbaImage::from_raw(size, size, icon.rgba)
                .ok_or_else(|| "图标缓冲区大小不正确".to_owned())?;
            let mut png = Vec::new();
            image::DynamicImage::ImageRgba8(image)
                .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
                .map_err(|err| format!("无法编码图标：{err}"))?;
            png
        } else {
            encode_icon_bmp(size, &icon.rgba)
        };
        entries.push((size, data));
    }

    let mut bytes = Vec::new();
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * entries.len();
    for (size, data) in &entries {
        let dimension = if *size >= 256 { 0 } else { *size as u8 };
        bytes.push(dimension);
        bytes.push(dimension);
        bytes.push(0);
        bytes.push(0);
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&32u16.to_le_bytes());
        bytes.extend_from_slice(&(data.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&(offset as u32).to_le_bytes());
        offset += data.len();
    }
    for (_, data) in &entries {
        bytes.extend_from_slice(data);
    }
    Ok(bytes)
}

/// Encode one 32-bit BMP icon image: BITMAPINFOHEADER, bottom-up BGRA pixels,
/// then a 1-bit transparency mask. ICO directory entries expect the doubled
/// height and no BMP file header.
fn encode_icon_bmp(size: u32, rgba: &[u8]) -> Vec<u8> {
    let mask_stride = size.div_ceil(32) * 4;
    let xor_len = (size * size * 4) as usize;
    let and_len = (mask_stride * size) as usize;
    let mut out = Vec::with_capacity(40 + xor_len + and_len);
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(size as i32).to_le_bytes());
    out.extend_from_slice(&(size as i32 * 2).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&((xor_len + and_len) as u32).to_le_bytes());
    out.extend_from_slice(&[0u8; 16]);
    for y in (0..size).rev() {
        for x in 0..size {
            let index = ((y * size + x) * 4) as usize;
            out.push(rgba[index + 2]);
            out.push(rgba[index + 1]);
            out.push(rgba[index]);
            out.push(rgba[index + 3]);
        }
    }
    for y in (0..size).rev() {
        let mut row = vec![0u8; mask_stride as usize];
        for x in 0..size {
            let index = ((y * size + x) * 4) as usize;
            if rgba[index + 3] == 0 {
                row[(x / 8) as usize] |= 0x80 >> (x % 8);
            }
        }
        out.extend_from_slice(&row);
    }
    out
}

/// Write the application icon next to the user settings so Explorer can use it
/// as the `.se-workspace` file icon.
pub(crate) fn write_app_icon_ico() -> Result<PathBuf, String> {
    let dir = config::settings_path()
        .parent()
        .map(PathBuf::from)
        .ok_or_else(|| "无法确定设置目录".to_owned())?;
    std::fs::create_dir_all(&dir).map_err(|err| format!("无法创建 {}：{err}", dir.display()))?;
    let path = dir.join("simple-embedded.ico");
    let bytes = app_icon_ico()?;
    std::fs::write(&path, bytes).map_err(|err| format!("无法写入 {}：{err}", path.display()))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_icon_uses_embedded_wordmark() {
        let icon = app_icon(32);
        assert_eq!(icon.width, 32);
        assert_eq!(icon.rgba.len(), 32 * 32 * 4);
        assert!(icon
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .any(|pixel| pixel[0] > 200 && pixel[1] > 200 && pixel[2] > 200));
        let mark = app_icon(16);
        assert!(mark
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .any(|pixel| pixel[0] > 200 && pixel[1] > 200 && pixel[2] > 200));
        let window = window_icon();
        assert_eq!(window.width, 48);
        assert!(window
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .any(|pixel| pixel[0] > 200 && pixel[1] > 200 && pixel[2] > 200));
    }

    #[test]
    fn generated_ico_has_adaptive_entries() {
        let bytes = app_icon_ico().unwrap();
        assert_eq!(&bytes[..4], &[0, 0, 1, 0]);
        let count = u16::from_le_bytes([bytes[4], bytes[5]]) as usize;
        assert_eq!(count, 7);
        let mut dimensions = Vec::new();
        for index in 0..count {
            let entry = &bytes[6 + index * 16..6 + (index + 1) * 16];
            dimensions.push(entry[0]);
            let len = u32::from_le_bytes(entry[8..12].try_into().unwrap()) as usize;
            let offset = u32::from_le_bytes(entry[12..16].try_into().unwrap()) as usize;
            let data = &bytes[offset..offset + len];
            if index == count - 1 {
                assert_eq!(&data[..8], b"\x89PNG\r\n\x1a\n");
            } else {
                // 32-bit BMP entries are required for LoadImageW below 256 px.
                assert_eq!(&data[..4], &40u32.to_le_bytes());
                assert_eq!(u16::from_le_bytes([data[14], data[15]]), 32);
            }
        }
        assert_eq!(dimensions, vec![16, 24, 32, 48, 64, 128, 0]);
    }
}
