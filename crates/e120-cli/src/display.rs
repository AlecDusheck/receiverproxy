//! Putting images on the wall: patterns, stills, and video.

use crate::util::open;
use crate::{protocol, Cli};
use anyhow::{Context, Result};
use e120_net::bpf;
use e120_video::FrameSource;
use std::io::Write;
use std::time::Duration;

/// Load a wall layout, or build a single-panel one from the size flags.
pub fn load_canvas(cli: &Cli, layout: Option<&str>) -> Result<e120_canvas::Canvas> {
    match layout {
        Some(path) => {
            let text = std::fs::read_to_string(path).with_context(|| format!("read {path}"))?;
            serde_json::from_str(&text).with_context(|| format!("parse {path}"))
        }
        None => Ok(e120_canvas::Canvas::single(
            u32::from(cli.width),
            u32::from(cli.height),
        )),
    }
}

pub fn wall_settings(cli: &Cli) -> e120_driver::Settings {
    e120_driver::Settings {
        brightness: cli.brightness,
        color_order: cli.order,
        announce_layout: true,
    }
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

/// Draw a built-in pattern through the full wall pipeline.
pub fn show_pattern(cli: &Cli, name: &str, hold: bool, layout: Option<&str>) -> Result<()> {
    let canvas = load_canvas(cli, layout)?;
    let pattern: e120_video::Pattern = name.parse().map_err(|e: String| anyhow::anyhow!(e))?;
    let frame = e120_video::pattern(pattern, canvas.width, canvas.height);
    let mut wall = e120_driver::Wall::open(&cli.iface, canvas, wall_settings(cli))?;
    if hold {
        println!("holding {name}, Ctrl-C to stop");
        let mut pacer = e120_driver::Pacer::new(30);
        loop {
            wall.show(&frame)?;
            pacer.wait();
        }
    } else {
        for _ in 0..3 {
            wall.show(&frame)?;
        }
        println!("sent {name}");
        Ok(())
    }
}

/// Send one full frame of pixels: row packets, then the sync/display frame.
/// Send chosen pieces of a display refresh with explicit pacing, so a current
/// meter can attribute a change to one component instead of to `fill`'s whole
/// burst. The pixel content is a solid colour.
pub fn probe(
    cli: &Cli,
    rows: u16,
    row_gap_us: u64,
    sync_after: bool,
    repeat: u32,
    rgb: [u8; 3],
) -> Result<()> {
    let mut dev = open(cli)?;
    let line = vec![[rgb[0], rgb[1], rgb[2]]; cli.width as usize];
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

pub fn send_frame(dev: &mut bpf::Bpf, cli: &Cli, fb: &[[u8; 3]]) -> Result<()> {
    let w = cli.width as usize;
    for row in 0..cli.height {
        let line = &fb[row as usize * w..(row as usize + 1) * w];
        let mut offset = 0usize;
        for chunk in line.chunks(protocol::MAX_PIXELS_PER_PACKET) {
            dev.send(&protocol::pixel_row(row, offset as u16, chunk, cli.order))?;
            offset += chunk.len();
        }
    }
    // FPP sends the display/latch frame twice per refresh on firmware v13+
    // (this card runs 16.53); older firmware tolerates the duplicate.
    dev.send(&protocol::sync(cli.brightness))?;
    dev.send(&protocol::sync(cli.brightness))?;
    Ok(())
}

pub fn show(cli: &Cli, fb: &[[u8; 3]], hold: bool) -> Result<()> {
    let mut dev = open(cli)?;
    dev.send(&protocol::brightness(cli.brightness))?;
    if hold {
        println!("refreshing at ~30fps, Ctrl-C to stop");
        loop {
            send_frame(&mut dev, cli, fb)?;
            std::thread::sleep(Duration::from_millis(33));
        }
    } else {
        // Send a few frames so at least one lands after the card settles
        for _ in 0..3 {
            send_frame(&mut dev, cli, fb)?;
            std::thread::sleep(Duration::from_millis(33));
        }
        println!(
            "frame sent ({}x{}, order {:?})",
            cli.width, cli.height, cli.order
        );
        Ok(())
    }
}

pub fn solid(cli: &Cli, r: u8, g: u8, b: u8) -> Vec<[u8; 3]> {
    vec![[r, g, b]; cli.width as usize * cli.height as usize]
}

pub fn test_pattern(cli: &Cli, pattern: &str) -> Result<Vec<[u8; 3]>> {
    let (w, h) = (cli.width as usize, cli.height as usize);
    let mut fb = vec![[0u8; 3]; w * h];
    match pattern {
        "gradient" => {
            for y in 0..h {
                for x in 0..w {
                    fb[y * w + x] = [(x * 255 / w.max(1)) as u8, (y * 255 / h.max(1)) as u8, 128];
                }
            }
        }
        "rows" => {
            // Each row: red if row%3==0, green if 1, blue if 2 — for mapping checks
            for y in 0..h {
                let c = match y % 3 {
                    0 => [255, 0, 0],
                    1 => [0, 255, 0],
                    _ => [0, 0, 255],
                };
                for x in 0..w {
                    fb[y * w + x] = c;
                }
            }
        }
        "border" => {
            for y in 0..h {
                for x in 0..w {
                    if x == 0 || y == 0 || x == w - 1 || y == h - 1 {
                        fb[y * w + x] = [255, 255, 255];
                    }
                }
            }
            // Corner markers: top-left red, top-right green, bottom-left blue
            fb[0] = [255, 0, 0];
            fb[w - 1] = [0, 255, 0];
            fb[(h - 1) * w] = [0, 0, 255];
        }
        "rgb" => {
            // Thirds: red / green / blue vertical bands — color order check
            for y in 0..h {
                for x in 0..w {
                    fb[y * w + x] = if x < w / 3 {
                        [255, 0, 0]
                    } else if x < 2 * w / 3 {
                        [0, 255, 0]
                    } else {
                        [0, 0, 255]
                    };
                }
            }
        }
        other => anyhow::bail!("unknown pattern {other:?} (gradient|rows|border|rgb)"),
    }
    Ok(fb)
}
