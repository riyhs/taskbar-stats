use std::cell::Cell;
use std::ffi::c_void;
use std::mem::size_of;

use windows::core::{w, Result};
use windows::Foundation::Numerics::Matrix3x2;
use windows::Win32::Foundation::{COLORREF, HANDLE, HWND, POINT, RECT, SIZE};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT, D2D_RECT_F, D2D_SIZE_U,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1CreateFactory, ID2D1Bitmap, ID2D1Brush, ID2D1DCRenderTarget, ID2D1Factory,
    ID2D1SolidColorBrush, D2D1_BITMAP_INTERPOLATION_MODE_LINEAR, D2D1_BITMAP_PROPERTIES,
    D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_FEATURE_LEVEL_DEFAULT,
    D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_DEFAULT, D2D1_RENDER_TARGET_USAGE_NONE,
    D2D1_ROUNDED_RECT,
};
use windows::Win32::Graphics::DirectWrite::{
    DWriteCreateFactory, IDWriteFactory, DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL,
    DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT, DWRITE_MEASURING_MODE_NATURAL,
    DWRITE_PARAGRAPH_ALIGNMENT_NEAR, DWRITE_TEXT_ALIGNMENT, DWRITE_TEXT_ALIGNMENT_LEADING,
    DWRITE_WORD_WRAPPING_NO_WRAP,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, SelectObject,
    AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS,
    HBITMAP, HDC, HGDIOBJ, SRCCOPY,
};
use windows::Win32::UI::WindowsAndMessaging::{UpdateLayeredWindow, ULW_ALPHA};

pub struct Renderer {
    factory: ID2D1Factory,
    dwrite: IDWriteFactory,
    target: ID2D1DCRenderTarget,
    mem_dc: HDC,
    dib: HBITMAP,
    old: HGDIOBJ,
    rect: RECT,
    width: i32,
    height: i32,
    dpi: f32,
    alpha: Cell<f32>,
    font_scale: Cell<f32>,
}

impl Renderer {
    pub fn new() -> Result<Self> {
        unsafe {
            let factory: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let dwrite: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let target = create_target(&factory, 96.0)?;
            let mem_dc = CreateCompatibleDC(HDC::default());
            Ok(Self {
                factory,
                dwrite,
                target,
                mem_dc,
                dib: HBITMAP::default(),
                old: HGDIOBJ::default(),
                rect: RECT::default(),
                width: 0,
                height: 0,
                dpi: 96.0,
                alpha: Cell::new(1.0),
                font_scale: Cell::new(1.0),
            })
        }
    }

    pub fn resize(&mut self, width: i32, height: i32, dpi: u32) -> Result<()> {
        if width <= 0 || height <= 0 {
            return Ok(());
        }
        let dpi = if dpi == 0 { 96.0 } else { dpi as f32 };
        if self.width == width && self.height == height && self.dpi == dpi {
            return Ok(());
        }
        if self.dpi != dpi {
            self.target = create_target(&self.factory, dpi)?;
            self.dpi = dpi;
        }

        unsafe {
            if !self.dib.is_invalid() {
                SelectObject(self.mem_dc, self.old);
                let _ = DeleteObject(HGDIOBJ(self.dib.0));
            }
            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width,
                    biHeight: -height,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits: *mut c_void = std::ptr::null_mut();
            let dib = CreateDIBSection(
                self.mem_dc,
                &bmi,
                DIB_RGB_COLORS,
                &mut bits,
                HANDLE::default(),
                0,
            )?;
            self.old = SelectObject(self.mem_dc, HGDIOBJ(dib.0));
            self.dib = dib;
            self.rect = RECT {
                left: 0,
                top: 0,
                right: width,
                bottom: height,
            };
            self.width = width;
            self.height = height;
        }
        Ok(())
    }

    pub fn begin(&self) -> Result<()> {
        unsafe {
            self.target.BindDC(self.mem_dc, &self.rect)?;
            self.target.BeginDraw();
            let clear = D2D1_COLOR_F {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.0,
            };
            self.target.Clear(Some(&clear));
        }
        Ok(())
    }

    pub fn translate(&self, x: f32, y: f32) {
        let transform = Matrix3x2 {
            M11: 1.0,
            M12: 0.0,
            M21: 0.0,
            M22: 1.0,
            M31: x,
            M32: y,
        };
        unsafe {
            self.target.SetTransform(&transform);
        }
    }

    pub fn set_alpha(&self, alpha: f32) {
        self.alpha.set(alpha.clamp(0.0, 1.0));
    }

    pub fn set_font_scale(&self, scale: f32) {
        self.font_scale.set(scale.clamp(0.85, 1.15));
    }

    pub fn end(&self, hwnd: HWND) -> Result<()> {
        self.end_draw()?;
        self.present(hwnd)
    }

    pub fn end_draw(&self) -> Result<()> {
        unsafe {
            self.target.EndDraw(None, None)?;
        }
        Ok(())
    }

    pub fn blit(&self, hdc: HDC) {
        unsafe {
            let _ = BitBlt(
                hdc,
                0,
                0,
                self.width,
                self.height,
                self.mem_dc,
                0,
                0,
                SRCCOPY,
            );
        }
    }

    pub fn present(&self, hwnd: HWND) -> Result<()> {
        let alpha = self.alpha.get();
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: (alpha * 255.0).round() as u8,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        let src = POINT { x: 0, y: 0 };
        let size = SIZE {
            cx: self.width,
            cy: self.height,
        };
        unsafe {
            UpdateLayeredWindow(
                hwnd,
                HDC::default(),
                None,
                Some(&size),
                self.mem_dc,
                Some(&src),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            )
        }
    }

    pub fn fill_round(&self, x: f32, y: f32, w: f32, h: f32, radius: f32, color: D2D1_COLOR_F) {
        let rounded = D2D1_ROUNDED_RECT {
            rect: D2D_RECT_F {
                left: x,
                top: y,
                right: x + w,
                bottom: y + h,
            },
            radiusX: radius,
            radiusY: radius,
        };
        unsafe {
            if let Ok(brush) = self.brush(color) {
                self.target.FillRoundedRectangle(&rounded, &brush);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn stroke_round(
        &self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radius: f32,
        color: D2D1_COLOR_F,
        width: f32,
    ) {
        let rounded = D2D1_ROUNDED_RECT {
            rect: D2D_RECT_F {
                left: x,
                top: y,
                right: x + w,
                bottom: y + h,
            },
            radiusX: radius,
            radiusY: radius,
        };
        unsafe {
            if let Ok(brush) = self.brush(color) {
                self.target.DrawRoundedRectangle(
                    &rounded,
                    &brush,
                    width,
                    None::<&windows::Win32::Graphics::Direct2D::ID2D1StrokeStyle>,
                );
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn text_weight(
        &self,
        text: &str,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        size: f32,
        color: D2D1_COLOR_F,
        weight: DWRITE_FONT_WEIGHT,
        align: DWRITE_TEXT_ALIGNMENT,
    ) {
        let scale = self.font_scale.get();
        let size = size * scale;
        let h = h * scale;
        unsafe {
            let Ok(format) = self.dwrite.CreateTextFormat(
                w!("Segoe UI"),
                None,
                weight,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                size,
                w!("en-us"),
            ) else {
                return;
            };
            let _ = format.SetTextAlignment(align);
            let _ = format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_NEAR);
            let _ = format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
            let Ok(brush) = self.brush(color) else { return };
            let rect = D2D_RECT_F {
                left: x,
                top: y,
                right: x + w,
                bottom: y + h,
            };
            let utf16: Vec<u16> = text.encode_utf16().collect();
            self.target.DrawText(
                &utf16,
                &format,
                &rect,
                &brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn text_left_weight(
        &self,
        text: &str,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        size: f32,
        color: D2D1_COLOR_F,
        weight: DWRITE_FONT_WEIGHT,
    ) {
        self.text_weight(
            text,
            x,
            y,
            w,
            h,
            size,
            color,
            weight,
            DWRITE_TEXT_ALIGNMENT_LEADING,
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw_background(
        &self,
        pixels: &[u8],
        width: u32,
        height: u32,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    ) -> Result<()> {
        if width == 0 || height == 0 || pixels.len() < width as usize * height as usize * 4 {
            return Ok(());
        }
        let properties = D2D1_BITMAP_PROPERTIES {
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            dpiX: 96.0,
            dpiY: 96.0,
        };
        unsafe {
            let bitmap: ID2D1Bitmap = self.target.CreateBitmap(
                D2D_SIZE_U { width, height },
                Some(pixels.as_ptr().cast()),
                width * 4,
                &properties,
            )?;
            let destination = D2D_RECT_F {
                left: x,
                top: y,
                right: x + w,
                bottom: y + h,
            };
            self.target.DrawBitmap(
                &bitmap,
                Some(&destination),
                1.0,
                D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
                None,
            );
        }
        Ok(())
    }

    fn brush(&self, color: D2D1_COLOR_F) -> Result<ID2D1SolidColorBrush> {
        unsafe { self.target.CreateSolidColorBrush(&color, None) }
    }

    #[allow(dead_code)]
    pub fn brush_as(&self, color: D2D1_COLOR_F) -> Result<ID2D1Brush> {
        Ok(self.brush(color)?.into())
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        unsafe {
            if !self.dib.is_invalid() {
                SelectObject(self.mem_dc, self.old);
                let _ = DeleteObject(HGDIOBJ(self.dib.0));
            }
            if !self.mem_dc.is_invalid() {
                let _ = DeleteDC(self.mem_dc);
            }
        }
        let _ = &self.factory;
    }
}

fn create_target(factory: &ID2D1Factory, dpi: f32) -> Result<ID2D1DCRenderTarget> {
    let properties = D2D1_RENDER_TARGET_PROPERTIES {
        r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
        pixelFormat: D2D1_PIXEL_FORMAT {
            format: DXGI_FORMAT_B8G8R8A8_UNORM,
            alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
        },
        dpiX: dpi,
        dpiY: dpi,
        usage: D2D1_RENDER_TARGET_USAGE_NONE,
        minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
    };
    unsafe { factory.CreateDCRenderTarget(&properties) }
}
