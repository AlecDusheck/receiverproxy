//! Linux backend of [`Link`]: an `AF_PACKET`/`SOCK_RAW` socket bound to one
//! interface. Needs `CAP_NET_RAW` (and `CAP_NET_ADMIN` for promiscuous mode).

// std has no raw packet sockets; the unsafety is confined to the libc calls.
#![allow(unsafe_code)]

use anyhow::{bail, Context, Result};
use std::ffi::CString;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::time::Duration;

/// Largest frame one `recv` can return; more than any Ethernet MTU.
const RECV_BUF_LEN: usize = 65536;

#[derive(Debug)]
pub struct Link {
    fd: OwnedFd,
    /// Receive buffer, reused across `recv` calls.
    buf: Vec<u8>,
}

impl Link {
    /// Open a raw packet socket bound to `iface`, promiscuous: the card
    /// replies to `SENDER_MAC`, not to the host's own address.
    ///
    /// # Errors
    /// Fails without `CAP_NET_RAW`, or if the interface does not exist.
    pub fn open(iface: &str, read_timeout: Duration) -> Result<Self> {
        let proto = (libc::ETH_P_ALL as u16).to_be();
        let raw =
            unsafe { libc::socket(libc::AF_PACKET, libc::SOCK_RAW, libc::c_int::from(proto)) };
        if raw < 0 {
            let e = std::io::Error::last_os_error();
            if e.raw_os_error() == Some(libc::EPERM) || e.raw_os_error() == Some(libc::EACCES) {
                bail!(
                    "socket(AF_PACKET): {e} (try: sudo setcap cap_net_raw,cap_net_admin+ep \
                     $(command -v rxp), or run as root)"
                );
            }
            return Err(e).context("socket(AF_PACKET)");
        }
        let fd = unsafe { OwnedFd::from_raw_fd(raw) };

        let name = CString::new(iface).with_context(|| format!("interface name: {iface}"))?;
        let ifindex = unsafe { libc::if_nametoindex(name.as_ptr()) };
        if ifindex == 0 {
            bail!("no such interface: {iface}");
        }

        let mut sa: libc::sockaddr_ll = unsafe { std::mem::zeroed() };
        sa.sll_family = libc::AF_PACKET as u16;
        sa.sll_protocol = proto;
        sa.sll_ifindex = ifindex as libc::c_int;
        let rc = unsafe {
            libc::bind(
                fd.as_raw_fd(),
                std::ptr::from_ref(&sa).cast(),
                std::mem::size_of::<libc::sockaddr_ll>() as libc::socklen_t,
            )
        };
        if rc < 0 {
            return Err(std::io::Error::last_os_error()).with_context(|| format!("bind {iface}"));
        }

        let mut mr: libc::packet_mreq = unsafe { std::mem::zeroed() };
        mr.mr_ifindex = ifindex as libc::c_int;
        mr.mr_type = libc::PACKET_MR_PROMISC as u16;
        setsockopt(&fd, libc::SOL_PACKET, libc::PACKET_ADD_MEMBERSHIP, &mr)
            .context("PACKET_ADD_MEMBERSHIP promisc")?;

        // `suseconds_t` is i32 on 32-bit targets, so `From<u32>` would not port.
        #[allow(clippy::cast_lossless)]
        let tv = libc::timeval {
            tv_sec: read_timeout.as_secs() as libc::time_t,
            tv_usec: read_timeout.subsec_micros() as libc::suseconds_t,
        };
        setsockopt(&fd, libc::SOL_SOCKET, libc::SO_RCVTIMEO, &tv).context("SO_RCVTIMEO")?;

        Ok(Self {
            fd,
            buf: vec![0u8; RECV_BUF_LEN],
        })
    }

    /// Send one whole Ethernet frame.
    ///
    /// # Errors
    /// Fails if the kernel rejects or truncates the write.
    pub fn send(&mut self, frame: &[u8]) -> Result<()> {
        let n = unsafe { libc::send(self.fd.as_raw_fd(), frame.as_ptr().cast(), frame.len(), 0) };
        if n < 0 {
            return Err(std::io::Error::last_os_error()).context("send on packet socket");
        }
        if n as usize != frame.len() {
            bail!("short write: {n} of {} bytes", frame.len());
        }
        Ok(())
    }

    /// At most one frame received within the read timeout, borrowed from the
    /// socket buffer until the next call. A timeout or EINTR yields no frames.
    ///
    /// # Errors
    /// Fails on any other receive error.
    pub fn recv(&mut self) -> Result<Frames<'_>> {
        let n = unsafe {
            libc::recv(
                self.fd.as_raw_fd(),
                self.buf.as_mut_ptr().cast(),
                self.buf.len(),
                0,
            )
        };
        if n < 0 {
            let e = std::io::Error::last_os_error();
            return match e.raw_os_error() {
                Some(libc::EAGAIN | libc::EINTR) => Ok(Frames(None)),
                _ => Err(e).context("recv on packet socket"),
            };
        }
        Ok(Frames(Some(&self.buf[..n as usize])))
    }
}

/// The frames from one `recv`: zero or one on a packet socket.
#[derive(Clone, Debug)]
pub struct Frames<'a>(pub Option<&'a [u8]>);

impl<'a> Iterator for Frames<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<&'a [u8]> {
        self.0.take()
    }
}

fn setsockopt<T>(fd: &OwnedFd, level: libc::c_int, name: libc::c_int, value: &T) -> Result<()> {
    let rc = unsafe {
        libc::setsockopt(
            fd.as_raw_fd(),
            level,
            name,
            std::ptr::from_ref(value).cast(),
            std::mem::size_of::<T>() as libc::socklen_t,
        )
    };
    if rc < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_yield_at_most_one_frame() {
        assert!(Frames(None).next().is_none());
        let mut f = Frames(Some(&[1, 2, 3][..]));
        assert_eq!(f.next(), Some(&[1, 2, 3][..]));
        assert!(f.next().is_none());
    }
}
