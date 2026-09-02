//! Minimal libpcap file reader (classic tcpdump format, big/little endian).

use anyhow::{bail, Context, Result};
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PcapPacket<'a> {
    pub ts_sec: u32,
    /// Microseconds, also for nanosecond-resolution files.
    pub ts_usec: u32,
    /// The raw Ethernet frame, starting at the destination MAC.
    pub data: &'a [u8],
}

const GLOBAL_HDR_LEN: usize = 24;
const RECORD_HDR_LEN: usize = 16;

/// A classic pcap file held in memory; `packets` walks it without copying.
#[derive(Clone, Debug)]
pub struct Pcap {
    bytes: Vec<u8>,
    header: Header,
}

impl Pcap {
    /// # Errors
    /// Fails if the global header is short or the magic is not classic pcap.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self> {
        let header = Header::parse(&bytes)?;
        Ok(Self { bytes, header })
    }

    pub fn packets(&self) -> Packets<'_> {
        self.header.packets(&self.bytes)
    }
}

/// Read a classic pcap file. pcapng is rejected.
///
/// # Errors
/// Fails if the file cannot be read or is not a classic pcap file.
pub fn read_pcap(path: impl AsRef<Path>) -> Result<Pcap> {
    let path = path.as_ref();
    let bytes = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
    Pcap::from_bytes(bytes)
}

/// Walk the packets of a classic pcap file. A truncated last record is dropped.
///
/// # Errors
/// Fails if the global header is short or the magic is not classic pcap.
pub fn parse_pcap(d: &[u8]) -> Result<Packets<'_>> {
    Ok(Header::parse(d)?.packets(d))
}

/// What the magic number says about the record headers.
#[derive(Clone, Copy, Debug)]
struct Header {
    read_u32: fn([u8; 4]) -> u32,
    nano: bool,
}

impl Header {
    fn parse(d: &[u8]) -> Result<Self> {
        if d.len() < GLOBAL_HDR_LEN {
            bail!("pcap too short");
        }
        let (le, nano) = match u32::from_le_bytes(bytes_at(d, 0)) {
            0xa1b2_c3d4 => (true, false),
            0xd4c3_b2a1 => (false, false),
            0xa1b2_3c4d => (true, true),
            0x4d3c_b2a1 => (false, true),
            m => bail!(
                "not a classic pcap file (magic {m:08x}); if pcapng, convert: tcpdump -r in -w out"
            ),
        };
        let read_u32 = if le {
            u32::from_le_bytes
        } else {
            u32::from_be_bytes
        };
        Ok(Self { read_u32, nano })
    }

    fn packets(self, d: &[u8]) -> Packets<'_> {
        Packets {
            header: self,
            rest: &d[GLOBAL_HDR_LEN..],
        }
    }
}

/// Iterator over the packets of a pcap file, borrowing the file bytes.
#[derive(Clone, Debug)]
pub struct Packets<'a> {
    header: Header,
    rest: &'a [u8],
}

impl<'a> Iterator for Packets<'a> {
    type Item = PcapPacket<'a>;

    fn next(&mut self) -> Option<PcapPacket<'a>> {
        let d = self.rest;
        if d.len() < RECORD_HDR_LEN {
            return None;
        }
        let rd = self.header.read_u32;
        let ts_sec = rd(bytes_at(d, 0));
        let mut ts_usec = rd(bytes_at(d, 4));
        let caplen = rd(bytes_at(d, 8)) as usize;
        let data = d[RECORD_HDR_LEN..].get(..caplen)?;
        if self.header.nano {
            ts_usec /= 1000;
        }
        self.rest = &d[RECORD_HDR_LEN + caplen..];
        Some(PcapPacket {
            ts_sec,
            ts_usec,
            data,
        })
    }
}

fn bytes_at(d: &[u8], off: usize) -> [u8; 4] {
    [d[off], d[off + 1], d[off + 2], d[off + 3]]
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

    fn packets(d: &[u8]) -> Result<Vec<PcapPacket<'_>>> {
        Ok(parse_pcap(d)?.collect())
    }

    #[test]
    fn little_endian_records_come_back_in_order() {
        let d = file(0xa1b2_c3d4, true, &[(1, 2, &[0xaa, 0xbb]), (3, 4, &[0xcc])]);
        let p = packets(&d).unwrap();
        assert_eq!(
            p,
            [
                PcapPacket {
                    ts_sec: 1,
                    ts_usec: 2,
                    data: &[0xaa, 0xbb]
                },
                PcapPacket {
                    ts_sec: 3,
                    ts_usec: 4,
                    data: &[0xcc]
                },
            ]
        );
    }

    #[test]
    fn big_endian_magic_selects_big_endian_headers() {
        let d = file(0xd4c3_b2a1, false, &[(0x0102_0304, 7, &[1, 2, 3])]);
        let p = packets(&d).unwrap();
        assert_eq!((p[0].ts_sec, p[0].ts_usec), (0x0102_0304, 7));
        assert_eq!(p[0].data, [1, 2, 3]);
    }

    #[test]
    fn nanosecond_files_report_microseconds() {
        let d = file(0xa1b2_3c4d, true, &[(9, 123_456_789, &[0])]);
        let p = packets(&d).unwrap();
        assert_eq!(p[0].ts_usec, 123_456);
        let d = file(0x4d3c_b2a1, false, &[(9, 5_000, &[0])]);
        assert_eq!(packets(&d).unwrap()[0].ts_usec, 5);
    }

    #[test]
    fn truncated_last_record_is_dropped() {
        let mut d = file(0xa1b2_c3d4, true, &[(1, 1, &[1, 2, 3, 4])]);
        d.pop();
        assert!(packets(&d).unwrap().is_empty());
    }

    #[test]
    fn pcapng_and_short_files_are_rejected() {
        let mut d = vec![0u8; 24];
        d[..4].copy_from_slice(&0x0a0d_0d0au32.to_le_bytes());
        assert!(parse_pcap(&d).unwrap_err().to_string().contains("tcpdump"));
        assert!(parse_pcap(&d[..10]).is_err());
    }

    #[test]
    fn an_owned_file_walks_the_same_packets() {
        let d = file(0xa1b2_c3d4, true, &[(1, 2, &[0xaa]), (3, 4, &[0xbb, 0xcc])]);
        let pcap = Pcap::from_bytes(d.clone()).unwrap();
        assert_eq!(pcap.packets().collect::<Vec<_>>(), packets(&d).unwrap());
        assert!(Pcap::from_bytes(vec![0; 24]).is_err());
    }
}
