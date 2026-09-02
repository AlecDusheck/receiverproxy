use crate::parse_geometry;
use anyhow::Result;
use clap::Subcommand;
use e120_commands::display::{play, show_image, show_pattern, show_solid};
use e120_commands::util::parse_color;
use e120_commands::{ingest, Ctx, Progress};

#[derive(Subcommand)]
pub enum Show {
    /// Display an image file scaled to the panel
    Image {
        /// Any image format the `image` crate reads
        path: String,
        /// Keep refreshing until Ctrl-C
        #[arg(long)]
        hold: bool,
    },
    /// Play a video or any ffmpeg-readable source
    Video {
        /// File path or URL
        input: String,
        /// Frames per second
        #[arg(long, default_value_t = 30)]
        fps: u32,
        /// stretch | contain | cover
        #[arg(long, default_value = "contain")]
        fit: String,
        /// Loop forever
        #[arg(long = "loop", alias = "looping")]
        looping: bool,
        /// Wall layout JSON; defaults to a single panel of --width x --height
        #[arg(long)]
        layout: Option<String>,
    },
    /// Show raw rgb24 frames read from stdin (e.g. ffmpeg -f rawvideo -pix_fmt rgb24 -)
    Stream {
        /// Frame size as WIDTHxHEIGHT [default: the wall size]
        #[arg(long, value_parser = parse_geometry)]
        size: Option<(u16, u16)>,
        /// Frames per second
        #[arg(long, default_value_t = 30)]
        fps: u32,
        /// stretch | contain | cover, when --size differs from the wall
        #[arg(long, default_value = "contain")]
        fit: String,
        /// Wall layout JSON; defaults to a single panel of --width x --height
        #[arg(long)]
        layout: Option<String>,
    },
    /// Accept rgb24 streams on a unix socket, one client at a time
    Serve {
        /// Socket path; a stale file is replaced and the file removed on exit
        #[arg(long)]
        socket: String,
        /// stretch | contain | cover, when a client's size differs from the wall
        #[arg(long, default_value = "contain")]
        fit: String,
        /// Wall layout JSON; defaults to a single panel of --width x --height
        #[arg(long)]
        layout: Option<String>,
    },
    /// Fill the panel with a solid color, e.g. `fill ff0000` or `fill 255 0 0`
    Fill {
        /// RRGGBB hex, or three 0-255 values
        #[arg(required = true)]
        color: Vec<String>,
        /// Keep refreshing until Ctrl-C, so a meter can settle on the draw
        #[arg(long)]
        hold: bool,
    },
    /// Show a built-in pattern through the wall pipeline
    Pattern {
        /// rgb | border | rows | gradient | white
        #[arg(default_value = "rgb")]
        name: String,
        /// Keep refreshing until Ctrl-C
        #[arg(long)]
        hold: bool,
        /// Wall layout JSON; defaults to a single panel of --width x --height
        #[arg(long)]
        layout: Option<String>,
    },
    /// Show a test pattern on a single panel
    Test {
        /// gradient | rows | border | rgb | white
        #[arg(default_value = "gradient")]
        pattern: String,
        /// Keep refreshing until Ctrl-C
        #[arg(long)]
        hold: bool,
    },
    /// Blank the panel
    Blank,
}

pub fn run(ctx: &Ctx, cmd: &Show, p: &mut dyn Progress) -> Result<()> {
    match cmd {
        Show::Image { path, hold } => show_image(ctx, path, *hold, p),
        Show::Video {
            input,
            fps,
            fit,
            looping,
            layout,
        } => play(ctx, input, *fps, fit, *looping, layout.as_deref(), p),
        Show::Stream {
            size,
            fps,
            fit,
            layout,
        } => ingest::stream(ctx, *size, *fps, fit, layout.as_deref(), p),
        Show::Serve {
            socket,
            fit,
            layout,
        } => ingest::serve(ctx, socket, fit, layout.as_deref(), p),
        Show::Fill { color, hold } => show_solid(ctx, parse_color(color)?, *hold, p),
        Show::Pattern { name, hold, layout } => {
            show_pattern(ctx, name, *hold, layout.as_deref(), p)
        }
        Show::Test { pattern, hold } => show_pattern(ctx, pattern, *hold, None, p),
        Show::Blank => show_solid(ctx, [0, 0, 0], false, p),
    }
}
