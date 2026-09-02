//! The anti-void-line packs (`GetAntiVoidLineParam` @ 0x1604d0).
//!
//! With no void lines configured the generator degenerates to two identical
//! blocks of 2048 big-endian counters `0x2000 + n` (bit 5 always set, bit 7
//! cleared for every line since none is void), sliced into four 0x400-byte
//! packs at 0x1800. Packs 4–7 (0x7000) stay zero without large-load support.

pub const LEN: usize = 0x1000;

#[must_use]
pub fn region() -> [u8; LEN] {
    let mut out = [0u8; LEN];
    for (pair, n) in out[..0x800].chunks_exact_mut(2).zip(0u16..) {
        pair.copy_from_slice(&(0x2000 + n).to_be_bytes());
    }
    out.copy_within(..0x800, 0x800);
    out
}
