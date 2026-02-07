use std::collections::HashMap;

use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use image::{DynamicImage, Rgb, RgbImage, Rgba, RgbaImage};
use imageproc::drawing::{draw_filled_rect_mut, draw_text_mut};
use imageproc::rect::Rect;
use strum::{AsRefStr, EnumString, VariantNames};
use verandah_plugin::api::prelude::*;
use verandah_plugin::utils::prelude::*;

use crate::timer::{Phase, Timer};

/// Convert an RgbaImage to an RgbImage, discarding the alpha channel
fn rgba_to_rgb(rgba: RgbaImage) -> RgbImage {
    DynamicImage::ImageRgba8(rgba).into_rgb8()
}

/// Render mode for the timer display
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, EnumString, AsRefStr, VariantNames)]
#[strum(serialize_all = "snake_case")]
pub enum RenderMode {
    /// Traditional text-based display with time countdown
    #[default]
    Text,
    /// Fill background from bottom to top (or vice versa) as progress indicator
    FillBg,
    /// Fill an icon from bottom to top (or vice versa) as progress indicator
    FillIcon,
    /// Icon starts green (unripe) and gradually returns to original colors as timer progresses
    Ripen,
}

/// Fill direction for fill modes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, EnumString, AsRefStr, VariantNames)]
#[strum(serialize_all = "snake_case")]
pub enum FillDirection {
    /// Fill from bottom to top (empty → full)
    #[default]
    EmptyToFull,
    /// Drain from top to bottom (full → empty)
    FullToEmpty,
}

/// When to display the phase indicator (work, short brk, long brk)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, EnumString, AsRefStr, VariantNames)]
#[strum(serialize_all = "snake_case")]
pub enum PhaseIndicatorDisplay {
    /// Never show the phase indicator
    None,
    /// Show phase indicator only when running
    Running,
    /// Show phase indicator only when paused (default)
    #[default]
    Paused,
    /// Always show the phase indicator
    Both,
}

impl PhaseIndicatorDisplay {
    /// Returns true if the phase indicator should be shown given the current running state
    pub fn should_show(self, is_running: bool) -> bool {
        match self {
            PhaseIndicatorDisplay::None => false,
            PhaseIndicatorDisplay::Running => is_running,
            PhaseIndicatorDisplay::Paused => !is_running,
            PhaseIndicatorDisplay::Both => true,
        }
    }
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
    dot_running: Rgba<u8>,
    dot_paused: Rgba<u8>,
    padding: f32,
    paused_icon: Option<&PluginImage>,
    phase_icon: Option<&PluginImage>,
    fallback_text: Option<&str>,
    paused_text: &str,
    phases: &HashMap<String, String>,
    render_mode: RenderMode,
    fill_direction: FillDirection,
    phase_indicator_display: PhaseIndicatorDisplay,
    pulse_on_pause: bool,
) -> RgbImage {
    // At phase boundary (elapsed=0) and not running: show icon or fallback
    if !timer.is_running() && timer.at_phase_boundary() {
        if let Some(icon) = paused_icon {
            return render_icon_with_dots(
                icon,
                width,
                height,
                display_iterations(timer),
                dot_paused,
            );
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
            phase_indicator_display,
            dot_running,
            dot_paused,
        ),
        RenderMode::FillBg => render_fill_bg_mode(
            timer,
            width,
            height,
            fg_color,
            work_bg,
            break_bg,
            empty_bg,
            phases,
            fill_direction,
            paused_text,
            phase_indicator_display,
            pulse_on_pause,
            dot_running,
            dot_paused,
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
            paused_text,
            phase_indicator_display,
            pulse_on_pause,
            dot_running,
            dot_paused,
        ),
        RenderMode::Ripen => render_ripen_mode(
            timer,
            width,
            height,
            fg_color,
            phase_icon,
            phases,
            paused_text,
            phase_indicator_display,
            pulse_on_pause,
            dot_running,
            dot_paused,
        ),
    }
}

/// Get the phase indicator text from the phases map
fn get_phase_indicator<'a>(timer: &Timer, phases: &'a HashMap<String, String>) -> &'a str {
    match timer.phase() {
        Phase::Work => phases.get("work").map(|s| s.as_str()).unwrap_or("work"),
        Phase::ShortBreak => phases
            .get("short_break")
            .map(|s| s.as_str())
            .unwrap_or("short brk"),
        Phase::LongBreak => phases
            .get("long_break")
            .map(|s| s.as_str())
            .unwrap_or("long brk"),
    }
}

/// Configuration for the common overlay elements
struct OverlayConfig<'a> {
    fg_color: Rgba<u8>,
    dot_running: Rgba<u8>,
    dot_paused: Rgba<u8>,
    /// Phase indicator text (e.g., "work", "short brk") - shown at top normally, bottom when paused
    phase_indicator: Option<&'a str>,
    /// When to display the phase indicator
    phase_indicator_display: PhaseIndicatorDisplay,
    /// Text to show centered when paused (e.g., "||")
    paused_text: Option<&'a str>,
}

/// Render common overlay elements (top/bottom indicators, paused text)
///
/// This handles:
/// - Top indicator: remaining time when paused mid-interval, phase indicator when configured
/// - Bottom indicator: phase indicator when paused mid-interval and configured, dots otherwise
/// - Centered paused text when not running (if provided)
fn render_overlay(rgba: &mut RgbaImage, timer: &Timer, config: &OverlayConfig) {
    let width = rgba.width();
    let is_running = timer.is_running();
    let is_paused_mid_interval = !is_running && !timer.at_phase_boundary();
    let show_phase = config.phase_indicator_display.should_show(is_running);

    let dot_color = if is_running {
        config.dot_running
    } else {
        config.dot_paused
    };

    // Top indicator: remaining time when paused mid-interval, phase indicator when configured
    if is_paused_mid_interval {
        draw_remaining_time_top(rgba, &timer.remaining_formatted(), width, config.fg_color);
    } else if show_phase && let Some(phase) = config.phase_indicator {
        draw_phase_indicator(rgba, phase, config.fg_color);
    }

    // Overlay paused text if not running
    if !is_running && let Some(text) = config.paused_text {
        draw_centered_text_with_reserved(rgba, text, config.fg_color, 0.1, 18.0, 18.0, 0.0);
    }

    // Bottom indicator: phase indicator when paused mid-interval and configured, dots otherwise
    let show_phase_bottom =
        is_paused_mid_interval && config.phase_indicator_display.should_show(false);
    draw_bottom_indicator(
        rgba,
        timer,
        width,
        config.fg_color,
        dot_color,
        config.phase_indicator,
        show_phase_bottom,
    );
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
    phase_indicator_display: PhaseIndicatorDisplay,
    dot_running: Rgba<u8>,
    dot_paused: Rgba<u8>,
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

    // Draw main centered text: paused_text when not running, remaining time when running
    let time_text = if !timer.is_running() {
        paused_text.to_string()
    } else {
        timer.remaining_formatted()
    };
    draw_centered_text_with_reserved(&mut rgba, &time_text, fg_color, padding, 18.0, 18.0, 0.0);

    // Render common overlay elements (top/bottom indicators)
    // paused_text is None because the centered text above already serves that purpose
    let phase_indicator = get_phase_indicator(timer, phases);
    render_overlay(
        &mut rgba,
        timer,
        &OverlayConfig {
            fg_color,
            dot_running,
            dot_paused,
            phase_indicator: Some(phase_indicator),
            phase_indicator_display,
            paused_text: None,
        },
    );

    rgba_to_rgb(rgba)
}

/// Render filling-bucket mode with progress fill
#[allow(clippy::too_many_arguments)]
fn render_fill_bg_mode(
    timer: &Timer,
    width: u32,
    height: u32,
    fg_color: Rgba<u8>,
    work_bg: Rgba<u8>,
    break_bg: Rgba<u8>,
    empty_bg: Rgba<u8>,
    phases: &HashMap<String, String>,
    fill_direction: FillDirection,
    paused_text: &str,
    phase_indicator_display: PhaseIndicatorDisplay,
    pulse_on_pause: bool,
    dot_running: Rgba<u8>,
    dot_paused: Rgba<u8>,
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

    // Apply brightness pulse before overlay (if paused and enabled)
    if !timer.is_running() && pulse_on_pause {
        apply_brightness_pulse(&mut rgba);
    }

    // Render common overlay elements
    let phase_indicator = get_phase_indicator(timer, phases);
    render_overlay(
        &mut rgba,
        timer,
        &OverlayConfig {
            fg_color,
            dot_running,
            dot_paused,
            phase_indicator: Some(phase_indicator),
            phase_indicator_display,
            paused_text: Some(paused_text),
        },
    );

    rgba_to_rgb(rgba)
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
    paused_text: &str,
    phase_indicator_display: PhaseIndicatorDisplay,
    pulse_on_pause: bool,
    dot_running: Rgba<u8>,
    dot_paused: Rgba<u8>,
) -> RgbImage {
    let mut rgba = RgbaImage::new(width, height);

    // If no icon available, fall back to fill_bg mode
    let Some(icon) = phase_icon else {
        static FILL_ICON_NO_ICON_WARNED: std::sync::atomic::AtomicBool =
            std::sync::atomic::AtomicBool::new(false);
        if !FILL_ICON_NO_ICON_WARNED.swap(true, std::sync::atomic::Ordering::Relaxed) {
            tracing::warn!(
                "fill_icon mode configured but no icon available, falling back to fill_bg"
            );
        }
        return render_fill_bg_mode(
            timer,
            width,
            height,
            fg_color,
            work_bg,
            break_bg,
            empty_bg,
            phases,
            fill_direction,
            paused_text,
            phase_indicator_display,
            pulse_on_pause,
            dot_running,
            dot_paused,
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

    // Apply brightness pulse before overlay (if paused and enabled)
    if !timer.is_running() && pulse_on_pause {
        apply_brightness_pulse(&mut rgba);
    }

    // Render common overlay elements
    let phase_indicator = get_phase_indicator(timer, phases);
    render_overlay(
        &mut rgba,
        timer,
        &OverlayConfig {
            fg_color,
            dot_running,
            dot_paused,
            phase_indicator: Some(phase_indicator),
            phase_indicator_display,
            paused_text: Some(paused_text),
        },
    );

    rgba_to_rgb(rgba)
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

    rgba_to_rgb(rgba)
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

/// Render an icon image with iteration dots overlay (used when paused at phase boundary)
fn render_icon_with_dots(
    icon: &PluginImage,
    width: u32,
    height: u32,
    display_iters: u8,
    dot_color: Rgba<u8>,
) -> RgbImage {
    let rgb = render_icon(icon, width, height);

    let mut rgba = RgbaImage::from_fn(width, height, |x, y| {
        let pixel = rgb.get_pixel(x, y);
        Rgba([pixel[0], pixel[1], pixel[2], 255])
    });

    draw_iteration_dots(&mut rgba, display_iters, width, dot_color);

    rgba_to_rgb(rgba)
}

/// Calculate display iterations: dots fill when work STARTS (not ends)
/// During work (running or paused mid-interval): show iterations + 1
/// During work (paused at boundary, not yet started): show iterations
/// During break: iterations already reflects completed work
fn display_iterations(timer: &Timer) -> u8 {
    let work_has_started = timer.is_running() || !timer.at_phase_boundary();
    if timer.phase() == Phase::Work && work_has_started {
        (timer.iterations() + 1).min(4)
    } else {
        timer.iterations()
    }
}

/// Draw bottom indicator: phase indicator when requested, dots otherwise
fn draw_bottom_indicator(
    rgba: &mut RgbaImage,
    timer: &Timer,
    width: u32,
    fg_color: Rgba<u8>,
    dot_color: Rgba<u8>,
    phase_indicator: Option<&str>,
    show_phase_indicator: bool,
) {
    if show_phase_indicator {
        if let Some(phase) = phase_indicator {
            draw_phase_indicator_bottom(rgba, phase, fg_color);
        }
    } else {
        draw_iteration_dots(rgba, display_iterations(timer), width, dot_color);
    }
}

/// Draw iteration progress dots at the bottom
/// `display_iterations` should account for the current phase (work shows +1)
fn draw_iteration_dots(
    rgba: &mut RgbaImage,
    display_iterations: u8,
    width: u32,
    dot_color: Rgba<u8>,
) {
    let Some(font_bytes) = get_system_monospace_font() else {
        return;
    };
    let Ok(font) = FontRef::try_from_slice(font_bytes) else {
        return;
    };

    // Build dots string: filled for active/completed, empty for remaining
    let dots: String = (0..4)
        .map(|i| if i < display_iterations { '●' } else { '○' })
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

    draw_text_mut(rgba, dot_color, x, y, scale, &font, &dots);
}

/// Draw remaining time at the top of the button
fn draw_remaining_time_top(rgba: &mut RgbaImage, text: &str, width: u32, color: Rgba<u8>) {
    let Some(font_bytes) = get_system_monospace_font() else {
        return;
    };
    let Ok(font) = FontRef::try_from_slice(font_bytes) else {
        return;
    };

    let scale = PxScale::from(24.0);
    let scaled_font = font.as_scaled(scale);

    let text_width: f32 = text
        .chars()
        .map(|c| scaled_font.h_advance(font.glyph_id(c)))
        .sum();

    let x = ((width as f32 - text_width) / 2.0).max(0.0) as i32;
    let y = 4; // 4px margin from top

    draw_text_mut(rgba, color, x, y, scale, &font, text);
}

/// Draw phase indicator at the top
fn draw_phase_indicator(rgba: &mut RgbaImage, text: &str, fg_color: Rgba<u8>) {
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

/// Draw phase indicator at the bottom
fn draw_phase_indicator_bottom(rgba: &mut RgbaImage, text: &str, fg_color: Rgba<u8>) {
    let Some(font_bytes) = get_system_monospace_font() else {
        return;
    };
    let Ok(font) = FontRef::try_from_slice(font_bytes) else {
        return;
    };

    let width = rgba.width();

    let scale = PxScale::from(14.0);
    let scaled_font = font.as_scaled(scale);
    let line_height = scaled_font.height();

    let text_width: f32 = text
        .chars()
        .map(|c| scaled_font.h_advance(font.glyph_id(c)))
        .sum();

    let x = ((width as f32 - text_width) / 2.0).max(0.0) as i32;
    let y = (rgba.height() as f32 - line_height - 4.0) as i32;

    draw_text_mut(rgba, fg_color, x, y, scale, &font, text);
}

/// Render ripen mode: icon starts green (unripe) and gradually returns to original colors
#[allow(clippy::too_many_arguments)]
fn render_ripen_mode(
    timer: &Timer,
    width: u32,
    height: u32,
    fg_color: Rgba<u8>,
    phase_icon: Option<&PluginImage>,
    phases: &HashMap<String, String>,
    paused_text: &str,
    phase_indicator_display: PhaseIndicatorDisplay,
    pulse_on_pause: bool,
    dot_running: Rgba<u8>,
    dot_paused: Rgba<u8>,
) -> RgbImage {
    let mut rgba = RgbaImage::new(width, height);

    // If no icon available, show a simple colored background
    let Some(icon) = phase_icon else {
        static RIPEN_NO_ICON_WARNED: std::sync::atomic::AtomicBool =
            std::sync::atomic::AtomicBool::new(false);
        if !RIPEN_NO_ICON_WARNED.swap(true, std::sync::atomic::Ordering::Relaxed) {
            tracing::warn!("ripen mode configured but no icon available");
        }
        // Just show a green-ish background
        let green_bg = Rgba([60, 120, 60, 255]);
        draw_filled_rect_mut(&mut rgba, Rect::at(0, 0).of_size(width, height), green_bg);
        let phase_indicator = get_phase_indicator(timer, phases);
        render_overlay(
            &mut rgba,
            timer,
            &OverlayConfig {
                fg_color,
                dot_running,
                dot_paused,
                phase_indicator: Some(phase_indicator),
                phase_indicator_display,
                paused_text: None,
            },
        );
        return rgba_to_rgb(rgba);
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

    // Apply brightness pulse before overlay (if paused and enabled)
    if !timer.is_running() && pulse_on_pause {
        apply_brightness_pulse(&mut rgba);
    }

    // Render common overlay elements
    let phase_indicator = get_phase_indicator(timer, phases);
    render_overlay(
        &mut rgba,
        timer,
        &OverlayConfig {
            fg_color,
            dot_running,
            dot_paused,
            phase_indicator: Some(phase_indicator),
            phase_indicator_display,
            paused_text: Some(paused_text),
        },
    );

    rgba_to_rgb(rgba)
}

/// Shift a pixel's hue towards green (120°) by the given factor (0.0 = no shift, 1.0 = full shift)
#[inline(always)]
fn shift_hue_towards_green(r: u8, g: u8, b: u8, factor: f32) -> (u8, u8, u8) {
    const INV_255: f32 = 1.0 / 255.0;
    const GREEN_HUE: f32 = 120.0;

    let rf = r as f32 * INV_255;
    let gf = g as f32 * INV_255;
    let bf = b as f32 * INV_255;

    let max = rf.max(gf).max(bf);
    let min = rf.min(gf).min(bf);
    let d = max - min;
    let l = (max + min) * 0.5;

    // Branchless saturation: use a small epsilon to avoid division by zero
    let denom = if l > 0.5 { 2.0 - max - min } else { max + min };
    let s = if d < f32::EPSILON { 0.0 } else { d / denom };

    // Branchless hue calculation
    // Compute all three possible hue values, select based on which channel is max
    let h = if d < f32::EPSILON {
        0.0
    } else {
        let h_r = ((gf - bf) / d).rem_euclid(6.0) * 60.0;
        let h_g = ((bf - rf) / d + 2.0) * 60.0;
        let h_b = ((rf - gf) / d + 4.0) * 60.0;

        // Select based on max channel using conditional moves
        let r_is_max = (max - rf).abs() < f32::EPSILON;
        let g_is_max = (max - gf).abs() < f32::EPSILON;

        if r_is_max {
            h_r
        } else if g_is_max {
            h_g
        } else {
            h_b
        }
    };

    // Calculate shortest path to green on the hue circle
    let raw_diff = GREEN_HUE - h;
    let diff = raw_diff - 360.0 * (raw_diff / 360.0 + 0.5).floor();

    // Shift hue towards green, blend with original based on factor
    let new_h = (h + diff * factor).rem_euclid(360.0);

    // HSL to RGB using chroma-based formula (more amenable to vectorization)
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let h_prime = new_h / 60.0;
    let x = c * (1.0 - (h_prime.rem_euclid(2.0) - 1.0).abs());
    let m = l - c * 0.5;

    // Sector-based RGB assignment using lookup pattern
    let sector = h_prime as u32 % 6;
    let (r1, g1, b1) = match sector {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };

    (
        ((r1 + m) * 255.0 + 0.5) as u8,
        ((g1 + m) * 255.0 + 0.5) as u8,
        ((b1 + m) * 255.0 + 0.5) as u8,
    )
}
