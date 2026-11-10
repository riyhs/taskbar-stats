use std::ffi::c_void;
use std::mem::size_of;
use std::slice;

use windows::Win32::Foundation::{HANDLE, HWND};
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC,
    SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HGDIOBJ, SRCCOPY,
};

pub fn screen_bgra(x: i32, y: i32, width: i32, height: i32) -> Option<Vec<u8>> {
    if width <= 0 || height <= 0 {
        return None;
    }
    let len = width as usize * height as usize * 4;
    unsafe {
        let screen = GetDC(HWND::default());
        if screen.is_invalid() {
            return None;
        }
        let mem = CreateCompatibleDC(screen);
        if mem.is_invalid() {
            let _ = ReleaseDC(HWND::default(), screen);
            return None;
        }
        let info = BITMAPINFO {
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
        let Ok(bitmap) =
            CreateDIBSection(mem, &info, DIB_RGB_COLORS, &mut bits, HANDLE::default(), 0)
        else {
            let _ = DeleteDC(mem);
            let _ = ReleaseDC(HWND::default(), screen);
            return None;
        };
        if bits.is_null() {
            let _ = DeleteObject(HGDIOBJ(bitmap.0));
            let _ = DeleteDC(mem);
            let _ = ReleaseDC(HWND::default(), screen);
            return None;
        }
        let old = SelectObject(mem, HGDIOBJ(bitmap.0));
        let captured = BitBlt(mem, 0, 0, width, height, screen, x, y, SRCCOPY).is_ok();
        let mut pixels = vec![0u8; len];
        if captured {
            slice::from_raw_parts(bits.cast::<u8>(), len).clone_into(&mut pixels);
            for pixel in pixels.chunks_exact_mut(4) {
                pixel[3] = 255;
            }
        }
        let _ = SelectObject(mem, old);
        let _ = DeleteObject(HGDIOBJ(bitmap.0));
        let _ = DeleteDC(mem);
        let _ = ReleaseDC(HWND::default(), screen);
        captured.then_some(pixels)
    }
}

pub fn blur(pixels: &mut [u8], width: usize, height: usize, radius: usize) {
    if radius == 0 || width == 0 || height == 0 || pixels.len() < width * height * 4 {
        return;
    }
    let mut source = pixels.to_vec();
    let mut target = vec![0u8; source.len()];
    for _ in 0..2 {
        blur_horizontal(&source, &mut target, width, height, radius);
        blur_vertical(&target, &mut source, width, height, radius);
    }
    pixels.copy_from_slice(&source);
}

pub fn round_corners(pixels: &mut [u8], width: usize, height: usize, radius: usize) {
    if width == 0 || height == 0 {
        return;
    }
    let radius = radius.min(width / 2).min(height / 2);
    if radius == 0 {
        return;
    }
    let radius_sq = (radius * radius) as u64;
    for y in 0..height {
        for x in 0..width {
            let dx = if x < radius {
                (radius - x) as u64
            } else if x >= width.saturating_sub(radius) {
                (x - (width - radius - 1)) as u64
            } else {
                0
            };
            let dy = if y < radius {
                (radius - y) as u64
            } else if y >= height.saturating_sub(radius) {
                (y - (height - radius - 1)) as u64
            } else {
                0
            };
            if dx * dx + dy * dy > radius_sq {
                let offset = (y * width + x) * 4;
                pixels[offset] = 0;
                pixels[offset + 1] = 0;
                pixels[offset + 2] = 0;
                pixels[offset + 3] = 0;
            }
        }
    }
}

fn blur_horizontal(source: &[u8], target: &mut [u8], width: usize, height: usize, radius: usize) {
    for y in 0..height {
        let row = y * width * 4;
        let mut sums = [0u32; 4];
        let mut count = 0usize;
        for x in 0..=radius.min(width - 1) {
            add_pixel(source, (row + x * 4) as usize, &mut sums);
            count += 1;
        }
        for x in 0..width {
            let offset = row + x * 4;
            for channel in 0..4 {
                target[offset + channel] = (sums[channel] / count as u32) as u8;
            }
            let add = x + radius + 1;
            if add < width {
                add_pixel(source, (row + add * 4) as usize, &mut sums);
                count += 1;
            }
            if x >= radius {
                remove_pixel(source, (row + (x - radius) * 4) as usize, &mut sums);
                count -= 1;
            }
        }
    }
}

fn blur_vertical(source: &[u8], target: &mut [u8], width: usize, height: usize, radius: usize) {
    for x in 0..width {
        let mut sums = [0u32; 4];
        let mut count = 0usize;
        for y in 0..=radius.min(height - 1) {
            add_pixel(source, (y * width + x) * 4, &mut sums);
            count += 1;
        }
        for y in 0..height {
            let offset = (y * width + x) * 4;
            for channel in 0..4 {
                target[offset + channel] = (sums[channel] / count as u32) as u8;
            }
            let add = y + radius + 1;
            if add < height {
                add_pixel(source, (add * width + x) * 4, &mut sums);
                count += 1;
            }
            if y >= radius {
                remove_pixel(source, ((y - radius) * width + x) * 4, &mut sums);
                count -= 1;
            }
        }
    }
}

fn add_pixel(source: &[u8], offset: usize, sums: &mut [u32; 4]) {
    for channel in 0..4 {
        sums[channel] += source[offset + channel] as u32;
    }
}

fn remove_pixel(source: &[u8], offset: usize, sums: &mut [u32; 4]) {
    for channel in 0..4 {
        sums[channel] -= source[offset + channel] as u32;
    }
}
