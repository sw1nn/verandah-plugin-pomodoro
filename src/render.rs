use std::sync::OnceLock;

use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use image::{Rgb, RgbImage, Rgba, RgbaImage};
use imageproc::drawing::{draw_filled_rect_mut, draw_text_mut};
use imageproc::rect::Rect;
use verandah_plugin_api::prelude::PluginImage;

use crate::config::Colour;
use crate::timer::{Phase, Timer};

static SYSTEM_FONT: OnceLock<Option<Vec<u8>>> = OnceLock::new();

fn get_system_monospace_font() -> Option<&'static Vec<u8>> {
    SYSTEM_FONT.get_or_init(load_system_monospace_font).as_ref()
}

fn load_system_monospace_font() -> Option<Vec<u8>> {
    use fontconfig::Fontconfig;

    let fc = Fontconfig::new()?;
    if let Some(font) = fc.find("monospace", None) {
        let path = font.path.to_string_lossy();
        if let Ok(bytes) = std::fs::read(&*path) {
            return Some(bytes);
        }
    }
    None
}

/// Render the pomodoro button image
pub fn render_button(
    timer: &Timer,
    width: u32,
    height: u32,
    fg_color: &Colour,
    work_bg: &Colour,
    break_bg: &Colour,
    paused_bg: &Colour,
    padding: f32,
    paused_icon: Option<&PluginImage>,
    fallback_text: Option<&str>,
) -> RgbImage {
    // If paused and we have an icon, render the icon with dots overlay
    if !timer.is_running() {
        if let Some(icon) = paused_icon {
            return render_icon_with_dots(icon, width, height, timer.iterations(), fg_color);
        }
        // No icon found, render fallback text
        if let Some(text) = fallback_text {
            return render_paused_text(text, width, height, fg_color, paused_bg, padding);
        }
    }

    let mut rgba = RgbaImage::new(width, height);

    // Determine background color based on state
    let bg = if !timer.is_running() {
        paused_bg
    } else if timer.phase().is_break() {
        break_bg
    } else {
        work_bg
    };

    // Fill background
    let bg_rgba = Rgba([bg.r, bg.g, bg.b, 255]);
    draw_filled_rect_mut(&mut rgba, Rect::at(0, 0).of_size(width, height), bg_rgba);

    // Build display text
    let time_text = timer.remaining_formatted();

    let phase_indicator = match timer.phase() {
        Phase::Work => "work",
        Phase::ShortBreak => "short brk",
        Phase::LongBreak => "long brk",
    };

    // Draw phase indicator (top)
    draw_phase_indicator(&mut rgba, phase_indicator, fg_color, padding);

    // Draw main time (center)
    draw_centered_text(&mut rgba, &time_text, fg_color, padding, 0.0);

    // Draw iteration progress dots (bottom)
    draw_iteration_dots(&mut rgba, timer.iterations(), width, fg_color);

    // Convert to RGB
    RgbImage::from_fn(width, height, |x, y| {
        let pixel = rgba.get_pixel(x, y);
        Rgb([pixel[0], pixel[1], pixel[2]])
    })
}

/// Render fallback text when paused and no icon is available
fn render_paused_text(
    text: &str,
    width: u32,
    height: u32,
    fg_color: &Colour,
    paused_bg: &Colour,
    padding: f32,
) -> RgbImage {
    let mut rgba = RgbaImage::new(width, height);

    // Fill with paused background
    let bg_rgba = Rgba([paused_bg.r, paused_bg.g, paused_bg.b, 255]);
    draw_filled_rect_mut(&mut rgba, Rect::at(0, 0).of_size(width, height), bg_rgba);

    // Draw centered text (supports multiline)
    if let Some(font_bytes) = get_system_monospace_font() {
        if let Ok(font) = FontRef::try_from_slice(font_bytes) {
            let lines: Vec<&str> = text.lines().collect();
            let content_fraction = 1.0 - (2.0 * padding);
            let target_width = width as f32 * content_fraction;
            let target_height = height as f32 * content_fraction;
            let scale_value = find_optimal_scale(&font, &lines, target_width, target_height);
            let scale = PxScale::from(scale_value);

            let scaled_font = font.as_scaled(scale);
            let line_height = scaled_font.height();
            let total_height = line_height * lines.len() as f32;
            let start_y = (height as f32 - total_height) / 2.0;

            let color = Rgba([fg_color.r, fg_color.g, fg_color.b, 255]);

            for (i, line) in lines.iter().enumerate() {
                let text_width: f32 = line
                    .chars()
                    .map(|c| scaled_font.h_advance(font.glyph_id(c)))
                    .sum();
                let x = ((width as f32 - text_width) / 2.0).max(0.0) as i32;
                let y = (start_y + line_height * i as f32) as i32;
                draw_text_mut(&mut rgba, color, x, y, scale, &font, line);
            }
        }
    }

    // Convert to RGB
    RgbImage::from_fn(width, height, |x, y| {
        let pixel = rgba.get_pixel(x, y);
        Rgb([pixel[0], pixel[1], pixel[2]])
    })
}

/// Render an icon image, scaling to fit the button
fn render_icon(icon: &PluginImage, width: u32, height: u32) -> RgbImage {
    // If icon is the right size, just copy it
    if icon.width == width && icon.height == height {
        return RgbImage::from_fn(width, height, |x, y| {
            let idx = ((y * width + x) * 3) as usize;
            if idx + 2 < icon.data.len() {
                Rgb([icon.data[idx], icon.data[idx + 1], icon.data[idx + 2]])
            } else {
                Rgb([0, 0, 0])
            }
        });
    }

    // Otherwise, scale the icon to fit
    let src_img = RgbImage::from_fn(icon.width, icon.height, |x, y| {
        let idx = ((y * icon.width + x) * 3) as usize;
        if idx + 2 < icon.data.len() {
            Rgb([icon.data[idx], icon.data[idx + 1], icon.data[idx + 2]])
        } else {
            Rgb([0, 0, 0])
        }
    });

    image::imageops::resize(
        &src_img,
        width,
        height,
        image::imageops::FilterType::Lanczos3,
    )
}

/// Render an icon image with iteration dots overlay
fn render_icon_with_dots(
    icon: &PluginImage,
    width: u32,
    height: u32,
    iterations: u8,
    fg_color: &Colour,
) -> RgbImage {
    let rgb = render_icon(icon, width, height);

    // Convert to RGBA so we can draw on it
    let mut rgba = RgbaImage::from_fn(width, height, |x, y| {
        let pixel = rgb.get_pixel(x, y);
        Rgba([pixel[0], pixel[1], pixel[2], 255])
    });

    // Draw iteration dots
    draw_iteration_dots(&mut rgba, iterations, width, fg_color);

    // Convert back to RGB
    RgbImage::from_fn(width, height, |x, y| {
        let pixel = rgba.get_pixel(x, y);
        Rgb([pixel[0], pixel[1], pixel[2]])
    })
}

/// Draw iteration progress dots at the bottom
fn draw_iteration_dots(rgba: &mut RgbaImage, iterations: u8, width: u32, fg_color: &Colour) {
    let Some(font_bytes) = get_system_monospace_font() else {
        return;
    };
    let Ok(font) = FontRef::try_from_slice(font_bytes) else {
        return;
    };

    // Build dots string: filled for completed, empty for remaining
    let dots: String = (0..4)
        .map(|i| if i < iterations { '●' } else { '○' })
        .collect();

    let scale = PxScale::from(18.0);
    let scaled_font = font.as_scaled(scale);
    let line_height = scaled_font.height();

    // Calculate width for centering
    let text_width: f32 = dots
        .chars()
        .map(|c| scaled_font.h_advance(font.glyph_id(c)))
        .sum();

    let x = ((width as f32 - text_width) / 2.0).max(0.0) as i32;
    let y = (rgba.height() as f32 - line_height - 4.0) as i32; // Small margin from bottom

    let color = Rgba([fg_color.r, fg_color.g, fg_color.b, 255]);
    draw_text_mut(rgba, color, x, y, scale, &font, &dots);
}

/// Draw centered time text
fn draw_centered_text(
    rgba: &mut RgbaImage,
    text: &str,
    fg_color: &Colour,
    padding: f32,
    y_offset: f32,
) {
    let Some(font_bytes) = get_system_monospace_font() else {
        return;
    };
    let Ok(font) = FontRef::try_from_slice(font_bytes) else {
        return;
    };

    let width = rgba.width();
    let height = rgba.height();

    // Reserve space for phase indicator (top) and dots (bottom)
    let reserved_top = 18.0;
    let reserved_bottom = 18.0;
    let available_height = height as f32 - reserved_top - reserved_bottom;

    // Calculate optimal scale
    let content_fraction = 1.0 - (2.0 * padding);
    let target_width = width as f32 * content_fraction;
    let target_height = available_height * content_fraction;
    let scale_value = find_optimal_scale(&font, &[text], target_width, target_height);
    let scale = PxScale::from(scale_value);

    let scaled_font = font.as_scaled(scale);
    let line_height = scaled_font.height();

    // Calculate text width
    let text_width: f32 = text
        .chars()
        .map(|c| scaled_font.h_advance(font.glyph_id(c)))
        .sum();

    // Center horizontally and vertically in available space
    let x = ((width as f32 - text_width) / 2.0).max(0.0) as i32;
    let y = (reserved_top + (available_height - line_height) / 2.0 + y_offset) as i32;

    let color = Rgba([fg_color.r, fg_color.g, fg_color.b, 255]);
    draw_text_mut(rgba, color, x, y, scale, &font, text);
}

/// Draw phase indicator at the top
fn draw_phase_indicator(rgba: &mut RgbaImage, text: &str, fg_color: &Colour, _padding: f32) {
    let Some(font_bytes) = get_system_monospace_font() else {
        return;
    };
    let Ok(font) = FontRef::try_from_slice(font_bytes) else {
        return;
    };

    let width = rgba.width();

    let scale = PxScale::from(14.0);
    let scaled_font = font.as_scaled(scale);

    let text_width: f32 = text
        .chars()
        .map(|c| scaled_font.h_advance(font.glyph_id(c)))
        .sum();

    let x = ((width as f32 - text_width) / 2.0).max(0.0) as i32;
    let y = 4; // 4px margin from top

    let color = Rgba([fg_color.r, fg_color.g, fg_color.b, 255]);
    draw_text_mut(rgba, color, x, y, scale, &font, text);
}

/// Calculate the width of a line of text using actual font metrics
fn measure_text_width<F>(font: &F, text: &str) -> f32
where
    F: Font,
{
    let scaled = font.as_scaled(PxScale::from(1.0));
    text.chars()
        .map(|c| scaled.h_advance(font.glyph_id(c)))
        .sum()
}

/// Find optimal font scale to fit text within target dimensions
fn find_optimal_scale<F>(font: &F, lines: &[&str], target_width: f32, target_height: f32) -> f32
where
    F: Font,
{
    let num_lines = lines.len().max(1) as f32;

    let max_line_width = lines
        .iter()
        .map(|line| measure_text_width(font, line))
        .fold(0.0_f32, |a, b| a.max(b));

    let scaled = font.as_scaled(PxScale::from(1.0));
    let line_height = scaled.height();

    let scale_for_width = if max_line_width > 0.0 {
        target_width / max_line_width
    } else {
        target_height
    };

    let total_height_at_1 = num_lines * line_height;
    let scale_for_height = if total_height_at_1 > 0.0 {
        target_height / total_height_at_1
    } else {
        target_width
    };

    scale_for_width.min(scale_for_height).clamp(8.0, 96.0)
}
