//! Minimal libpcap file reader (classic tcpdump format, big/little endian).

use anyhow::{bail, Context, Result};

#[derive(Debug)]
pub struct PcapPacket {
    pub ts_sec: u32,
    /// Microseconds, also for nanosecond-resolution files.
    pub ts_usec: u32,
    /// The raw Ethernet frame, starting at the destination MAC.
    pub data: Vec<u8>,
}

const GLOBAL_HDR_LEN: usize = 24;
const RECORD_HDR_LEN: usize = 16;

/// Read every packet of a classic pcap file. pcapng is rejected.
///
/// # Errors
/// Fails if the file cannot be read or is not a classic pcap file.
pub fn read_pcap(path: &str) -> Result<Vec<PcapPacket>> {
    let d = std::fs::read(path).with_context(|| format!("read {path}"))?;
    parse_pcap(&d)
}

/// Parse the bytes of a classic pcap file. A truncated last record is dropped.
///
/// # Errors
/// Fails if the global header is short or the magic is not classic pcap.
pub fn parse_pcap(d: &[u8]) -> Result<Vec<PcapPacket>> {
    if d.len() < GLOBAL_HDR_LEN {
        bail!("pcap too short");
    }
    let (le, nano) = match u32_at(d, 0, true)? {
        0xa1b2_c3d4 => (true, false),
        0xd4c3_b2a1 => (false, false),
        0xa1b2_3c4d => (true, true),
        0x4d3c_b2a1 => (false, true),
        m => bail!(
            "not a classic pcap file (magic {m:08x}); if pcapng, convert: tcpdump -r in -w out"
        ),
    };
    let mut pkts = Vec::new();
    let mut off = GLOBAL_HDR_LEN;
    while off + RECORD_HDR_LEN <= d.len() {
        let ts_sec = u32_at(d, off, le)?;
        let mut ts_usec = u32_at(d, off + 4, le)?;
        let caplen = u32_at(d, off + 8, le)? as usize;
        off += RECORD_HDR_LEN;
        if off + caplen > d.len() {
            break;
        }
        if nano {
            ts_usec /= 1000;
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

fn u32_at(d: &[u8], off: usize, le: bool) -> Result<u32> {
    let b: [u8; 4] = d
        .get(off..off + 4)
        .context("truncated pcap header")?
        .try_into()?;
    Ok(if le {
        u32::from_le_bytes(b)
    } else {
        u32::from_be_bytes(b)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(magic: u32, le: bool, records: &[(u32, u32, &[u8])]) -> Vec<u8> {
        let w = |v: u32| if le { v.to_le_bytes() } else { v.to_be_bytes() };
        let mut d = Vec::new();
        d.extend_from_slice(&magic.to_le_bytes());
        d.extend_from_slice(&[0u8; 20]);
        for (sec, frac, data) in records {
            d.extend_from_slice(&w(*sec));
            d.extend_from_slice(&w(*frac));
            d.extend_from_slice(&w(data.len() as u32));
            d.extend_from_slice(&w(data.len() as u32));
            d.extend_from_slice(data);
        }
        d
    }

    #[test]
    fn little_endian_records_come_back_in_order() {
        let d = file(0xa1b2_c3d4, true, &[(1, 2, &[0xaa, 0xbb]), (3, 4, &[0xcc])]);
        let p = parse_pcap(&d).unwrap();
        assert_eq!(p.len(), 2);
        assert_eq!((p[0].ts_sec, p[0].ts_usec), (1, 2));
        assert_eq!(p[0].data, [0xaa, 0xbb]);
        assert_eq!((p[1].ts_sec, p[1].ts_usec), (3, 4));
        assert_eq!(p[1].data, [0xcc]);
    }

    #[test]
    fn big_endian_magic_selects_big_endian_headers() {
        let d = file(0xd4c3_b2a1, false, &[(0x0102_0304, 7, &[1, 2, 3])]);
        let p = parse_pcap(&d).unwrap();
        assert_eq!((p[0].ts_sec, p[0].ts_usec), (0x0102_0304, 7));
        assert_eq!(p[0].data, [1, 2, 3]);
    }

    #[test]
    fn nanosecond_files_report_microseconds() {
        let d = file(0xa1b2_3c4d, true, &[(9, 123_456_789, &[0])]);
        let p = parse_pcap(&d).unwrap();
        assert_eq!(p[0].ts_usec, 123_456);
        let d = file(0x4d3c_b2a1, false, &[(9, 5_000, &[0])]);
        assert_eq!(parse_pcap(&d).unwrap()[0].ts_usec, 5);
    }

    #[test]
    fn truncated_last_record_is_dropped() {
        let mut d = file(0xa1b2_c3d4, true, &[(1, 1, &[1, 2, 3, 4])]);
        d.pop();
        assert!(parse_pcap(&d).unwrap().is_empty());
    }

    #[test]
    fn pcapng_and_short_files_are_rejected() {
        let mut d = vec![0u8; 24];
        d[..4].copy_from_slice(&0x0a0d_0d0au32.to_le_bytes());
        assert!(parse_pcap(&d).unwrap_err().to_string().contains("tcpdump"));
        assert!(parse_pcap(&d[..10]).is_err());
    }
}
