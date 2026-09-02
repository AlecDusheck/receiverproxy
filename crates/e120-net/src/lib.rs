//! Transport layer: raw Ethernet I/O and capture-file reading.
//!
//! Deliberately protocol-agnostic — it moves opaque Ethernet frames so that
//! the Colorlight protocol itself lives entirely in `e120-proto`.

pub mod bpf;
pub mod pcap;

pub use bpf::Bpf;
pub use pcap::{parse_pcap, read_pcap, PcapPacket};
