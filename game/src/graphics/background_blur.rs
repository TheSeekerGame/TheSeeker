use bevy::image::Image;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;

/// Plugin that simply registers the blur settings resource.
pub struct BackgroundBlurPlugin;

impl Plugin for BackgroundBlurPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BackgroundBlurSettings>();
    }
}

/// Per-parallax blur radii expressed in screen pixels.
#[derive(Resource, Clone)]
pub struct BackgroundBlurSettings {
    /// Blur radius in pixels for each background parallax group.
    /// Index 0: bg1-3, 1: bg4-6, 2: bg7-8.
    pub group_blur_pixels: [f32; 3],
}

impl Default for BackgroundBlurSettings {
    fn default() -> Self {
        Self {
            group_blur_pixels: [2.0, 3.0, 5.0],
        }
    }
}

impl BackgroundBlurSettings {
    pub fn blur_for_group(&self, idx: usize) -> f32 {
        *self.group_blur_pixels.get(idx).unwrap_or(&0.0)
    }
}

#[derive(Default)]
struct BlurScratch { linear: Vec<Vec4>, temp: Vec<Vec4> }
thread_local! { static SCRATCH: std::cell::RefCell<BlurScratch> = std::cell::RefCell::default(); }

/// Blurs linear premultiplied pixels; conversion occurs only at the image boundary.
pub fn apply_background_blur(image: &mut Image, radius_px: f32) {
    SCRATCH.with_borrow_mut(|scratch| blur_with_scratch(image, radius_px, scratch));
}

fn blur_with_scratch(image: &mut Image, radius_px: f32, scratch: &mut BlurScratch) {
    let _timer = crate::levelgen::diagnostics::StageTimer::new("apply_background_blur");
    if radius_px <= 0.0001 { return; }
    if !matches!(image.texture_descriptor.format, TextureFormat::Rgba8UnormSrgb | TextureFormat::Rgba8Unorm) { return; }
    let srgb = image.texture_descriptor.format == TextureFormat::Rgba8UnormSrgb;
    let width = image.width() as usize;
    let height = image.height() as usize;
    let Some(data) = image.data.as_mut() else { return; };
    if width == 0 || height == 0 || data.len() != width * height * 4 { return; }
    let kernel = gaussian_kernel(radius_px);
    let BlurScratch { linear, temp } = scratch;
    linear.clear();
    linear.extend(data.as_chunks::<4>().0.iter().map(|px| decode(px, srgb)));
    temp.resize(linear.len(), Vec4::ZERO);
    horizontal(linear, temp, width, height, &kernel);
    vertical(temp, linear, width, height, &kernel);
    for (pixel, &value) in data.as_chunks_mut::<4>().0.iter_mut().zip(linear.iter()) {
        encode(pixel, value, srgb);
    }
    image.texture_descriptor.mip_level_count = 1;
}

pub(crate) fn support(radius_px: f32) -> u32 {
    (radius_px.max(0.5) * 1.5).ceil() as u32
}

fn gaussian_kernel(radius: f32) -> Vec<f32> {
    let sigma = radius.max(0.5) * 0.5;
    let mut kernel: Vec<_> = (0..=support(radius)).map(|i| {
        (-((i * i) as f32) / (2.0 * sigma * sigma)).exp()
    }).collect();
    let sum = kernel[0] + 2.0 * kernel.iter().skip(1).sum::<f32>();
    for weight in &mut kernel { *weight /= sum; }
    kernel
}

fn horizontal(src: &[Vec4], dst: &mut [Vec4], width: usize, height: usize, kernel: &[f32]) {
    for y in 0..height {
        let row = &src[y * width..(y + 1) * width];
        let out = &mut dst[y * width..(y + 1) * width];
        for (x, pixel) in out.iter_mut().enumerate() {
            *pixel = horizontal_pixel(row, x, kernel);
        }
    }
}

fn vertical(src: &[Vec4], dst: &mut [Vec4], width: usize, height: usize, kernel: &[f32]) {
    for y in 0..height {
        let out = &mut dst[y * width..(y + 1) * width];
        for (x, pixel) in out.iter_mut().enumerate() {
            *pixel = src[y * width + x] * kernel[0];
        }
        for (offset, &weight) in kernel.iter().enumerate().skip(1) {
            let top = y.saturating_sub(offset) * width;
            let bottom = (y + offset).min(height - 1) * width;
            add_rows(out, &src[top..top + width], &src[bottom..bottom + width], weight);
        }
    }
}

fn decode(pixel: &[u8], srgb: bool) -> Vec4 {
    static LINEAR: std::sync::OnceLock<[f32; 256]> = std::sync::OnceLock::new();
    let lut = LINEAR.get_or_init(|| std::array::from_fn(|i| {
        let value = i as f32 / 255.0;
        if value <= 0.04045 { value / 12.92 } else { ((value + 0.055) / 1.055).powf(2.4) }
    }));
    let alpha = pixel[3] as f32 / 255.0;
    let channel = |i: usize| if srgb { lut[pixel[i] as usize] } else { pixel[i] as f32 / 255.0 };
    Vec4::new(channel(0) * alpha, channel(1) * alpha, channel(2) * alpha, alpha)
}

fn encode(pixel: &mut [u8], value: Vec4, srgb: bool) {
    static SRGB: std::sync::OnceLock<[u8; 4097]> = std::sync::OnceLock::new();
    let lut = SRGB.get_or_init(|| std::array::from_fn(|i| {
        let value = i as f32 / 4096.0;
        let encoded = if value <= 0.003_130_8 { value * 12.92 } else { 1.055 * value.powf(1.0 / 2.4) - 0.055 };
        (encoded * 255.0 + 0.5) as u8
    }));
    let color = if value.w > 0.0001 { value / value.w } else { Vec4::ZERO };
    for channel in 0..3 {
        let value = color[channel].clamp(0.0, 1.0);
        pixel[channel] = if srgb { lut[(value * 4096.0 + 0.5) as usize] } else { (value * 255.0 + 0.5) as u8 };
    }
    pixel[3] = (value.w.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
}

fn horizontal_pixel(row: &[Vec4], x: usize, kernel: &[f32]) -> Vec4 {
    let mut value = row[x] * kernel[0];
    for (offset, &weight) in kernel.iter().enumerate().skip(1) {
        value += (row[x.saturating_sub(offset)] + row[(x + offset).min(row.len() - 1)]) * weight;
    }
    value
}

fn add_rows(out: &mut [Vec4], top: &[Vec4], bottom: &[Vec4], weight: f32) {
    for ((pixel, &a), &b) in out.iter_mut().zip(top).zip(bottom) {
        *pixel += (a + b) * weight;
    }
}
