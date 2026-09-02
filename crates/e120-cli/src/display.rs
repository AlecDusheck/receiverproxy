//! Putting images on the wall: patterns, stills, and video. Every content
//! command sends through `e120_driver::Wall`, so the frame recipe lives once.

use crate::util::open;
use crate::{protocol, Cli};
use anyhow::{Context, Result};
use e120_canvas::{Canvas, Frame};
use e120_video::FrameSource;
use std::io::Write;
use std::time::Duration;

/// Load a wall layout, or build a single-panel one from the size flags.
pub fn load_canvas(cli: &Cli, layout: Option<&str>) -> Result<Canvas> {
    match layout {
        Some(path) => {
            let text = std::fs::read_to_string(path).with_context(|| format!("read {path}"))?;
            serde_json::from_str(&text).with_context(|| format!("parse {path}"))
        }
        None => Ok(Canvas::single(u32::from(cli.width), u32::from(cli.height))),
    }
}

/// An environment override for bench experiments, or the measured default.
fn env_or<T: std::str::FromStr>(name: &str, default: T) -> T {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Driver settings from the CLI flags. `E120_LATCHES`, `E120_LATCH_GAP_US`
/// and `E120_ROW_GAP_US` override the measured timing for experiments.
pub fn wall_settings(cli: &Cli) -> e120_driver::Settings {
    e120_driver::Settings {
        brightness: cli.brightness,
        color_order: cli.order,
        announce_layout: false,
        timing: e120_driver::Timing {
            latches: env_or("E120_LATCHES", 3),
            latch_gap: Duration::from_micros(env_or("E120_LATCH_GAP_US", 500)),
            // The card's receive FIFO is 1 KB; a gap here is the experiment
            // for a line-rate burst dropping its tail. None was needed.
            row_gap: Duration::from_micros(env_or("E120_ROW_GAP_US", 0)),
        },
    }
}

/// Refresh period for held stills; `E120_FRAME_MS` overrides it.
fn frame_period() -> Duration {
    Duration::from_millis(env_or("E120_FRAME_MS", 33))
}

/// Play a video source onto the wall.
pub fn play(
    cli: &Cli,
    input: &str,
    fps: u32,
    fit: &str,
    looping: bool,
    layout: Option<&str>,
) -> Result<()> {
    let canvas = load_canvas(cli, layout)?;
    let fit = match fit {
        "stretch" => e120_video::Fit::Stretch,
        "contain" => e120_video::Fit::Contain,
        "cover" => e120_video::Fit::Cover,
        other => anyhow::bail!("unknown fit {other:?} (stretch|contain|cover)"),
    };
    println!(
        "playing {input} on {}x{} at {fps} fps",
        canvas.width, canvas.height
    );

    let mut source =
        e120_video::VideoSource::open(input, canvas.width, canvas.height, fps, fit, looping)?;
    let mut wall = e120_driver::Wall::open(&cli.iface, canvas, wall_settings(cli))?;
    let mut pacer = e120_driver::Pacer::new(fps);

    while let Some(frame) = source.next_frame()? {
        wall.show(&frame)?;
        pacer.wait();
        if wall.frames_sent().is_multiple_of(60) {
            print!(
                "\r{} frames, {:.1} fps",
                wall.frames_sent(),
                pacer.achieved_fps()
            );
            std::io::stdout().flush().ok();
        }
    }
    println!(
        "\rplayed {} frames at {:.1} fps",
        wall.frames_sent(),
        pacer.achieved_fps()
    );
    Ok(())
}

/// Show one still: three refreshes, or refresh until Ctrl-C when `hold`.
pub fn show_frame(cli: &Cli, canvas: Canvas, frame: &Frame, hold: bool, what: &str) -> Result<()> {
    let mut wall = e120_driver::Wall::open(&cli.iface, canvas, wall_settings(cli))?;
    let period = frame_period();
    if hold {
        println!(
            "holding {what}, refreshing every {} ms, Ctrl-C to stop",
            period.as_millis()
        );
        loop {
            wall.show(frame)?;
            std::thread::sleep(period);
        }
    }
    // Three, so at least one lands after the card settles.
    for _ in 0..3 {
        wall.show(frame)?;
        std::thread::sleep(period);
    }
    println!(
        "sent {what} ({}x{}, order {:?})",
        frame.width, frame.height, cli.order
    );
    Ok(())
}

/// Draw a built-in pattern.
pub fn show_pattern(cli: &Cli, name: &str, hold: bool, layout: Option<&str>) -> Result<()> {
    let canvas = load_canvas(cli, layout)?;
    let pattern: e120_video::Pattern = name.parse().map_err(|e: String| anyhow::anyhow!(e))?;
    let frame = e120_video::pattern(pattern, canvas.width, canvas.height);
    show_frame(cli, canvas, &frame, hold, name)
}

/// Fill the panel with one colour.
pub fn show_solid(cli: &Cli, rgb: [u8; 3], hold: bool) -> Result<()> {
    let canvas = load_canvas(cli, None)?;
    let frame = Frame::from_rgb(
        canvas.width,
        canvas.height,
        rgb.repeat((canvas.width * canvas.height) as usize),
    )?;
    let what = format!("{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2]);
    show_frame(cli, canvas, &frame, hold, &what)
}

/// Display an image file, scaled to the panel.
pub fn show_image(cli: &Cli, path: &str, hold: bool) -> Result<()> {
    let canvas = load_canvas(cli, None)?;
    let img = image::open(path)
        .with_context(|| format!("open image {path}"))?
        .resize_exact(
            canvas.width,
            canvas.height,
            image::imageops::FilterType::Lanczos3,
        )
        .to_rgb8();
    let frame = Frame::from_rgb(canvas.width, canvas.height, img.into_raw())?;
    show_frame(cli, canvas, &frame, hold, path)
}

/// Send chosen pieces of a refresh with explicit pacing, so a current meter
/// can attribute a change to one component instead of to a whole burst.
/// The pixel content is a solid colour. Diagnosis only.
pub fn probe(
    cli: &Cli,
    rows: u16,
    row_gap_us: u64,
    sync_after: bool,
    repeat: u32,
    rgb: [u8; 3],
) -> Result<()> {
    let mut dev = open(cli)?;
    let line = vec![rgb; cli.width as usize];
    for pass in 0..repeat {
        for row in 0..rows {
            dev.send(&protocol::pixel_row(row, 0, &line, cli.order))?;
            if row_gap_us > 0 {
                std::thread::sleep(Duration::from_micros(row_gap_us));
            }
        }
        if sync_after {
            dev.send(&protocol::sync(cli.brightness))?;
        }
        if repeat > 1 && pass + 1 < repeat {
            std::thread::sleep(Duration::from_millis(33));
        }
    }
    println!(
        "probe: {repeat}x {rows} rows, {row_gap_us}us apart, sync {}",
        if sync_after { "after each pass" } else { "never" }
    );
    Ok(())
}
