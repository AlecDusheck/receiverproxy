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

/// How a framebuffer is cut into row packets.
///
/// The card's own pixel map (record 0x03) is indexed by `row * width + col`
/// over the *stored* height — half the panel — because the two halves of the
/// module hang off separate hub data groups. That leaves open whether the wire
/// wants one packet per panel row or one double-width packet per stored row,
/// and if the latter, which panel row supplies the second half. The vendor
/// sender is no help here: it packetises whatever framebuffer it is handed
/// without knowing the panel geometry at all (docs/pixel-protocol.md §3).
/// So the layout is a measurement, not a derivation.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Raster {
    /// One packet per panel row, `width` pixels: the FPP layout.
    Rows,
    /// `height/2` packets of `2*width`: row r carries panel rows r and r+h/2.
    SplitHalves,
    /// `height/2` packets of `2*width`: the halves the other way round.
    SplitHalvesSwapped,
    /// `height/2` packets of `2*width`: row r carries panel rows 2r and 2r+1.
    SplitInterleaved,
}

impl std::str::FromStr for Raster {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, String> {
        match s {
            "rows" => Ok(Self::Rows),
            "halves" => Ok(Self::SplitHalves),
            "halves-swapped" => Ok(Self::SplitHalvesSwapped),
            "interleaved" => Ok(Self::SplitInterleaved),
            other => Err(format!(
                "unknown raster {other:?} (rows|halves|halves-swapped|interleaved)"
            )),
        }
    }
}

/// Send one frame using the chosen raster layout.
pub fn send_frame_as(
    dev: &mut bpf::Bpf,
    cli: &Cli,
    fb: &[[u8; 3]],
    raster: Raster,
) -> Result<()> {
    dev.send(&protocol::sync(cli.brightness))?;
    dev.send(&protocol::brightness(cli.brightness))?;
    let w = cli.width as usize;
    let h = cli.height as usize;
    if raster == Raster::Rows {
        return send_rows(dev, cli, fb, w, h);
    }
    let half = h / 2;
    let mut line = vec![[0u8; 3]; w * 2];
    for r in 0..half {
        let (a, b) = match raster {
            Raster::SplitHalves => (r, r + half),
            Raster::SplitHalvesSwapped => (r + half, r),
            _ => (2 * r, 2 * r + 1),
        };
        line[..w].copy_from_slice(&fb[a * w..(a + 1) * w]);
        line[w..].copy_from_slice(&fb[b * w..(b + 1) * w]);
        let mut offset = 0usize;
        for chunk in line.chunks(protocol::MAX_PIXELS_PER_PACKET) {
            dev.send(&protocol::pixel_row(
                r as u16,
                offset as u16,
                chunk,
                cli.order,
            ))?;
            offset += chunk.len();
        }
    }
    Ok(())
}

fn send_rows(
    dev: &mut bpf::Bpf,
    cli: &Cli,
    fb: &[[u8; 3]],
    w: usize,
    h: usize,
) -> Result<()> {
    for row in 0..h {
        let line = &fb[row * w..(row + 1) * w];
        let mut offset = 0usize;
        for chunk in line.chunks(protocol::MAX_PIXELS_PER_PACKET) {
            dev.send(&protocol::pixel_row(
                row as u16,
                offset as u16,
                chunk,
                cli.order,
            ))?;
            offset += chunk.len();
        }
    }
    Ok(())
}

pub fn send_frame(dev: &mut bpf::Bpf, cli: &Cli, fb: &[[u8; 3]]) -> Result<()> {
    // The vendor's own sender leads each frame with the latch, follows it with
    // the brightness frame, and only then sends the rows — the whole burst
    // back to back (docs/pixel-protocol.md). We previously sent rows first and
    // latched afterwards, which is FPP's order, not Colorlight's.
    dev.send(&protocol::sync(cli.brightness))?;
    dev.send(&protocol::brightness(cli.brightness))?;
    let w = cli.width as usize;
    for row in 0..cli.height {
        let line = &fb[row as usize * w..(row as usize + 1) * w];
        let mut offset = 0usize;
        for chunk in line.chunks(protocol::MAX_PIXELS_PER_PACKET) {
            dev.send(&protocol::pixel_row(row, offset as u16, chunk, cli.order))?;
            offset += chunk.len();
        }
    }
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

/// `show`, but with an explicit raster layout.
pub fn show_as(cli: &Cli, fb: &[[u8; 3]], hold: bool, raster: Raster) -> Result<()> {
    let mut dev = open(cli)?;
    dev.send(&protocol::brightness(cli.brightness))?;
    if hold {
        println!("refreshing at ~30fps ({raster:?}), Ctrl-C to stop");
        loop {
            send_frame_as(&mut dev, cli, fb, raster)?;
            std::thread::sleep(Duration::from_millis(33));
        }
    } else {
        for _ in 0..3 {
            send_frame_as(&mut dev, cli, fb, raster)?;
            std::thread::sleep(Duration::from_millis(33));
        }
        println!("frame sent ({raster:?})");
        Ok(())
    }
}
