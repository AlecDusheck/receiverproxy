//! Raw Ethernet I/O and classic pcap reading. Frames are opaque here; the
//! Colorlight protocol lives in `e120-proto`.
//!
//! [`Link`] is one interface opened for whole-frame send/receive: `/dev/bpf`
//! on macOS, an `AF_PACKET` socket on Linux. Same API on both; `recv` returns
//! within the read timeout with whatever arrived, possibly nothing.

#[cfg(target_os = "macos")]
mod bpf;
#[cfg(target_os = "macos")]
pub use bpf::{Frames, Link};

#[cfg(target_os = "linux")]
mod packet;
#[cfg(target_os = "linux")]
pub use packet::{Frames, Link};

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
compile_error!("e120-net supports macOS (/dev/bpf) and Linux (AF_PACKET) only");

mod pcap;
pub use pcap::{parse_pcap, read_pcap, Packets, Pcap, PcapPacket};
