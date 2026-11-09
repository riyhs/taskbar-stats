use std::ffi::c_void;
use std::mem::size_of;

use windows::core::{w, Result};
use windows::Win32::Foundation::{COLORREF, HANDLE, HWND, POINT, RECT, SIZE};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D_POINT_2F, D2D_RECT_F, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1CreateFactory, D2D1_ELLIPSE, D2D1_ROUNDED_RECT, ID2D1Brush, ID2D1DCRenderTarget,
    ID2D1Factory, ID2D1SolidColorBrush, D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_FACTORY_TYPE_SINGLE_THREADED,
    D2D1_FEATURE_LEVEL_DEFAULT, D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_DEFAULT,
    D2D1_RENDER_TARGET_USAGE_NONE,
};
use windows::Win32::Graphics::DirectWrite::{
    DWriteCreateFactory, IDWriteFactory, DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL,
    DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT, DWRITE_MEASURING_MODE_NATURAL,
    DWRITE_PARAGRAPH_ALIGNMENT_NEAR, DWRITE_TEXT_ALIGNMENT, DWRITE_TEXT_ALIGNMENT_LEADING,
    DWRITE_WORD_WRAPPING_NO_WRAP,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, SelectObject,
    AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION, DIB_RGB_COLORS,
    HBITMAP, HDC, HGDIOBJ,
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
}

impl Renderer {
    pub fn new() -> Result<Self> {
        unsafe {
            let factory: ID2D1Factory =
                D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
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
            let dib = CreateDIBSection(self.mem_dc, &bmi, DIB_RGB_COLORS, &mut bits, HANDLE::default(), 0)?;
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

    pub fn end(&self, hwnd: HWND) -> Result<()> {
        unsafe {
            self.target.EndDraw(None, None)?;
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            let src = POINT { x: 0, y: 0 };
            let size = SIZE {
                cx: self.width,
                cy: self.height,
            };
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

    pub fn fill_round(
        &self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radius: f32,
        color: D2D1_COLOR_F,
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
                self.target.FillRoundedRectangle(&rounded, &brush);
            }
        }
    }

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

    pub fn fill_circle(&self, cx: f32, cy: f32, radius: f32, color: D2D1_COLOR_F) {
        let ellipse = D2D1_ELLIPSE {
            point: D2D_POINT_2F { x: cx, y: cy },
            radiusX: radius,
            radiusY: radius,
        };
        unsafe {
            if let Ok(brush) = self.brush(color) {
                let _ = self.target.FillEllipse(&ellipse, &brush);
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
            let _ = self.target.DrawText(
                &utf16,
                &format,
                &rect,
                &brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );
        }
    }

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
