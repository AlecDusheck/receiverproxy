//! The basic-parameter pack body (page 0 of the boot image; real-time pack
//! sub-index 0), built from record 0x01 as `GetBasicParam` @ 0x1dfb50 does.
//! Byte-exact against the factory pack (tests/factory.rs). Fields the vendor
//! takes from chip-specific tables are zero for this chip family.

use super::PanelSpec;
use crate::record01::{off, View};

const CRC: usize = 0xFC;

pub fn body(spec: &PanelSpec, rec: View<'_>, prov: &mut Vec<String>) -> [u8; 256] {
    let mut b = [0u8; 256];
    let r = rec.bytes();
    let mut put = |at: usize, bytes: &[u8], what: &str| {
        b[at..at + bytes.len()].copy_from_slice(bytes);
        prov.push(format!("basicpack +{at:#04x} <- {what}"));
    };
    let (w, h2) = (spec.module.width as u8, (spec.module.height / 2) as u8);
    let modules = spec.modules_in_line_dir();
    let scan_len = spec.card_scan_len();
    let [s0, s1, s2] = spec.color.source;
    let lum = rec.luminance_level();

    put(0x00, &[0xA8], "marker");
    put(0x01, &r[off::PACK_HEAD3..off::PACK_HEAD3 + 3], "record +0x028");
    if spec.module.line_dir >= 2 {
        put(0x04, &[w, h2], "module width, height/2 (horizontal line dir)");
    } else {
        put(0x04, &[h2, w], "module height/2, width (vertical line dir)");
    }
    put(0x06, &[modules as u8], "modules in line dir");
    put(0x07, &[rec.scan()], "scan");
    put(0x08, &[rec.gray()], "gray bits");
    put(0x09, &rec.serial_clock().to_be_bytes(), "serial clock (BE)");
    put(0x0B, &spec.one_scan_len().to_be_bytes(), "OneScanLen (BE)");
    put(0x0D, &scan_len.to_be_bytes(), "CardScanLen (BE)");
    put(0x0F, &[head_code(r[0x008], (spec.module.width >> 8) as u8)], "record +0x008 code | module-dim high bits");
    put(0x10, &[(spec.color.swap << 6) | (s2 << 4) | (s1 << 2) | s0], "colour byte");
    put(0x14, &rec.u16_le(off::LUMINANCE).to_be_bytes(), "record +0x024 (BE)");
    put(0x16, &lum.to_be_bytes(), "record +0x026 (BE)");
    put(0x19, &[r[0x02F]], "record +0x02F");
    put(0x1A, &[0x80], "constant (image writer)");
    // ResetChipType @ 0x1e5130: ids below 0x100 go in the byte slot with the
    // 16-bit field zeroed; larger ids set the 0xFE escape and ride at 0xE7.
    let chip_id = rec.chip_id();
    put(0x1B, &[if chip_id < 0x100 { chip_id as u8 } else { 0xFE }], "chip id / escape");
    put(0x1C, &[r[0x037]], "record +0x037 serial type");
    put(0x1D, &[rec.line_dir()], "line dir");
    put(0x1E, &[r[0x0E6]], "record +0x0E6 packed flags");
    put(0x1F, &rec.u16_le(0x003).to_be_bytes(), "void point count (BE)");
    put(0x22, &[r[0x03D] & 0x0F], "scan method");
    put(0x23, &[r[0x03E]], "split");
    put(0x25, &[modules as u8], "modules in line dir / split segment");
    put(0x26, &[r[0x043], r[0x044] | (r[0x04E] & 1)], "record +0x043, +0x044 | output-model bit");
    put(0x28, &r[0x045..0x047], "gray compensation (LE)");
    put(0x2A, &[((spec.screen.width / spec.module.width) * (spec.screen.height / spec.module.height)) as u8], "module count");
    put(0x2C, &rec.u16_le(off::SERIAL_CLOCK_HALF).to_be_bytes(), "serial clock / 2 (BE)");
    put(0x2E, &rec.u16_le(off::SERIAL_CLOCK_DUP).to_be_bytes(), "serial clock (BE)");
    put(0x30, &r[off::GAINS..off::GAINS + 4], "current gains");
    put(0x37, &[r[0x050]], "8ns OE enable info");
    put(0x39, &scan_len.to_be_bytes(), "CardScanLen / split (BE)");
    put(0x3B, &spec.screen_extent_in_line_dir().to_be_bytes(), "screen extent in line dir (BE)");
    put(0x42, &[r[0x057], r[0x058]], "grid unit");
    put(0x46, &[spec.module_input_count()], "module input count");
    put(0x47, &[r[0x0B3]], "hub type");
    put(0x48, &current_split(lum, rec), "luminance split R, B, rest, G (BE)");
    put(0x50, &r[0x07A..0x08A], "swap block 1");
    put(0x60, &r[0x05A..0x06A], "swap block 0");
    put(0x70, rec.chip_custom(), "chip-custom block");
    put(0x80, &r[0x038..0x03C], "record +0x038");
    put(0x84, &r[0x0DC..0x0E0], "record +0x0DC");
    put(0x88, &spec.screen.width.to_be_bytes(), "MaxWidth (BE)");
    put(0x8A, &spec.screen.height.to_be_bytes(), "MaxHeight (BE)");
    put(0x8C, &[0x01, r[0x0E8]], "constant, record +0x0E8");
    put(0x8E, &(u16::from(h2) * 32).to_be_bytes(), "module dim x 32 (BE)");
    put(0x90, &[if r[0x01A] & 0x40 != 0 { r[0x052] } else { 0 }], "special-module setting (gated)");
    put(0x91, &r[0x0C4..0x0D8], "SChipControl");
    put(0xA5, &[r[0x009]], "record +0x009");
    put(0xB0, &r[0x08A..0x0AA], "swap blocks 2-3");
    put(0xD0, &r[0x0E0..0x0E4], "chip-custom-EX");
    put(0xD4, &[r[0x0E7], r[0x179]], "record +0x0E7, +0x179");
    put(0xD7, &[r[0x191]], "record +0x191");
    put(0xD8, &r[0x0EA..0x0F0], "record +0x0EA");
    put(0xE3, &scan_len.to_be_bytes(), "MaxPsc full (BE)");
    put(0xE5, &scan_len.to_be_bytes(), "MaxPsc max (BE)");
    let escaped = if chip_id < 0x100 { 0 } else { chip_id };
    put(0xE7, &escaped.to_be_bytes(), "chip id (BE, zero when it fits the byte slot)");
    put(0xF5, &[r[0x1EE], r[0x1F0], r[0x1F7]], "record +0x1EE, +0x1F0, +0x1F7");
    put(0xFA, &[r[0x193].wrapping_mul(2)], "2 x record +0x193");
    let crc = body_crc(&b);
    b[CRC..CRC + 4].copy_from_slice(&crc.to_le_bytes());
    prov.push(format!("basicpack +{CRC:#04x} <- CRC-32 of body[..0xFC] (chip-id bytes zeroed), LE"));
    b
}

/// Pack +0x0F: a code from record +0x008 through {2,3,0,1} (0 if >= 4), with
/// the module-height high bits mirrored into bits 4-5 and 6-7.
fn head_code(rec_008: u8, dim_hi: u8) -> u8 {
    let code = match rec_008 & 0xF {
        0 => 2,
        1 => 3,
        2 => 0,
        3 => 1,
        _ => 0,
    };
    code | ((dim_hi & 3) << 4) | ((dim_hi & 3) << 6)
}

/// Pack +0x48..+0x4F: luminance level split by the current percents,
/// R = floor(V*pR), G = floor(V*pG), B = floor((V-R-G)*pB), rest = V-R-G-B,
/// emitted as R, B, rest, G. The factory tool floors; the SDK dylib rounds.
fn current_split(v: u16, rec: View<'_>) -> [u8; 8] {
    let pr = rec.f32_le(off::CURRENT_PCT);
    let pg = rec.f32_le(off::CURRENT_PCT + 4);
    let pb = rec.f32_le(off::CURRENT_PCT + 8);
    let v_f = f32::from(v);
    let r = (v_f * pr).floor() as u16;
    let g = (v_f * pg).floor() as u16;
    let b = (f32::from(v - r - g) * pb).floor() as u16;
    let rest = v - r - g - b;
    let mut out = [0u8; 8];
    out[0..2].copy_from_slice(&r.to_be_bytes());
    out[2..4].copy_from_slice(&b.to_be_bytes());
    out[4..6].copy_from_slice(&rest.to_be_bytes());
    out[6..8].copy_from_slice(&g.to_be_bytes());
    out
}

/// Standard CRC-32 over body[..0xFC], computed before `ResetChipType` fills
/// the chip-id escape (0x1B) and chip id (0xE7..0xE8), so those hash as zero.
fn body_crc(body: &[u8; 256]) -> u32 {
    let mut hashed = *body;
    hashed[0x1B] = 0;
    hashed[0xE7] = 0;
    hashed[0xE8] = 0;
    !crate::crc32::update(0xFFFF_FFFF, &hashed[..CRC])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_factory_pack_carries_its_own_crc() {
        let body = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/factory-basic-pack-body.bin"
        ))
        .unwrap();
        let body: [u8; 256] = body.try_into().unwrap();
        assert_eq!(body_crc(&body).to_le_bytes(), body[CRC..], "74 a9 51 a3 expected");
    }
}
