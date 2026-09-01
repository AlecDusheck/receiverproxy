//! Minimal libpcap file reader (classic tcpdump format, big/little endian).

use anyhow::{bail, Context, Result};

pub struct PcapPacket {
    pub ts_sec: u32,
    pub ts_usec: u32,
    pub data: Vec<u8>,
}

pub fn read_pcap(path: &str) -> Result<Vec<PcapPacket>> {
    let d = std::fs::read(path).with_context(|| format!("read {path}"))?;
    if d.len() < 24 {
        bail!("pcap too short");
    }
    let magic = u32::from_le_bytes(d[0..4].try_into().unwrap());
    let (le, nano) = match magic {
        0xa1b2c3d4 => (true, false),
        0xd4c3b2a1 => (false, false),
        0xa1b23c4d => (true, true),
        0x4d3cb2a1 => (false, true),
        m => bail!("not a classic pcap file (magic {m:08x}); if pcapng, convert: tcpdump -r in -w out"),
    };
    let _ = nano;
    let rd32 = |b: &[u8]| -> u32 {
        let a: [u8; 4] = b.try_into().unwrap();
        if le { u32::from_le_bytes(a) } else { u32::from_be_bytes(a) }
    };
    let mut pkts = Vec::new();
    let mut off = 24usize;
    while off + 16 <= d.len() {
        let ts_sec = rd32(&d[off..off + 4]);
        let ts_usec = rd32(&d[off + 4..off + 8]);
        let caplen = rd32(&d[off + 8..off + 12]) as usize;
        off += 16;
        if off + caplen > d.len() {
            break;
        }
        pkts.push(PcapPacket {
            ts_sec,
            ts_usec,
            data: d[off..off + caplen].to_vec(),
        });
        off += caplen;
    }
    Ok(pkts)
}
