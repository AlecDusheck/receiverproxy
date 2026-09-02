//! Raw Ethernet I/O (`/dev/bpf`) and classic pcap reading. Frames are opaque
//! here; the Colorlight protocol lives in `e120-proto`.

mod bpf;
mod pcap;

pub use bpf::{Bpf, Records};
pub use pcap::{parse_pcap, read_pcap, Packets, Pcap, PcapPacket};
