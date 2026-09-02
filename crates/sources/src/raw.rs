//! Raw rgb24 frame ingest from any `Read`: stdin fed by `ffmpeg -f rawvideo`,
//! a socket, a pipe.
//!
//! A stream is consecutive frames of `width * height * 3` bytes and nothing
//! else. Socket clients prefix one [`Header`] with size and rate; stdin
//! streams carry no header and take the size from the command line.

use wall::Frame;
use std::io::{self, Read, Write};

/// Reads consecutive rgb24 frames of one size from `r`.
pub struct RawSource<R> {
    r: R,
    width: u32,
    height: u32,
    /// Bytes of the current frame already in the caller's buffer, so a read
    /// that stops early (a timeout) resumes instead of losing the frame.
    filled: usize,
}

impl<R: Read> RawSource<R> {
    pub fn new(r: R, width: u32, height: u32) -> Self {
        Self {
            r,
            width,
            height,
            filled: 0,
        }
    }

    /// Fill `frame` with the next image, resizing `frame` to the stream's
    /// size if it differs. `Ok(false)` at end of stream; a short final
    /// frame is dropped.
    ///
    /// # Errors
    /// Any read error other than an interrupt. On a timeout the bytes read
    /// so far stay in `frame` and the next call with the same `frame`
    /// continues the frame.
    pub fn read_frame(&mut self, frame: &mut Frame) -> io::Result<bool> {
        if (frame.width, frame.height) != (self.width, self.height) {
            *frame = Frame::black(self.width, self.height);
            self.filled = 0;
        }
        let buf = frame.as_bytes_mut();
        while self.filled < buf.len() {
            match self.r.read(&mut buf[self.filled..]) {
                Ok(0) => {
                    self.filled = 0;
                    return Ok(false);
                }
                Ok(n) => self.filled += n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
        self.filled = 0;
        Ok(true)
    }
}

impl<R: Read> crate::FrameSource for RawSource<R> {
    fn next_frame(&mut self, frame: &mut Frame) -> anyhow::Result<bool> {
        self.read_frame(frame).map_err(Into::into)
    }
}

/// The 12-byte header a socket client sends before its first frame.
///
/// `E120`, version 1, one reserved byte, then width, height and fps as
/// little-endian u16.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    pub width: u16,
    pub height: u16,
    pub fps: u16,
}

impl Header {
    pub const LEN: usize = 12;
    const MAGIC: [u8; 4] = *b"E120";
    const VERSION: u8 = 1;

    #[must_use]
    pub fn to_bytes(self) -> [u8; Self::LEN] {
        let mut b = [0u8; Self::LEN];
        b[..4].copy_from_slice(&Self::MAGIC);
        b[4] = Self::VERSION;
        b[6..8].copy_from_slice(&self.width.to_le_bytes());
        b[8..10].copy_from_slice(&self.height.to_le_bytes());
        b[10..12].copy_from_slice(&self.fps.to_le_bytes());
        b
    }

    /// # Errors
    /// `InvalidData` on a wrong magic or version, `InvalidInput` on a zero size.
    pub fn from_bytes(b: &[u8; Self::LEN]) -> io::Result<Self> {
        if b[..4] != Self::MAGIC {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "not an E120 stream header",
            ));
        }
        if b[4] != Self::VERSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("stream header version {} (want {})", b[4], Self::VERSION),
            ));
        }
        let h = Self {
            width: u16::from_le_bytes([b[6], b[7]]),
            height: u16::from_le_bytes([b[8], b[9]]),
            fps: u16::from_le_bytes([b[10], b[11]]),
        };
        if h.width == 0 || h.height == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "zero-sized stream",
            ));
        }
        Ok(h)
    }

    /// # Errors
    /// The write error.
    pub fn write(self, w: &mut impl Write) -> io::Result<()> {
        w.write_all(&self.to_bytes())
    }

    /// # Errors
    /// The read error, or the errors of [`from_bytes`](Self::from_bytes).
    pub fn read(r: &mut impl Read) -> io::Result<Self> {
        let mut b = [0u8; Self::LEN];
        r.read_exact(&mut b)?;
        Self::from_bytes(&b)
    }

    /// Bytes per frame.
    #[must_use]
    pub fn frame_len(self) -> usize {
        usize::from(self.width) * usize::from(self.height) * 3
    }
}

/// The client side of `e120 show serve`: writes a [`Header`], then frames.
///
/// ```no_run
/// use std::os::unix::net::UnixStream;
/// let stream = UnixStream::connect("/tmp/e120.sock")?;
/// let mut writer = sources::raw::Writer::new(stream, 128, 64, 30)?;
/// let rgb = vec![0u8; 128 * 64 * 3];
/// writer.frame(&rgb)?;
/// # Ok::<(), std::io::Error>(())
/// ```
pub struct Writer<W> {
    w: W,
    frame_len: usize,
}

impl<W: Write> Writer<W> {
    /// Send the header for `width` x `height` frames at `fps`.
    ///
    /// # Errors
    /// The write error.
    pub fn new(mut w: W, width: u16, height: u16, fps: u16) -> io::Result<Self> {
        let header = Header { width, height, fps };
        header.write(&mut w)?;
        w.flush()?;
        Ok(Self {
            w,
            frame_len: header.frame_len(),
        })
    }

    /// Send one rgb24 frame; `rgb` must be exactly `width * height * 3` bytes.
    ///
    /// # Errors
    /// `InvalidInput` on a wrong length, else the write error.
    pub fn frame(&mut self, rgb: &[u8]) -> io::Result<()> {
        if rgb.len() != self.frame_len {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("frame is {} bytes, want {}", rgb.len(), self.frame_len),
            ));
        }
        self.w.write_all(rgb)?;
        self.w.flush()
    }

    pub fn into_inner(self) -> W {
        self.w
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{pattern, Pattern};
    use std::io::Cursor;

    #[test]
    fn frames_are_read_back_to_back_and_a_short_tail_is_dropped() {
        let (w, h) = (4, 2);
        let a = pattern(Pattern::Rgb, w, h);
        let b = pattern(Pattern::Gradient, w, h);
        let c = pattern(Pattern::White, w, h);
        let mut bytes = a.as_bytes().to_vec();
        bytes.extend_from_slice(b.as_bytes());
        bytes.extend_from_slice(c.as_bytes());
        bytes.extend_from_slice(&[7; 5]);
        let mut src = RawSource::new(Cursor::new(bytes), w, h);
        // Handed a frame of the wrong size, the source resizes it.
        let mut f = Frame::black(1, 1);
        for want in [&a, &b, &c] {
            assert!(src.read_frame(&mut f).unwrap());
            assert_eq!(&f, want);
        }
        assert!(!src.read_frame(&mut f).unwrap());
        assert!(!src.read_frame(&mut f).unwrap());
    }

    /// A reader that hands out one byte per call, then times out once.
    struct Dribble {
        data: Vec<u8>,
        at: usize,
        timeouts: usize,
    }

    impl Read for Dribble {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.at == 3 && self.timeouts > 0 {
                self.timeouts -= 1;
                return Err(io::ErrorKind::TimedOut.into());
            }
            if self.at == self.data.len() || buf.is_empty() {
                return Ok(0);
            }
            buf[0] = self.data[self.at];
            self.at += 1;
            Ok(1)
        }
    }

    #[test]
    fn a_timeout_mid_frame_resumes_on_the_next_call() {
        let a = pattern(Pattern::Gradient, 2, 2);
        let mut src = RawSource::new(
            Dribble {
                data: a.as_bytes().to_vec(),
                at: 0,
                timeouts: 1,
            },
            2,
            2,
        );
        let mut f = Frame::black(2, 2);
        assert_eq!(
            src.read_frame(&mut f).unwrap_err().kind(),
            io::ErrorKind::TimedOut
        );
        assert!(src.read_frame(&mut f).unwrap());
        assert_eq!(f, a);
    }

    #[test]
    fn header_round_trips_and_rejects_strangers() {
        let h = Header {
            width: 128,
            height: 64,
            fps: 30,
        };
        let b = h.to_bytes();
        assert_eq!(&b, b"E120\x01\x00\x80\x00\x40\x00\x1e\x00");
        assert_eq!(Header::read(&mut Cursor::new(b)).unwrap(), h);
        assert_eq!(h.frame_len(), 128 * 64 * 3);

        let mut bad = b;
        bad[0] = b'X';
        assert_eq!(
            Header::from_bytes(&bad).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        let mut v2 = b;
        v2[4] = 2;
        assert!(Header::from_bytes(&v2).is_err());
        let mut zero = b;
        zero[6..8].fill(0);
        assert_eq!(
            Header::from_bytes(&zero).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        assert!(Header::read(&mut Cursor::new(&b[..5])).is_err());
    }

    #[test]
    fn writer_output_reads_back_through_a_raw_source() {
        let (w, h) = (6, 3);
        let a = pattern(Pattern::Border, w, h);
        let b = pattern(Pattern::Rows, w, h);
        let mut writer = Writer::new(Vec::new(), w as u16, h as u16, 25).unwrap();
        writer.frame(a.as_bytes()).unwrap();
        writer.frame(b.as_bytes()).unwrap();
        assert_eq!(
            writer.frame(&[0; 4]).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        let bytes = writer.into_inner();
        assert_eq!(bytes.len(), Header::LEN + 2 * (w * h * 3) as usize);

        let mut r = Cursor::new(bytes);
        let header = Header::read(&mut r).unwrap();
        assert_eq!((header.width, header.height, header.fps), (6, 3, 25));
        let mut src = RawSource::new(r, u32::from(header.width), u32::from(header.height));
        let mut f = Frame::black(w, h);
        assert!(src.read_frame(&mut f).unwrap());
        assert_eq!(f, a);
        assert!(src.read_frame(&mut f).unwrap());
        assert_eq!(f, b);
        assert!(!src.read_frame(&mut f).unwrap());
    }
}
