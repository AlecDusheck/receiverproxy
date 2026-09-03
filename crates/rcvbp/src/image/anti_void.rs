//! The anti-void-line packs (`GetAntiVoidLineParam` @ 0x1604d0).
//!
//! With no void lines: two identical blocks of 2048 BE counters `0x2000 + n`
//! (bit 5 set, bit 7 clear since no line is void) at 0x1800. Packs 4-7 at
//! 0x7000 stay zero without large-load support.

pub const LEN: usize = 0x1000;

#[must_use]
pub fn region() -> [u8; LEN] {
    let mut out = [0u8; LEN];
    let (pairs, _) = out[..0x800].as_chunks_mut::<2>();
    for (pair, n) in pairs.iter_mut().zip(0u16..) {
        pair.copy_from_slice(&(0x2000 + n).to_be_bytes());
    }
    out.copy_within(..0x800, 0x800);
    out
}
