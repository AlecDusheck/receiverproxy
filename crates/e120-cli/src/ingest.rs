//! Frames from other processes: raw rgb24 on stdin (`show stream`) or over a
//! unix socket with a header per client (`show serve`). Both send through the
//! same `Wall`/`Pacer` path as `show video`.

use crate::display::{load_canvas, wall_settings};
use crate::util::warn;
use crate::Cli;
use anyhow::{Context, Result};
use e120_canvas::{Canvas, Frame};
use e120_video::raw::{Header, RawSource};
use e120_video::{Fit, FrameSource};
use std::io;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Set by SIGINT/SIGTERM; `show serve` checks it between reads.
static STOP: AtomicBool = AtomicBool::new(false);

/// How long a socket read blocks before `show serve` looks at `STOP` again.
const READ_TIMEOUT: Duration = Duration::from_millis(250);

/// Show raw rgb24 frames read from stdin, `size` pixels each (the canvas
/// size when `None`), at `fps`.
pub fn stream(
    cli: &Cli,
    size: Option<(u16, u16)>,
    fps: u32,
    fit: &str,
    layout: Option<&str>,
) -> Result<()> {
    let canvas = load_canvas(cli, layout)?;
    let fit: Fit = fit.parse()?;
    let (w, h) = size.map_or((canvas.width, canvas.height), |(w, h)| {
        (u32::from(w), u32::from(h))
    });
    anyhow::ensure!(w > 0 && h > 0, "size must be at least 1x1");

    let mut source = RawSource::new(io::stdin().lock(), w, h);
    let mut wall = e120_driver::Wall::open(&cli.iface, canvas.clone(), wall_settings(cli))?;
    let mut pacer = e120_driver::Pacer::new(fps);
    let mut src = Frame::black(w, h);
    let mut out = Frame::black(canvas.width, canvas.height);
    while source
        .next_frame(&mut src)
        .context("read frame from stdin")?
    {
        wall.show(fitted(&src, fit, &canvas, &mut out))?;
        pacer.wait();
    }
    println!(
        "{} frames, {:.1} fps",
        wall.frames_sent(),
        pacer.achieved_fps()
    );
    Ok(())
}

/// Serve one client at a time on a unix socket at `path`.
///
/// Each client sends a [`Header`] then frames, paced at the header's fps. The
/// panel keeps the last frame between clients. Ctrl-C removes the socket and
/// exits.
pub fn serve(cli: &Cli, path: &str, fit: &str, layout: Option<&str>) -> Result<()> {
    let canvas = load_canvas(cli, layout)?;
    let fit: Fit = fit.parse()?;
    let socket = SocketFile::bind(Path::new(path))?;
    socket.listener.set_nonblocking(true)?;
    install_stop_handler();

    let mut wall = e120_driver::Wall::open(&cli.iface, canvas.clone(), wall_settings(cli))?;
    let mut out = Frame::black(canvas.width, canvas.height);
    eprintln!("listening on {path}");
    while !STOP.load(Ordering::Relaxed) {
        let stream = match socket.listener.accept() {
            Ok((stream, _)) => stream,
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(50));
                continue;
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e).context("accept"),
        };
        // BSD accept() hands out the listener's non-blocking flag.
        stream.set_nonblocking(false)?;
        stream.set_read_timeout(Some(READ_TIMEOUT))?;
        if let Err(e) = serve_client(stream, &mut wall, &canvas, fit, &mut out) {
            warn(format!("client: {e:#}"));
        }
    }
    drop(socket);
    Ok(())
}

/// Show one client's frames until it disconnects or `STOP` is set.
fn serve_client(
    mut stream: UnixStream,
    wall: &mut e120_driver::Wall,
    canvas: &Canvas,
    fit: Fit,
    out: &mut Frame,
) -> Result<()> {
    let header = Header::read(&mut stream).context("read stream header")?;
    eprintln!(
        "client: {}x{} at {} fps",
        header.width, header.height, header.fps
    );
    let (w, h) = (u32::from(header.width), u32::from(header.height));
    let mut source = RawSource::new(stream, w, h);
    let mut pacer = e120_driver::Pacer::new(u32::from(header.fps));
    let mut src = Frame::black(w, h);
    while !STOP.load(Ordering::Relaxed) {
        match source.read_frame(&mut src) {
            Ok(true) => {
                wall.show(fitted(&src, fit, canvas, out))?;
                pacer.wait();
            }
            Ok(false) => break,
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) => {}
            Err(e) => return Err(e).context("read frame"),
        }
    }
    eprintln!("client: gone after {} frames", wall.frames_sent());
    Ok(())
}

/// `src` itself when it is already canvas-sized, else `src` fitted into `out`.
fn fitted<'a>(src: &'a Frame, fit: Fit, canvas: &Canvas, out: &'a mut Frame) -> &'a Frame {
    if (src.width, src.height) == (canvas.width, canvas.height) {
        src
    } else {
        fit_into(src, fit, out);
        out
    }
}

/// Scaled size and placement of a `sw` x `sh` image fitted to `w` x `h`:
/// `(scaled_w, scaled_h, dst_x, dst_y, src_x, src_y)`.
fn fit_geometry(fit: Fit, sw: u32, sh: u32, w: u32, h: u32) -> (u32, u32, u32, u32, u32, u32) {
    let (sw, sh, w, h) = (f64::from(sw), f64::from(sh), f64::from(w), f64::from(h));
    let scale = match fit {
        Fit::Stretch => return (w as u32, h as u32, 0, 0, 0, 0),
        Fit::Contain => (w / sw).min(h / sh),
        Fit::Cover => (w / sw).max(h / sh),
    };
    let rw = (sw * scale).round().max(1.0);
    let rh = (sh * scale).round().max(1.0);
    let pad = |space: f64, size: f64| ((space - size) / 2.0).round().max(0.0) as u32;
    (
        rw as u32,
        rh as u32,
        pad(w, rw),
        pad(h, rh),
        pad(rw, w),
        pad(rh, h),
    )
}

/// Resample `src` into `out` (already the canvas size) honouring `fit`.
/// One allocation per frame for the scaled image; the same-size path in
/// [`fitted`] has none.
fn fit_into(src: &Frame, fit: Fit, out: &mut Frame) {
    use image::{imageops, ImageBuffer, Rgb};
    let (w, h) = (out.width, out.height);
    let (rw, rh, dx, dy, sx, sy) = fit_geometry(fit, src.width, src.height, w, h);
    let Some(img) = ImageBuffer::<Rgb<u8>, &[u8]>::from_raw(src.width, src.height, src.as_bytes())
    else {
        return;
    };
    let scaled = imageops::resize(&img, rw, rh, imageops::FilterType::Triangle);
    out.as_bytes_mut().fill(0);
    let cols = w.saturating_sub(dx).min(rw.saturating_sub(sx)) as usize;
    let rows = h.saturating_sub(dy).min(rh.saturating_sub(sy));
    for y in 0..rows {
        let from = &scaled.as_raw()[((sy + y) * rw + sx) as usize * 3..][..cols * 3];
        out.as_bytes_mut()[((dy + y) * w + dx) as usize * 3..][..cols * 3].copy_from_slice(from);
    }
}

/// A bound listener whose socket file is removed when dropped.
struct SocketFile {
    listener: UnixListener,
    path: PathBuf,
}

impl SocketFile {
    fn bind(path: &Path) -> Result<Self> {
        // A stale file from an earlier run would make bind fail.
        if let Err(e) = std::fs::remove_file(path) {
            if e.kind() != io::ErrorKind::NotFound {
                return Err(e).with_context(|| format!("remove {}", path.display()));
            }
        }
        let listener =
            UnixListener::bind(path).with_context(|| format!("bind {}", path.display()))?;
        Ok(Self {
            listener,
            path: path.to_path_buf(),
        })
    }
}

impl Drop for SocketFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

extern "C" fn on_stop(_sig: libc::c_int) {
    STOP.store(true, Ordering::Relaxed);
}

#[allow(unsafe_code)] // registers a handler that only stores a flag
fn install_stop_handler() {
    unsafe {
        libc::signal(
            libc::SIGINT,
            on_stop as extern "C" fn(libc::c_int) as libc::sighandler_t,
        );
        libc::signal(
            libc::SIGTERM,
            on_stop as extern "C" fn(libc::c_int) as libc::sighandler_t,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use e120_video::{pattern, Pattern};

    #[test]
    fn same_size_frames_pass_through_untouched() {
        let canvas = Canvas::single(8, 4);
        let src = pattern(Pattern::Gradient, 8, 4);
        let mut out = Frame::black(8, 4);
        let shown = fitted(&src, Fit::Contain, &canvas, &mut out);
        assert!(std::ptr::eq(
            std::ptr::from_ref(shown),
            std::ptr::from_ref(&src)
        ));
    }

    #[test]
    fn fit_geometry_pads_or_crops_to_keep_the_aspect() {
        // 2:1 source onto a 4:1 wall.
        assert_eq!(
            fit_geometry(Fit::Stretch, 64, 32, 128, 32),
            (128, 32, 0, 0, 0, 0)
        );
        assert_eq!(
            fit_geometry(Fit::Contain, 64, 32, 128, 32),
            (64, 32, 32, 0, 0, 0)
        );
        assert_eq!(
            fit_geometry(Fit::Cover, 64, 32, 128, 32),
            (128, 64, 0, 0, 0, 16)
        );
    }

    #[test]
    fn contain_letterboxes_a_white_source_with_black_bars() {
        let canvas = Canvas::single(8, 4);
        let src = pattern(Pattern::White, 4, 4);
        let mut out = Frame::black(8, 4);
        let f = fitted(&src, Fit::Contain, &canvas, &mut out);
        assert_eq!(f.pixel(0, 0), [0, 0, 0]);
        assert_eq!(f.pixel(2, 0), [255, 255, 255]);
        assert_eq!(f.pixel(5, 3), [255, 255, 255]);
        assert_eq!(f.pixel(7, 3), [0, 0, 0]);
    }

    /// Microseconds to resample a 1920x1080 source onto a fifty-card
    /// 1280x320 wall. Run with
    /// `cargo test --release -p e120-cli -- --ignored --nocapture`.
    #[test]
    #[ignore = "timing; run in release with --nocapture"]
    fn fit_into_time_for_fifty_cards() {
        const FRAMES: u32 = 100;
        let src = pattern(Pattern::Gradient, 1920, 1080);
        let mut out = Frame::black(1280, 320);
        for fit in [Fit::Contain, Fit::Cover, Fit::Stretch] {
            let t = std::time::Instant::now();
            for _ in 0..FRAMES {
                fit_into(&src, fit, &mut out);
            }
            let us = t.elapsed().as_secs_f64() * 1e6 / f64::from(FRAMES);
            println!("fit_into {fit:?} 1920x1080 -> 1280x320: {us:.0} us/frame");
        }
        std::hint::black_box(&out);
    }

    #[test]
    fn cover_and_stretch_fill_the_whole_wall() {
        let canvas = Canvas::single(8, 4);
        let src = pattern(Pattern::White, 4, 4);
        let mut out = Frame::black(8, 4);
        for fit in [Fit::Cover, Fit::Stretch] {
            let f = fitted(&src, fit, &canvas, &mut out);
            assert!(f.as_bytes().iter().all(|&b| b == 255), "{fit:?}");
        }
    }
}
