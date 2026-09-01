//! Raw Ethernet I/O on macOS via `/dev/bpf`.
//!
//! The Colorlight protocol is pure layer 2 — no IP — so frames have to be
//! written whole, including their Ethernet header. On Darwin that means a BPF
//! device bound to an interface. Requires read/write access to `/dev/bpf*`
//! (root, or `chmod o+rw`).

// Talking to a character device through ioctl has no safe abstraction in std;
// the unsafety is confined to `ioctl` and the header decode below.
#![allow(unsafe_code)]

use anyhow::{bail, Context, Result};
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::os::fd::AsRawFd;

// Darwin ioctl numbers, from <net/bpf.h>.
const BIOCSETIF: libc::c_ulong = 0x8020_426c; // _IOW('B', 108, struct ifreq)
const BIOCIMMEDIATE: libc::c_ulong = 0x8004_4270; // _IOW('B', 112, u_int)
const BIOCSHDRCMPLT: libc::c_ulong = 0x8004_4275; // _IOW('B', 117, u_int)
const BIOCGBLEN: libc::c_ulong = 0x4004_4266; // _IOR('B', 102, u_int)
const BIOCPROMISC: libc::c_ulong = 0x2000_4269; // _IO('B', 105)
const BIOCSRTIMEOUT: libc::c_ulong = 0x8010_426d; // _IOW('B', 109, struct timeval)

/// BPF records are padded so each starts on a word boundary.
const BPF_ALIGNMENT: usize = 4;

/// Field offsets within `struct bpf_hdr` (Darwin, 32-bit timestamp fields).
/// Decoded by hand rather than by pointer cast: the buffer is only byte
/// aligned, so casting to the struct type would be undefined behaviour.
const HDR_CAPLEN: usize = 8;
const HDR_HDRLEN: usize = 16;
const HDR_MIN_LEN: usize = 18;

pub struct Bpf {
    file: std::fs::File,
    buf_len: usize,
}

impl Bpf {
    /// Open the first free `/dev/bpf*` device and bind it to `iface`.
    ///
    /// # Errors
    /// Fails if every BPF device is busy, if permissions deny access, or if
    /// the interface does not exist.
    pub fn open(iface: &str, promisc: bool, read_timeout_ms: u64) -> Result<Self> {
        let (file, last_err) = Self::open_device();
        let Some(file) = file else {
            bail!(
                "could not open any /dev/bpf* device: {} (try: sudo chmod o+rw /dev/bpf*)",
                last_err.map_or_else(|| "all busy".to_string(), |e| e.to_string())
            );
        };
        let fd = file.as_raw_fd();

        let mut ifr: libc::ifreq = unsafe { std::mem::zeroed() };
        if iface.len() >= ifr.ifr_name.len() {
            bail!("interface name too long: {iface}");
        }
        for (slot, b) in ifr.ifr_name.iter_mut().zip(iface.bytes()) {
            *slot = b as libc::c_char;
        }
        unsafe { ioctl(fd, BIOCSETIF, std::ptr::from_mut(&mut ifr).cast()) }
            .with_context(|| format!("BIOCSETIF {iface}"))?;

        // Deliver frames as they arrive rather than waiting for a full buffer.
        let mut on: u32 = 1;
        unsafe { ioctl(fd, BIOCIMMEDIATE, std::ptr::from_mut(&mut on).cast()) }
            .context("BIOCIMMEDIATE")?;

        // We supply the whole Ethernet header ourselves, source MAC included.
        let mut on: u32 = 1;
        unsafe { ioctl(fd, BIOCSHDRCMPLT, std::ptr::from_mut(&mut on).cast()) }
            .context("BIOCSHDRCMPLT")?;

        if promisc {
            unsafe { ioctl(fd, BIOCPROMISC, std::ptr::null_mut()) }.context("BIOCPROMISC")?;
        }

        let mut tv = libc::timeval {
            tv_sec: (read_timeout_ms / 1000) as libc::time_t,
            tv_usec: ((read_timeout_ms % 1000) * 1000) as libc::suseconds_t,
        };
        unsafe { ioctl(fd, BIOCSRTIMEOUT, std::ptr::from_mut(&mut tv).cast()) }
            .context("BIOCSRTIMEOUT")?;

        let mut blen: u32 = 0;
        unsafe { ioctl(fd, BIOCGBLEN, std::ptr::from_mut(&mut blen).cast()) }
            .context("BIOCGBLEN")?;

        Ok(Self {
            file,
            buf_len: blen as usize,
        })
    }

    fn open_device() -> (Option<std::fs::File>, Option<std::io::Error>) {
        let mut last_err = None;
        for i in 0..256 {
            match OpenOptions::new()
                .read(true)
                .write(true)
                .open(format!("/dev/bpf{i}"))
            {
                Ok(f) => return (Some(f), None),
                Err(e) if e.raw_os_error() == Some(libc::EBUSY) => {}
                Err(e) => {
                    last_err = Some(e);
                    break;
                }
            }
        }
        (None, last_err)
    }

    /// Send one raw Ethernet frame: destination MAC, source MAC, type, payload.
    ///
    /// # Errors
    /// Fails if the write is rejected or truncated by the kernel.
    pub fn send(&mut self, frame: &[u8]) -> Result<()> {
        let n = self.file.write(frame).context("write to bpf")?;
        if n != frame.len() {
            bail!("short write: {n} of {} bytes", frame.len());
        }
        Ok(())
    }

    /// Read whatever frames are available, up to the configured read timeout.
    ///
    /// # Errors
    /// Fails on a read error other than a timeout or interruption.
    pub fn recv(&mut self) -> Result<Vec<Vec<u8>>> {
        let mut buf = vec![0u8; self.buf_len];
        let n = match self.file.read(&mut buf) {
            Ok(n) => n,
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.raw_os_error() == Some(libc::EINTR) =>
            {
                0
            }
            Err(e) => return Err(e).context("read from bpf"),
        };
        Ok(split_records(&buf[..n]))
    }
}

/// Split a BPF read buffer into the individual frames it packs together.
fn split_records(buf: &[u8]) -> Vec<Vec<u8>> {
    let mut frames = Vec::new();
    let mut off = 0usize;
    while off + HDR_MIN_LEN <= buf.len() {
        let caplen = read_u32(buf, off + HDR_CAPLEN) as usize;
        let hdrlen =
            u16::from_ne_bytes([buf[off + HDR_HDRLEN], buf[off + HDR_HDRLEN + 1]]) as usize;
        let start = off + hdrlen;
        let Some(end) = start.checked_add(caplen).filter(|e| *e <= buf.len()) else {
            break;
        };
        frames.push(buf[start..end].to_vec());
        off += (hdrlen + caplen + BPF_ALIGNMENT - 1) & !(BPF_ALIGNMENT - 1);
    }
    frames
}

fn read_u32(buf: &[u8], off: usize) -> u32 {
    u32::from_ne_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]])
}

/// # Safety
/// `arg` must be valid for the ioctl request being issued.
unsafe fn ioctl(fd: libc::c_int, req: libc::c_ulong, arg: *mut libc::c_void) -> Result<()> {
    if libc::ioctl(fd, req, arg) < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}
