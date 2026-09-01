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
    let magic = le_u32(&d, 0)?;
    let (le, nano) = match magic {
        0xa1b2_c3d4 => (true, false),
        0xd4c3_b2a1 => (false, false),
        0xa1b2_3c4d => (true, true),
        0x4d3c_b2a1 => (false, true),
        m => bail!(
            "not a classic pcap file (magic {m:08x}); if pcapng, convert: tcpdump -r in -w out"
        ),
    };
    let _ = nano;
    let rd32 = |d: &[u8], off: usize| -> Result<u32> {
        let a: [u8; 4] = d
            .get(off..off + 4)
            .context("truncated packet header")?
            .try_into()?;
        Ok(if le {
            u32::from_le_bytes(a)
        } else {
            u32::from_be_bytes(a)
        })
    };
    let mut pkts = Vec::new();
    let mut off = 24usize;
    while off + 16 <= d.len() {
        let ts_sec = rd32(&d, off)?;
        let ts_usec = rd32(&d, off + 4)?;
        let caplen = rd32(&d, off + 8)? as usize;
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

/// Read a little-endian u32 without risking a panic on a short buffer.
fn le_u32(d: &[u8], off: usize) -> Result<u32> {
    let b: [u8; 4] = d
        .get(off..off + 4)
        .context("truncated pcap header")?
        .try_into()?;
    Ok(u32::from_le_bytes(b))
}
