//! Putting images on the wall: patterns, stills, and video. Every content
//! command sends through `driver::Wall`, so the frame recipe lives once.

use crate::util::open;
use crate::{protocol, Ctx, Progress};
use anyhow::{Context, Result};
use sources::{Fit, FrameSource};
use std::time::Duration;
use wall::{Canvas, Frame};

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

/// Set the panel's brightness.
///
/// The brightness frame, then the latches that commit it, in the order and
/// with the gap a refresh uses: one latch with no gap leaves a held frame at
/// its old brightness (docs/rendering.md).
///
/// # Errors
/// Fails if the link cannot be opened.
pub fn brightness(ctx: &Ctx, value: u8) -> Result<()> {
    let timing = wall_settings(ctx).timing;
    let mut dev = open(ctx)?;
    dev.send(&protocol::brightness(value))?;
    std::thread::sleep(timing.latch_gap);
    for _ in 0..timing.latches {
        dev.send(&protocol::sync(value))?;
    }
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
/// `module` sizes the calibration pattern's tiles and is ignored by the rest.
pub fn show_pattern(
    ctx: &Ctx,
    name: &str,
    hold: bool,
    layout: Option<&str>,
    module: Option<(u32, u32)>,
    p: &mut dyn Progress,
) -> Result<()> {
    let canvas = load_canvas(ctx, layout)?;
    show_pattern_on(ctx, canvas, name, hold, module, p)
}

/// Draw a built-in pattern on `canvas`.
pub fn show_pattern_on(
    ctx: &Ctx,
    canvas: Canvas,
    name: &str,
    hold: bool,
    module: Option<(u32, u32)>,
    p: &mut dyn Progress,
) -> Result<()> {
    let pattern: sources::Pattern = name.parse()?;
    let frame = pattern_frame(pattern, &canvas, module);
    show_frame(ctx, canvas, &frame, hold, p)
}

/// The frame a pattern draws on `canvas`. Every pattern but `calibrate` is
/// drawn once across the whole canvas; `calibrate` needs to know where the
/// modules are.
#[must_use]
pub fn pattern_frame(
    pattern: sources::Pattern,
    canvas: &Canvas,
    module: Option<(u32, u32)>,
) -> Frame {
    if pattern == sources::Pattern::Calibrate {
        return calibration_frame(canvas, module);
    }
    sources::pattern(pattern, canvas.width, canvas.height)
}

/// The calibration pattern for `canvas`: a tile per panel where the layout
/// places them, so a wall of several cards is labelled by its own geometry,
/// or a `module`-sized grid when the caller names one.
#[must_use]
pub fn calibration_frame(canvas: &Canvas, module: Option<(u32, u32)>) -> Frame {
    if let Some(module) = module {
        return sources::calibration(canvas.width, canvas.height, module, (0, 0));
    }
    let mut frame = Frame::black(canvas.width, canvas.height);
    for panel in &canvas.panels {
        let tile = (panel.x / panel.width.max(1), panel.y / panel.height.max(1));
        sources::calibration_tile(
            &mut frame,
            (panel.x, panel.y),
            (panel.width, panel.height),
            tile,
        );
    }
    sources::calibration_diagonal(&mut frame);
    frame
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
pub fn show_image(
    ctx: &Ctx,
    path: &str,
    hold: bool,
    layout: Option<&str>,
    p: &mut dyn Progress,
) -> Result<()> {
    let canvas = load_canvas(ctx, layout)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The frames a brightness change sends, in order.
    fn frames(value: u8, latches: u32) -> Vec<Vec<u8>> {
        let mut out = vec![protocol::brightness(value).to_vec()];
        out.extend(std::iter::repeat_n(
            protocol::sync(value).to_vec(),
            latches as usize,
        ));
        out
    }

    #[test]
    fn brightness_sends_the_frame_then_the_measured_number_of_latches() {
        let t = driver::Timing::default();
        let f = frames(40, t.latches);
        assert_eq!(f.len(), 1 + t.latches as usize);
        assert_eq!(
            f[0][12..14],
            [0x0a, 40],
            "brightness frame type carries the value"
        );
        assert_eq!(f[0][14..17], [40, 40, 0xff], "brightness block");
        assert_eq!(f[1][12..14], [0x01, 0x07], "latch frame type");
        assert_eq!(f[1][35], 40, "master brightness");
        assert_eq!(f[1][38..41], [40, 40, 40], "channel gains");
        assert!(f[1..].iter().all(|x| *x == f[1]), "every latch identical");
    }

    #[test]
    fn calibration_labels_a_wall_from_the_layout_and_a_panel_from_the_module() {
        // Four cards, one 64x32 panel each.
        let canvas = wall::Canvas::cards(64, 32, 2, 2);
        let wall = calibration_frame(&canvas, None);
        assert_eq!(wall, sources::calibration(128, 64, (64, 32), (0, 0)));

        // A single panel with a module size splits into the same four tiles.
        let one = calibration_frame(&wall::Canvas::single(128, 64), Some((64, 32)));
        assert_eq!(one, wall);

        // Without one, the single panel is a single module.
        let whole = calibration_frame(&wall::Canvas::single(128, 64), None);
        assert_eq!(
            whole,
            sources::pattern(sources::Pattern::Calibrate, 128, 64)
        );
        assert_ne!(whole, wall);
    }
}
