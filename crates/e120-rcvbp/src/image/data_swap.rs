//! The data-swap pack body (`GetDataSwapEx2ParamPack` @ 0x1ec700).
//!
//! The 64-byte lane map from record +0x19A (an identity ramp 0x40..0x7F
//! unless the file overrides it), zeros, and three deseam-correction pairs
//! written by `CalDeseamCorrectData` — 8.8 fixed point 1.0 (`01 00`) when
//! deseam is off, which is the case the vendor's own image shows.

use crate::record01::View;

/// Offsets of the three deseam pairs' high bytes within the body.
const DESEAM_PAIRS: [usize; 3] = [0xEA, 0xF0, 0xF6];

#[must_use]
pub fn body(rec: &View) -> [u8; 256] {
    let mut body = [0u8; 256];
    body[..64].copy_from_slice(rec.swap_ramp());
    for at in DESEAM_PAIRS {
        body[at] = 0x01;
    }
    body
}
