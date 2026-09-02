//! Putting images on the wall: patterns, stills, and video. Every content
//! command sends through `driver::Wall`, so the frame recipe lives once.

use crate::util::open;
use crate::{protocol, Ctx, Progress};
use anyhow::{Context, Result};
use wall::{Canvas, Frame};
use sources::{Fit, FrameSource};
use std::time::Duration;

/// Load a wall layout, or build a single-panel one from the size flags.
pub fn load_canvas(ctx: &Ctx, layout: Option<&str>) -> Result<Canvas> {
    match layout {
        Some(path) => {
            let text = std::fs::read_to_string(path).with_context(|| format!("read {path}"))?;
            serde_json::from_str(&text).with_context(|| format!("parse {path}"))
        }
        None => Ok(Canvas::single(u32::from(ctx.width), u32::from(ctx.height))),
    }
}

/// An environment override for bench experiments, or the measured default.
fn env_or<T: std::str::FromStr>(name: &str, default: T) -> T {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Driver settings from the CLI flags. `RXP_LATCHES`, `RXP_LATCH_GAP_US`
/// and `RXP_ROW_GAP_US` override the measured timing for experiments.
pub fn wall_settings(ctx: &Ctx) -> driver::Settings {
    let t = driver::Timing::default();
    let micros = |name, d: Duration| Duration::from_micros(env_or(name, d.as_micros() as u64));
    driver::Settings {
        brightness: ctx.brightness,
        color_order: ctx.order,
        announce_layout: false,
        timing: driver::Timing {
            latches: env_or("RXP_LATCHES", t.latches),
            latch_gap: micros("RXP_LATCH_GAP_US", t.latch_gap),
            row_gap: micros("RXP_ROW_GAP_US", t.row_gap),
        },
    }
}

/// Refresh period for held stills; `RXP_FRAME_MS` overrides it.
fn frame_period() -> Duration {
    Duration::from_millis(env_or("RXP_FRAME_MS", 33))
}

/// Send the brightness and sync frames once.
///
/// # Errors
/// Fails if the link cannot be opened.
pub fn brightness(ctx: &Ctx, value: u8) -> Result<()> {
    let mut dev = open(ctx)?;
    dev.send(&protocol::brightness(value))?;
    dev.send(&protocol::sync(value))?;
    Ok(())
}

/// Play a video source onto the wall in `layout`, or a single panel.
pub fn play(
    ctx: &Ctx,
    input: &str,
    fps: u32,
    fit: &str,
    looping: bool,
    layout: Option<&str>,
    p: &mut dyn Progress,
) -> Result<()> {
    let canvas = load_canvas(ctx, layout)?;
    let fit: Fit = fit.parse()?;
    play_on(ctx, canvas, input, fps, fit, looping, p)
}

/// Play a video source onto `canvas`. Reports `N frames, F fps` every 60
/// frames as a transient line and once at the end; stops when cancelled.
#[allow(clippy::too_many_arguments)]
pub fn play_on(
    ctx: &Ctx,
    canvas: Canvas,
    input: &str,
    fps: u32,
    fit: Fit,
    looping: bool,
    p: &mut dyn Progress,
) -> Result<()> {
    let mut source =
        sources::VideoSource::open(input, canvas.width, canvas.height, fps, fit, looping)?;
    let mut frame = Frame::black(canvas.width, canvas.height);
    let mut wall = driver::Wall::open(&ctx.iface, canvas, wall_settings(ctx))?;
    let mut pacer = driver::Pacer::new(fps);

    while !p.cancelled() && source.next_frame(&mut frame)? {
        wall.show(&frame)?;
        pacer.wait();
        if wall.frames_sent().is_multiple_of(60) {
            p.transient(&format!(
                "{} frames, {:.1} fps",
                wall.frames_sent(),
                pacer.achieved_fps()
            ));
        }
    }
    p.clear_transient();
    p.out(&format!(
        "{} frames, {:.1} fps",
        wall.frames_sent(),
        pacer.achieved_fps()
    ));
    Ok(())
}

/// Show one still: three refreshes, or refresh until cancelled when `hold`.
pub fn show_frame(
    ctx: &Ctx,
    canvas: Canvas,
    frame: &Frame,
    hold: bool,
    p: &mut dyn Progress,
) -> Result<()> {
    let mut wall = driver::Wall::open(&ctx.iface, canvas, wall_settings(ctx))?;
    let period = frame_period();
    if hold {
        while !p.cancelled() {
            wall.show(frame)?;
            std::thread::sleep(period);
        }
        return Ok(());
    }
    // Three, so at least one lands after the card settles.
    for _ in 0..3 {
        wall.show(frame)?;
        std::thread::sleep(period);
    }
    Ok(())
}

/// Draw a built-in pattern on the wall in `layout`, or a single panel.
pub fn show_pattern(
    ctx: &Ctx,
    name: &str,
    hold: bool,
    layout: Option<&str>,
    p: &mut dyn Progress,
) -> Result<()> {
    let canvas = load_canvas(ctx, layout)?;
    show_pattern_on(ctx, canvas, name, hold, p)
}

/// Draw a built-in pattern on `canvas`.
pub fn show_pattern_on(
    ctx: &Ctx,
    canvas: Canvas,
    name: &str,
    hold: bool,
    p: &mut dyn Progress,
) -> Result<()> {
    let pattern: sources::Pattern = name.parse()?;
    let frame = sources::pattern(pattern, canvas.width, canvas.height);
    show_frame(ctx, canvas, &frame, hold, p)
}

/// Fill the panel with one colour.
pub fn show_solid(ctx: &Ctx, rgb: [u8; 3], hold: bool, p: &mut dyn Progress) -> Result<()> {
    let canvas = load_canvas(ctx, None)?;
    show_solid_on(ctx, canvas, rgb, hold, p)
}

/// Fill `canvas` with one colour.
pub fn show_solid_on(
    ctx: &Ctx,
    canvas: Canvas,
    rgb: [u8; 3],
    hold: bool,
    p: &mut dyn Progress,
) -> Result<()> {
    let frame = Frame::from_rgb(
        canvas.width,
        canvas.height,
        rgb.repeat((canvas.width * canvas.height) as usize),
    )?;
    show_frame(ctx, canvas, &frame, hold, p)
}

/// Display an image file, scaled to the panel.
pub fn show_image(ctx: &Ctx, path: &str, hold: bool, p: &mut dyn Progress) -> Result<()> {
    let canvas = load_canvas(ctx, None)?;
    let img = image::open(path).with_context(|| format!("open image {path}"))?;
    let frame = image_frame(&img, &canvas, Fit::Stretch)?;
    show_frame(ctx, canvas, &frame, hold, p)
}

/// `img` as a canvas-sized frame: `Stretch` ignores the aspect ratio,
/// `Contain` letterboxes in black, `Cover` crops. Lanczos3 throughout.
///
/// # Errors
/// Fails if the resampled image does not match the canvas size.
pub fn image_frame(img: &image::DynamicImage, canvas: &Canvas, fit: Fit) -> Result<Frame> {
    use image::imageops::FilterType::Lanczos3;
    let (w, h) = (canvas.width, canvas.height);
    let rgb = match fit {
        Fit::Stretch => img.resize_exact(w, h, Lanczos3).to_rgb8(),
        Fit::Cover => img.resize_to_fill(w, h, Lanczos3).to_rgb8(),
        Fit::Contain => {
            let scaled = img.resize(w, h, Lanczos3).to_rgb8();
            let mut out = image::RgbImage::new(w, h);
            let x = i64::from((w - scaled.width()) / 2);
            let y = i64::from((h - scaled.height()) / 2);
            image::imageops::replace(&mut out, &scaled, x, y);
            out
        }
    };
    Ok(Frame::from_rgb(w, h, rgb.into_raw())?)
}

/// Send chosen pieces of a refresh with explicit pacing, so a current meter
/// can attribute a change to one component instead of to a whole burst.
/// The pixel content is a solid colour. Diagnosis only.
pub fn probe(
    ctx: &Ctx,
    rows: u16,
    row_gap_us: u64,
    sync_after: bool,
    repeat: u32,
    rgb: [u8; 3],
) -> Result<()> {
    let mut dev = open(ctx)?;
    let line = vec![rgb; ctx.width as usize];
    for pass in 0..repeat {
        for row in 0..rows {
            dev.send(&protocol::pixel_row(row, 0, &line, ctx.order))?;
            if row_gap_us > 0 {
                std::thread::sleep(Duration::from_micros(row_gap_us));
            }
        }
        if sync_after {
            dev.send(&protocol::sync(ctx.brightness))?;
        }
        if repeat > 1 && pass + 1 < repeat {
            std::thread::sleep(Duration::from_millis(33));
        }
    }
    Ok(())
}
