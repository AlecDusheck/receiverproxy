//! Transport layer: raw Ethernet I/O and capture-file reading.
//!
//! Deliberately protocol-agnostic — it moves opaque Ethernet frames so that
//! the Colorlight protocol itself lives entirely in `e120-proto`.

mod bpf;
mod pcap;

pub use bpf::{Bpf, Records};
pub use pcap::{parse_pcap, read_pcap, Packets, Pcap, PcapPacket};
