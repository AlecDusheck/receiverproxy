//! `e120-demo`: effects that lean on what an LED is, driven through
//! `driver` the way any other program would.

mod effects;
mod util;

use anyhow::{bail, Context, Result};
use clap::Parser;
use wall::{Canvas, Frame};
use driver::{Pacer, Settings, Wall};
use effects::{Entry, Refresh, REGISTRY};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// Effects that look better on an LED panel than on an LCD. Ctrl-C leaves
/// the panel as it is.
#[derive(Parser)]
#[command(name = "e120-demo", version)]
struct Cli {
    /// Effect name, `list`, or `cycle`
    #[arg(value_name = "EFFECT")]
    effect: String,
    /// Stop after N seconds; until Ctrl-C by default
    #[arg(long, value_name = "N")]
    seconds: Option<f32>,
    /// Seconds per effect in `cycle`
    #[arg(long, default_value_t = 20, value_name = "N")]
    every: u32,
    /// Frame rate; 30, or more where an effect asks for it
    #[arg(long, value_name = "N")]
    fps: Option<u32>,
    /// Network interface directly connected to the receiving card
    #[arg(long, default_value = "en24")]
    iface: String,
    /// Wall layout JSON (`e120 card layout-example`); one 128x64 panel by default
    #[arg(long, value_name = "FILE")]
    layout: Option<String>,
    /// Brightness 0-255; effects that pulse do so below it
    #[arg(long, default_value_t = 255, value_name = "N")]
    brightness: u8,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    if cli.effect == "list" {
        print!("{}", list());
        return Ok(());
    }
    let canvas = load_canvas(cli.layout.as_deref())?;
    let (width, height) = (canvas.width, canvas.height);
    let settings = Settings {
        brightness: cli.brightness,
        ..Settings::default()
    };
    let mut show = Show {
        wall: Wall::open(&cli.iface, canvas, settings)?,
        frame: Frame::black(width, height),
        base: cli.brightness,
        partial: cli.layout.is_none(),
        fps: cli.fps,
        gains: (cli.brightness, [cli.brightness; 3]),
    };
    if cli.effect == "cycle" {
        loop {
            for entry in REGISTRY {
                eprintln!("{}", entry.name);
                show.run(entry, Some(cli.every as f32))?;
            }
        }
    }
    let Some(entry) = effects::find(&cli.effect) else {
        bail!(
            "{}: unknown effect; `e120-demo list` names them",
            cli.effect
        );
    };
    show.run(entry, cli.seconds)
}

/// One line per effect: the name and its blurb.
fn list() -> String {
    use std::fmt::Write;
    let mut text = String::new();
    for e in REGISTRY {
        // Writing to a String cannot fail.
        let _ = writeln!(text, "{:<10} {}", e.name, e.blurb);
    }
    text
}

/// A layout file, or the bench panel.
fn load_canvas(layout: Option<&str>) -> Result<Canvas> {
    match layout {
        Some(path) => {
            let text = std::fs::read_to_string(path).with_context(|| format!("read {path}"))?;
            serde_json::from_str(&text).with_context(|| format!("parse {path}"))
        }
        None => Ok(Canvas::single(128, 64)),
    }
}

fn seed() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(1, |d| d.as_nanos() as u64)
}

/// `a * b / 255`, rounded.
fn scale(a: u8, b: u8) -> u8 {
    ((u16::from(a) * u16::from(b) + 127) / 255) as u8
}

/// The wall, one reused frame, and the gains last sent.
struct Show {
    wall: Wall,
    frame: Frame,
    /// The command line's brightness; effect gains are fractions of it.
    base: u8,
    /// Row ranges from effects are canvas rows, which are screen rows only
    /// on the single unrotated panel; with a layout every frame goes whole.
    partial: bool,
    fps: Option<u32>,
    gains: (u8, [u8; 3]),
}

impl Show {
    /// Run one effect for `seconds`, or until the process is stopped.
    fn run(&mut self, entry: &Entry, seconds: Option<f32>) -> Result<()> {
        let mut effect = (entry.build)(self.frame.width, self.frame.height, seed());
        let fps = self.fps.or_else(|| effect.fps()).unwrap_or(30).max(1);
        let mut pacer = Pacer::new(fps);
        let start = Instant::now();
        let mut last = start;
        let mut dt = 1.0 / fps as f32;
        loop {
            let t = start.elapsed().as_secs_f32();
            effect.step(t, dt, &mut self.frame);
            self.send(effect.refresh())
                .with_context(|| format!("{}: frame {}", entry.name, self.wall.frames_sent()))?;
            pacer.wait();
            let now = Instant::now();
            // A stall (a sleeping laptop) is not a frame's worth of motion.
            dt = (now - last).as_secs_f32().min(0.1);
            last = now;
            if seconds.is_some_and(|s| t >= s) {
                return Ok(());
            }
        }
    }

    fn send(&mut self, refresh: Refresh) -> Result<()> {
        let master = scale(self.base, refresh.gain);
        let gains = refresh.cast.map(|c| scale(master, c));
        if self.gains != (master, gains) {
            self.wall.set_gains(master, gains);
            self.gains = (master, gains);
        }
        match refresh.rows {
            Some(rows) if self.partial => self.wall.show_rows(&self.frame, rows),
            _ => self.wall.show(&self.frame),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_names_every_registered_effect_once() {
        let text = list();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), REGISTRY.len());
        for (line, entry) in lines.iter().zip(REGISTRY) {
            assert!(line.starts_with(entry.name), "{line}");
            assert!(line.ends_with(entry.blurb), "{line}");
        }
    }

    #[test]
    fn gains_scale_and_round() {
        assert_eq!(scale(255, 255), 255);
        assert_eq!(scale(40, 255), 40);
        assert_eq!(scale(255, 0), 0);
        assert_eq!(scale(40, 128), 20);
    }

    #[test]
    fn the_command_line_parses_the_documented_forms() {
        let cli =
            Cli::try_parse_from(["e120-demo", "stars", "--seconds", "5", "--fps", "60"]).unwrap();
        assert_eq!(
            (cli.effect.as_str(), cli.seconds, cli.fps),
            ("stars", Some(5.0), Some(60))
        );
        let cli = Cli::try_parse_from(["e120-demo", "cycle", "--every", "9", "--brightness", "40"])
            .unwrap();
        assert_eq!(
            (cli.every, cli.brightness, cli.iface.as_str()),
            (9, 40, "en24")
        );
        assert!(Cli::try_parse_from(["e120-demo"]).is_err());
    }
}
