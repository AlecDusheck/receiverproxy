//! Colorlight receiver-card layer-2 protocol (as spoken by LEDVISION sender
//! cards / FPP's ColorLight-5a-75 output, reverse-engineered by the community).
//!
//! All frames are raw Ethernet:
//!   dst MAC 11:22:33:44:55:66, src MAC 22:22:33:44:55:66
//! "EtherType" is abused as a 2-byte packet type:
//!   0x0700       discovery request (270 zero bytes)
//!   0x0805       discovery response from the card (src 11:22:33:44:55:66)
//!   0x0107       display/vsync frame (98 bytes; carries brightness)
//!   0x0Abb       brightness (bb = brightness value; 63-byte payload)
//!   0x55rr       pixel row data (rr = row number MSB)

pub mod discovery;
pub mod flash;
pub mod params;
pub mod pixel;
pub mod upgrade;

pub use discovery::*;
pub use flash::*;
pub use pixel::*;

pub const CARD_MAC: [u8; 6] = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66];
pub const SENDER_MAC: [u8; 6] = [0x22, 0x22, 0x33, 0x44, 0x55, 0x66];

fn frame(ethertype: [u8; 2], payload: &[u8]) -> Vec<u8> {
    let mut f = Vec::with_capacity(14 + payload.len());
    f.extend_from_slice(&CARD_MAC);
    f.extend_from_slice(&SENDER_MAC);
    f.extend_from_slice(&ethertype);
    f.extend_from_slice(payload);
    f
}

impl std::fmt::Display for WriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ForbiddenBlock(b) => write!(
                f,
                "refusing to touch flash block 0x{b:02x}; only block 0x{PARAM_BLOCK:02x} \
                 (receiver parameters) may be written"
            ),
            Self::WrongPageSize(n) => {
                write!(f, "page payload is {n} bytes, must be {FLASH_PAGE_BYTES}")
            }
            Self::ForbiddenAddress(a) => write!(
                f,
                "refusing linear flash access at 0x{a:08x}; only the screen-size \
                 record at 0x{SCREEN_RECORD_ADDR:08x} may be reached this way"
            ),
        }
    }
}

impl std::error::Error for WriteError {}

impl std::str::FromStr for ColorOrder {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "rgb" => Ok(Self::Rgb),
            "bgr" => Ok(Self::Bgr),
            "grb" => Ok(Self::Grb),
            _ => Err(format!("unknown color order {s:?} (rgb|bgr|grb)")),
        }
    }
}
