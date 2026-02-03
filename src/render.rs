use std::collections::HashMap;
use std::sync::OnceLock;

use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use image::{Rgb, RgbImage, Rgba, RgbaImage};
use imageproc::drawing::{draw_filled_rect_mut, draw_text_mut};
use imageproc::rect::Rect;
use strum::{AsRefStr, EnumString, VariantNames};
use verandah_plugin_api::prelude::PluginImage;

use crate::timer::{Phase, Timer};

/// Render mode for the timer display
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, EnumString, AsRefStr, VariantNames)]
#[strum(serialize_all = "snake_case")]
pub enum RenderMode {
    /// Traditional text-based display with time countdown
    #[default]
    Text,
    /// Fill background from bottom to top (or vice versa) as progress indicator
    FillingBucket,
    /// Fill an icon from bottom to top (or vice versa) as progress indicator
    FillIcon,
    /// Icon starts green (unripe) and gradually returns to original colors as timer progresses
    Ripen,
}

/// Fill direction for filling-bucket mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, EnumString, AsRefStr, VariantNames)]
#[strum(serialize_all = "snake_case")]
pub enum FillDirection {
    /// Fill from bottom to top (empty → full)
    #[default]
    EmptyToFull,
    /// Drain from top to bottom (full → empty)
    FullToEmpty,
}

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
#[allow(clippy::too_many_arguments)]
pub fn render_button(
    timer: &Timer,
    width: u32,
    height: u32,
    fg_color: Rgba<u8>,
    work_bg: Rgba<u8>,
    break_bg: Rgba<u8>,
    paused_bg: Rgba<u8>,
    empty_bg: Rgba<u8>,
    padding: f32,
    paused_icon: Option<&PluginImage>,
    phase_icon: Option<&PluginImage>,
    fallback_text: Option<&str>,
    paused_text: &str,
    phases: &HashMap<String, String>,
    render_mode: RenderMode,
    fill_direction: FillDirection,
) -> RgbImage {
    // At phase boundary (elapsed=0) and not running: show icon or fallback
    if !timer.is_running() && timer.at_phase_boundary() {
        if let Some(icon) = paused_icon {
            return render_icon_with_dots(icon, width, height, timer.iterations(), fg_color);
        }
        if let Some(text) = fallback_text {
            return render_paused_text(text, width, height, fg_color, paused_bg, padding);
        }
    }

    match render_mode {
        RenderMode::Text => render_text_mode(
            timer,
            width,
            height,
            fg_color,
            work_bg,
            break_bg,
            paused_bg,
            padding,
            paused_text,
            phases,
        ),
        RenderMode::FillingBucket => render_filling_bucket_mode(
            timer,
            width,
            height,
            fg_color,
            work_bg,
            break_bg,
            empty_bg,
            phases,
            fill_direction,
        ),
        RenderMode::FillIcon => render_fill_icon_mode(
            timer,
            width,
            height,
            fg_color,
            work_bg,
            break_bg,
            empty_bg,
            phase_icon,
            phases,
            fill_direction,
        ),
        RenderMode::Ripen => render_ripen_mode(timer, width, height, fg_color, phase_icon),
    }
}

/// Render traditional text-based timer display
#[allow(clippy::too_many_arguments)]
fn render_text_mode(
    timer: &Timer,
    width: u32,
    height: u32,
    fg_color: Rgba<u8>,
    work_bg: Rgba<u8>,
    break_bg: Rgba<u8>,
    paused_bg: Rgba<u8>,
    padding: f32,
    paused_text: &str,
    phases: &HashMap<String, String>,
) -> RgbImage {
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
    draw_filled_rect_mut(&mut rgba, Rect::at(0, 0).of_size(width, height), bg);

    // Build display text - show paused_text when paused mid-interval
    let time_text = if !timer.is_running() {
        paused_text.to_string()
    } else {
        timer.remaining_formatted()
    };

    let phase_indicator = match timer.phase() {
        Phase::Work => phases.get("work").map(|s| s.as_str()).unwrap_or("work"),
        Phase::ShortBreak => phases
            .get("short_break")
            .map(|s| s.as_str())
            .unwrap_or("short brk"),
        Phase::LongBreak => phases
            .get("long_break")
            .map(|s| s.as_str())
            .unwrap_or("long brk"),
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

/// Render filling-bucket mode with progress fill
#[allow(clippy::too_many_arguments)]
fn render_filling_bucket_mode(
    timer: &Timer,
    width: u32,
    height: u32,
    fg_color: Rgba<u8>,
    work_bg: Rgba<u8>,
    break_bg: Rgba<u8>,
    empty_bg: Rgba<u8>,
    phases: &HashMap<String, String>,
    fill_direction: FillDirection,
) -> RgbImage {
    let mut rgba = RgbaImage::new(width, height);

    // Fill with empty_bg as the base/unfilled color
    draw_filled_rect_mut(&mut rgba, Rect::at(0, 0).of_size(width, height), empty_bg);

    // Determine the fill color based on current phase
    let fill_color = if timer.phase().is_break() {
        break_bg
    } else {
        work_bg
    };

    // Calculate progress and fill height
    let progress = timer.progress_ratio();
    let fill_height = (height as f32 * progress) as u32;

    if fill_height > 0 {
        match fill_direction {
            FillDirection::EmptyToFull => {
                // Fill from bottom to top
                let y_start = height.saturating_sub(fill_height);
                draw_filled_rect_mut(
                    &mut rgba,
                    Rect::at(0, y_start as i32).of_size(width, fill_height),
                    fill_color,
                );
            }
            FillDirection::FullToEmpty => {
                // Fill from top, draining down (full - progress = remaining fill)
                let remaining = 1.0 - progress;
                let remaining_height = (height as f32 * remaining) as u32;
                if remaining_height > 0 {
                    draw_filled_rect_mut(
                        &mut rgba,
                        Rect::at(0, 0).of_size(width, remaining_height),
                        fill_color,
                    );
                }
            }
        }
    } else if matches!(fill_direction, FillDirection::FullToEmpty) {
        // At start (progress=0), full_to_empty should show full fill
        draw_filled_rect_mut(&mut rgba, Rect::at(0, 0).of_size(width, height), fill_color);
    }

    // Overlay phase indicator (top)
    let phase_indicator = match timer.phase() {
        Phase::Work => phases.get("work").map(|s| s.as_str()).unwrap_or("work"),
        Phase::ShortBreak => phases
            .get("short_break")
            .map(|s| s.as_str())
            .unwrap_or("short brk"),
        Phase::LongBreak => phases
            .get("long_break")
            .map(|s| s.as_str())
            .unwrap_or("long brk"),
    };
    draw_phase_indicator(&mut rgba, phase_indicator, fg_color, 0.0);

    // Overlay iteration progress dots (bottom)
    draw_iteration_dots(&mut rgba, timer.iterations(), width, fg_color);

    // Convert to RGB
    RgbImage::from_fn(width, height, |x, y| {
        let pixel = rgba.get_pixel(x, y);
        Rgb([pixel[0], pixel[1], pixel[2]])
    })
}

/// Render fill-icon mode: fills an icon from bottom to top (or vice versa)
/// Falls back to a simple progress bar if no icon is available
#[allow(clippy::too_many_arguments)]
fn render_fill_icon_mode(
    timer: &Timer,
    width: u32,
    height: u32,
    fg_color: Rgba<u8>,
    work_bg: Rgba<u8>,
    break_bg: Rgba<u8>,
    empty_bg: Rgba<u8>,
    phase_icon: Option<&PluginImage>,
    phases: &HashMap<String, String>,
    fill_direction: FillDirection,
) -> RgbImage {
    let mut rgba = RgbaImage::new(width, height);

    // If no icon available, fall back to filling_bucket mode
    let Some(icon) = phase_icon else {
        static WARNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if !WARNED.swap(true, std::sync::atomic::Ordering::Relaxed) {
            tracing::warn!(
                "fill_icon mode configured but no icon available, falling back to filling_bucket"
            );
        }
        return render_filling_bucket_mode(
            timer,
            width,
            height,
            fg_color,
            work_bg,
            break_bg,
            empty_bg,
            phases,
            fill_direction,
        );
    };

    // First, render the full icon
    let icon_rgb = render_icon(icon, width, height);

    // Copy icon to rgba buffer
    for y in 0..height {
        for x in 0..width {
            let pixel = icon_rgb.get_pixel(x, y);
            rgba.put_pixel(x, y, Rgba([pixel[0], pixel[1], pixel[2], 255]));
        }
    }

    // Calculate progress
    let progress = timer.progress_ratio();

    // Convert unfilled portion to greyscale
    match fill_direction {
        FillDirection::EmptyToFull => {
            // Greyscale from top down to (height - fill_height)
            let fill_height = (height as f32 * progress) as u32;
            let mask_height = height.saturating_sub(fill_height);
            for y in 0..mask_height {
                for x in 0..width {
                    let pixel = rgba.get_pixel(x, y);
                    let grey = to_greyscale(pixel[0], pixel[1], pixel[2]);
                    rgba.put_pixel(x, y, Rgba([grey, grey, grey, pixel[3]]));
                }
            }
        }
        FillDirection::FullToEmpty => {
            // Greyscale from bottom up by progress amount
            let mask_height = (height as f32 * progress) as u32;
            if mask_height > 0 {
                let y_start = height.saturating_sub(mask_height);
                for y in y_start..height {
                    for x in 0..width {
                        let pixel = rgba.get_pixel(x, y);
                        let grey = to_greyscale(pixel[0], pixel[1], pixel[2]);
                        rgba.put_pixel(x, y, Rgba([grey, grey, grey, pixel[3]]));
                    }
                }
            }
        }
    }

    // Overlay iteration progress dots (bottom)
    draw_iteration_dots(&mut rgba, timer.iterations(), width, fg_color);

    // Convert to RGB
    RgbImage::from_fn(width, height, |x, y| {
        let pixel = rgba.get_pixel(x, y);
        Rgb([pixel[0], pixel[1], pixel[2]])
    })
}

/// Render fallback text when paused at phase boundary and no icon is available
fn render_paused_text(
    text: &str,
    width: u32,
    height: u32,
    fg_color: Rgba<u8>,
    bg_color: Rgba<u8>,
    padding: f32,
) -> RgbImage {
    let mut rgba = RgbaImage::new(width, height);

    draw_filled_rect_mut(&mut rgba, Rect::at(0, 0).of_size(width, height), bg_color);

    if let Some(font_bytes) = get_system_monospace_font()
        && let Ok(font) = FontRef::try_from_slice(font_bytes)
    {
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

        for (i, line) in lines.iter().enumerate() {
            let text_width: f32 = line
                .chars()
                .map(|c| scaled_font.h_advance(font.glyph_id(c)))
                .sum();
            let x = ((width as f32 - text_width) / 2.0).max(0.0) as i32;
            let y = (start_y + line_height * i as f32) as i32;
            draw_text_mut(&mut rgba, fg_color, x, y, scale, &font, line);
        }
    }

    RgbImage::from_fn(width, height, |x, y| {
        let pixel = rgba.get_pixel(x, y);
        Rgb([pixel[0], pixel[1], pixel[2]])
    })
}

/// Render an icon image, scaling to fit the button
fn render_icon(icon: &PluginImage, width: u32, height: u32) -> RgbImage {
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
    fg_color: Rgba<u8>,
) -> RgbImage {
    let rgb = render_icon(icon, width, height);

    let mut rgba = RgbaImage::from_fn(width, height, |x, y| {
        let pixel = rgb.get_pixel(x, y);
        Rgba([pixel[0], pixel[1], pixel[2], 255])
    });

    draw_iteration_dots(&mut rgba, iterations, width, fg_color);

    RgbImage::from_fn(width, height, |x, y| {
        let pixel = rgba.get_pixel(x, y);
        Rgb([pixel[0], pixel[1], pixel[2]])
    })
}

/// Draw iteration progress dots at the bottom
fn draw_iteration_dots(rgba: &mut RgbaImage, iterations: u8, width: u32, fg_color: Rgba<u8>) {
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

    draw_text_mut(rgba, fg_color, x, y, scale, &font, &dots);
}

/// Draw centered time text
fn draw_centered_text(
    rgba: &mut RgbaImage,
    text: &str,
    fg_color: Rgba<u8>,
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

    draw_text_mut(rgba, fg_color, x, y, scale, &font, text);
}

/// Draw phase indicator at the top
fn draw_phase_indicator(rgba: &mut RgbaImage, text: &str, fg_color: Rgba<u8>, _padding: f32) {
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

    draw_text_mut(rgba, fg_color, x, y, scale, &font, text);
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

/// Convert RGB to greyscale using luminosity method
fn to_greyscale(r: u8, g: u8, b: u8) -> u8 {
    // Standard luminosity coefficients: 0.299*R + 0.587*G + 0.114*B
    ((0.299 * r as f32) + (0.587 * g as f32) + (0.114 * b as f32)) as u8
}

/// Render ripen mode: icon starts green (unripe) and gradually returns to original colors
fn render_ripen_mode(
    timer: &Timer,
    width: u32,
    height: u32,
    fg_color: Rgba<u8>,
    phase_icon: Option<&PluginImage>,
) -> RgbImage {
    let mut rgba = RgbaImage::new(width, height);

    // If no icon available, show a simple colored background
    let Some(icon) = phase_icon else {
        static WARNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if !WARNED.swap(true, std::sync::atomic::Ordering::Relaxed) {
            tracing::warn!("ripen mode configured but no icon available");
        }
        // Just show a green-ish background
        let green_bg = Rgba([60, 120, 60, 255]);
        draw_filled_rect_mut(&mut rgba, Rect::at(0, 0).of_size(width, height), green_bg);
        draw_iteration_dots(&mut rgba, timer.iterations(), width, fg_color);
        return RgbImage::from_fn(width, height, |x, y| {
            let pixel = rgba.get_pixel(x, y);
            Rgb([pixel[0], pixel[1], pixel[2]])
        });
    };

    // Render the icon
    let icon_rgb = render_icon(icon, width, height);

    // Calculate how "unripe" the icon should be (1.0 = fully green, 0.0 = original)
    // At start (progress=0), we want full green effect
    // At end (progress=1), we want original colors
    let progress = timer.progress_ratio();
    let unripe_factor = 1.0 - progress;

    // Apply hue shift towards green for each pixel
    for y in 0..height {
        for x in 0..width {
            let pixel = icon_rgb.get_pixel(x, y);
            let (new_r, new_g, new_b) =
                shift_hue_towards_green(pixel[0], pixel[1], pixel[2], unripe_factor);
            rgba.put_pixel(x, y, Rgba([new_r, new_g, new_b, 255]));
        }
    }

    // Overlay iteration progress dots
    draw_iteration_dots(&mut rgba, timer.iterations(), width, fg_color);

    RgbImage::from_fn(width, height, |x, y| {
        let pixel = rgba.get_pixel(x, y);
        Rgb([pixel[0], pixel[1], pixel[2]])
    })
}

/// Shift a pixel's hue towards green (120°) by the given factor (0.0 = no shift, 1.0 = full shift)
fn shift_hue_towards_green(r: u8, g: u8, b: u8, factor: f32) -> (u8, u8, u8) {
    if factor <= 0.0 {
        return (r, g, b);
    }

    let (h, s, l) = rgb_to_hsl(r, g, b);

    // Target hue is green (120°)
    const GREEN_HUE: f32 = 120.0;

    // Calculate shortest path to green on the hue circle
    let mut diff = GREEN_HUE - h;
    if diff > 180.0 {
        diff -= 360.0;
    } else if diff < -180.0 {
        diff += 360.0;
    }

    // Shift hue towards green
    let new_h = (h + diff * factor).rem_euclid(360.0);

    hsl_to_rgb(new_h, s, l)
}

/// Convert RGB (0-255) to HSL (H: 0-360, S: 0-1, L: 0-1)
fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let r = r as f32 / 255.0;
    let g = g as f32 / 255.0;
    let b = b as f32 / 255.0;

    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;

    if (max - min).abs() < f32::EPSILON {
        // Achromatic (grey)
        return (0.0, 0.0, l);
    }

    let d = max - min;
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };

    let h = if (max - r).abs() < f32::EPSILON {
        let mut h = (g - b) / d;
        if g < b {
            h += 6.0;
        }
        h * 60.0
    } else if (max - g).abs() < f32::EPSILON {
        ((b - r) / d + 2.0) * 60.0
    } else {
        ((r - g) / d + 4.0) * 60.0
    };

    (h, s, l)
}

/// Convert HSL (H: 0-360, S: 0-1, L: 0-1) to RGB (0-255)
fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    if s.abs() < f32::EPSILON {
        // Achromatic (grey)
        let v = (l * 255.0) as u8;
        return (v, v, v);
    }

    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;

    let h = h / 360.0; // Normalize to 0-1

    let r = hue_to_rgb(p, q, h + 1.0 / 3.0);
    let g = hue_to_rgb(p, q, h);
    let b = hue_to_rgb(p, q, h - 1.0 / 3.0);

    (
        (r * 255.0).round() as u8,
        (g * 255.0).round() as u8,
        (b * 255.0).round() as u8,
    )
}

/// Helper for HSL to RGB conversion
fn hue_to_rgb(p: f32, q: f32, mut t: f32) -> f32 {
    if t < 0.0 {
        t += 1.0;
    }
    if t > 1.0 {
        t -= 1.0;
    }

    if t < 1.0 / 6.0 {
        p + (q - p) * 6.0 * t
    } else if t < 1.0 / 2.0 {
        q
    } else if t < 2.0 / 3.0 {
        p + (q - p) * (2.0 / 3.0 - t) * 6.0
    } else {
        p
    }
}
