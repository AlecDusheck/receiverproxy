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
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::time::Duration;

/// BPF records are padded so each starts on a word boundary.
const BPF_ALIGNMENT: usize = 4;

/// Field offsets within `struct bpf_hdr` (Darwin, 32-bit timestamp fields).
/// Decoded by hand rather than by pointer cast: the buffer is only byte
/// aligned, so casting to the struct type would be undefined behaviour.
const HDR_CAPLEN: usize = 8;
const HDR_HDRLEN: usize = 16;
const HDR_MIN_LEN: usize = 18;

#[derive(Debug)]
pub struct Bpf {
    file: File,
    /// One kernel read buffer (BIOCGBLEN bytes), reused across `recv` calls.
    buf: Vec<u8>,
}

impl Bpf {
    /// Open the first free `/dev/bpf*` device and bind it to `iface`.
    ///
    /// The device is put in promiscuous mode: the card's replies are
    /// addressed to the spoofed sender MAC, not to the host's.
    ///
    /// # Errors
    /// Fails if every BPF device is busy, if permissions deny access, or if
    /// the interface does not exist.
    pub fn open(iface: &str, read_timeout: Duration) -> Result<Self> {
        let file = Self::open_device()?;
        let fd = file.as_raw_fd();

        // Darwin ioctl numbers come from libc (<net/bpf.h>).
        let mut ifr: libc::ifreq = unsafe { std::mem::zeroed() };
        if iface.len() >= ifr.ifr_name.len() {
            bail!("interface name too long: {iface}");
        }
        for (slot, b) in ifr.ifr_name.iter_mut().zip(iface.bytes()) {
            *slot = b as libc::c_char;
        }
        unsafe { ioctl(fd, libc::BIOCSETIF, std::ptr::from_mut(&mut ifr).cast()) }
            .with_context(|| format!("BIOCSETIF {iface}"))?;

        // Deliver frames as they arrive rather than waiting for a full buffer.
        let mut on: u32 = 1;
        unsafe { ioctl(fd, libc::BIOCIMMEDIATE, std::ptr::from_mut(&mut on).cast()) }
            .context("BIOCIMMEDIATE")?;

        // We supply the whole Ethernet header ourselves, source MAC included.
        let mut on: u32 = 1;
        unsafe { ioctl(fd, libc::BIOCSHDRCMPLT, std::ptr::from_mut(&mut on).cast()) }
            .context("BIOCSHDRCMPLT")?;

        unsafe { ioctl(fd, libc::c_ulong::from(libc::BIOCPROMISC), std::ptr::null_mut()) }
            .context("BIOCPROMISC")?;

        let mut tv = libc::timeval {
            tv_sec: read_timeout.as_secs() as libc::time_t,
            tv_usec: read_timeout.subsec_micros() as libc::suseconds_t,
        };
        unsafe { ioctl(fd, libc::BIOCSRTIMEOUT, std::ptr::from_mut(&mut tv).cast()) }
            .context("BIOCSRTIMEOUT")?;

        let mut blen: u32 = 0;
        unsafe { ioctl(fd, libc::BIOCGBLEN, std::ptr::from_mut(&mut blen).cast()) }
            .context("BIOCGBLEN")?;

        Ok(Self {
            file,
            buf: vec![0u8; blen as usize],
        })
    }

    fn open_device() -> Result<File> {
        let mut reason = "all busy".to_string();
        for i in 0..256 {
            match OpenOptions::new()
                .read(true)
                .write(true)
                .open(format!("/dev/bpf{i}"))
            {
                Ok(f) => return Ok(f),
                Err(e) if e.raw_os_error() == Some(libc::EBUSY) => {}
                Err(e) => {
                    reason = e.to_string();
                    break;
                }
            }
        }
        bail!("could not open any /dev/bpf* device: {reason} (try: sudo chmod o+rw /dev/bpf*)")
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
    /// The frames borrow the device's buffer and are valid until the next call.
    ///
    /// # Errors
    /// Fails on a read error other than a timeout or interruption.
    pub fn recv(&mut self) -> Result<Records<'_>> {
        let n = match self.file.read(&mut self.buf) {
            Ok(n) => n,
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.raw_os_error() == Some(libc::EINTR) =>
            {
                0
            }
            Err(e) => return Err(e).context("read from bpf"),
        };
        Ok(Records(&self.buf[..n]))
    }
}

/// The individual frames packed into one BPF read buffer.
/// A truncated trailing record ends the walk.
#[derive(Clone, Debug)]
pub struct Records<'a>(pub &'a [u8]);

impl<'a> Iterator for Records<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<&'a [u8]> {
        let buf = self.0;
        if buf.len() < HDR_MIN_LEN {
            return None;
        }
        let caplen = read_u32(buf, HDR_CAPLEN) as usize;
        let hdrlen = u16::from_ne_bytes([buf[HDR_HDRLEN], buf[HDR_HDRLEN + 1]]) as usize;
        let end = hdrlen.checked_add(caplen).filter(|e| *e <= buf.len())?;
        self.0 = buf.get(end.next_multiple_of(BPF_ALIGNMENT)..).unwrap_or(&[]);
        Some(&buf[hdrlen..end])
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// One bpf_hdr record with the given header length, padded to alignment.
    fn record(hdrlen: u16, data: &[u8]) -> Vec<u8> {
        let mut r = vec![0u8; hdrlen as usize];
        r[HDR_CAPLEN..HDR_CAPLEN + 4].copy_from_slice(&(data.len() as u32).to_ne_bytes());
        r[HDR_HDRLEN..HDR_HDRLEN + 2].copy_from_slice(&hdrlen.to_ne_bytes());
        r.extend_from_slice(data);
        while !r.len().is_multiple_of(BPF_ALIGNMENT) {
            r.push(0xEE);
        }
        r
    }

    fn split_records(buf: &[u8]) -> Vec<Vec<u8>> {
        Records(buf).map(<[u8]>::to_vec).collect()
    }

    #[test]
    fn one_record() {
        let buf = record(18, &[1, 2, 3, 4, 5]);
        assert_eq!(split_records(&buf), vec![vec![1, 2, 3, 4, 5]]);
    }

    #[test]
    fn two_records_with_padding_between_them() {
        let mut buf = record(18, &[0xaa; 7]);
        assert_eq!(buf.len(), 28);
        buf.extend(record(18, &[0xbb; 60]));
        assert_eq!(split_records(&buf), vec![vec![0xaa; 7], vec![0xbb; 60]]);
    }

    #[test]
    fn hdrlen_is_taken_from_the_record() {
        let buf = record(24, &[9, 8, 7]);
        assert_eq!(split_records(&buf), vec![vec![9, 8, 7]]);
    }

    #[test]
    fn truncated_tail_is_dropped() {
        let mut buf = record(18, &[1, 2, 3]);
        buf.extend(&record(18, &[4, 5, 6, 7, 8])[..20]);
        assert_eq!(split_records(&buf), vec![vec![1, 2, 3]]);
        assert!(split_records(&buf[..10]).is_empty());
    }

    #[test]
    fn zero_caplen_yields_an_empty_frame_and_keeps_walking() {
        let mut buf = record(18, &[]);
        buf.extend(record(18, &[42]));
        assert_eq!(split_records(&buf), vec![Vec::new(), vec![42]]);
    }

    #[test]
    fn empty_buffer_yields_no_frames() {
        assert!(split_records(&[]).is_empty());
    }
}
