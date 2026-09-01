//! Raw Ethernet I/O on macOS via /dev/bpf.
//!
//! The Colorlight protocol is pure layer-2 (no IP), so we need to write raw
//! Ethernet frames. On Darwin that means a BPF device bound to the interface.
//! Requires root (or read/write access to /dev/bpf*).

use anyhow::{bail, Context, Result};
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::os::fd::AsRawFd;

// Darwin ioctl numbers (from <net/bpf.h>, 64-bit)
const BIOCSETIF: libc::c_ulong = 0x8020426c; // _IOW('B', 108, struct ifreq)
const BIOCIMMEDIATE: libc::c_ulong = 0x80044270; // _IOW('B', 112, u_int)
const BIOCSHDRCMPLT: libc::c_ulong = 0x80044275; // _IOW('B', 117, u_int)
const BIOCGBLEN: libc::c_ulong = 0x40044266; // _IOR('B', 102, u_int)
const BIOCPROMISC: libc::c_ulong = 0x20004269; // _IO('B', 105)
const BIOCSRTIMEOUT: libc::c_ulong = 0x8010426d; // _IOW('B', 109, struct timeval)

const BPF_ALIGNMENT: usize = 4;

#[repr(C)]
struct BpfHdr {
    tv_sec: u32,
    tv_usec: u32,
    bh_caplen: u32,
    bh_datalen: u32,
    bh_hdrlen: u16,
}

pub struct Bpf {
    file: std::fs::File,
    buf_len: usize,
}

impl Bpf {
    /// Open the first free /dev/bpf* device and bind it to `iface`.
    pub fn open(iface: &str, promisc: bool, read_timeout_ms: u64) -> Result<Self> {
        let mut file = None;
        let mut last_err = None;
        for i in 0..256 {
            match OpenOptions::new()
                .read(true)
                .write(true)
                .open(format!("/dev/bpf{i}"))
            {
                Ok(f) => {
                    file = Some(f);
                    break;
                }
                Err(e) if e.raw_os_error() == Some(libc::EBUSY) => continue,
                Err(e) => {
                    last_err = Some(e);
                    break;
                }
            }
        }
        let file = match file {
            Some(f) => f,
            None => bail!(
                "could not open any /dev/bpf* device: {} (are you running with sudo?)",
                last_err
                    .map(|e| e.to_string())
                    .unwrap_or_else(|| "all busy".into())
            ),
        };
        let fd = file.as_raw_fd();

        unsafe {
            // Bind to interface
            let mut ifr: libc::ifreq = std::mem::zeroed();
            if iface.len() >= std::mem::size_of_val(&ifr.ifr_name) {
                bail!("interface name too long: {iface}");
            }
            for (i, b) in iface.bytes().enumerate() {
                ifr.ifr_name[i] = b as libc::c_char;
            }
            ioctl(fd, BIOCSETIF, &mut ifr as *mut _ as *mut libc::c_void)
                .with_context(|| format!("BIOCSETIF {iface}"))?;

            // Deliver packets as they arrive rather than buffering
            let mut on: u32 = 1;
            ioctl(fd, BIOCIMMEDIATE, &mut on as *mut _ as *mut libc::c_void)
                .context("BIOCIMMEDIATE")?;

            // We build the full Ethernet header ourselves (incl. source MAC)
            let mut on: u32 = 1;
            ioctl(fd, BIOCSHDRCMPLT, &mut on as *mut _ as *mut libc::c_void)
                .context("BIOCSHDRCMPLT")?;

            if promisc {
                ioctl(fd, BIOCPROMISC, std::ptr::null_mut()).context("BIOCPROMISC")?;
            }

            let mut tv = libc::timeval {
                tv_sec: (read_timeout_ms / 1000) as i64,
                tv_usec: ((read_timeout_ms % 1000) * 1000) as i32,
            };
            ioctl(fd, BIOCSRTIMEOUT, &mut tv as *mut _ as *mut libc::c_void)
                .context("BIOCSRTIMEOUT")?;

            let mut blen: u32 = 0;
            ioctl(fd, BIOCGBLEN, &mut blen as *mut _ as *mut libc::c_void).context("BIOCGBLEN")?;

            Ok(Self {
                file,
                buf_len: blen as usize,
            })
        }
    }

    /// Send one raw Ethernet frame (dst mac + src mac + ethertype + payload).
    pub fn send(&mut self, frame: &[u8]) -> Result<()> {
        let n = self.file.write(frame).context("write to bpf")?;
        if n != frame.len() {
            bail!("short write: {n} of {} bytes", frame.len());
        }
        Ok(())
    }

    /// Read all frames currently available (or until the read timeout expires).
    /// Returns raw Ethernet frames.
    pub fn recv(&mut self) -> Result<Vec<Vec<u8>>> {
        let mut buf = vec![0u8; self.buf_len];
        let n = match self.file.read(&mut buf) {
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => 0,
            Err(e) if e.raw_os_error() == Some(libc::EINTR) => 0,
            Err(e) => return Err(e).context("read from bpf"),
        };
        let mut frames = Vec::new();
        let mut off = 0usize;
        while off + std::mem::size_of::<BpfHdr>() <= n {
            let hdr: &BpfHdr = unsafe { &*(buf[off..].as_ptr() as *const BpfHdr) };
            let start = off + hdr.bh_hdrlen as usize;
            let end = start + hdr.bh_caplen as usize;
            if end > n {
                break;
            }
            frames.push(buf[start..end].to_vec());
            // Advance to next word-aligned record
            let rec = hdr.bh_hdrlen as usize + hdr.bh_caplen as usize;
            off += (rec + BPF_ALIGNMENT - 1) & !(BPF_ALIGNMENT - 1);
        }
        Ok(frames)
    }
}

unsafe fn ioctl(fd: libc::c_int, req: libc::c_ulong, arg: *mut libc::c_void) -> Result<()> {
    if libc::ioctl(fd, req, arg) < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}
